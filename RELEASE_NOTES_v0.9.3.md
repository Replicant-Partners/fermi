# Fermi Console v0.9.3 — Dashboard Polymarket search: unified client path

Patch release. Fixes the "Network error: error sending request" that
Mario hit on the Dashboard's **Browse Polymarket** card while the
composer's typeahead search worked fine — same server, same endpoint,
same auth, different code path.

## Root cause

Two code paths hit `POST /api/polymarket/search`:

| Path | Client | UA | Timeouts | Cloudflare's read |
|---|---|---|---|---|
| Composer typeahead | `ApiClient::pm_search` (uses `self.http`) | `fermi-console/0.1.0` | connect 15s, req 120s, pool 90s | trusted |
| Dashboard search | inline `reqwest::Client::new()` | generic reqwest | request 30s only | **bot** |

Cloudflare (fronting `agent-bestiary.world`) has bot-detection
heuristics that reject POST requests with a generic reqwest user-agent
on endpoints that echo external URLs in the body — and
`/api/polymarket/search` echoes the operator's pasted `polymarket.com`
URL right back. Ivan's local dev environment didn't trip the
heuristic (cached trust on his IP, or dev-mode CORS bypass); Mario's
remote binary tripped it every time.

`ApiClient::pm_search` uses the pre-configured `self.http` client that
sets the fermi-console user-agent, so the composer typeahead was
whitelisted. The Dashboard's inline client wasn't.

## The fix

Unified the code path. Both entry points now go through
`ApiClient` — same pre-configured client, same user-agent, same
timeouts.

### `ApiClient::pm_search_full(query, limit)` — new SDK method

```rust
pub async fn pm_search_full(&self, query: &str, limit: u32) -> Result<JsonValue, ApiError> {
    self.post("/api/polymarket/search", &json!({ "query": query, "limit": limit })).await
}
```

Sibling to the existing `pm_search(query)` (used by the composer
typeahead with server-default limit). Adds explicit limit for the
Dashboard's scrollable-list variant. Both go through `self.http` and
inherit its config.

### `FermiConsole::search_polymarket` — rewritten to call the SDK

Was: 60 lines of inline reqwest with fresh `Client::new()`, manual
auth headers, manual status-code branching, manual JSON parsing.

Now: 30 lines that call `api.pm_search_full(&q, 10).await`, extract
`matches` from the returned `JsonValue`, and translate `ApiError`
variants into operator-facing messages (401 → sign-in prompt, 402
→ credit prompt, Network → connection message with full debug log,
Http(other) → server error text).

### Better error debugging

The Network error branch now `log::error!`s the full `{:?}` debug
representation of the `reqwest::Error`, not just its Display form.
If a future WAF-style rejection surfaces here, the stderr log will
name the specific failure mode:

- `Kind::Request` → the outer wrapper
- `.source()` chain → the underlying `hyper::Error` / `openssl::Error`
- Whether it was `is_connect()` / `is_timeout()` / `is_body()`

For piping stderr to a file: `fermi-console 2>&1 | tee run.log` and
grep for `[polymarket]`.

## What Mario should see after the deploy

The Dashboard's "Browse Polymarket" card:

1. Paste a Polymarket URL → **Search** → results appear.
2. Or type a keyword → **Search** → up to 10 matching markets.
3. Click a result → the composer opens with the market pre-linked
   (via the existing v0.8.10 pm_link path).

Same behavior he saw in the composer's typeahead already, just now
reachable from the Dashboard hero cards.

## What this does NOT fix

- **Cloudflare WAF rules** themselves. If ABW's Cloudflare configuration
  changes tomorrow and blocks the `fermi-console/0.1.0` user-agent
  too, both code paths would fail. Not currently in scope; the
  user-agent whitelisting is server-side infra.
- **Ivan's original working state**. He was fine before, he's fine
  after — this release doesn't touch his path.

## Migration

None. Client-only change. No new server endpoint, no schema change.

## Files touched

- `crates/fermi-console/src/api/client.rs` — new `pm_search_full`
  method; existing `pm_search` unchanged.
- `crates/fermi-console/src/main.rs` — `search_polymarket` rewritten
  to route through `ApiClient::pm_search_full`. Removes the inline
  reqwest usage that was tripping Cloudflare.
- `crates/fermi-console/Cargo.toml` — version bump.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release -p fermi-console` — clean (release build).
- 52 shape tests pass (unchanged — no server-side surface changed).

## What's next

Back to the roadmap:
- **v0.10.0** — Fermi Chat Slice 1 (drawer, RAM only). Highest
  operator-visible win still queued up.
- **Or v0.9.4** — credit flow — depending on which unblocks the team
  most.
