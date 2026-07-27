# Scenario Tree — cascade-aware Portfolio Risk view

Status: **design** · Owner: console · Companion to v0.8.10 (scenario builder Slice B).

## Vocabulary note (read this first)

The engine layer calls these things **cascade groups** — that's the
Rust type name (`cascade_group`, `CascadeGroupSummary`), the DB table
name (`forecast_relationship_groups`), and the SDK method names
(`list_cascade_groups`, `get_forecast_cascade_groups`). Those names
stay. They're correct for the implementation: a group is a rule that
propagates ("cascades") a resolution across siblings.

**In user-facing text we use "scenario".** A scenario is a shared
constraint over multiple forecasts — a mutex is a "one-winner
scenario", `at_most_n` is a "limit scenario", `implies` is a
"dependency scenario". Operators build scenarios; the engine
propagates cascades when a member resolves. Two names for two
audiences, same object.

Practical mapping table:

| User-facing (UI) | Internal (Rust / DB / SDK) |
| --- | --- |
| **Scenario** | `cascade_group` / `relationship_group` |
| Scenario builder | `render_cascade_group_strip` + picker + create form |
| Scenario constraint | `kind` field on the group |
| Joint scenario tree | `joint_tree` in `PortfolioRiskMetrics` |
| — | `cascade` verb / `cascade_undo` (post-resolve propagation — keep) |

This doc uses UI vocabulary throughout, cross-referencing internal
names where useful.

## What we're fixing

The Portfolio Risk view's **JOINT SCENARIOS (TOP 4)** widget picks the
four most-opinionated active forecasts in a portfolio (ranked by
`|p − 0.5|`) and enumerates all 2⁴ = 16 YES/NO combinations,
displaying the top 8 by joint probability. Today the joint
probability is computed under an **independence assumption**:

```
P(mask) = Π p_i         for YES bits
        · Π (1 − p_i)   for NO bits
```

When the top-4 belong to a scenario constraint — most obviously a
mutex like "EPL 2026 winner" — the independence model is wrong. It
produces rows like *(Man City YES, Arsenal YES, Chelsea NO, Man Utd
NO)* with a non-zero probability, when the mutex forbids two winners
in the same season. Currently:

1. **v0.8.10 (shipped)** — the widget carries an `assumes
   independence` label on the tile and a footer with the shown-mass
   fraction. The math is unchanged; the operator is warned.
2. **Slice 3 (this doc)** — the widget becomes scenario-aware: rows
   that violate any known scenario constraint are pruned to zero and
   the surviving mass is renormalized.

## The algorithm — three levels of correctness

### Level 0 — independence (current, v0.8.10)

```rust
for mask in 0..(1 << n) {
    let joint = probs.iter().enumerate()
        .map(|(i, p)| if (mask >> i) & 1 == 1 { *p } else { 1.0 - p })
        .product();
    push((mask, joint));
}
```

Correct when the top-4 are genuinely independent (e.g. a book of
uncorrelated event bets). Misleading when they're not, and the
widget itself now says so.

### Level 1 — naive filter (do **not** ship)

For each 16-mask, check every scenario constraint; if the mask
violates any, drop it and renormalize. **This is wrong** whenever a
scenario has members outside the top-4. A mutex group with 48 members
of which only 4 are in the top-4 has 44 "invisible" members. The mask
*(NO, NO, NO, NO)* is *not* a mutex violation — some team outside the
top-4 could win. But *(YES, YES, NO, NO)* is (two of the top-4 both
winning). Level 1 needs to know the difference.

### Level 2 — marginal constraints (what we ship)

For each scenario group `g` with `k = |members(g) ∩ top4|` members in
the tree:

- **`kind = mutex`** — the top-4 sub-mask must have at most 1 YES bit
  among the `k` members. Rationale: mutex says at most one winner
  overall; therefore at most one winner in any subset.
- **`kind = at_most_n, n = N`** — top-4 sub-mask must have at most
  `min(N, group_member_count)` YES bits among the `k` members. In
  practice this collapses to `≤ N` (portfolios rarely have all
  members of a group in the top-4).
- **`kind = implies, antecedent = A, consequent = C`** — if both
  `A ∈ top4` and `C ∈ top4`, the mask must not have `A=YES ∧ C=NO`.
  If only one is in the top-4 the constraint is not observable at the
  top-4 marginal — skip.

Algorithm:

```rust
let mut Z = 0.0;
for mask in 0..(1 << n) {
    let joint_indep = compute_joint_indep(mask, probs);
    let valid = scenarios.iter().all(|g| passes(mask, g, top4_ids));
    let joint = if valid { joint_indep } else { 0.0 };
    Z += joint;
    push((mask, joint));
}
if Z > 0.0 {
    for row in &mut tree { row.joint /= Z; }
}
```

Applied to the EPL screenshot's mutex (`Chelsea, Man Utd, Man City,
Arsenal` are 4 of ~20 EPL clubs, all in one mutex):

| Mask (top-4) | Indep P | Mutex ≤ 1 YES | Post-renormalize |
|---|---:|---|---:|
| (NO, NO, NO, NO) | 35.9% | pass | 41.7% |
| (NO, NO, NO, YES) | 20.6% | pass | 23.9% |
| (NO, NO, YES, NO) | 15.0% | pass | 17.4% |
| (NO, NO, YES, YES) | 8.6% | **violation** | 0.0% |
| (NO, YES, NO, NO) | 4.7% | pass | 5.5% |
| (YES, NO, NO, NO) | 3.8% | pass | 4.4% |
| (NO, YES, NO, YES) | 2.7% | **violation** | 0.0% |
| (YES, NO, NO, YES) | 2.2% | **violation** | 0.0% |
| … 8 more rows | ~7% | ~all violations | ~0.0% |
| **Σ** | 100% | **Z ≈ 86.1%** | **100%** |

Now the tree shows only physically consistent scenarios and the
mass genuinely sums to 100%. The "all NO" row (a top-4 outsider wins)
remains the mode, single-YES rows are boosted proportionally.

## Data plumbing

The Risk view is in `FermiConsole::render_portfolios_section` (not
the cockpit), which today has no scenario-membership data. Three
routes:

| | A. Client fan-out | B. Backend enrichment | **C. Cached listing (chosen)** |
|---|---|---|---|
| Backend change | none | JSONB agg on portfolios/:id/forecasts | +1 column on `list_groups_handler` |
| API calls | 4 per portfolio open | 0 new | 1 per session |
| Latency | worst 4× RTT | matches portfolio row | matches session cold-load |
| Staleness | per portfolio refresh | per portfolio row | session (until edit) |
| Effort | 2h | 3h | 1.5h |

**Route C rationale.** The scenario primitives are O(dozens) per user
— fits in one page. Adding `member_ids: uuid[]` to `list_groups_handler`
is a one-liner; the console caches it on session cold-load, and the
Risk view intersects `member_ids ∩ top4_ids` locally. Refetch on
scenario mutations (create / add-member / remove-member) — those
already round-trip.

### Backend change (Slice 3a)

`src/handlers/relationships/groups.rs::list_groups_handler` currently
returns `group_id, kind, parameters, description, member_count`. Add
`member_ids`:

```sql
SELECT frg.group_id, frg.kind, frg.parameters, frg.description,
       frg.created_at, frg.updated_at, frg.archived_at,
       (SELECT COUNT(*) …) AS member_count,
       (SELECT COALESCE(array_agg(ff.id::text), ARRAY[]::text[])
        FROM public.fermi_forecasts ff
        WHERE ff.relationship_groups @> ARRAY[frg.group_id]
          AND (ff.status IS NULL OR ff.status != 'archived')) AS member_ids
FROM public.forecast_relationship_groups frg
WHERE frg.owner_id = $1
ORDER BY frg.created_at DESC
```

Wire format gains one field, backwards-compatible.

### Client change (Slice 3b)

`FermiConsole`:

```rust
// Cached scenario primitives — the union of scenario constraints
// this operator owns, with member ids for local intersection.
// Populated lazily on first render of a portfolio Risk view; refetched
// after any scenario mutation.
scenarios_cache: Option<Vec<ScenarioSummary>>,
scenarios_cache_loading: bool,
```

Where `ScenarioSummary` extends the existing `CascadeGroupSummary`:

```rust
pub struct ScenarioSummary {
    pub group_id: String,
    pub kind: String,
    pub parameters: JsonValue,
    pub member_ids: Vec<String>,
}
```

`compute_portfolio_risk` signature grows one parameter:

```rust
fn compute_portfolio_risk(
    forecasts: &[PortfolioForecast],
    correlation_rho: f64,
    scenarios: &[ScenarioSummary],  // NEW
) -> PortfolioRiskMetrics
```

When `scenarios` is empty (feature-flagged or not yet loaded) the
computation falls through to Level 0 and the widget renders `assumes
independence` in the footer (current behaviour). When scenarios are
loaded and any constraint touches ≥2 members of the top-4, the
computation switches to Level 2 and the footer changes to
`scenario-aware · N/16 valid`.

## UX changes

Three visible adjustments in the widget itself:

1. **Tile subtitle** — swap `assumes independence · see tree ↓` for
   `scenario-aware · N/16 valid · see tree ↓` when Level 2 is active.
   Keep the independence variant when the top-4 have no scenario
   overlap (still correct + honest).
2. **Footer strip** — swap the gold `assumes independence` label for a
   cyan `scenario-aware` label when Level 2 kicks in. The shown-mass
   line already exists (`Σ shown = X% · N/M scenarios`).
3. **Correlation slider (ρ)** — leave as-is. Cascades ARE the
   correlation model for scenario-connected forecasts; adding ρ on
   top double-counts. Document in the design that ρ is for
   *residual* correlation between forecasts not covered by any
   scenario. The `P(any YES)` KPI keeps applying ρ as it does today.

## Implementation slices

Each ships independently, each is valuable on its own.

- **Slice 1 (v0.8.10) — Label the assumption. ✓ SHIPPED.**
  Tile subtitle gains `assumes independence`, tree gets a footer
  with `Σ shown = X% · N/M scenarios · assumes independence`. Zero
  data changes, immediately clarifies the widget for operators
  reading a scenario-linked portfolio.
- **Slice 2 — UI vocabulary sweep (cockpit).**
  Rename user-visible strings in `render_cascade_group_strip`,
  `render_cascade_picker`, `render_cascade_create_form`,
  `render_cascade_detail_panel`, and their kin from "cascade" →
  "scenario". Internal fn names + comments unchanged. ~15 string
  edits, no logic changes. Enables consistent vocabulary before
  Slice 3.
- **Slice 3a — Backend `member_ids` on list_groups_handler.**
  One SQL change + one JSON field. Independent PR; the composer's
  picker ignores the new field until it wants it.
- **Slice 3b — Console cache + Level 2 compute.**
  `scenarios_cache` on FermiConsole, `compute_portfolio_risk`
  gains the parameter, `Level 2` filter + renormalization implemented,
  tile/footer swap to `scenario-aware` when the constraint set is
  non-empty. Ship as v0.8.11 or v0.8.12.
- **Slice 4 (future) — Scenario-first alternate rendering.**
  When *all* top-4 belong to the same mutex scenario, the joint
  tree collapses to a boring shape (one non-zero row per team, one
  "all NO" row). Replace it with a **winner rollup** — a single
  ranked bar of every member of the scenario (all 20 EPL clubs,
  not just 4), sorted by probability, with a "field / others" row
  for members outside the top-N. This is the mental model
  operators actually want for winner-take-all books.
- **Slice 5 (future) — Scenario overlay on individual forecast
  rows.** Small badge on portfolio rows indicating scenario
  membership + the constraint kind, so operators can see at a
  glance that "these four rows are mutex-linked; if I resolve one
  the others move".

## Non-goals

- No changes to the server's `relationship_group` contract, table
  names, or engine mechanics.
- No renaming of the Provenance tab (the "how did this forecast's
  probability get here?" waterfall) — it uses "cascade" as the
  verb for after-the-fact propagation events, and that reading is
  correct. A separate design if we want to reframe that surface as
  "Impact history" or similar.
- No changes to how the ρ correlation slider works today.
- No support for arbitrary custom scenario kinds beyond
  `mutex / at_most_n / implies`. New kinds slot in via the
  existing `passes(mask, g, top4_ids)` per-kind branch when
  they're added engine-side.

## Open questions

1. **When the top-4 spans multiple scenario groups, is Level 2's
   AND-of-constraints the right semantic?** I think yes — a mask is
   valid iff it violates none of the applicable constraints. But if
   two scenarios contradict each other in the constraint set (e.g. a
   mutex says at most 1 YES, an implies says A→B forcing 2 YES),
   we'd get Z = 0 and hide the whole tree. Rare in practice, but
   worth an error path: "your scenarios are inconsistent — see all-mutex
   check" or similar. Punt to Slice 3b review.
2. **Should the Risk view surface a scenarios chip strip** ("this
   portfolio touches: `wc_2026_winner`, `q1_earnings_beat`, …") next
   to the tree, so operators can see which constraints are being
   applied? Cheap addition once `scenarios_cache` exists. Probably
   yes — one line under the tree title.
3. **How do we handle scenarios the operator can't see?** A forecast
   in the top-4 could belong to a scenario owned by a teammate that
   the operator has view access on but not the primitive. Depends on
   ACL — the list_groups endpoint is scoped to `owner_id`, so shared
   scenarios wouldn't appear in `scenarios_cache`. May need
   `GET /api/me/scenarios` (union of owned + shared) later.
