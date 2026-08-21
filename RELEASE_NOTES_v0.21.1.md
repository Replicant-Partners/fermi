# v0.21.1 — the retry that never ran, the check that could not see, and a label read one unit too narrow

v0.21.0 shipped a base-rate agreement check and a driver-assignment gate. This
release is what happened when both met a live forecast: **the check was inert on
every real response, and three of four agent runs died with the recovery path
skipped.**

One report — *"the agent toast is still not triggering and I still see this 12%
bogus number"* — three defects, one of them introduced by the previous release.

---

## The forecast that found them

Houston, 2026-08-21: *Will the lowest temperature be between 76-77°F?*

| source | P(bucket) |
|---|---|
| the forecast's declared base rate | **12.0%** |
| the climatology it stated (mean 75°F, sd 2.5°F), integrated over [75.5, 77.5) | **26.4%** |
| `weather_oracle`, 330 ERA5 observations at KHOU, trend-adjusted | **32%** |
| Polymarket crowd | 27.5% |

Every driver is a multiplier on the base rate, so the error scaled through the
whole model. The console reported the resulting 15.5pp gap to the market as a
possible edge. The market was right and the forecast was wrong by a factor of
about 2.5.

---

## 1. Three of four runs died and the recovery path was skipped

The non-streaming retry was gated on `else if let Some(err) = stream_error`, so
it ran **only** when the server had sent an explicit `error` event. When a stream
merely *ends* — connection dropped, proxy timeout, server close — both
`final_result` and `stream_error` are `None`, and the third arm returned a bare
error without ever attempting the recovery written for exactly that case.

That is not the rare path. From one forecast's activity log:

* `weather_oracle` on `august_21_date_specificity`, running **alone**: 117s,
  completed, full findings, suggestion accepted.
* three more fired 19s apart: all failed `"SSE stream ended without complete
  event"` at the **same wall-clock instant**, at 44s, 52s and 63s elapsed.

Same instant, different elapsed times — a shared drop, not a per-request
timeout. The fallback was skipped on all three, so three paid-for runs produced
nothing and the operator saw drivers that never moved and no toast.

The retry now runs whenever there is no result. Both error paths name the SSE
failure alongside their own, so a transport failure is distinguishable from an
agent failure.

---

## 2. The check shipped in v0.21.0 was inert on every real response

`extract_measured_base_rate` walked `Value::Object` and `Value::Array` and never
descended into a `Value::String`. The agent's document does not arrive as a
nested object — it arrives as **text** in `metadata.reasoning`, which is why
`apply_base_rate_only` has always used `serde_json::from_str` on that field.

So the extractor returned `None` on every real response and the comparison
silently never ran. `weather_oracle` reported
`stages.calibration.climatology_base_rate = 0.32` against a declared 12.0% — a
20pp disagreement in the term that *is* the forecast — and nothing said a word.

An inert check is not a passing one. This is precisely the failure mode the
check exists to catch, one level up, and it was introduced by the release that
added the check.

It now descends into embedded documents, tolerating a ```json fence and
surrounding prose, bounded at two levels of nesting. Pinned with a test built
from the real production response shape.

---

## 3. A bucket label is an integer set, and no card said so

`"76-77"` denotes the integer SET `{76, 77}` — the half-open interval
`[75.5, 77.5)`, **two** units wide. The agent's own reasoning was internally
contradictory: it said *"a 2°F band (76-77°F)"* and returned the one-degree
answer.

The rule was stated correctly in `examples/reference_bucket_indicator_kord.fpl`,
in `examples/weather_spawn_plan.rs` and in two test files — and in **no agent
card at all**. Nine curated cards declare a `[BASE RATE]` finding label; none
mentioned bucket bounds. The agents doing the counting read cards, not examples.

Added to `fermi`, the generalist that decomposes every question and produced the
12%, with the measurement attached — a rule with a number is one an agent can
check itself against. Added to `weather_oracle`, which already derives the
bounds correctly, so that stays true rather than being rediscovered each run.

Spliced as raw text rather than `json.load`/`dump`: a reserialise rewrites all
140 lines and buries a one-paragraph change in an unreviewable diff. **Both
files show one line changed.** Disk prompts were hashed against the deployed
`agents` rows before editing, so a re-seed deploys exactly this.

`tests/bucket_bounds_contract.rs` asserts the two covered cards state the rule
*and* that every other `[BASE RATE]` producer is explicitly listed with a
reason. A base-rate card that is neither covered nor deferred fails the build —
the shape `CROSS_CHECK_EXEMPTIONS` uses. `equity_analyst` is the one to watch:
an EPS or revenue band has the same shape as a temperature band.

---

## Also in this release

A generalist that answers a question a specialist declares now says so. The log
showed the domain detected as `climate` and `fermi` producing the base rate
seconds later. Decomposition is `fermi`'s job — it produces drivers, model and
base rate together and no specialist does that — but that leaves the
generalist's estimate in the one place a wrong number does the most damage.
Reported rather than re-routed: firing the specialist would spend credits the
operator has not agreed to, and the decomposition flow's contract is that
nothing runs until asked. The message names the button that does it.

Three commits in this range are from concurrent work on the verification ladder
and feedback loops, described in their own commit messages rather than
paraphrased here: `014e0a58` (the gates were declared, not enforced),
`ba398849` (a proposal is a delta, and dreaming is opt-in), and `dc39df72`
(the exemption list shrank, because the check ran).

---

## Deploying this

The console fixes ship in the binary. **The card fixes do not** — a card change
takes effect only when the cards are re-seeded into the `agents` table.

`seed_agents_to_database` runs at api-server boot and upserts with
`ON CONFLICT (agent_name) DO UPDATE SET … system_prompt = EXCLUDED.system_prompt`,
so a deploy of `main` is sufficient. There is no separate migration step and no
manual `Cargo.toml` bump: the release workflow stamps the console version from
the tag.

---

## Verification

```
fermi-console lib            330
api-server                   197
bucket_bounds_contract         5  (new)
base_rate_provenance           4
declaration_contract           4
agent_block_roundtrip          7
reference_forecast_contract    5
```

**Known red, and not from these changes:** `cargo test -p fermi --lib` fails on
`test_all_migrations_registered`. `migrations/212_composition_delta.sql` landed
in `ba398849` without being registered in `run_migrations()`, so the migration
will never apply. Left alone deliberately — registering another author's
migration mid-work is a destructive guess about intent, and the failing test is
the correct signal. It does not block the console release build, which does not
run tests.
