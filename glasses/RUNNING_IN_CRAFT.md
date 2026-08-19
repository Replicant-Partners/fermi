# Running these in Craft — step by step

Written because "Run Agent" came up greyed out and the AIUI docs do not say what
enables it. This is a bisect, not a diagnosis: I cannot see your Craft session, so
the plan is to remove one variable at a time, cheapest first.

Craft: <https://js.rokid.com/craft?region=global>

---

## Step 0 — Check whether a model is selected

**Most likely cause, and free to rule out.**

Craft simulates the whole loop — wake word, speech recognition, the language
model, text to speech. It cannot run the model half without a model. Docs say it
ships a free one (DeepSeek V4 Pro) and supports bring-your-own, which implies a
selection exists somewhere.

Open the **gear / settings icon** in the top toolbar, next to the sun and
translate icons. Look for a model or provider setting. If nothing is chosen,
choose the bundled free model.

If Run Agent lights up, that was it, and nothing in the project was wrong.

---

## Step 1 — Try the eye icon instead

There is a **preview (eye) icon** to the left of Run Agent. Preview and Run are
different affordances: preview renders the page, Run drives the full assistant
loop. If preview works and Run does not, the page is fine and the blocker is on
the agent-binding side, not the rendering side.

Note this from the quickstart, which suggests Run may require cloud state that a
freshly imported folder does not have:

> Before you can debug the AIUI project on the physical glasses, it must be bound
> to an AIUI Agent, packaged, and uploaded.

That sentence is about the glasses, but the same binding requirement plausibly
gates Run in Craft. **Unverified** — I am inferring from an adjacent doc, which is
exactly the kind of guess that has already cost us a day this week, so treat it as
a hypothesis to test rather than a fact.

---

## Step 2 — Import `minimal_probe` instead

**This is the step that actually produces information.**

`glasses/minimal_probe/` is the smallest project that can render: one page, one
line of text, no `fetch`, no loops, no conditionals, no stub, five files.

Import it and press Run Agent.

| Outcome | What it means | Where to look next |
| --- | --- | --- |
| Probe runs, shell does not | The project layout is fine. Something in the shell's page is rejected — most likely the `<script setup>` top-level `const` declarations, `AbortController`, or `async` methods on the page object. | Step 3 |
| Neither runs | The blocker is the project layout, the manifest, or the session — not my page. | Steps 0 and 1, then Rokid's forum |
| Both run | Something was stale. Re-import the shell. | Nothing |

---

## Step 3 — If the probe runs and the shell does not

Bisect the shell by deleting from the bottom of `<script setup>` upward. In
likelihood order, the suspects are:

1. **`AbortController` / `setTimeout`** in `callAgent`. Documented as WinterCG
   Minimum Common Web API, but this is QuickJS and the streaming APIs only landed
   in v0.16.0. Delete `callAgent` entirely — the stub path does not need it.
2. **`async` page methods.** `ask()` and `callAgent()` are `async`. If the page
   object must hold plain functions, make `ask` synchronous and use the stub
   directly.
3. **Top-level `const` before `export default`.** If the block must be a bare
   module with only an export, move `STUB_CARD` inside `data`.
4. **`Array.prototype.filter` / arrow functions** in the unstamped check.
   Unlikely, but it is the only place the page does real work.

Each of these is a two-line deletion. Whichever one unblocks it is the thing to
report back, because it tells the generator what the runtime will actually accept.

---

## What I changed to remove my own variables

Both projects now match the `create-aiui-agent` scaffold as closely as I can
without a copy of it:

- **Page moved to `pages/index/index.ink`.** It was `pages/card/index.ink`. The
  scaffold puts its page at `pages/index/index.ink`, and a runtime looking for a
  default entry page named `index` would not have found `card`. Cheap to rule out,
  so ruled out.
- **`VERSION` file added.** The publish docs say the platform validates "the
  `VERSION` file and `AGENTS.md` declaration in the package". The scaffold does
  not appear to create one, so this may be unnecessary — it is also harmless.
- **`app.json` updated** to point at the new path.

Verified on disk: both projects have all five files, `app.json` points at a page
that exists, every page has all four `.ink` blocks and exports a default, and no
stylesheet uses a colour outside `#40ff5e` / `#000000`.

---

## Two things worth knowing

**The `README.md` in `hud_field_scout/` may confuse the importer.** It is
documentation for humans, not part of the package. If the probe runs and the
shell does not, deleting `README.md` is a free thing to try before Step 3.

**`npm start` does not work**, and that is Rokid's bug, not yours. The scaffold's
README advertises it while the generated `package.json` contains no `scripts`
block at all. Do not spend time on it; Craft and `aix preview` are the dev loop.
