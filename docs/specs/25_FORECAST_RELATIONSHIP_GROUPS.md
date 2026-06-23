# Spec 25 — Forecast Relationship Groups

**Status:** Draft v0.1 · 2026-06-22
**Supersedes:** the `forecast_relationships` table from Spec 23 (migration 150) — see §10 Migration Path.
**Depends on:** `fermi_forecasts` (Spec 094), `forecast_spacetime` (Spec 23 / mig 140), `pending_cascades` (mig 151)
**Owners:** ilabra + Kilo

---

## 1. Why

Forecasts in the real world are rarely independent. The 48 WC team-prior forecasts sum to ≤ 1 by construction. "Will Argentina win" implies "Will Argentina reach the semifinal." "Both AAPL beats Q1 earnings AND launches AR glasses by Sep" is a conjunction of two underlying forecasts. Today the system treats each forecast as standalone — there's no first-class way to **declare** these relationships, no way to **propagate** consequences across siblings when one resolves or moves, and no way for the operator to **review and apply / undo** the rebalance.

We need a generalized primitive — owned by ABW, decoupled from any single domain — that lets the operator:

1. **Declare** that some set of forecasts share a structural relationship (mutex, at-most-N, implies, etc).
2. **Watch the system queue** rebalance proposals when any member resolves or moves.
3. **Review** the proposed deltas (which siblings, by how much) before they fire.
4. **Apply or dismiss** explicitly. No automatic parameter mutation.
5. **Undo** an applied rebalance.
6. **Re-queue** a fresh rebalance when sibling probabilities have shifted since a prior apply.
7. **See** the rebalance state at portfolio AND individual-forecast levels.

This spec replaces the per-relationship ID-list model of mig 150 with a **group tag** model.

---

## 2. Core abstraction: the group tag

A **relationship group** is a named structural constraint over a dynamic set of forecasts. The group is identified by an arbitrary string (`wc_2026_winner`, `tech_giants_q1_earnings_beat`, etc.). Membership is **dynamic**: forecasts carry a list of group tags they belong to, and the group's propagation logic queries the membership at trigger time. This is Path B from the design discussion.

Two reasons we picked this over explicit ID lists:

- **Lifecycle resilience.** Adding a new WC team workspace = spawn a forecast with `relationship_groups: ["wc_2026_winner"]`. No separate registration step, no risk of forgetting to enroll. Archiving a forecast = it drops out of the group automatically.
- **Composability.** A forecast can be in many groups simultaneously without explicit cross-references. Argentina belongs to `wc_2026_winner` (mutex over 48), `wc_2026_semifinalist` (at_most_n with n=4 over 48), and `conmebol_winner` (mutex over 6). All three groups fire on relevant events.

Groups are **orthogonal to portfolios.** A portfolio is an operator's lens; a group is a math constraint. A forecast can be in `portfolio = "WC sims"` AND `groups = ["wc_2026_winner", "conmebol_winner"]` simultaneously. Portfolio membership doesn't imply any constraint; group membership does.

---

## 3. The three v1 kinds

### 3.1 `mutex` — Exactly one is true

Constraint: `Σ P(F_i) = 1` over all non-resolved members. When one member resolves (or moves), the others rebalance proportional to current p.

Trigger: any member resolves, or any member's probability changes.

Propagation (existing logic from mig 150, generalized):

- **trigger resolves NO** (outcome=false): trigger's previous probability redistributes across surviving members, proportional to each survivor's current probability.
- **trigger resolves YES** (outcome=true): all other members drop to ~0 (clamped at 0.001 for display).
- **trigger probability changes (no resolution)**: redistribute the delta across siblings proportional to current p so the total stays at 1.0.

Math: if survivors are {F_1, ..., F_k} with current probs {p_1, ..., p_k}, and trigger had probability p_T before resolving NO, then each survivor's new probability is `p_i + p_T · (p_i / Σ p_j)`. Mass-conserving, ranking-preserving.

Use case: WC winner. Election. "Who buys Twitter next."

### 3.2 `at_most_n` — Up to N can be true

Constraint: `Σ P(F_i) ≤ N` over all non-resolved members. Stronger than mutex (mutex is at_most_1) but weaker than exhaustive. Parameter: `{"n": N}`.

Trigger: same as mutex.

Propagation:

- **trigger resolves NO**: redistribute its probability proportionally across survivors, BUT clamp so the post-cascade sum doesn't exceed N. If the sum was already at N and a member drops, the survivors' sum effectively absorbs no mass (because they were already at capacity). If the sum was below N, redistribute fully.
- **trigger resolves YES**: subtract 1 from the "capacity" implicitly — survivors' sum is now capped at N-1. If their current sum > N-1, scale them down proportionally so the constraint holds.
- **trigger probability changes**: same scale-to-fit-constraint logic.

Math: solve for the redistribution that keeps each survivor's relative rank while satisfying `Σ ≤ N` post-cascade.

Use case: "Reaches semifinal" across 48 teams (at_most_n=4). "Top 10 stocks in S&P by year-end." "Tournament round-of-16 winners" (at_most_n=8).

### 3.3 `implies` — F1 ⇒ F2 (asymmetric pair)

Constraint: `P(F2) ≥ P(F1)` always. If F1 rises above F2, F2 must rise to match. If F2 falls below F1, F1 must fall to match. Parameter: `{"antecedent": F1_id, "consequent": F2_id}` — the only kind that requires explicit per-member roles (mutex and at_most_n treat all members symmetrically).

Trigger: F1 moves or resolves; F2 moves or resolves.

Propagation:

- **F1 resolves YES**: F2 jumps to ~1 (clamped at 0.999) since the antecedent forces the consequent.
- **F1 resolves NO**: F2 unconstrained from this relationship (other relationships F2 is in may still constrain it).
- **F2 resolves YES**: F1 unchanged (consequent being true doesn't force the antecedent).
- **F2 resolves NO**: F1 drops to ~0 since `F1 ⇒ ¬F2` is the contrapositive.
- **F1 probability changes up to value V > P(F2)**: F2 jumps to V.
- **F2 probability changes down to value V < P(F1)**: F1 drops to V.

Note: `implies` is a **2-member** group, not N-member. The group's parameters carry the antecedent/consequent role labels.

Use case: "Argentina wins WC" ⇒ "Argentina reaches WC final." "Q1 revenue > $10B" ⇒ "Q1 revenue > $5B." Tournament progression (Round of 16 win ⇒ Quarter-final appearance).

---

## 4. Data model

### 4.1 `fermi_forecasts.relationship_groups` (new column)

```sql
ALTER TABLE public.fermi_forecasts
    ADD COLUMN IF NOT EXISTS relationship_groups TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_forecasts_relationship_groups
    ON public.fermi_forecasts USING gin (relationship_groups);
```

A forecast lists every group it belongs to. Order doesn't matter. Empty array = no constraints from this system.

### 4.2 `forecast_relationship_groups` (new table)

```sql
CREATE TABLE public.forecast_relationship_groups (
    group_id            TEXT        PRIMARY KEY,
    kind                TEXT        NOT NULL,   -- 'mutex' | 'at_most_n' | 'implies'
    parameters          JSONB       NOT NULL DEFAULT '{}'::jsonb,
    description         TEXT,
    owner_id            TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at         TIMESTAMPTZ,
    CHECK (kind IN ('mutex', 'at_most_n', 'implies'))
);

CREATE INDEX idx_relationship_groups_owner
    ON public.forecast_relationship_groups(owner_id) WHERE archived_at IS NULL;
```

A group declares the *semantics* of constraint that applies to its members. Members are not listed here — they're discovered by querying `fermi_forecasts` where `relationship_groups @> ARRAY[group_id]`.

### 4.3 `pending_cascades` (existing, schema unchanged but `relationship_id` semantically refers to a group)

Already exists from mig 151. The `relationship_id` column conceptually becomes `group_id` after migration. See §10 Migration Path.

### 4.4 New: `pending_cascades.applied_deltas`

For undo support, capture what was actually written at Apply time:

```sql
ALTER TABLE public.pending_cascades
    ADD COLUMN IF NOT EXISTS applied_deltas JSONB;
```

Shape on Apply: `[{forecast_id, prev_pp, new_pp, delta_pp}, ...]` — the exact deltas the propagation wrote, recorded for the undo path. Authoritative (not a projection).

### 4.5 New: `pending_cascades.status='undone'`

Extend the CHECK constraint to allow `undone` and `superseded` lifecycle states (mig 151 declared them; we make sure the lifecycle is wired).

### 4.6 `pending_cascades.superseded_by`

```sql
ALTER TABLE public.pending_cascades
    ADD COLUMN IF NOT EXISTS superseded_by UUID
    REFERENCES public.pending_cascades(id);
```

When a cascade is re-queued, the prior entries are marked `superseded` and their `superseded_by` points at the new entry. Audit trail: "this cascade replaced 2 earlier attempts."

---

## 5. Lifecycle

```
                  ┌──── PENDING ────┐
                  │      │          │
              apply      │       dismiss
                  │      │          │
                  ▼      │          ▼
              APPLIED    │      DISMISSED
                  │      │          │
                  │      │          │
                undo  re-queue   re-queue
                  │      │          │
                  ▼      ▼          ▼
               UNDONE  PENDING'   PENDING'
                          (new, supersedes old)
```

Statuses:

| Status | Meaning | Allowed transitions |
|---|---|---|
| `pending` | queued, awaiting operator review | → `applied`, `dismissed`, `superseded` |
| `applied` | propagation fired, deltas written, in `applied_deltas` | → `undone`, `superseded` |
| `dismissed` | operator declined, no writes | → `superseded` |
| `undone` | applied cascade has been reversed | terminal |
| `superseded` | replaced by a newer pending_cascade for the same (group, trigger) | terminal |

`undone` and `superseded` are terminal — once a cascade is undone or superseded, the row is preserved for audit but can't be re-applied. To re-fire, the operator must explicitly **Re-queue**, which creates a fresh `pending` row.

---

## 6. Endpoints

### 6.1 Group CRUD

```
POST   /api/relationship-groups
       body: { group_id, kind, parameters?, description? }
       returns: created group

GET    /api/relationship-groups
       returns: { groups: [...], count }

GET    /api/relationship-groups/:group_id
       returns: { group, member_count, members: [...] }
       (members joined from fermi_forecasts where this group_id is in
        relationship_groups[])

GET    /api/relationship-groups/:group_id/members
       returns: { members: [forecast_summary, ...] }

PATCH  /api/relationship-groups/:group_id
       body: partial { kind?, parameters?, description? }

DELETE /api/relationship-groups/:group_id   (soft — archived_at)
```

### 6.2 Forecast group membership

```
PUT    /api/forecasts/:forecast_id/groups
       body: { groups: ["wc_2026_winner", "conmebol_winner"] }
       (replaces the forecast's relationship_groups array)

POST   /api/forecasts/:forecast_id/groups/:group_id
       (adds one group)

DELETE /api/forecasts/:forecast_id/groups/:group_id
       (removes one group)
```

### 6.3 Pending cascades (already exist; extend)

```
GET    /api/pending-cascades?status=pending|applied|dismissed|undone|all
       Default: pending. ?status=all returns the full lifecycle for the
       operator's history view.

POST   /api/pending-cascades/:id/apply
       (existing — captures applied_deltas in the row before returning)

POST   /api/pending-cascades/:id/dismiss
       (existing)

POST   /api/pending-cascades/:id/undo                       NEW
       (only valid on status='applied'. Reads applied_deltas, writes
        reverse update_probability rows with revision_trigger='cascade_undo',
        sets status='undone'.)

POST   /api/pending-cascades/requeue                         NEW
       body: { group_id, trigger_forecast_id, supersede_ids?: [...] }
       If supersede_ids omitted, server defaults to "all non-terminal
       prior cascades for this (group, trigger)" — marks them superseded
       and queues a fresh pending row with current sibling probabilities.
```

### 6.4 Per-forecast cascade history

```
GET    /api/forecasts/:forecast_id/cascade-history
       returns: { incoming: [...], outgoing: [...] }
       incoming: cascades that wrote a delta to THIS forecast
       outgoing: cascades this forecast triggered (only populated if the
                 forecast is part of a relationship and has resolved/moved)
       Each entry includes the cascade ID, status, applied delta (if any),
       triggered_at, applied_at.
```

This is the per-forecast trajectory data feed. The cockpit's per-forecast cascade bar reads from here.

---

## 7. Operator UI surfaces

### 7.1 Sidebar badge (already exists)

Shows the global count of `status='pending'` for the operator. Unchanged.

### 7.2 Pending cascades sheet (already exists; extend with history view)

Add a tab/filter to switch between:
- **Pending** — actionable (Apply / Dismiss)
- **Applied** — recent applies; each row has [Undo] [Re-queue]
- **All** — full history including dismissed + undone + superseded; read-only audit

### 7.3 Portfolio panel — new cascade section

In the portfolio detail panel, a new section above or below the forecast list:

```
┌───────────────────────────────────────────────────────────┐
│  Cascades affecting this portfolio                         │
│                                                             │
│  ⚠ 2 pending review   ✓ 5 applied   ↺ 1 undone   • 0 dismissed │
│  [Review all]                                              │
└───────────────────────────────────────────────────────────┘
```

Aggregates **all cascades where any affected forecast is in this portfolio** (not just where the trigger is). Click [Review all] → opens the pending-cascades sheet pre-filtered to this portfolio's forecasts.

### 7.4 Per-forecast cascade bar (cockpit)

When a forecast is opened in the cockpit, surface a bar at the top showing cascade involvement:

```
This forecast has received 3 cascade deltas (last applied 2h ago, +0.65pp)
Open cascade history →
```

Click → shows the per-forecast history from §6.4. From there, the operator can:
- See each cascade that hit this forecast with prev/new/delta
- Undo a single cascade (reverts all deltas it wrote across all affected forecasts — `applied_deltas` is authoritative)
- Re-queue a cascade if this forecast was the trigger

If this forecast IS the trigger of a relationship and has resolved, also surface:
```
This resolution has 1 cascade applied (1h ago, 47 forecasts shifted)
[Re-queue cascade with current values]
```

### 7.5 Explicit affordance to declare groups

A console UI section — could live in the Portfolios panel or a new "Relationships" panel — for declaring + managing groups:

```
┌───────────────────────────────────────────────────────────┐
│  Relationship groups                              [+ New]   │
│                                                             │
│  wc_2026_winner          mutex            48 members        │
│  wc_2026_semifinalist    at_most_n n=4    48 members        │
│  arg_wins_implies_final  implies          2 members         │
│  ...                                                        │
└───────────────────────────────────────────────────────────┘
```

Click a group → see members, edit kind/parameters/description, archive.

[+ New] → modal to declare:
- Group ID (free-form string, must be unique per operator)
- Kind (mutex / at_most_n / implies)
- Kind-specific parameters (n for at_most_n, antecedent/consequent picker for implies)
- Description
- Member picker (search + add forecasts to this group)

After creation, the system queues no cascades retroactively — it just starts watching from this point forward.

---

## 8. Propagation logic — module structure

```
src/handlers/relationships/
  mod.rs                  — module barrel
  groups.rs               — CRUD on forecast_relationship_groups
  membership.rs           — query: "for forecast F, what groups is it in?" / "for group G, who are members?"
  propagation.rs          — dispatch by kind: mutex / at_most_n / implies
  apply.rs                — execute propagation, write update_probability rows, capture applied_deltas
  undo.rs                 — reverse a prior apply using applied_deltas
  requeue.rs              — supersede prior cascades, queue fresh
```

Each kind's propagation logic lives in `propagation.rs` as `propagate_mutex` / `propagate_at_most_n` / `propagate_implies`. They are pure functions of `(group, members, current_probs, trigger, trigger_kind, outcome) → Vec<(forecast_id, prev, new)>`. They don't write — `apply.rs` does that. This separation lets us dry-run for proposed_snapshot, then apply, with the same code path.

---

## 9. Operator-gate invariants

Across all paths, these invariants hold:

1. **No probability mutation happens without an operator click.** Resolution queues; doesn't apply.
2. **Apply is recomputed at apply time**, not from a stale snapshot. The proposed_snapshot at queue time is for display; the actual writes use fresh sibling probabilities at click time.
3. **Undo uses authoritative deltas.** Each apply captures `applied_deltas` (what was actually written, not what was projected). Undo reads that and reverses each delta atomically.
4. **Re-queue supersedes, doesn't shadow.** Prior cascades for the same (group, trigger) are explicitly marked `superseded` with a pointer to the new row. Audit trail is preserved; queue stays clean.
5. **All cascade events are first-class in trajectory data.** Every `update_probability` row written by apply/undo carries `revision_trigger='cascade'` or `'cascade_undo'` and a `reason` that references the cascade ID. The trajectory tab surfaces both.

---

## 10. Migration path

> **Note 2026-06-23**: the migration-number collision (two `151_*.sql`
> files) has been resolved — `pending_cascades` is now mig 153,
> `forecast_invites` stays at 151. Spec 25 migrations therefore start
> at 154/155.

The existing mig 150 (`forecast_relationships` with `forecast_ids TEXT[]`) is replaced by:

- **mig 154** — `relationship_groups` column on `fermi_forecasts` + `forecast_relationship_groups` table.
- **mig 155** — `pending_cascades` schema extensions: `applied_deltas`, `superseded_by`, ensure all 5 statuses are CHECK-valid.

The existing `forecast_relationships` table can be archived (rename + drop later) once the new groups table is populated. The existing WC mutex relationship (`90e1eea8-fdcb-...`) becomes a single `forecast_relationship_groups` row with `group_id='wc_2026_winner'`, and each of the 48 forecasts gets `'wc_2026_winner'` added to its `relationship_groups` array. Migration script in `scripts/world_cup/migrate_mutex_to_group.py`.

`pending_cascades.relationship_id` semantically becomes the group_id at the application layer; the column type (UUID) is wrong for that and needs to migrate to TEXT. Mig 155 adds a new `group_id TEXT` column, backfills from the relationship_id → forecast_relationships.id → forecast_ids lookup (or just nulls for legacy rows since we don't have any with pre-existing data anyway), drops `relationship_id`.

---

## 11. What's deferred (explicit non-goals)

- **More than 3 kinds** (`conjunction`, `conditional`, `exhaustive_cover`, `at_least_n`): can be added later as new kind values + a new propagation function. No schema change required.
- **Cross-group inference**. If group G1 says `A → B` and group G2 says `B → C`, the system doesn't auto-derive `A → C`. Each group is independent.
- **Automatic stale-cascade detection**. If sibling probabilities shift after an Apply, the system doesn't warn that the cascade is now stale. Operator must Re-queue explicitly when they want to refire.
- **Soft / probabilistic relationships** ("usually correlated"). Only hard constraints. Soft is its own future kind.
- **Multi-trigger cascades**. Each cascade has exactly one trigger forecast. If two siblings resolve in the same minute, two cascades queue, operator handles each.
- **Auto-apply for high-confidence cascades**. Stays operator-gated. Auto-apply is a single config toggle away once we trust the math at scale.

---

## 12. Implementation passes

This spec is too big for one commit. Order:

| Pass | What | Effort |
|---|---|---|
| 1 | Migration 154 — relationship_groups column + groups table | 30 min |
| 2 | Server: group CRUD + membership endpoints | 1 hour |
| 3 | Server: refactor existing `propagate_mutex` into the new dispatch | 1 hour |
| 4 | Implement `propagate_at_most_n` + `propagate_implies` | 2 hours |
| 5 | Migration 155 — pending_cascades extensions for undo/supersede | 30 min |
| 6 | Server: undo + requeue endpoints | 1 hour |
| 7 | WC migration script: register existing mutex as group, tag the 48 forecasts | 30 min |
| 8 | Console: pending-cascades sheet with Pending/Applied/All tabs + Undo + Re-queue | 2 hours |
| 9 | Console: per-forecast cascade bar in cockpit | 1.5 hours |
| 10 | Console: portfolio panel cascade summary section | 1 hour |
| 11 | Console: relationship groups explorer (CRUD UI) | 3 hours |
| 12 | Tests: per-kind propagation contracts, mass conservation, undo correctness | 2 hours |

Total ~16 hours. We do this across multiple sessions, smallest passes first so we can verify each on the live demo.

---

## 13. Open questions still on the table

1. **Group ID uniqueness scope** — per-operator or global? I lean per-operator (each operator can have their own `wc_2026_winner`). Confirm.

2. **What happens to an applied cascade if the underlying group is archived?** I lean: applied stays applied; the audit row keeps a snapshot of the group's state at apply time. The archived group can't queue new cascades. Confirm.

3. **Membership query timing for at_most_n cascade** — when at_most_n fires, do we treat resolved-YES members as still counting toward N, or exclude them? I lean exclude (resolved forecasts have factual outcomes; the constraint becomes `at_most (N - resolved_yes_count)` over the survivors). Confirm.

4. **For `implies`, does the consequent moving up also pull the antecedent up?** No — only the antecedent moving up pulls the consequent. The consequent rising doesn't force the antecedent (its probability is unconstrained from above by this relationship). Confirm.

5. **Order of cascade application when multiple groups involve the same trigger** — e.g. Argentina resolves NO, fires both `wc_2026_winner` mutex (47 survivors get bumped) and `conmebol_winner` mutex (5 survivors get bumped). Today's design queues 2 separate cascade rows. Should they apply independently (current design) or as a single bundled review? I lean independent for clarity. Confirm.
