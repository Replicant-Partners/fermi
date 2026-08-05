# v0.11.2 — Orchestra registry: first-class membership with admin-gated joins

## Why

`guidance_tracker` now publishes cleanly (v0.10.27 → v0.10.29). Mario
asked the obvious next question: **how does it become a Fermi member?**

Answer under the old substrate: manually edit the agent card JSON
to add `capabilities.fermi_contract = {finding_labels: [...], ...}`,
call `PUT /api/agents/:id`, done. The mechanism worked, but it was:

- **Opaque** — Mario needed to read `agent_backend/agent_card.rs`
  to know the field exists.
- **Ungoverned** — anyone with edit rights on an agent card could
  self-declare Fermi membership without a review of whether the
  agent actually improves the composition's calibration.
- **Invisible** — no UI, no roster page, no notion of "the Fermi
  orchestra" as a first-class artifact.

This release ships the substrate: explicit **request → admin review
→ approve/reject** flow, roster views, and a Manage-tab panel that
makes orchestra membership legible.

## The football-manager model (locked in)

Confirmed with Ivan before building. Two axes:

- **Roster-locked** — Fermi's Brier score IS the Brier of forecasts
  produced by *this specific team* on *this specific question*.
  Swap a member, the score changes. There is no "Fermi score"
  abstracted from the players.
- **Roster-orthogonal** — Fermi's *skill* is `Team Brier −
  Counterfactual Brier`, where Counterfactual = what the same
  forecast would have scored under naive-average aggregation
  without Fermi's `cep_weighted` synthesis. Same-roster,
  different-strategist A/B is what isolates the manager effect.

This release owns the **roster** substrate. The counterfactual
column is reserved (see below) but not populated — that's the next
research release once we have real approved members to measure.

## Change

### 1. Migration 172 — `orchestra_membership_requests` + views

`migrations/172_orchestra_membership.sql`

- **Table `orchestra_membership_requests`** — audit trail of every
  request. Statuses: `pending | approved | rejected | withdrawn`.
  Preserves proposed_contract, rationale, reviewer, review_note,
  timestamps. FKs on `agent_id` and `requested_by`.
- **View `orchestra_fermi_members`** — `SELECT ... FROM agents
  WHERE status = 'published' AND fermi_contract IS NOT NULL`. The
  contract IS the membership; no membership table needed.
- **View `orchestra_xaman_ek_members`** — every published agent.
  xaman_ek is implicit; publishing IS joining.
- **Column `fermi_forecasts.counterfactual_brier`** — nullable
  placeholder for the manager-effect metric (`Team − Counterfactual`).
  Reserved so future counterfactual computation doesn't need a
  schema change.

Idempotent, PgBouncer-safe DO blocks with EXCEPTION handlers,
`RAISE NOTICE` observability. Registered in `run_migrations()`.

### 2. `src/handlers/orchestras.rs` — new module

Seven endpoints:

```
GET  /api/orchestras                                 # list orchestras + counts
GET  /api/orchestras/:name/members                   # full roster
GET  /api/orchestras/:name/requests?status=pending   # admin inbox
POST /api/orchestras/:name/requests                  # submit (owner)
POST /api/orchestras/:name/requests/:id/approve      # admin
POST /api/orchestras/:name/requests/:id/reject       # admin (note required)
POST /api/orchestras/:name/requests/:id/withdraw     # requester
GET  /api/agents/:id/orchestras                      # per-agent memberships
```

Governance:

- **Owner gate on submit** — only the agent's owner (or a platform
  admin) can propose the agent for membership.
- **Admin gate on approve/reject** — the caller must own the
  orchestra's strategist agent (for Fermi: whoever owns the `fermi`
  agent card in the DB) OR be a platform admin. Extensible: to
  delegate Fermi later, transfer the `fermi` agent's ownership.
- **Anti-duplicate** — refuses to insert a second pending request
  for the same (orchestra, agent) pair, and refuses if the agent
  is already a member.
- **Withdrawal** — the requester (or a platform admin) can
  withdraw a pending request. Preserves the row with
  `status = 'withdrawn'` so the audit trail survives.
- **Contract validation** — Fermi-specific: rejects empty
  `finding_labels`, malformed `multiplier_range`. Wrong-shape
  payloads never reach the admin's inbox.

Approval flow (transactional):

1. Update `agents.fermi_contract` with the final contract (either
   the proposed one, or an admin-edited variant via the `note` +
   `final_contract` fields).
2. Update `orchestra_membership_requests` to `status = 'approved'`
   with reviewer + timestamp + note.
3. Insert audit row in `admin_bypass_events` (target_type='agent',
   action='orchestra_approve') so the governance trail is legible
   six months later.

All three in one `tx.begin()` — a partial write can't leave the DB
in an inconsistent state (contract set but request still pending,
or vice versa).

### 3. Manage-tab UI panel

`templates/agent_detail.html`

New section on the owner's Manage tab, between Agent Wallet and
Edit Agent:

```
ORCHESTRAS
Compositions this agent participates in.
xaman_ek is implicit (every published agent). fermi is opt-in and
admin-gated — request membership, the Fermi maintainer reviews
for calibration fit.

┌────────────────────────────────────────────────────────────────┐
│ xaman_ek                                            [MEMBER]   │
│  implicit: every published agent                                │
├────────────────────────────────────────────────────────────────┤
│ fermi                                        [NOT A MEMBER]     │
│  explicit: fermi_contract declared, admin-approved              │
│                                              [Request Membership]│
└────────────────────────────────────────────────────────────────┘
```

Clicking `Request Membership` on Fermi opens a three-step prompt:

1. `finding_labels` (comma-separated, default `"BASE RATE, MULTIPLIER"`)
2. `multiplier_range` (`min, max`, default `"0.1, 3.0"`)
3. Rationale (optional)

Submits `POST /api/orchestras/fermi/requests`. The panel re-renders
with the `PENDING REVIEW` pill and a `Withdraw` button.

When pending, the admin's review note (if any) is displayed
inline so the requester sees rejection reasons the moment they
land.

### 4. `schema_trust` contract expanded

Added to the boot-time check:

- Table `orchestra_membership_requests`.
- Columns: `orchestra_membership_requests.{request_id,
  orchestra_name, agent_id, requested_by, proposed_contract,
  status, reviewed_by, review_note}`.
- Column `fermi_forecasts.counterfactual_brier`.

Any drift on these now surfaces at boot per v0.11.0.

## What Mario does now

1. Opens `/agent/guidance_tracker` → Manage tab.
2. Sees the Orchestras panel with two rows: `xaman_ek` (already a
   member — implicit) and `fermi` (not a member, Request button).
3. Clicks `Request Membership`, walks through the three prompts.
4. Sees `PENDING REVIEW` immediately.

## What Ivan does now

Ivan is the owner of the curated `fermi` agent card, so he's the
Fermi orchestra admin.

1. Opens the admin inbox — one of two shapes:
   - `GET /api/orchestras/fermi/requests?status=pending` — the JSON.
   - (Future v0.11.3 candidate: a dedicated admin page in
     `templates/admin.html`. For this release, curl or a bespoke
     dashboard.)
2. Reviews the agent card: description, system prompt, sample
   queries, tags. Checks the agent's own execution history if any.
3. Approves via
   `POST /api/orchestras/fermi/requests/:id/approve` with an
   optional `note` and optional `final_contract` override.
4. Or rejects via `.../:id/reject` with a required `note`.

Approval: `agents.fermi_contract` is set → `orchestra_fermi_members`
view immediately includes the agent → Fermi's dispatcher (once
wired) picks it up on next forecast.

## What this release does NOT do

**Admin-inbox UI.** Ivan can approve via curl today. A dedicated
admin page (`/admin/orchestras/fermi`) with per-request
approve/reject buttons + agent-card preview is a v0.11.3 candidate.
Keeping this release tight to the substrate + owner-side UX so we
can validate the flow with a real request first.

**Counterfactual Brier computation.** Column is reserved. When
we ship it (v0.11.3+), the executor will produce two parallel
resolutions per Fermi forecast: the actual synthesis and the naive
average of members' raw multipliers. Delta = manager skill. Not
now.

**Fermi's dispatcher wiring.** Right now Fermi's specialist roster
is hard-coded in its system prompt (`macro_forecaster,
equity_analyst, sentiment_analyzer, entity_investigator`). Making
Fermi query `/api/orchestras/fermi/members` at composition time is
a next step — needs a change to how Fermi builds its research
plan. v0.11.3 candidate. Until then: approved agents ARE in the
roster view, but Fermi's actual invocations still use the
hardcoded list.

**xaman_ek's list.** Same — its ontology is hardcoded in its
system prompt. The view exists; wiring Xaman Ek's `list_agents`
tool to query it is the same shape of follow-up.

Both wire-ups are agent-card / prompt-tuning tasks, not infra
tasks — the substrate is the hard part and it's now done.

## Post-deploy verification

```bash
# The orchestras endpoint returns the two known ones.
curl -s "https://agent-bestiary.world/api/orchestras" | jq '.orchestras[].name'
# → "fermi"
# → "xaman_ek"

# Roster views work.
curl -s "https://agent-bestiary.world/api/orchestras/xaman_ek/members" \
     | jq '.member_count'
# → 100+ (every published agent)

curl -s "https://agent-bestiary.world/api/orchestras/fermi/members" \
     | jq '.member_count'
# → the current fermi_contract-declaring set

# Mario's guidance_tracker sees the panel on Manage tab.
# UI: /agent/guidance_tracker → Manage → Orchestras section
```

Boot-time schema_trust check should show all new objects present:

```bash
railway logs | grep '\[schema_trust\]' | head -5
# Expected: [schema_trust] ✓ contract verified — 41 tables, 78 columns, ...
```

## Follow-ups (v0.11.3+ candidates)

1. **Admin inbox UI** — `/admin/orchestras/fermi` page with pending
   requests, approve/reject buttons, agent-card preview.
2. **Counterfactual Brier** — dual-resolution + delta computation.
3. **Wire Fermi's dispatcher to the view** — Fermi queries
   `/api/orchestras/fermi/members` at forecast time instead of
   using its hardcoded roster.
4. **Wire xaman_ek's `list_agents` to the view** — same, for the
   platform-navigator ontology.
5. **Additional orchestras** — `simops`, `coherence`, etc. Adding
   a new orchestra = one migration (view) + one line in
   `ORCHESTRAS` const.

## Related

- v0.10.29 — publish-checks UX (unblocked Mario's ability to
  publish at all, which is what surfaced this need).
- v0.11.0 — schema trust contract (v0.11.2 extends its manifest).
- `src/agent_backend/agent_card.rs::FermiContract` — the shape
  requests carry and the shape approval writes to `agents`.
- `admin_bypass_events` (mig-164) — where every orchestra approval
  lands for the audit trail.
