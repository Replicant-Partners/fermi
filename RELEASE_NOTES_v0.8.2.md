## What's new

- **Polymarket type-ahead in the Composer.** As you type your forecast question, Fermi live-searches Polymarket in the background (500ms debounced). Up to five matching markets appear as click-to-import chips directly under the question field — each chip shows the market's title and current crowd price. Click one and the forecast is initialized with the PM link pre-populated: question text, crowd price seeded as the inside view, 5-minute background poll enabled, ready for Ctrl+Enter to decompose. Zero panel-hopping, zero search modal.

- **Paste-a-URL shortcut.** Paste a `polymarket.com/event/...` link into the question field and the search skips straight to that market — one click to import.

- **Dismissible.** Click the ✕ on the type-ahead strip to hide it for the session. Clearing the question field brings it back automatically. Once a forecast is linked to a market, the strip stays out of the way — no re-suggesting.

- **Sign-in via browser is fully automatic again.** Fixed a server-side OAuth state serialization bug that was corrupting the desktop redirect target (colons in `http://127.0.0.1:PORT/callback` were being truncated). The console's browser sign-in now completes without asking you to paste a token.

## Fixes

- **OAuth state serialization**: server now URL-encodes the redirect target before folding it into the OAuth `state` parameter, and URL-decodes on callback. Fixes the "browser opens, signs in on ABW, but console keeps waiting" regression.

## Known issues

- Type-ahead needs at least 3 characters to search, and the Polymarket search endpoint's fuzzy matcher won't return anything for very generic queries ("will it happen"). Refine the phrasing (add subject/date) and the matches show up.

- The Portfolio panel's "🔮 Import from Polymarket" button is still there — it's the browse-catalog flow for when you want to shop markets without a specific question in mind. The new composer type-ahead is the primary path for "I know what I want to forecast, is there a market for it?".

## Breaking changes

None.

## Upgrade notes

Just Update & Restart. The OAuth fix is server-side and applies to all clients as soon as the API server on `agent-bestiary.world` picks up this commit.
