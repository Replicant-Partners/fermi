# v0.15.0 — Fermi can change the forecast

The loop is closed.

Conversation with Fermi is neuro. The FPL program is symbolic. Research
agents are neuro again. The Brier score closes it. Until this release the
first link was broken: Fermi could *reason* about a forecast perfectly
well and say "manager_continuity should be 0.65, Guardiola has left" —
and then you had to go and type it in yourself.

The action vocabulary was navigation only: `open_forecast`, `open_panel`,
`run_simulation`, `search_polymarket`. Nothing could touch the model.

## Four tools that write

```
set_driver_distribution   {driver, p5, p50, p95}
set_driver_probability    {driver, probability, impact_multiplier}
set_base_rate             {historical_frequency, reference_class,
                           sample_size, reasoning}
assign_agent              {driver, agent_id}
```

So a conversation can now run:

> **You:** Guardiola left in May, this forecast still assumes he's there.
>
> **Fermi:** Then `manager_continuity` is answering the wrong question —
> it prices a 35% chance he extends, and the answer is now 0. The live
> question is Maresca's adaptation year.
>
> `[▶ set_driver_distribution manager_continuity → 0.70/0.85/1.00]`
> `[▶ run_simulation]`

You click. The model changes. The index moves.

## Refusing the bad edits is the actual work

These are writes to a forecast authored by a language model.
`fermi_console::mutations` validates every field before anything reaches
the AST:

- **`p5 ≤ p50 ≤ p95`.** A backwards triangular doesn't fail loudly — the
  executor samples from it and produces a nonsense forecast. This
  codebase has shipped that failure mode twice already.
- **Multipliers must be positive**, and a `p95` above 100 is rejected as
  percent/multiplier confusion (`65` meaning `0.65`) with the likely
  intent named in the error.
- **Probabilities in [0,1]**, same percent check.
- **`set_base_rate` requires a `reference_class`.** A frequency with no
  class is precisely what the v0.14.0 calibration checks exist to catch;
  letting chat write one would route around them. Chat-authored base
  rates get the same Wilson-interval and circularity critique as
  hand-entered ones.
- **Quoted numbers are accepted.** LLMs quote numbers. Rejecting `"0.8"`
  would make the feature look broken for a reason you can't see or fix.
- **`assign_agent` refuses an agent nothing can execute**, which would
  otherwise leave a driver that looks researched and never will be.

Validation is pure and tested — 18 tests. The cockpit does the mutation.
Approval is unchanged: every edit is a chip you click, all of them honour
`refuse_write()`, and the driver editor buffers are kept in step so
opening Edit afterwards can't write stale values back over the edit.

The prompt now states the multiplier convention explicitly, tells Fermi to
show its arithmetic in `reason` (you are approving that arithmetic, so it
should be visible), and to propose `run_simulation` separately so the
effect is yours to trigger.

## The research key is now testable

Ctrl+Enter's three outcomes — run staged research, decompose, refuse to
overwrite — include two irreversible ones, and the branch was an `if`
ladder inside a GPUI event handler where it could not be tested.

Extracted to `fermi_console::flow` with 10 tests over the priority order
that matters:

- **Staged research always wins.** Decomposition has just told you
  "review, then press again" — re-decomposing at that moment would
  discard the assignment you were reviewing.
- **An empty composer never arms a warning** about work it cannot touch.
  Noise trains people to dismiss the prompt that matters.
- **Drivers *or* evidence counts as work.** A forecast with hand-tuned
  estimates and no evidence is still hours of your afternoon.

## Testing

`fermi` lib: **245 passing**. `fermi-console` lib: **230 passing** (up
from 196 two releases ago; the new logic went into the lib target
specifically so it could be tested in seconds rather than the binary
target, where rustc segfaults expanding the GPUI element tree).

Release binary builds, launches, initialises the renderer, loads 100
agents, survives a run with no panics, and the new tool vocabulary is
present in the shipped artefact.

**Still not verified by driving the UI.** Nobody has clicked a
`set_driver_distribution` chip. The validation layer is thoroughly
tested; the click-to-mutation path is verified by compilation only. Try it
on a throwaway forecast first.

## Known, not fixed

- A **duplicate schedule row** keyed on the bound agent name
  (`football_analyst_squad_quality_trajectory`, `every 0h`,
  `next 3000-01-01`) violates the invariant documented at
  `cockpit.rs:5793`. The writing path hasn't been found.
- One shared forecast showed **index 99.9% against a simulation mean of
  0.3497**, revision recorded as "manual". Needs that forecast's revision
  rows.
- **Clicking a driver doesn't open the Edit tab.** Deliberate —
  `focus_driver` avoids yanking the panel away from what you were reading
  — but it reads as a bug because there's no feedback.
- The server's own upstream timeout to `api.anthropic.com` still kills
  some agent runs. Server-side configuration, not reachable from here.
