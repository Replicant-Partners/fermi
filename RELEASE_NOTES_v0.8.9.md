# Fermi Console v0.8.9 — Cascade composition (Slice A)

Ships the **authoring surface** for cascade dependencies — the missing
half of v0.8.8's Provenance tab. Where v0.8.8 let you *see* which
upstream forecasts moved a probability, v0.8.9 lets you *wire* a
forecast into a cascade group in the first place.

The backend for composition (Spec 25 `relationship_groups` API) has
existed since June — CRUD endpoints, group membership add/remove,
server-side kind validation, all live. The console just had no UI for
it. This release wires the picker.

## What's new

### Cascade chips on the question header

Every composer now has a "CASCADES:" chip strip alongside the
Portfolios strip:

```
CASCADES:  ⊗ wc_2026_winner  ×    ≤n playoff_seed  ×    + add
```

- Each chip = one cascade group membership, with a **kind glyph**:
  `⊗` mutex, `≤n` at_most_n, `⇒` implies (matching the vocabulary
  used on the Provenance tab and in the WC roadmap doc).
- Click `×` on a chip → remove this forecast from that group.
  Optimistic update; error surfaces inline and we re-fetch truth.
- Click `+ add` → open the inline picker (see below).
- Draft forecasts (no `forecast_id` yet) collapse to a hint chip:
  *"Publish this forecast first, then add it to a cascade group."*

The strip is pre-warmed on cockpit open (`open_workspace_forecast` /
`open_forecast`), so the operator sees their memberships the moment
the composer lands — same lifecycle as the Trajectory / Provenance
prewarm.

### Inline cascade picker

Clicking `+ add` opens an inline panel below the chip strip (same
visual pattern as the PM typeahead strip — no modal). Two modes:

**Browse mode (default).** Lists the operator's existing cascade
groups with kind glyph, description, and member count. Groups this
forecast is already in are filtered out. Click a row → add + close.

**Create mode.** Reached via `+ New cascade group`. Inline form with:
- Auto-suggested `group_id` derived from the forecast question via
  `cascade_suggest_group_id` (slugify + `cascade_` prefix; e.g.
  *"Will Spain win the 2026 FIFA World Cup?"* →
  `cascade_spain_win_the_2026_fifa_world_cup`). Read-only display for
  Slice A; Slice B will add an EditorEntity so the operator can
  override.
- Three kind chips: **Mutually exclusive (⊗)** / **At most N (≤n)** /
  **Implies (⇒)**, click-to-select with cyan border on the current
  choice.
- Kind-specific hint below the chips explaining the rule
  ("Only one member can resolve YES. When one resolves NO, its
  probability mass redistributes across the survivors weighted by
  prior. (This is the WC 2026 winner group's rule.)").
- One-click **Create + add current forecast** — creates the group via
  `POST /api/relationship-groups`, then adds the current forecast via
  `POST /api/forecasts/:id/groups/:gid` in the same operator action.
  The two-step orchestration is done client-side so the picker
  doesn't need a separate "now add me" flow.

### API client methods (new)

Six methods on `ApiClient` mirroring the Spec 25 routes:

- `list_cascade_groups()` → `GET /api/relationship-groups`
- `get_cascade_group(id)` → `GET /api/relationship-groups/:id`
- `create_cascade_group(id, kind, params, desc)`
  → `POST /api/relationship-groups`
- `get_forecast_cascade_groups(fid)`
  → `GET /api/forecasts/:id/groups`
- `add_forecast_to_cascade_group(fid, gid)`
  → `POST /api/forecasts/:id/groups/:gid`
- `remove_forecast_from_cascade_group(fid, gid)`
  → `DELETE /api/forecasts/:id/groups/:gid`

Zero server-side changes. The endpoints have been shipped since
migration 155 (June); we're just now consuming them from the client.

### The three-surface composition mental model

```
Level 1: Per-forecast chip strip (this release)   ← primary composer
Level 2: Group detail modal (Slice B, next up)   ← group-level composer
Level 3: All Groups page (Slice B or later)      ← portfolio lens
```

Slice A wires Level 1 end-to-end, with a working create-new flow
reachable from the picker. That's the minimum viable authoring
workflow — an operator can compose a cascade end-to-end without
leaving the composer or touching the CLI.

## Vocabulary note

In the UI everything is **cascade group / cascade kind / cascade
member**, matching the Provenance tab. Under the hood we keep calling
it `relationship_group` — Spec 25 is the stable server contract and
renaming migrations 155/156 is not on the critical path. `groups.rs` /
`membership.rs` server handlers stay unchanged.

## Migration

None. Zero schema changes. All new state fields on `CockpitState` are
initialised in `CockpitState::new`. All new API methods hit endpoints
that have been live since June.

## Known follow-ups (Slice B and beyond)

Explicitly deferred and documented in code where relevant:

- **Editable `group_id` and `description`.** Today auto-generated
  from the question; the operator can't override without a
  separate PATCH. Slice B wires an `EditorEntity` into the create
  form so both fields become text inputs.
- **`at_most_n.n` input.** Hard-coded to 1 in Slice A. Slice B
  adds a numeric input.
- **`implies` antecedent/consequent forecast pickers.** Slice A
  ships with empty strings and disables the Create button when
  the kind is `implies` without both filled in. Slice B adds two
  forecast picker dropdowns.
- **Group detail modal.** Click on a chip currently just removes
  (via the `×`). Slice B opens a modal with member list, current
  probability per member, invariant health strip (`Σp = 1.003 ✓`),
  and a dry-run preview ("if I resolve this member NO, here's how
  the others shift"). The dry-run engine already exists server-side
  via `dispatch_propagation(..., dry_run=true)`; wiring the modal is
  pure UI.
- **All-Groups Dashboard section.** Level 3 in the three-surface
  mental model above. Not strictly necessary since the picker
  already lets you browse and create; add when operators start
  managing >10 groups.
- **Search box on the picker.** Deliberately omitted from Slice A
  because it needs an `EditorEntity` — the picker instead shows all
  groups (filtered to exclude ones the forecast is already in).
  Fine at O(10) groups per operator; add if operators start owning
  more.

## Files touched

- `crates/fermi-console/src/api/client.rs` — six new
  `*_cascade_group*` methods on `ApiClient`.
- `crates/fermi-console/src/cockpit.rs` — `CascadeGroupSummary` +
  `CascadeCreateDraft` types; state fields
  (`forecast_cascade_groups`, `available_cascade_groups`,
  `show_cascade_picker`, `cascade_picker_query`,
  `cascade_create_draft`, `cascade_groups_loading`,
  `cascade_groups_error`); loader + mutator methods
  (`load_forecast_cascade_groups`, `load_available_cascade_groups`,
  `open_cascade_picker`, `close_cascade_picker`,
  `begin_cascade_create`, `cancel_cascade_create`,
  `add_forecast_to_cascade_group`,
  `remove_forecast_from_cascade_group`,
  `create_and_add_cascade_group`); render functions
  (`render_cascade_group_strip`, `render_cascade_picker`,
  `render_cascade_browse_list`, `render_cascade_create_form`); and
  helpers (`cascade_kind_glyph`, `cascade_kind_label`,
  `cascade_suggest_group_id`). Wired into `render_question_section`
  as two new children (chip strip + picker).
- `crates/fermi-console/src/main.rs` — pre-warm calls in
  `open_workspace_forecast` and `open_forecast`.
- `crates/fermi-console/Cargo.toml` — version bump.
