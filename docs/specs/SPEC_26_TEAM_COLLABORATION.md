# Spec 26 — Team Collaboration: provenance, inheritance, attribution

**Status:** implemented (v0.11.7)
**Supersedes the collaboration slice of:** Spec 24 §3.5 (teams/shares UI)

## 1. The problem

Teams shipped as plumbing: `teams`, `team_members`, `object_shares` and a
`+ New Team` button. What was missing was every question a human team
actually asks:

| Question the operator asks | Before | After |
|---|---|---|
| Who shared this forecast with me? | not exposed | `access_via` + `shared_by` + `shared_at` on every list row |
| Am I looking at a portfolio item or a standalone forecast? | not exposed | `portfolios[]` on every forecast list row |
| If a portfolio is shared with my team, do I get its forecasts? | **no** | yes — inherited access, one rule in `fermi_auth::visibility` |
| Which teammate moved this number? | not recorded | `fermi_forecast_updates.actor_user_id`, surfaced in three activity feeds |
| What has my team been doing? | client-side guess from `updated_at` | `GET /api/teams/:id/activity` — real attributed events |
| Who on the team is actually contributing? | nothing | `GET /api/teams/:id/contributions` |
| What exactly is shared with this team, by whom? | client-side filter over own forecasts | `GET /api/teams/:id/shared` |

Three root causes, addressed in order below: **no inheritance**, **no
provenance**, **no attribution**.

## 2. Inheritance — a portfolio share cascades to its forecasts

### 2.1 Rule

> A forecast is accessible to a principal if it belongs to a portfolio
> that principal can access **through a share** (direct user share, team
> share, or team-owned portfolio), *and* the forecast is in-scope for
> that share.

"In scope" is the leak guard. A portfolio is a curation surface: I can add
someone else's forecast to my portfolio. Sharing my portfolio must not
re-share their private work. So inheritance only propagates to a forecast
when either:

* **(a)** the forecast owner is the portfolio owner — the ordinary case
  ("my portfolio of my forecasts"), or
* **(b)** the share is a *team* share and the forecast owner is a member
  of that same team — joint team work inside a team portfolio.

Everything else stays denied. Public/`shared` visibility is unaffected;
it was already permissive.

### 2.2 Where it lives

Inside `fermi_auth::visibility::can_access`, as **step 5b**, gated on
`object_type == Forecast`. Putting it in the one canonical helper means
every existing call site — `get_forecast_handler`, `update_*`, `resolve`,
`shares.rs` guards, `polymarket`, `invites` — inherits the behaviour with
no call-site churn and no risk of one handler disagreeing with another.

It runs *only after* the cheap checks (admin, owner, public, direct user
share, team share) have all missed, so the common paths cost nothing.

Permission granted = the portfolio share's permission. A portfolio shared
`edit` with a team gives that team `edit` on the member forecasts, which
is what "we jointly manage these" means.

### 2.3 List parity

`list_forecasts_handler`'s `WHERE` clause carries the same fifth branch,
so a forecast reachable only by inheritance shows up in
`GET /api/forecasts?scope=shared` instead of being invisible in the list
but openable by id.

## 3. Provenance — "who shared this with me, and how"

### 3.1 `AccessProvenance`

`handlers::collab::forecast_access_provenance` /
`portfolio_access_provenance` batch-resolve, for a set of object ids and
one principal:

```jsonc
{
  "access_via": "owner" | "user_share" | "team_owned" | "team_share"
              | "portfolio" | "public" | "link",
  "permission": "view" | "edit" | "admin",
  "shared_by": "<user_id>",              // object_shares.granted_by
  "shared_by_display_name": "Alice",
  "shared_at": "2026-08-01T…",           // object_shares.created_at
  "team_id": "…", "team_name": "WC analysts",
  "via_portfolio_id": "…", "via_portfolio_title": "WC 2026"
}
```

Precedence is deliberate and matches `can_access`: ownership beats an
explicit user share, which beats team ownership, which beats a team
share, which beats portfolio inheritance, which beats bare visibility.
The console renders the *strongest true* statement rather than a pile of
badges.

One query per object type per request (`= ANY($ids)`), not one per row.

### 3.2 Where it is attached

* `GET /api/forecasts` — every row gets `access`, plus `share_count` and
  `portfolios: [{id,title,owner_id}]`.
* `GET /api/portfolios` — every row gets `access`, `share_count`,
  `team_ids`, `member_count`.
* `GET /api/portfolios/:id/forecasts` — every row gets `access` and
  `portfolios`, so the portfolio detail can say "also in: Base rates".

`portfolios[]` is what makes portfolio-context legible: an empty array
means **standalone**, one entry means it lives in that portfolio, two or
more means it is shared curation. The console shows `◈ standalone` vs
`◈ WC 2026 +1`.

## 4. Attribution — who did what

### 4.1 Schema (migration 176)

```sql
ALTER TABLE fermi_forecast_updates    ADD COLUMN actor_user_id TEXT;
ALTER TABLE fermi_portfolio_forecasts ADD COLUMN added_by      TEXT;
```

Every revision writer now stamps the acting principal:
`update_forecast_handler`, `update_probability_handler`,
`bayesops` refit-accept, `workspace::refit`, `polymarket` snapshot. Agent
runs keep `agent_id` *and* carry the human who triggered them, so an
event reads "Alice · via elo-scout" rather than losing one half.

`added_by` closes the same gap for curation: "Bo added this to the
portfolio" is a real team event.

Backfill sets `added_by` to the portfolio owner for pre-existing rows —
approximately true, and honest: those rows predate attribution.

### 4.2 Derived, not dual-written

There is no `collab_events` table. Events are **derived** by UNION over
the tables that already hold the truth:

| Event kind | Source |
|---|---|
| `created` / `published` | `fermi_forecasts.created_at`, `owner_id` |
| `revised` | `fermi_forecast_updates` (+ `actor_user_id`, `agent_id`, `revision_trigger`) |
| `resolved` | `fermi_forecasts.resolved_at`, `resolved_by`, `brier_score` |
| `shared` | `object_shares.created_at`, `granted_by` |
| `portfolio_add` | `fermi_portfolio_forecasts.added_at`, `added_by` |
| `member_joined` | `team_members.joined_at`, `invited_by` |
| `invited` | `forecast_invites.created_at`, `inviter_id` |

Derivation means the feeds are correct for all historical data on day
one, and no writer can forget to log. The cost is a wider query; all the
joins are on existing indexes and the feeds are `LIMIT`-bounded.

### 4.3 Feeds

```
GET /api/forecasts/:id/activity      one forecast's attributed history
GET /api/portfolios/:id/activity     portfolio + all member forecasts
GET /api/teams/:id/activity          everything the team can see
GET /api/teams/:id/contributions     per-member roll-up
GET /api/teams/:id/shared            inventory: what's shared, by whom
GET /api/forecasts/:id/access        who can see this, and how
GET /api/portfolios/:id/access       ditto
```

Event shape:

```jsonc
{
  "ts": "2026-08-04T12:03:00Z",
  "kind": "revised",
  "actor_id": "…", "actor_display_name": "Alice", "actor_kind": "user",
  "agent_id": "elo-scout",
  "object_type": "forecast", "object_id": "…", "object_title": "…",
  "summary": "revised 41% → 47%",
  "detail": { "previous_probability": 0.41, "new_probability": 0.47,
              "reason": "…", "revision_trigger": "manual" }
}
```

`actor_kind` is `user`, `agent`, or `system` (unattributed legacy rows).
The console never invents a name: unattributed rows render as
`— · system` rather than silently blaming the owner.

### 4.4 Team scope

`GET /api/teams/:id/activity` is the answer to "the team context is hard
to follow". Its scope is **the team's shared surface**:

1. forecasts owned by the team (`team_id`),
2. forecasts shared with the team (`object_shares`),
3. forecasts inside portfolios owned by or shared with the team
   (inheritance — same rule as §2),
4. the team's own membership events.

Then every event on that surface, by any actor, newest first. Filterable
by `?actor=<user_id>` and `?kind=`, which is what makes "which teammate
did which things" a one-click question.

## 5. Console surfacing

| Surface | Change |
|---|---|
| Portfolio → 📥 Shared with me | each row: `shared by Alice · via WC analysts · edit · 3d ago` |
| Portfolio → any forecast row | portfolio-context chip: `◈ WC 2026` or `standalone` |
| Portfolio detail header | Access strip: owner, teams with member peek, share count, "shared by" |
| Portfolio detail | Activity tab — attributed feed for the portfolio + members |
| Teams → Roster | contribution columns: revisions / resolutions / created / last active |
| Teams → Shared | server-driven inventory, split *direct* vs *inherited via portfolio*, each with grantor + timestamp; inline "share a portfolio with this team" |
| Teams → Activity | real feed, day-grouped, actor-filterable |
| Forecast → Access | inherited shares shown read-only as `via portfolio ‹title›`; effective-viewer list expands team → members |

## 6. Non-goals

* No share revocation history (a revoked `object_shares` row is gone;
  reconstructing it would need an audit table — deliberate deferral).
* No real-time push. Feeds are pull, cached client-side per selection.
* No per-driver attribution inside a forecast. `actor_user_id` is at
  revision granularity, which is where team disagreement actually shows.
