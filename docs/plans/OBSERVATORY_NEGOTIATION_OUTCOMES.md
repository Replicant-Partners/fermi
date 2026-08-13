# Aggregate outcome by invocation provenance

**Status:** proposed
**Depends on:** v0.16.0 (`stamp_invocation`, `qsrc:*` / `ibind:*` episode tags)

## Why

v0.16.0 made every research run record *how the agent was asked* alongside
how it did. Episodes now carry:

| Field | Values |
|---|---|
| `qsrc:*` tag | `agent_template`, `declared_contract`, `undeclared`, `user_authored` |
| `ibind:*` tag | `declared-<label>`, `no_text_input`, `undeclared` |
| `recomposed:true` tag | set when a stale pre-fill was re-composed |
| `context.invocation.declared_label_count` | integer |

Outcome was already recorded per episode: `execution_status`,
`error_details`, `confidence`, `tokens_used`, and — once the forecast the
run fed resolves — a Brier score.

Both halves of the join now exist. Nothing joins them.

That matters because the platform's premise is composing fleets across
heterogeneous providers and designers, and *learning from those
interactions which agents and compositions are effective*. Right now that
learning would run on outcome alone, which cannot separate two very
different failures:

1. the agent was asked properly and is bad at the job;
2. the agent was sent the wrong shape of question.

An adaptation loop that cannot tell those apart will blame agents for
caller mistakes, and will drift toward preferring the agents the caller
already knows how to talk to. That is the closed world regenerating itself
through the reward signal — the same failure v0.16.0 removed from the
query path, reintroduced one layer up.

## Proposal

`GET /api/observatory/negotiation-outcomes`

Group resolved episodes by invocation provenance and report outcome per
group.

```
?since=2026-08-01        # default: 30d
&agent_id=<optional>     # scope to one agent
&group_by=query_source   # query_source | input_binding | declared_label_count
```

Response shape:

```json
{
  "since": "2026-08-01T00:00:00Z",
  "total_episodes": 412,
  "excluded_no_record": 1180,
  "groups": [
    {
      "key": "declared_contract",
      "episodes": 231,
      "success_rate": 0.91,
      "mean_confidence": 0.72,
      "parse_rate": 0.88,
      "mean_brier": 0.14,
      "resolved_forecasts": 37
    },
    {
      "key": "undeclared",
      "episodes": 44,
      "success_rate": 0.61,
      "mean_confidence": 0.51,
      "parse_rate": 0.39,
      "mean_brier": 0.27,
      "resolved_forecasts": 6
    }
  ]
}
```

`parse_rate` is the interesting one and needs defining: the fraction of
runs whose reply yielded at least one labelled finding. That is the
mechanism by which a badly-posed question becomes a useless answer, and it
is measurable without waiting for resolution.

## The question it should answer

> Does an agent that declares its contract produce better evidence than one
> that doesn't — and by how much?

If yes, the platform has an evidence-backed reason to require declarations
rather than a stylistic preference, and agent authors get a number instead
of a lecture. If no, the negotiation machinery is ceremony and should be
simplified.

Either answer is worth having. That is the point of measuring it.

## Correctness constraints

1. **Absence is not a category.** Episodes predating v0.16.0 have no
   invocation record. They must be *excluded* and counted in
   `excluded_no_record`, never bucketed as `undeclared` — otherwise "before
   this change" masquerades as a finding about card quality. This is the
   single most likely way to get a confidently wrong answer here.

2. **Report n, and refuse to imply significance without it.** Early on
   every group is small. Suppress or flag groups below a floor
   (~20 episodes) rather than serving a ratio computed on three runs. The
   `calibration` module already does Wilson intervals for exactly this
   reason — reuse it rather than emitting bare point estimates.

3. **`user_authored` is not a control group.** A human writes a custom
   query precisely when the default looked wrong, so that cohort is
   selected for hard drivers. Do not treat it as a baseline.

4. **Confounded by agent identity.** Declared-contract agents are mostly
   curated; undeclared ones mostly third-party. A raw split partly measures
   "curated vs community", not declaration quality. Support
   `?agent_id=` so a single agent can be compared against itself across
   query sources, which is the only clean comparison available.

5. **Don't double-count the fan-out.** One forecast stages one run per
   driver; five runs from one forecast are not five independent
   observations of anything. Report `resolved_forecasts` alongside
   `episodes` so the reader can see the effective sample size.

## Not in scope

Acting on the result. This endpoint reports; it does not gate admission,
re-rank routing, or auto-nag card authors. Those are separate decisions
that should be made once there is a number, not designed against a
hypothesis.

## Acceptance

- [ ] Endpoint returns groups with the fields above, RBAC-gated like the
      rest of `/api/observatory`.
- [ ] Episodes without an invocation record are excluded and counted
      separately; a test asserts they are never bucketed as `undeclared`.
- [ ] Groups below the sample floor are flagged, with intervals not just
      point estimates.
- [ ] `?agent_id=` scoping works, for the within-agent comparison.
- [ ] Observatory panel renders it as a table; no new chart needed.

## Wait for data

Worth building once there are enough post-v0.16.0 episodes to be
non-noise. Building it immediately produces a page of `n=2` ratios, which
is worse than no page because it will get screenshotted.
