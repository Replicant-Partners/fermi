# Workspace Resolution & BayesOps Refit Hook — Contract

**Status:** Shipped (handler + migration 147). BayesOps refit hook is a TODO insertion point waiting for the parallel string.
**Audience:** BayesOps implementation team; anyone wiring match/event resolvers that need to close the learning loop.

---

## Why this exists (the framing answer)

The original question — "where do match outcomes get written?" — assumed a domain-specific write target (a `wc_match_outcomes` table or a SOSA producer). That framing is too narrow.

**Resolution is a property of every forecast workspace, not just of matches.** A team-prior workspace resolves when its tournament ends. An H2H-match workspace resolves when its match is played. A "will X happen by Y" workspace resolves at date Y. A market-pricing workspace resolves when the asset's question is settled. All the same shape.

So the right primitive is `POST /api/workspaces/:id/resolve`. Match outcomes are *one domain* that triggers it; everything else (a `kask_simops` cultivation run finishing, a `rabble_chat` debate concluding, a manual user closure) flows through the same endpoint.

This makes BayesOps' contract clean: there's exactly one hook point regardless of domain.

---

## The endpoint

### POST /api/workspaces/:workspace_id/resolve

```json
{
  "outcome": <any JSON — domain-specific shape>,
  "resolved_at": "2026-07-15T18:00:00Z",   // optional, defaults to NOW()
  "resolution_notes": "Final-day result",  // optional
  "resolution_source": "fifa_official",    // optional provenance tag
  "failure": false                          // optional, default false
}
```

**Response:**
```json
{
  "workspace_id": "...",
  "workspace_status": "completed",
  "resolved_at": "...",
  "outcome": { ... },
  "brier_score": 0.0529,
  "predicted_probability": 0.27,
  "downstream_notified": 4,
  "refit_triggered": false
}
```

**Semantics:**

1. Caller must be a member of the workspace.
2. Workspace must be in `workspace_status = 'active'`. Resolving twice is a 409 Conflict.
3. The handler atomically:
   - Sets `teams.workspace_status` to `completed` (or `failed` if `failure: true`)
   - Records `resolved_at`, `resolved_by`, `resolution_outcome`, `resolution_notes`, `resolution_source`, `brier_score` on `teams`
   - Writes the outcome as a workspace output keyed `resolution` (versioned, queryable, propagates via dependency DAG)
   - Computes Brier against the last published `predicted_probability` output if both inputs are binary-scorable
   - Emits a `upstream_resolved` system event to every downstream workspace
4. Then calls the BayesOps refit hook (currently TODO; see below).

---

## Resolution payload shapes

The `outcome` field is domain-specific. The handler doesn't interpret most of it — it just stores it. But for Brier scoring, the handler looks for a binary value via these conventions (in priority order):

| Shape | Brier-scorable? | Example |
|-------|----------------|---------|
| `0.0` or `1.0` (bare number) | yes | `1.0` |
| `true` / `false` | yes | `true` |
| `{ "value": 0.0\|1.0 }` | yes | `{ "value": 1.0 }` |
| `{ "value": true\|false }` | yes | `{ "value": true }` |
| `{ "won_tournament": bool }` | yes | `{ "won_tournament": false }` |
| `{ "advanced": bool }` | yes | `{ "advanced": true, "group_position": 2 }` |
| `{ "won": bool }` | yes | `{ "won": true }` |
| anything else | no — Brier stays NULL | `{ "winner_team_id": "ARG", "home_goals": 2 }` |

The non-scorable cases (multi-class outcomes, free-form strings) are fine — they just don't get a Brier score. Phase 6+ adds multi-class scoring (log-loss, ranked probability score) for those.

---

## What gets written where

### Canonical lifecycle: `teams` columns

Added by migration 147:

```sql
ALTER TABLE teams ADD COLUMN resolved_at         TIMESTAMPTZ;
ALTER TABLE teams ADD COLUMN resolved_by         TEXT;
ALTER TABLE teams ADD COLUMN resolution_outcome  JSONB;
ALTER TABLE teams ADD COLUMN resolution_notes    TEXT;
ALTER TABLE teams ADD COLUMN resolution_source   TEXT;
ALTER TABLE teams ADD COLUMN brier_score         REAL;
```

Adjacent to the existing `workspace_status` from migration 143. Atomic with the status transition.

### Consumable artefact: `workspace_outputs[ws].resolution`

```json
{
  "outcome":               <the user-provided payload>,
  "resolved_at":           "2026-07-15T18:00:00Z",
  "resolved_by":           "user-uuid-or-id",
  "workspace_status":      "completed",
  "resolution_source":     "fifa_official",
  "resolution_notes":      "...",
  "brier_score":           0.0529,
  "predicted_probability": 0.27
}
```

Why both? The `teams` row is the atomic lifecycle truth. The `workspace_outputs` row is the **propagating artefact** — when it gets written, the existing dependency-DAG fan-out (which the BayesOps hook will use) wakes up downstream workspaces. One source-of-truth (teams), one consumable replica (outputs).

### Cross-workspace event: `workspace_messages`

Each downstream workspace (per `workspace_dependencies`) receives:

```json
{
  "event": "upstream_resolved",
  "upstream_workspace_id": "...",
  "outcome": { ... },
  "brier_score": 0.0529
}
```

Mirrors the existing `upstream_output_updated` event used by `set_output_handler`, so existing subscribers handle it uniformly.

---

## The BayesOps refit hook (insertion point)

The handler ends with a clearly-marked TODO block. Pseudocode for what should go there:

```rust
// At the very end of resolve_workspace_handler, AFTER tx.commit():

let pool_bg = state.db.clone();
let ws_id_bg = ws_uuid;
let user_id_bg = user_id.clone();

tokio::spawn(async move {
    // ── 1. Refit THIS workspace ────────────────────────────────────
    //
    // Reads:
    //   workspace_outputs[ws_id_bg].resolution        (just written)
    //   workspace_outputs[ws_id_bg].observations[]    (optional, if you accumulate incremental obs)
    //   workspace_outputs[ws_id_bg].learnable_manifest (priors)
    //
    // For each driver listed as learnable in the manifest:
    //   fit_marginal(observations, weights, family) -> FittedDistribution
    //   merge into workspace_outputs[ws_id_bg].params under key
    //     <driver_name>_fitted
    //
    // Then PUT the merged params back via the outputs API so the
    // workspace_output_updated fan-out fires for any further
    // downstream consumers.
    if let Err(e) = refit_workspace(&pool_bg, ws_id_bg, &user_id_bg).await {
        tracing::warn!(ws = %ws_id_bg, error = %e, "refit_workspace failed");
    }

    // ── 2. Refit UPSTREAM workspaces ───────────────────────────────
    //
    // For each upstream workspace whose outputs this one consumed,
    // their priors should update against the observed downstream
    // outcome. Example: when an H2H-match workspace resolves with
    // ARG winning, the two ARG and BRA team-prior workspaces it read
    // from should update their X3 (Dynamic Performance) posteriors.
    let upstreams: Vec<Uuid> = sqlx::query_scalar(
        "SELECT upstream_id FROM workspace_dependencies WHERE downstream_id = $1"
    )
    .bind(ws_id_bg)
    .fetch_all(&pool_bg)
    .await
    .unwrap_or_default();

    for up_id in upstreams {
        if let Err(e) = refit_workspace(&pool_bg, up_id, &user_id_bg).await {
            tracing::warn!(ws = %up_id, error = %e, "upstream refit_workspace failed");
        }
    }
});
```

**Important guarantees you can rely on:**

- The transaction has already committed when this hook fires. Resolution is durable. A failure in the hook never corrupts the resolution.
- The `resolution` output is already in `workspace_outputs` and is the latest version. You can read it back immediately.
- The fan-out event has already been written to `workspace_messages`. Downstream workspaces may already be reacting.
- The hook runs in a `tokio::spawn` background task. The HTTP response returns to the caller as soon as resolution is committed, even if the refit takes seconds.

**What you should change when wiring this:**

- Set `refit_triggered: true` in the response JSON so callers know the loop closed.
- Add a `refit_outcome` field to the response, populated synchronously OR delivered via a separate `GET /api/workspaces/:id/refit-status` endpoint if refits are slow.

**Failure mode policy:**

Refit failures MUST be log-and-continue. The resolution is the user's action; if BayesOps can't fit posteriors today, the user's resolution still stands and they can try again later. The hook is value-add, not on the critical path.

---

## Reading the data

For the WC demo specifically, after resolving an ARG team-prior workspace:

```bash
# Inspect the canonical lifecycle state
curl -s "$API/api/workspaces/$ARG_WS_ID" -H "Authorization: Bearer $TOKEN" | jq '
  { workspace_status, resolved_at, resolution_outcome, brier_score }
'

# Inspect the propagating artefact
curl -s "$API/api/workspaces/$ARG_WS_ID/outputs/resolution" -H "Authorization: Bearer $TOKEN" | jq

# Once the BayesOps hook is wired, see the fitted priors
curl -s "$API/api/workspaces/$ARG_WS_ID/outputs/params" -H "Authorization: Bearer $TOKEN" | jq '
  to_entries[] | select(.key | endswith("_fitted"))
'
```

---

## Resolution flows by domain

### WC team-prior workspace (Path B demo)

```bash
# After the 2026 World Cup final, resolve ARG's team-prior workspace
curl -X POST "$API/api/workspaces/$ARG_WS_ID/resolve" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "outcome": {
      "won_tournament": false,
      "final_round_reached": "quarterfinal",
      "knockout_wins": 1
    },
    "resolution_source": "fifa_official"
  }'
```

The handler:
- Sets workspace_status = completed
- Computes Brier against ARG's last published `predicted_probability` (a number in [0,1])
- Triggers BayesOps refit on ARG's team-prior workspace
- Triggers BayesOps refit on the Group B path workspace (which depended on ARG as upstream)
- The Group B refit cascades down to its own dependency graph if any

### H2H match workspace

```bash
# After ARG vs BRA in the semifinal
curl -X POST "$API/api/workspaces/$H2H_WS_ID/resolve" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "outcome": {
      "winner_team_id": "ARG",
      "home_team_id": "ARG",
      "away_team_id": "BRA",
      "home_goals": 2,
      "away_goals": 1,
      "match_time": "2026-07-12T18:00:00Z"
    },
    "resolution_source": "fifa_official"
  }'
```

No Brier score (winner_team_id isn't binary-extractable per the conventions). BayesOps still refits — it has the structured outcome and can fit team-strength differentials directly.

### Match outcome → bulk team workspace resolution

If we want match results to propagate as incremental observations on team-prior workspaces (rather than full resolutions until the tournament ends), the pattern is:

```bash
# Don't resolve the team-prior — append an observation
curl -X PUT "$API/api/workspaces/$ARG_WS_ID/outputs/observations" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{ "value": <append-merged observations array> }'

# BayesOps reads observations[] as it accrues, refits on each update
# Workspace status stays `active` until the tournament ends
```

Workspace `set_output_handler` already does the fan-out. BayesOps observes the same `upstream_output_updated` event regardless of whether the update is a resolution or an incremental observation. Choice is yours.

### Manual closure without resolution

```bash
# Tournament cancelled, question undefined
curl -X POST "$API/api/workspaces/$WS_ID/resolve" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "outcome": { "reason": "tournament_cancelled" },
    "resolution_notes": "Cancelled due to scheduling conflict",
    "failure": true
  }'
```

workspace_status → failed. No Brier. BayesOps hook still fires (it should treat `failure: true` workspaces as "no signal" rather than evidence — your call on the semantics).

---

## Idempotency / overwrite

The endpoint is **not idempotent** today — calling it twice returns 409. If we need amend-resolution semantics later (e.g. correcting a typo'd outcome), the right move is a separate `PATCH /api/workspaces/:id/resolution` endpoint that:
- Requires the workspace to be `completed` or `failed`
- Writes a new outcome and bumps the workspace_outputs version
- Re-runs the BayesOps refit hook

Designed but not implemented. Add when needed.

---

## Files

- `migrations/147_workspace_resolution.sql` — schema (adds resolved_at, resolution_outcome JSONB, brier_score, etc. to `teams`)
- `src/handlers/workspace/resolution.rs` — handler with the TODO refit-hook block at the bottom
- `src/api_server.rs` — route registration at `/api/workspaces/:id/resolve`, migration registration in boot list
- `docs/fermi/BAYESOPS_CONTRACT.md` — read/write contract for `params.<driver>_fitted` (the artifact BayesOps produces)
- `docs/fermi/WORKSPACE_RESOLUTION.md` — this document
