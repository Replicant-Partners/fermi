# v0.11.3 — Orchestra follow-ups: admin inbox, manager-effect metric, live roster

Three tightly-scoped items to make v0.11.2's orchestra registry
actually useful in practice. They belong together because they all
answer the same question after Mario successfully published
`guidance_tracker`: *now what?*

1. **How does Ivan approve Mario's join request without curl?** →
   admin inbox UI.
2. **How do we measure whether Fermi's roster is helping?** →
   counterfactual Brier substrate.
3. **How does the newly-approved agent actually show up in Fermi's
   plans?** → dynamic roster injection into the strategist's system
   prompt.

## Item 1: Admin inbox UI (`templates/admin.html`)

New `Orchestras` tab in `/admin`:

- Dropdown for orchestra (currently only `fermi`; extensible when
  more strategists ship).
- Status filter: `pending` (default) | `approved` | `rejected` |
  `withdrawn` | `all`.
- One card per request:
  - Agent name (links to `/agent/<name>`), agent_type, tier badge.
  - Description, requester display, submitted timestamp.
  - Contract summary (`finding_labels`, `multiplier_range`), plus a
    collapsible pretty-printed full `proposed_contract`.
  - Free-form rationale if present.
  - **Approve** / **Reject** buttons on `pending` rows only.
- Approve dialog: prompt for an optional review note.
- Reject dialog: prompt for a **required** review note (the server
  enforces this; the UI blocks empty submission client-side too).
- Post-review rows show `Reviewed by <user_id> at <ts> — "<note>"`.

Endpoints (unchanged from v0.11.2, this release is pure frontend):

- `GET /api/orchestras/:name/requests?status=…`
- `POST /api/orchestras/:name/requests/:id/approve` — `{note?, final_contract?}`
- `POST /api/orchestras/:name/requests/:id/reject` — `{note}` (required)

Admin gating is unchanged: `require_orchestra_admin` in
`handlers/orchestras.rs` checks that the caller owns the orchestra's
strategist agent card, with platform-admin bypass.

## Item 2: Counterfactual Brier substrate (manager-effect delta)

Football-manager model, confirmed with Ivan:

- **Roster-locked:** Fermi's public Brier IS the team's Brier.
- **Roster-orthogonal:** manager skill = `Team Brier − Counterfactual Brier`,
  where the counterfactual is what naive-average aggregation would
  have scored on the same forecast with the same member outputs.

**Split of responsibility, intentional:**

- **Client (Fermi harness):** owns the naive-baseline formula.
  Sends `counterfactual_probability ∈ [0,1]` at forecast creation.
- **Server:** persists verbatim; computes
  `counterfactual_brier = (cf_prob − outcome::real)²` at resolution;
  surfaces `manager_effect = brier_score − counterfactual_brier`
  on both `GET /api/fermi/forecasts/:id` and the resolve response.
- Negative `manager_effect` = team beat the naive baseline on that
  forecast. Positive = naive would have scored better.

Schema:

- New column `fermi_forecasts.counterfactual_probability REAL`
  with `CHECK (>=0 AND <=1)`, added via `ensure_critical_schema`
  (PgBouncer-safe, single-statement, idempotent). Companion to
  `counterfactual_brier` from mig-172.
- Nullable — legacy rows and non-Fermi forecasts pass through with
  `NULL` for both fields, and `manager_effect` degrades to `null`.

Wiring:

- `CreateForecastRequest.counterfactual_probability: Option<f64>`,
  clamped to `[0,1]` before insert as defense against client bugs
  (the `CHECK` catches it too, but a 400 beats a 500).
- `resolve_forecast_handler` runs an extra best-effort `UPDATE` to
  populate `counterfactual_brier` when `counterfactual_probability`
  was set. A failure here does NOT roll back the resolve — the
  delta is a nice-to-have metric, not part of the resolve contract.
- `get_forecast_handler` and the resolve response expose
  `counterfactual_probability`, `counterfactual_brier`, and the
  computed `manager_effect` delta.

## Item 3: Dynamic roster injection into Fermi's system prompt

Problem: Fermi's curated `system_prompt` hard-coded its specialist
roster (`macro_forecaster`, `equity_analyst`, `sentiment_analyzer`,
`entity_investigator`). Mario's approved `guidance_tracker` was in
the registry and view but never appeared in Fermi's decomposition
plan because the curated prompt didn't know about it.

Fix: at execute time, look up the strategist's live roster and
append a `## CURRENT ROSTER` block to the top-level
`AgentCard.system_prompt`. New helper
`handlers::orchestras::inject_orchestra_context(db, card) -> AgentCard`
runs unconditionally in both `execute_agent_handler` and
`execute_agent_stream_handler` — non-strategist cards pass through
unchanged.

- `STRATEGIST_AGENTS: &[(&str, &str)]` maps agent_name → members
  view. Currently `[("fermi", "orchestra_fermi_members")]`.
- Format per line: `` - `agent_name` (agent_type) — <one-line description, ≤140 chars> ``.
- Guardrail: instructs the strategist to prefer roster members and
  to flag gaps rather than invent agent names.
- DB error path logs and returns the card unchanged — a missing
  roster is far better than a failed execution.
- Empty roster skips injection (avoids emitting a "there are no
  members" block that would just confuse the LLM).

### xaman_ek intentionally deferred

Xaman Ek has 100+ members and its own `list_agents` tool for
catalogue queries. Injecting the full roster at ~40 tokens/entry
would inflate every invocation by 4–5k tokens. Deferred until we
build a compact per-tier / per-tag digest view. Adding it later is a
one-line change in `STRATEGIST_AGENTS`.

## Post-deploy verification

```bash
# Item 2: schema
psql "$DATABASE_URL" -c \
  "SELECT column_name, data_type FROM information_schema.columns
    WHERE table_name = 'fermi_forecasts'
      AND column_name IN ('counterfactual_probability','counterfactual_brier');"

# Item 2: create a Fermi-style forecast with cf_prob and resolve
curl -X POST /api/fermi/forecasts \
  -H 'content-type: application/json' \
  -d '{"question_text":"…","predicted_probability":0.7,
       "counterfactual_probability":0.5, ...}'
curl -X POST /api/fermi/forecasts/:id/resolve \
  -H 'content-type: application/json' \
  -d '{"actual_outcome": true}'
# Expect brier_score, counterfactual_brier, manager_effect all populated.

# Item 3: execute fermi and confirm system_prompt contains
# "## CURRENT ROSTER" plus a line for guidance_tracker.
curl /api/agents/fermi/execute -d '{"task":"…"}'

# Item 1: open /admin → Orchestras tab. Pending requests render.
# Approve/Reject flows update the list and reflect in
# /agent/<name>'s membership badges.
```

## Follow-ups (deliberately NOT in this release)

- **Xaman Ek digest injection.** Once a compact roster view exists
  (per-tier or per-tag), add `("xaman_ek", "orchestra_xaman_ek_digest_v1")`.
- **Harness computes naive baseline.** The Fermi harness needs to
  compute `counterfactual_probability` from member outputs before
  it POSTs the forecast. Server is ready; client is next.
- **Team Brier trend + manager-effect chart** on `/agent/fermi`
  (visualisation of the new metric now that we're recording it).
- **Additional orchestras.** Only Fermi is a strategist today; the
  UI dropdown accepts more when they land.

## Migrations

None. Item 2's column is added via `ensure_critical_schema`, the
single-statement idempotent pattern that plays nicely with PgBouncer
transaction pooling. Migration numbering is unchanged; next slot is
**mig-173**.

## Files changed

- `src/api_server.rs` — `ensure_critical_schema` entry for
  `counterfactual_probability`.
- `src/handlers/forecasts.rs` — request field, INSERT wiring,
  resolve `UPDATE`, response fields on both GET and resolve.
- `src/handlers/orchestras.rs` — `inject_orchestra_context` +
  `STRATEGIST_AGENTS` const.
- `src/handlers/execution.rs` / `src/handlers/execution_stream.rs` —
  one-line injection call each.
- `templates/admin.html` — new Orchestras tab, request loader,
  approve/reject handlers, tab-switch hook.
