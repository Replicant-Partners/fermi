# Fermi Console v0.9.4 — Locale-prefixed Polymarket URLs

Patch release. Fixes a real bug found while debugging Mario's Dashboard
network error: **Polymarket URLs with locale prefixes** (`/es/event/…`,
`/fr/event/…`, `/de/event/…`, etc.) were falling through the console's
slug-extraction and being sent as the raw fuzzy query — inflating the
request body from an 18-char slug to a 74+-char URL and producing junk
server-side matches.

Also folds in the tightened error logging from v0.9.3 in a form that
doesn't depend on the routing-through-ApiClient theory (which we
confirmed was fine on Ivan's side, but wasn't actually the fix for
Mario's specific issue).

## Root cause of the locale bug

Both entry points that accept a pasted URL —
`FermiConsole::search_polymarket` (Dashboard) and
`CockpitState::pm_typeahead_search` (composer) — had the same
prefix-stripping logic:

```rust
raw.strip_prefix("https://polymarket.com/event/")
    .or_else(|| raw.strip_prefix("http://polymarket.com/event/"))
    .or_else(|| raw.strip_prefix("polymarket.com/event/"))
```

None of those match Mario's real URL:

```
https://polymarket.com/es/event/laliga-2027-champion-20260701200737375
                       ^^^ locale prefix — no branch handles this
```

So the extractor returned nothing, and the code fell through to
`query = raw.clone()`. The full 74-char URL got sent to the server as
the search text, which either returned no matches (fuzzy match on a
URL string returns junk) or timed out mid-fuzzy-index.

## The fix

New shared helper `extract_polymarket_event_slug(raw)` in
`crates/fermi-console/src/main.rs`. Handles:

- All three URL schemes (`https://`, `http://`, bare host).
- **Optional locale prefixes** per Polymarket's supported set:
  `es fr de pt ja it tr vi zh th ko ru`.
- Trailing sub-market paths, query strings, and fragments — all
  stripped.
- Returns `None` for non-URL search phrases so the caller can send
  them as-is (composer typeahead + Dashboard search both preserve
  the "plain text goes through raw" contract).

Both call sites replaced their inline `strip_prefix` chains with one
call to the helper. Same URL handling everywhere so a future URL
variant (Polymarket adding another locale, changing path shape) only
needs updating in one place.

## Test coverage

`crates/fermi-console/src/main.rs::slug_tests` — 10 unit tests
including Mario's exact `/es/` URL as a fixture:

- `extracts_from_https_no_locale`
- `extracts_from_https_with_spanish_locale` — Mario's case
- `extracts_across_all_supported_locales` — table-driven over all 12
- `strips_sub_market_path_and_query`
- `strips_sub_market_path_with_locale`
- `accepts_bare_host_no_scheme`
- `returns_none_for_plain_query` — phrases still go through raw
- `returns_none_for_unrelated_url` — `example.com` etc. rejected
- `returns_none_for_locale_without_event_segment` — `/es/markets/…`
- `returns_none_for_empty_slug` — trailing slash edge case

All 10 pass.

## What this does NOT fix

**Mario's original "Network error: error sending request"** on the
Dashboard search remains unexplained. My v0.9.3 Cloudflare-user-agent
hypothesis was wrong (Ivan's Dashboard has always worked with the
same client config Mario's rejects). Real cause is likely
network-layer specific to Mario's environment — corporate proxy, IPv6
routing, DNS filtering, or similar — which we can't diagnose without
data from his side.

**What v0.9.4 does help with**: even if Mario's network issue clears
(temporary blip, VPN change, etc.), his `/es/` URL now extracts to
the clean slug and the server can respond correctly. And v0.9.3's
`log::error!("[polymarket] pm_search network error: {:?}", err)`
stays in place — when Mario next hits the failure, the log will
carry the full reqwest error chain (connect vs timeout vs TLS vs
body) so we can name the network layer that's rejecting him.

## What's still open

**The composer typeahead**: Ivan noted separately that it "has never
worked as it ought." That behaviour is *not* about the HTTP client
config (v0.9.3 confirmed the shared client works on his Dashboard).
Likely candidates for the composer's flakiness:

- The 500ms debounce + seq-counter race (had a leak I fixed in
  v0.8.13, might have more)
- The `pm_suggest_dismissed` flag getting stuck
- The `pm_suggest_last_query` duplicate-suppression firing on
  legitimate re-queries
- The 3-char minimum silently swallowing shorter queries

If Ivan can name the specific symptom he sees ("no matches show", "loads
forever", "wrong matches"), we can target it in v0.9.5. Not shipping
a speculative fix without knowing which quirk to chase.

**Mario's network issue**: same story — real diagnosis needs
`fermi-console 2>&1 | tee run.log` output showing the full
`{:?}`-formatted reqwest error kind, or a `curl -v` reproduction of
the failing POST to `agent-bestiary.world/api/polymarket/search`.

## Migration

None. Client-only change. No server / schema impact.

## Files touched

- `crates/fermi-console/src/main.rs` — new
  `extract_polymarket_event_slug` helper (+ 10 unit tests);
  `search_polymarket` uses it for URL parsing.
- `crates/fermi-console/src/cockpit.rs` — `pm_typeahead_search` also
  uses the shared helper; drops the local `strip_prefix` chain.
- `crates/fermi-console/Cargo.toml` — version bump.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release -p fermi-console` — clean.
- 10 new slug tests pass; 52 pre-existing shape tests unchanged.
