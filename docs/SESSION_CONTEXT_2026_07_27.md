# Session Context — 2026-07-27

Handoff document for the next session picking up the Fermi Console UX
work. Read this first before making changes.

## TL;DR

Shipped **v0.8.4** of `fermi-console` (tag `v0.8.4` pushed to
`origin/main`). Four commits, all in `crates/fermi-console/`. Focus was
usability: discoverability of hotkeys, visibility of orphan forecasts,
signal density of Recent Activity, and pre-publish portfolio
association. There is **one clear follow-up** carved out in the release
notes' "Known Issues" — unify the Composer's Polymarket type-ahead
strip with the new Dashboard PM search card.

## Where we started

At session start, `HEAD = 8603e26` (`v0.8.3`), plus **uncommitted WIP**
in `crates/fermi-console/src/main.rs` covering three things:

1. Draft forecasts surfacing in Recent Activity (✎ gold icon)
2. Explicit Published (◐) vs Revised (→) distinction on active
   forecasts, based on `created_at != updated_at`
3. Relative timestamps (`3m ago`) replacing bare ISO dates
4. Sidebar version chip made clickable with three states
   (update available / checking / up to date)
5. New `render_dashboard_hero()` — three hero buttons (＋ New forecast /
   🔮 From Polymarket / 📎 Paste PM URL)

The user asked me to push that WIP first ("some code and work that got
interrupted"), then build a keyboard-shortcuts help affordance because
the console was unusable without knowing the hotkeys.

## What shipped

Four commits, one tag:

| SHA | Description |
|-----|-------------|
| `d2ce5ba` | The rescued WIP — dashboard hero + drafts in Recent Activity + clickable version chip |
| `8685e8b` | Shortcuts help modal (Ctrl+/) |
| `2a1953c` | Dashboard drafts/resolved sections + curated Recent Activity + in-place PM search card + pre-publish portfolio picker |
| `f5aadde` | Release v0.8.4 (version bump + `RELEASE_NOTES_v0.8.4.md`) |
| tag `v0.8.4` | Points at `f5aadde` |

Between them there's an unrelated auto-commit from the running console
itself (`5467e0b` — a forecast v1 for "Will Argentina win the 2026 FIFA
World Cup?"). Leave it alone; that's the console's own commit trail.

### 1. Shortcuts help modal (Ctrl+/)

**Files:** `crates/fermi-console/src/main.rs` only.

- New actions `ShowShortcuts` / `DismissShortcuts` in the `actions!`
  block near line 182.
- New field `shortcuts_modal_showing: bool` on `FermiConsole`.
- New handlers `on_show_shortcuts` / `on_dismiss_shortcuts` next to the
  update-modal handlers.
- New `render_shortcuts_modal(&self, cx)` method — placed right after
  `render_update_modal`. It renders four categories (Forecast workflow /
  Navigation / Window / Help) with a fixed-width key-pill column.
- Wired keybindings in `main()`:
  ```
  KeyBinding::new("secondary-/",       ShowShortcuts,    Some("FermiConsole")),
  KeyBinding::new("secondary-shift-/", ShowShortcuts,    Some("FermiConsole")),
  KeyBinding::new("escape",            DismissShortcuts, Some("FermiConsole")),
  ```
- Three redundant entry points so it's impossible to miss:
  - `Ctrl+/` keybinding (and `Ctrl+?` for US layouts)
  - Sidebar chip: **⌨ Shortcuts · Ctrl+/**
  - Help menu: **Keyboard Shortcuts    Ctrl+/**
- **Single source of truth**: the shortcut list in
  `render_shortcuts_modal` is the canonical documentation. A comment
  above the section table tells future contributors to update it when
  new bindings are added.

### 2. Curated Recent Activity feed

**Files:** `crates/fermi-console/src/main.rs` — `recompute_recent_activity`
and two new helpers `activity_family_key`, `describe_activity_family`
near `truncate`.

Problem: testers with a bulk-imported WC portfolio saw 8 identical rows
("Resolved No: Will X win the 2026 FIFA World Cup") — pure noise.

Two-pass curation now runs after the sort:

**Pass 1 — run-length collapse.** Consecutive rows sharing a family
key (`activity_family_key` derives a stable stem by stripping
`will <subject>` prefix and truncating to 40 chars) collapse into one
summary row: `Resolved 8×: 2026 fifa world cup — all No`.

**Pass 2 — signal over floor.** Rows with `is_low_signal = true`
(trivial-Brier resolutions, Brier < 0.05) are demoted. The top 8 come
from the signal pool first, backfilled from floor only if there
weren't enough interesting rows. Perfect-Brier long-shot calls no
longer crowd out drafts and revisions.

The `Candidate` struct grew two fields (`is_low_signal`, `family_key`);
non-resolution rows (drafts / actives) set `family_key = String::new()`
so they never collapse.

### 3. Dashboard shows every forecast the operator owns

**Files:** `crates/fermi-console/src/main.rs` — `render_dashboard`.

Problem: forecasts not in any named portfolio were orphaned in the UX.
They didn't appear in the Portfolio panel (scoped to named portfolios)
and the Dashboard's Live section only rendered actives.

Two new `.when(...)` sections after the existing Live section, both
using the existing `render_forecast_section` helper:

- **Drafts** section — `.when(self.connected && !self.draft_forecasts.is_empty(), ...)`
- **Recently Resolved** section — last 10 resolutions,
  `.when(self.connected && !self.resolved_forecasts.is_empty(), ...)`

### 4. In-place Polymarket search card

**Files:** `crates/fermi-console/src/main.rs` — new
`render_pm_search_card` method, wired from both Dashboard and
Portfolio panel.

Problem: hero "🔮 From Polymarket" / "📎 Paste PM URL" buttons used to
punt the operator to the Portfolio panel just to see the PM search
sheet. Jarring context switch.

Solution:

- Extracted the ~300-line PM search block from `render_portfolio` into
  a shared `render_pm_search_card(&self, cx: &Context<Self>) -> impl IntoElement`
  method.
- Portfolio panel now calls
  `.when(self.pm_show_search, |el| el.child(self.render_pm_search_card(cx)))`.
- Dashboard renders the same card via
  `.when(self.connected && self.pm_show_search, ...)` between the hero
  and the stats row.
- Hero buttons only toggle `pm_show_search = true` — no panel switch.

**Single code path**: any change to the PM search UI lives in one
method and both entry points see it.

### 5. Pre-publish portfolio picker

**Files:** `crates/fermi-console/src/cockpit.rs`.

Problem: the portfolio chip strip under the composer's question field
was inert during composition ("publish (Ctrl+P) to enable" hint).
Operators had to publish, then remember to open the strip and click
chips.

Solution mirrors the existing `pending_publish_shares` /
`pending_publish_team_shares` pattern:

- New field `pending_publish_portfolios: HashSet<String>` on
  `CockpitState`, initialized in `new()`.
- `render_portfolio_membership_strip` now:
  - Reads header text from a mode-aware `is_draft` variable
    (`ADD TO PORTFOLIOS ON PUBLISH:` vs `IN PORTFOLIOS:`)
  - Renders a chip as "selected" if `is_member OR is_pending`
  - Uses dashed-gold border for `is_pending` (vs solid cyan for
    `is_member`) so operators can tell them apart
  - In draft mode, chip clicks toggle the pending set instead of the
    real API
- In `publish_forecast`, right after `pending_publish_team_shares`
  gets drained, we drain `pending_publish_portfolios` and call
  `toggle_portfolio_membership` for each — same code path chips take
  post-publish.

## Reverted this session (don't rebuild)

I built and then reverted a **bundled-agents** feature (embed
`agents/curated` at compile time via `include_dir!` so shipped release
binaries always have agents). The user said their dev build already
finds `agents/curated` fine, and asked me to revert.

**The concern is still real for shipped installs.** The updater
downloads only the bare binary (`updater.rs` → `download_and_install`),
so a user installing via Update & Restart from a bundle-less location
would see an empty Agent Fleet. If we ever hear that from a downstream
tester, the fix is:

```toml
# crates/fermi-console/Cargo.toml
include_dir = { version = "0.7", features = ["glob"] }
```

Plus a `bundled_agents` module using
`include_dir!("$CARGO_MANIFEST_DIR/../../agents/curated")` and a
`load_into(registry)` fn. Wire as fourth fallback in `main()` after
the filesystem search. Reference implementation was fully working
before revert — reconstruct from `git reflog` if needed.

## Follow-ups queued (in priority order)

### F1 — Composer's Polymarket type-ahead strip should be replaced by the shared search card (medium)

Called out in `RELEASE_NOTES_v0.8.4.md`'s Known Issues. The Composer
still has its own `render_pm_typeahead_strip` (`cockpit.rs`, around
line 13670) that runs debounced searches on question-field input.
Different UI, different code path, different everything.

**Goal:** Composer opens the same modal-style card the Dashboard hero
opens. Same UI, same handlers.

**Sketch:**
- Add `pm_show_search: bool` on `CockpitState` (mirror the
  `FermiConsole` field).
- Replace `render_pm_typeahead_strip` output with a "🔮 Browse
  Polymarket" chip that toggles the flag.
- Render `FermiConsole::render_pm_search_card` from the Composer.
  Trick: the card takes `&Context<FermiConsole>`, but Composer runs
  inside `CockpitState` — either lift the card into a free function
  taking a shared interface, or pass the parent handle down.
- Preserve URL-paste-in-question convenience: if someone pastes a PM
  URL directly into the question field, still auto-open the card
  with the pasted URL prefilled.

### F2 — In-place modal, not inline card (small)

Currently the Dashboard PM card renders **inline** between the hero
and the stats. Two consequences:

1. If it's tall (many results), it pushes stats down.
2. It doesn't dim the rest of the panel like a real modal would.

Consider promoting to a real overlay (like `render_update_modal`) with
backdrop and Escape-to-dismiss. Cheap change: wrap the existing card
in the same absolute/inset_0/backdrop pattern used by the shortcuts
modal.

### F3 — Composer's "typeahead strip" can be salvaged as an in-question hint (small)

If we go with F1, we lose one nice property of the current typeahead:
it surfaces PM matches passively as you type without any explicit
click. Consider keeping a tiny compact strip that shows
"3 markets match — click to browse" and opens the shared card on
click.

### F4 — Version chip mentions "BayesOps" no longer (cosmetic, done)

The v0.8.3 chip said `v0.8.3 — BayesOps`. My v0.8.4 changes label it
`v0.8.4 — up to date`. No action needed; noted here so the next
session doesn't add the tag back thinking it's missing.

### F5 — Bundle agents for shipped installs (deferred)

See "Reverted this session" above. Ship if any tester hits the empty
Agent Fleet on an updated install.

## Codebase orientation for next session

### Where the console lives
- `crates/fermi-console/src/main.rs` — the whole `FermiConsole`
  struct (~15,700 lines). Big but well-organized by section.
- `crates/fermi-console/src/cockpit.rs` — `CockpitState`, the
  composer's per-forecast state. Also large.
- `crates/fermi-console/src/composer.rs` — the older non-cockpit
  composer; barely used, kept for the "New Forecast" fallback path.
- `crates/fermi-console/src/text_input.rs` — custom text-input widget
  since GPUI doesn't ship one that suits this app.
- `crates/fermi-console/src/updater.rs` — self-updater (checks GitHub
  Releases, downloads bare binary, atomic swap, restart).
- `crates/fermi-console/src/charts.rs` — Plotters-backed image
  generation for trajectory worms, sparklines, etc.

### Action / keybinding pattern
The console uses GPUI's action system. To add a new hotkey:
1. Add a marker type to the `actions!` block near line 182 of
   `main.rs`.
2. Add a handler method (`fn on_do_the_thing(&mut self, _: &DoTheThing, _w, cx)`).
3. Register in the render function's `.on_action(cx.listener(...))`
   chain (line ~13200).
4. Bind in `main()`'s `cx.bind_keys(...)` block (line ~15600).
5. **Add a row to `render_shortcuts_modal`** so operators can find it.
   This is the single source of truth for user-facing hotkey docs.

### Modal / overlay pattern
Every modal follows the same shape: `bool` field on `FermiConsole`
gates rendering, `render_<name>_modal` returns a
`div().absolute().inset_0().flex()...` overlay with a semi-transparent
backdrop. See `render_update_modal`, `render_shortcuts_modal`, or
`render_invite_share_modal` for examples.

### Recent Activity feed
Rebuilt every time `fetch_forecasts` returns via
`recompute_recent_activity()`. The `Candidate` struct is now:
```rust
struct Candidate {
    sort_key: String,   // ISO-8601 timestamp, sorted desc
    item: ActivityItem, // rendered row
    is_low_signal: bool,   // trivial-Brier resolution
    family_key: String,    // for run-length collapse
}
```
If you add a new row type (e.g. "team invite received"), initialize
both new fields (`false`, empty string) — an empty `family_key` opts
out of collapse.

### Pending publish pattern (on `CockpitState`)
Three "collect during composition, apply on publish" queues:
- `pending_publish_shares: Vec<(String, String)>` — user shares
- `pending_publish_team_shares: Vec<(String, String)>` — team shares
- `pending_publish_portfolios: HashSet<String>` — portfolios (new
  this session)

All three drain right after `state.forecast_id = Some(fid.clone())`
in `publish_forecast`. If you need another "attach at publish"
concept, mirror the pattern.

## Release / distribution flow

- Bump `crates/fermi-console/Cargo.toml` version.
- Write `RELEASE_NOTES_v0.X.Y.md` at repo root — the updater surfaces
  this in the release-notes modal.
- Commit as `Release vX.Y.Z: <short summary>`.
- `git tag -a vX.Y.Z -m "..."` and `git push origin vX.Y.Z`.
- Downstream CI builds the artifact; the in-app updater's background
  check picks it up within one polling interval.

`FERMI_UPDATE_REPO` env var overrides the target repo for testing —
defaults to `Replicant-Partners/fermi`.

## Known noise in the tree

- **52 pre-existing warnings** on `cargo check -p fermi-console`.
  Mostly unused variables in cockpit's evidence/agent handling code
  and a handful of "field never read" in `updater.rs`. Not from this
  session's work — don't waste a session trying to clean them up
  unless asked.
- **`sqlx-postgres v0.8.0`** future-incompat warning. Upstream; not
  ours to fix.
- **42 Dependabot vulnerabilities** on `main` per the push output
  (11 high, 20 moderate, 11 low). Someone should triage; not urgent.

## Validation commands

```
cd /home/ilabra/fermi
cargo check -p fermi-console    # ~5-10s incremental, expect 52 warnings, 0 errors
cargo build -p fermi-console    # full build if you need to run it
```

The user runs their own dev build; don't try to launch the console
yourself.

## Open questions to ask the user next session

1. Do F1 (unify Composer's PM affordance) next, or something else?
2. Do you want F2 (promote in-place card to real overlay modal) or
   keep it inline?
3. Any regressions or usability issues surfacing from v0.8.4 that
   should jump the queue?

## Session hygiene notes

- User prefers **concise directives** and directional feedback. Don't
  gold-plate. Ship narrow, well-motivated changes with clear commits.
- User uses phonetic / abbreviated typing (e.g. "porfolio", "affordance",
  "concole") — parse charitably.
- Two-commit rhythm works well here: **feature commit** (all logic
  changes) + **release commit** (version bump + release notes). Push
  both, then tag. Don't tag before pushing.
- The user has been asking for **push before test** several times —
  they run the built binary themselves and want to test iteratively.
  Push early; don't hold a batch for polish.
- Comments in this codebase are unusually load-bearing — they explain
  the *why*, especially for UX decisions. Match the style: comment on
  design intent, not code mechanics.
