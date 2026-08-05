# v0.11.7 — Teams that actually work together

Teams shipped as plumbing: a `teams` table, a `team_members` table, an
`object_shares` table, and a **+ New Team** button. What was missing was
every question a human team actually asks about shared work.

The operator report that drove this release:

> *"in portfolio i can see that some forecasts are shared with my admin
> account but i cant see who has shared them — i cant tell the portfolio
> context or standalone context. and the team context is hard to follow.
> i need to see which team members did which things in the context of a
> forecast and a portfolio."*

Every one of those turned out to be a data problem, not a UI problem.

| Question | Before | After |
|---|---|---|
| Who shared this with me? | not exposed anywhere | `access` on every list row: path, grantor, timestamp, permission |
| Portfolio item or standalone? | not exposed | `portfolio_refs` on every forecast row — empty means standalone |
| Portfolio shared with my team ⇒ do I get its forecasts? | **no** | yes — inherited access, one rule in `fermi_auth::visibility` |
| Which teammate moved this number? | **not recorded at all** | `actor_user_id`, surfaced in three activity feeds |
| What has my team been doing? | guessed client-side from `updated_at` | `GET /api/teams/:id/activity` — real attributed events |
| Who is actually contributing? | nothing | `GET /api/teams/:id/contributions` |
| What exactly is shared with this team, by whom? | client-side filter over *your own* forecasts | `GET /api/teams/:id/shared` |

Design doc: `docs/specs/SPEC_26_TEAM_COLLABORATION.md`.

---

## The three root causes

**1. No inheritance.** `can_access` for a forecast only ever consulted
`object_shares` rows with `object_type='forecast'`. Sharing a portfolio
with a team did *nothing* for the forecasts inside it. Teams accumulated
piles of individually-shared forecasts because sharing a book didn't work.

**2. No provenance.** `object_shares` has carried `granted_by` and
`created_at` since migration 009, and no list projection ever returned
either. The console could not know who shared something with you. It was
fanning out one `/shares` call **per row** just to colour a team dot — and
that code was dead anyway, because it gated on a `share_count` field the
server hardcoded to `0`.

**3. No attribution.** `fermi_forecast_updates` had `agent_id` (which
agent produced a number) but no column for *which human caused the
revision*. Every revision on a shared forecast was anonymous. That is why
the Teams "Activity" tab was reduced to synthesising one fake event per
forecast from `updated_at` — over your own forecasts only, so it could
never show a teammate's work at all. It looked like an activity feed and
was really a list of forecasts sorted by date.

---

## Inheritance, with a leak guard

> A forecast is accessible if it belongs to a portfolio you can reach
> through a share — *and* the forecast is in scope for that share.

The second clause is the important one. A portfolio is a **curation
surface**: I can add *your* private forecast to *my* book. If a portfolio
share propagated unconditionally, then "add a colleague's private forecast
to a portfolio, share the portfolio" would be a privilege-escalation
primitive.

So a share on portfolio `P` reaches forecast `F` only when either:

* **(a)** `F.owner_id = P.owner_id` — the ordinary "my book of my
  forecasts" case, or
* **(b)** the share is a *team* share and `F`'s owner is on that same team
  — genuine joint team work in a team book.

The rule lives in exactly one place: step 5b of
`fermi_auth::visibility::can_access`. All ~13 existing call sites
(`get_forecast_handler`, the update/resolve paths, `shares.rs` guards,
polymarket linking, invite materialisation) inherit it with no call-site
churn, and no handler can drift out of agreement with another. It runs
last, after admin/owner/public/user-share/team-share have all missed, so
the hot paths pay nothing for it.

`INHERITED_ACCESS_RELATION_SQL` is a single `const` consumed three ways —
by the ACL, by the thing that *explains* the ACL to the operator, and by
the list `WHERE` clause. Hand-copying an ACL into three places is how
enforcement and explanation drift apart.

## Attribution is honest about what it doesn't know

Migration 176 adds `fermi_forecast_updates.actor_user_id` and
`fermi_portfolio_forecasts.added_by`. Every revision writer now stamps the
acting principal, keeping `agent_id` alongside it — an agent-assisted
revision has a human who pointed the agent at the problem, and dropping
that half is the bug.

`actor_user_id` is **not backfilled.** Pre-176 rows have no recoverable
actor, so they surface as `actor_kind: "system"` and render as `—`.
Attributing them to the forecast owner would have produced a UI that
cannot distinguish a guess from a fact.

`added_by` *is* backfilled to the portfolio owner, because before shares
existed only the owner could add to their own book — a defensible
approximation, not a guess.

## Events are derived, not dual-written

There is no `collab_events` table. Every event is derived by `UNION` over
tables that already hold the truth (forecasts, forecast updates,
`object_shares`, portfolio membership, team membership, invites). Two
consequences, both deliberate: the feeds are correct for **all**
pre-v0.11.7 history on day one, and no writer can forget to log because
there is nothing to log to.

---

## New endpoints

```
GET /api/forecasts/:id/access        who can see it, how, who granted it
GET /api/forecasts/:id/activity      attributed history
GET /api/portfolios/:id/access       + cascades_to
GET /api/portfolios/:id/activity
GET /api/teams/:id/shared            inventory: what, by whom, direct vs inherited
GET /api/teams/:id/activity          ?actor= / ?kind= filterable
GET /api/teams/:id/contributions     per-member roll-up
```

`access`, `share_count` and `portfolio_refs` are now batch-attached to the
forecast and portfolio list projections — one query per page, replacing
the old per-row fan-out.

`cascades_to` on the portfolio access endpoint is the number that makes
portfolio sharing comprehensible: it isn't one share, it's *N*. And
because of the leak guard it isn't always all of them, so the UI says
"8 of 11 forecasts inherit this access" rather than implying full coverage.

## Console

* **Portfolio → Shared with me** — a roll-up header
  (`from Alice (6) · Bo (2) · 👥 WC analysts (5) · ◈ 3 inherited`) plus a
  per-row line: `👥 shared by Alice · via WC analysts · edit`.
* **Every forecast row** — portfolio-context chip (`◈ WC 2026 +1`, or
  `standalone`). Suppressed for plain owned rows: chrome that appears on
  every row is chrome the eye learns to skip.
* **Portfolio detail** — always-visible access strip naming the teams, the
  effective viewer count (teams flattened to people, because "shared with
  2 teams" is not an answer to "who can read this"), and `cascades_to`.
  Plus an Activity toggle.
* **Teams → Roster** — contribution counts per member; clicking a member
  filters the Activity feed to them. One click, one question answered.
* **Teams → Shared** — server-driven. Portfolios first because they're
  causally upstream of the forecasts below them; inherited forecasts in
  their own group; and an inline **"◈ Share a portfolio with this team"**
  picker. That action previously existed only in the portfolio detail's
  Access panel, two panels away from the team you were looking at.
* **Teams → Activity** — real day-grouped feed with an actor column.
* **Forecast → Access** — expandable team rosters, a read-only "Inherited
  from portfolios" section (you revoke those on the portfolio, not here),
  and a "Who can see this" list.

---

## Validation

These are runtime `sqlx::query` strings with zero compile-time checking,
so `scripts/spec26_sql_check.sh` stands up a throwaway Postgres cluster in
`/tmp` — no credentials, never touches `DATABASE_URL` — and:

1. runs migration 176 twice, proving idempotency (the runner executes
   every migration on every boot);
2. `PREPARE`s all 20 queries, which fully parses and type-checks them
   against a live catalogue. This catches the whole class of bug that
   otherwise only appears as a 500 on first call — typos, wrong `::text`
   casts, `UNION` branch arity/type mismatches. The `TEXT`-vs-`UUID` split
   across these join keys makes every cast load-bearing;
3. asserts the **leak guard behaviourally**: Bob sees Alice's forecast via
   the shared book but *not* Carol's, until Carol joins the team.

Plus 19 unit tests (`cargo test -p fermi-auth --lib`,
`cargo test --bin api-server collab::`) covering the SQL placeholder
substitution, leak-guard presence, event summarisation, and the
attribution classifier.

## Migrations

| # | Purpose |
|---|---|
| 176 | `fermi_forecast_updates.actor_user_id`, `fermi_portfolio_forecasts.added_by`, covering indexes for the derived feeds |

## Known gaps

* **Share revocations can't appear in the feeds.** A revoked
  `object_shares` row is deleted, so revocation history is structurally
  unrecoverable without an audit table. Deferred deliberately.
* Feeds are pull, not push; cached client-side per selection.
* Attribution is at revision granularity, not per-driver.
