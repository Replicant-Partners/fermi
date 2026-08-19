# HUD Field Scout — glasses shell

An AIUI package that captures a question, sends it to the `hud_field_scout`
agent on ABW, and renders the card that comes back.

**It decides nothing.** Every marker, provenance tag and confidence band is
computed by `src/hud_contract.rs` server-side and arrives already stamped. The
shell copies them to the screen.

That split is the point. The agent stays on ABW, so it writes episodes,
consolidates, gets evaluated and has its grounding enforced — all of which it
would lose by running on the device. The glasses are I/O.

```
glasses (this package)
   fetch()  ->  POST /api/agents/hud_field_scout/execute      [ABW]
                   grounding + provenance boundary
                   episodes, dreaming, evals, coherence
   <-  card JSON, markers already stamped
   render: green tokens + those markers
```

## Status

**Nothing here has rendered.** It is written against documented AIUI APIs and
checked by `tests/glasses_shell_parity.rs`, which makes text assertions over
these files. That suite passing does **not** establish that:

- the runtime parses this `.ink` page
- `fetch` reaches ABW over the phone's Bluetooth-proxied link
- the flex layout lands where intended on a waveguide
- 60 characters actually fit 480px at 15px sans-serif
- the marker glyphs are legible green-on-black at arm's length

`the_uncovered_surface_is_named` in that suite exists to keep this list
honest rather than letting a green run imply coverage it does not have.

## Run it in the simulator, with no device and no backend

`STUB = true` in `pages/card/index.ink`, so the render can be validated before
ABW is reachable. This separates "does the card look right" from "does the
endpoint work" — and the stub's title says `STUB - not a real answer`, because a
convincing stub would demonstrate a pipeline that does not exist.

1. Open **Craft Global**: <https://js.rokid.com/craft?region=global>
2. Import this directory — Craft accepts a local folder, an `.aix`, or a GitHub
   subdirectory (`glasses/hud_field_scout`).
3. **Run Agent.** Craft simulates wake word, speech recognition, the model and
   text-to-speech; the controls on the right simulate Back, Tap and swipe.

What to look at, in order of what would most change the design:

- **Does the marker column scan?** The glyphs should form a readable column down
  the left. If the eye has to hunt for them, the fixed 18px column is wrong.
- **Do the lines fit 480px?** `hud_contract::LINE_MAX` is 60 characters, chosen
  before the canvas was known. The longest stub line is 38 characters; if that
  is already tight, 60 is wrong and should be re-derived from the type metrics.
- **Is `~` distinguishable from `!` at a glance?** They carry the entire
  provenance signal, since the panel has no second hue to spend.

## Point it at ABW

1. Set `STUB = false` in `pages/card/index.ink`.
2. Set `ABW_BASE` to your deployment. **Must be HTTPS.**
3. Register that domain in the AIUI console's allowlist. Requests to
   unregistered domains are rejected before publication.

The endpoint already exists on ABW; nothing needs building server-side.

Note the camera path is not wired: ABW's execute endpoint cannot accept an image
yet (`src/attachments.rs` has the payload rules, nothing plumbs a request into
them). `AGENTS.md` therefore does **not** request `camera` permission, and
`camera_is_not_requested_before_the_platform_can_carry_a_frame` fails if the two
drift apart.

## Onto real glasses

No cable and no ADB. Documented in AIUI's quickstart:

```
Settings > Developer > AIUI > Update Glasses Resource Package
```

1. `npm install -g @yodaos-pkg/aix-cli`
2. `aix pack . -o dist/hud-field-scout.aix`
3. Upload at <https://aiui-global.rokid.com/space> — Application Management →
   Create Application → type "AIUI Agent". Replace the default icon; submissions
   keeping it are rejected.
4. Version management → Upload Version. The platform validates `VERSION` and
   `AGENTS.md`; **the validation rules are not published**, so expect churn.
5. On the glasses: Settings → Developer → AIUI → Update Glasses Resource
   Package, then wake the assistant and say the agent's name.

Publishing to the store additionally requires Rokid review of performance,
interaction compliance and security. Whether an agent whose core function is
calling a third-party endpoint passes review is **not documented** — worth
learning early, on something cheap.

## Why `AGENTS.md` looks the way it does

Three incompatible conventions exist in Rokid's own material: the Open Agent
Format spec doc, the `aiui-dev` skill, and the `create-aiui-agent` template.
This follows **the skill's shape**, because that is what the shipped samples
use, and the samples are the only version known to have been packaged and
accepted. `the_manifest_follows_the_sample_convention` pins that choice so it
does not drift back.

## Files

| | |
|---|---|
| `AGENTS.md` | manifest — identity, permissions, and why `camera` is absent |
| `app.json` | page registration |
| `app.js` | entry point, deliberately empty of logic |
| `pages/card/index.ink` | the whole shell: capture, request, render |

Verified by `tests/glasses_shell_parity.rs` (13 assertions), which is the
counterpart of `port_binding_parity`: it exists because a second surface that
displays provenance is a second place for a trust rule to drift.
