# v0.16.0 — record how the agent was asked, not just how it did

v0.15.x taught the console to compose a research query from what an agent's
card *declares* rather than from a hardcoded match on its identifier. That
fixed the query. It did not fix the record.

Every episode already said how a run turned out: status, failure reason,
confidence, tokens, and — once the forecast it fed resolves — a Brier
score. None of them said how the agent had been *asked*. So the two most
important failures were indistinguishable in the data:

1. the agent was asked properly and is bad at the job;
2. the agent was sent the wrong shape of question.

That distinction is not academic. The platform's premise is composing
fleets across heterogeneous providers and designers, and learning from
those interactions which agents and compositions work. A loop learning
from outcome alone blames agents for the caller's mistakes, and drifts
toward preferring the agents the caller already knows how to talk to.
That is the closed world regenerating itself through the reward signal —
the same defect v0.15.x removed from the query path, one layer up.

## Invocation provenance

Every research run now carries a record of how its query came to be, and
the server stamps it onto the episode:

```
console: compose_query + bind_input
  → { query_source, input_binding, declared_label_count,
      recomposed_from?, driver? }
  → POST /api/agents/:id/execute[/stream]   body.invocation
  → stamp_invocation(&mut episode, inv)
  → tags:    qsrc:declared_contract   ibind:declared-query
             recomposed:true
     context.invocation: { … }
```

`query_source` is which rung of the composition ladder produced the query
— `agent_template` (the designer wrote it), `declared_contract` (composed
from the card's `finding_labels`), `undeclared` (nothing to compose
against, generic fallback), or `user_authored` (a human wrote it, never
overridden).

Written as **tags** as well as context because tags are queryable and
already render in the episode list and observatory, so the signal is
visible without building a view for it.

`invocation` is optional and `#[serde(default)]` on both execute
endpoints. curl, an older console, and any other orchestra keep working
unchanged.

### Callers cannot forge tags

The invocation record arrives from the caller, so `stamp_invocation`
slugs it: ≤64 chars, restricted charset, no whitespace. A test asserts a
caller cannot smuggle `status:success` onto a failed run or write
unbounded strings into an indexed column.

## Does the agent even accept a question?

The pipeline audit in v0.13.x found thirteen stages binding an agent to an
interface it never declared — `rabble_curator` handing `ar_beacon` a
`creature-record` when `ar_beacon` accepts `description`/`location`. The
console's research path could do the same thing: send free text to
whichever agent routing picked, having never checked the agent advertises
a free-text input at all.

`negotiate::bind_input` now checks, and reports three outcomes:

- **`Declared(label)`** — fine, and reported in the *designer's* own
  vocabulary.
- **`NoTextInput(declared)`** — a real mismatch. Warns on the driver,
  naming what the agent does accept.
- **`Undeclared`** — an absence, **not** a mismatch. `is_mismatch()`
  returns false. Silence must not read as contradiction, or the check
  earns the right to be ignored.

The corpus made the design decision. All twelve curated orchestra members
declare something question-shaped, but four don't call it `query`:

| Agent | `accepts` |
|---|---|
| `weather_oracle` | `forecast-question`, `market-question`, `evidence-set` |
| `macro_data_agent` | `country-code`, `country-list`, `indicator-request`, `factor-x1-query` |
| `fixture_context_agent` | `country-code`, `fixture-id`, `venue-list`, `factor-x6-query` |
| `football_institution_agent` | `country-code`, `country-list`, `confederation-query`, `factor-x2-query` |

A check that only recognised `query` would have flagged four correct
cards. Matching is on the shape of the word — `query`/`question`/`prompt`
— and a test pins each of those four as passing.

**This check is currently silent on the curated corpus.** All twelve
members pass. Its value is prospective: third-party agents admitted
through the database path, whose `accepts` nobody can predict.

## Also

- `GET /api/episodes/:id` returns `invocation`.
- `TagRenderer.color()` split on *every* colon, so any tag whose value
  contained one (`model:claude-3:latest`) lost its category colour. It now
  splits on the first only.
- Colours for the new namespaces. A matched `ibind` is deliberately
  neutral grey: only problems get colour.

## What this does not do

It reports; it does not act. Nothing gates admission on a declaration,
re-ranks routing by query source, or nags card authors. Those decisions
should be made once there is a number, not designed against a hypothesis
— see **#19**, which proposes the aggregate that produces the number.

Two caveats for whoever writes that query. `qsrc` only exists on runs from
this release onward, so historical episodes must be *excluded*, never
bucketed as `undeclared` — otherwise "before this change" masquerades as a
finding about card quality. And declared-contract agents are mostly
curated while undeclared ones are mostly third-party, so a raw split
partly measures "curated vs community"; the only clean comparison is a
single agent against itself across query sources.

## Gates

`cargo test --bin api-server` 139 passed · `cargo test -p fermi-console
--lib` 258 passed · `cargo check --bin api-server` clean ·
`cargo check -p fermi-console --bins` clean.

Wire shape verified end-to-end against a real third-party card
declaration.
