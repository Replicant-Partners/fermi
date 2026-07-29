# Fermi Console v0.9.5 — Composer Polymarket typeahead: fixed + unified

Patch release. Fixes **the** bug behind "the composer Polymarket
typeahead has never worked as it ought" — a JSON-key mismatch that
silently discarded every search result since the feature shipped —
and rebuilds the widget's UI to match the Dashboard's Polymarket
search card so the console has one consistent Polymarket look.

## Root cause — the composer typeahead has literally never returned a match

Two code paths hit `POST /api/polymarket/search`:

1. **Dashboard** `FermiConsole::search_polymarket` (`main.rs`) — reads
   `data.get("matches")` and populates `pm_search_results`. Works.
2. **Composer** `CockpitState::pm_typeahead_search` (`cockpit.rs`) —
   read `resp.get("results")` and populated `pm_suggestions`. **Broken
   since day one.**

The server's handler (`src/handlers/polymarket.rs::search_handler`)
returns:

```json
{
  "matches":       [ …MarketMatchResponse… ],
  "search_query":  "…",
  "results_count": N,
  "events_searched": N,
  "credits_charged": 1
}
```

There is no `"results"` key. The composer's lookup fell through the
`.get("results")` branch, tried `.or_else(|| resp.as_array())` on an
object (also None), and settled on `unwrap_or_default()` → empty
`Vec`. Every keystroke:

- fired the search successfully,
- returned matches successfully,
- parsed to a `Value::Object { matches: [...] }` successfully,
- then had every match silently thrown away by the wrong key lookup,
- leaving `pm_suggestions` empty,
- which failed the render gate `!has_content` in
  `render_pm_typeahead_strip`,
- which returned an empty `div()`,
- so the operator never saw a chip.

The debounce, seq-race machinery, dismiss flag, 3-char minimum, and
"no matches for that phrasing" empty-state all worked correctly — the
JSON key was the single silent failure and it starved every other
path. The Dashboard's key lookup (added later, by different hands) is
right.

The `.or_else(|| resp.as_array())` branch was speculative defense
against a shape the server never returns, and it masked the primary
lookup being wrong. Removed.

## The fix

**`crates/fermi-console/src/cockpit.rs::pm_typeahead_search`** —
key lookup switched from `"results"` to `"matches"`, defensive
`as_array()` fallback dropped. Success path now traces the query and
result count at info level:

```rust
Ok(Ok(resp)) => {
    let arr = resp
        .get("matches")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    log::info!(
        "[pm-typeahead] query={:?} returned {} suggestions",
        trimmed, arr.len(),
    );
    state.pm_suggestions = arr.into_iter().take(5).collect();
    state.pm_suggest_last_query = trimmed.clone();
}
```

Failure paths now include the query in the log:

```rust
log::warn!("[pm-typeahead] query={:?} search failed: {:?}", trimmed, e);
```

A future silent-empty regression is one `grep [pm-typeahead]
run.log` away — any zero-count trace is diagnostic.

## UX unification — composer typeahead adopts the Dashboard's row layout

Before v0.9.5 the two Polymarket widgets looked very different:

- **Dashboard** (`render_pm_search_card`) — rich rows: bold purple
  price column with 1-week change arrow, question + event title +
  metadata (24h vol / liquidity / confidence signal / end date),
  right-side "Import →" pill. Purple-bordered card, up to 10 results
  in a scrollable list.
- **Composer** (`render_pm_typeahead_strip`) — flex-wrap "chip pill"
  layout: truncated title + `NN%` on a single line, up to 5 chips
  wrapping across the row. No event title, no vol/liq, no confidence,
  no end date, no 1-week change.

Operator preference: keep the Dashboard's rich rows and use them in the
composer typeahead too. Same visual language everywhere Polymarket
appears in the console.

`render_pm_typeahead_strip` now mirrors the Dashboard row structure
one-for-one:

- Container: purple-bordered card, `bg(rgb(0x1A1A2E))`, `px 12 py 10`
  (was `px 8 py 6` on the chip strip).
- Header: bold "🔮 Polymarket matches" / "🔮 Searching Polymarket…" on
  the left, `✕` dismiss on the right — same look as the Dashboard's
  "🔮 Browse Polymarket" header.
- Rows: vertical `flex_col` list (was flex-wrap chips). Each row:
    - 60px price column, `18px BOLD PURPLE`, with `↑↓→` 1-week change
      pill underneath (green / red / dim);
    - question at 12px, event title at 9px `fg_faint` when it differs
      from the question, metadata row (24h vol / liq / confidence-
      colored / end date) at 9px;
    - "Import →" pill on the right, purple-bordered.
- Confidence coloring reads `confidence_signal` (server field on
  `MarketMatchResponse`), mapping "Very High" → green, "High" → cyan,
  "Medium" → gold, else `fg_faint` — same table as the Dashboard.
- No scroll container; the typeahead is already capped at 5 rows in
  `pm_typeahead_search`, so vertical growth is bounded (~5 × ~48px
  ≈ 240px worst case).
- The "no matches for that phrasing" empty-state block is preserved
  and still renders when `last_query` is set but `pm_suggestions` is
  empty.

Click behavior unchanged — hitting a row calls
`link_polymarket_market` with the same 8-argument signature as
before, so the persist / observation-write / poll-start chain is
untouched.

## Why we're confident this ships the promised behavior

- `MarketMatchResponse` in `src/handlers/polymarket.rs` exposes
  every field the new row layout reads: `pm_event_id`,
  `pm_market_id`, `question`, `event_title`, `market_price`,
  `market_price_pct`, `volume_24h`, `volume_24h_fmt`, `liquidity`,
  `liquidity_fmt`, `price_change_1w`, `end_date`, `confidence_signal`.
- The Dashboard has been shipping the same field lookups against
  the same handler since well before v0.9.3, so field names are
  known-stable.
- Build clean: `cargo check -p fermi-console` finishes with only
  pre-existing warnings and no errors.

## What's still open

**Mario's original Dashboard network error** remains unexplained.
v0.9.3 unified the client transport, v0.9.4 fixed locale prefixes,
v0.9.5 fixes the composer path; none of those directly explain a
`reqwest error sending request` from Mario's binary specifically. The
next step still requires a `{:?}`-formatted `reqwest::Error` from his
stderr, which v0.9.3's `[polymarket]` log surfaces on his next
reproduction.

## Files touched

- `crates/fermi-console/src/cockpit.rs` — key-lookup fix
  (`"results"` → `"matches"`), diagnostic logging, full
  `render_pm_typeahead_strip` rewrite to Dashboard-style rich rows.
- `crates/fermi-console/Cargo.toml` — 0.9.4 → 0.9.5.
- `RELEASE_NOTES_v0.9.5.md` — this file.

Validation: `cargo check -p fermi-console` clean.

v0.9.5
