# Sobol conditioning on a binary driver is estimated from random draws, not its support

**Status:** bug, unresolved
**Affects:** `variance_decomposition` / `compute_conditional_variance` in `src/sensitivity.rs`
**Measured:** `a_dominant_binary_driver_does_not_saturate_to_one` fails **8 times in 80 runs (10%)**

## Symptom

A binary driver that genuinely dominates the model is reported as
contributing almost nothing, roughly one run in ten:

```
a dominant binary driver should not read as negligible (0.002862212183920921)
  src/sensitivity.rs:826
```

This surfaced as CI flake — a **docs-only** commit (`21d23493`, no code
change from the green `1b818ee5`) went red on this test alone. It is not
flake in the harness; it is a nondeterministic estimator.

## Why

`compute_conditional_variance` estimates `V(E[Y|X_i])` by sampling
`m = 20` values of `X_i` from the baseline and running `n` iterations at each:

```rust
let m = 20; // Sample 20 different values of each driver
let n = (iterations / m).max(100);
let driver_samples = sample_single_driver(program, driver_name, &mut baseline_executor, m)?;
```

That is reasonable for a continuous driver. For a **binary** driver it is not:
the 20 "different values" are 20 Bernoulli draws, and they are frequently all
the same value.

The test's driver is `regulatory_risk binary { probability: 0.15 }`:

| event | probability |
|---|---|
| all 20 draws are `0` | `0.85^20` ≈ **3.9%** |
| exactly one draw is `1` | `20 · 0.15 · 0.85^19` ≈ **13.7%** |

In the first case every conditioning group has an identical `E[Y|X_i]`, so
between-group variance collapses to sampling noise and the index reads ≈ 0.
In the second, the entire between-group signal rests on one group of ~100
iterations, which is enough to land under the test's `> 0.2` threshold a fair
share of the time. Measured combined failure rate: **8/80**.

So the estimator's precision depends on getting lucky with the draw of a
variable whose support has exactly two points.

## This is not only a test problem

`full_sensitivity_analysis` is what the console's Sobol panel renders. A user
running sensitivity on a forecast with a low-probability binary driver — a
regulatory event, a launch failure, a manager departure, exactly the drivers
binary is *for* — has roughly a 1-in-10 chance of being told the dominant
driver is negligible, and no way to tell that run apart from a correct one.

That is the same class of defect as the one `e383f20f` fixed
("first-order was dividing by a denominator from a different Monte Carlo run
and saturating at 1.000"): a plausible-looking number that is wrong, in a
panel people use to decide what to research next.

## Fix

Condition on the driver's **actual support**, not on draws from it. For a
binary driver that is two groups — `X_i = 0` and `X_i = 1` — and the correct
first-order term is the probability-weighted between-group variance:

```
E[Y|X=0], E[Y|X=1]
V(E[Y|X]) = p(1-p) · (E[Y|X=1] - E[Y|X=0])²
```

This is exact rather than estimated, cheaper (2 conditioning groups instead
of 20, so each can take 10× the iterations for the same budget), and
deterministic in the conditioning dimension. The same argument applies to
`DriverType::Discrete`: enumerate the declared values and weight them.

Sketch:

- `sample_single_driver` — for binary/discrete drivers return the support and
  its weights instead of `m` random draws.
- `compute_conditional_variance` — accept weighted conditioning groups and
  compute the between-group term as a weighted variance, rather than an
  unweighted variance over `m` equally-likely samples. The unweighted form is
  also subtly wrong today even when the draw is lucky: it weights each of the
  20 draws equally, which only coincides with the true weighting in the limit.
- Keep the continuous path as-is.

## Also worth doing

Seed the Monte Carlo. `variance_decomposition` runs several independent
simulations and nothing in `src/sensitivity.rs` sets a seed, so no
sensitivity result is reproducible and no test over it can be stable. A
caller-supplied seed (defaulting to random) would make both the panel and the
tests repeatable, and would have made this bug a deterministic failure rather
than a 10% one.

Note the fix above removes the nondeterminism that matters here; seeding is
the belt-and-braces that makes the *whole* analysis reproducible.

## Reproduce

```bash
cargo test --lib -p fermi a_dominant_binary_driver_does_not_saturate_to_one --no-run
for i in $(seq 1 80); do
  cargo test --lib -p fermi a_dominant_binary_driver_does_not_saturate_to_one --quiet 2>/dev/null \
    | grep -oE "result: (ok|FAILED)"
done | sort | uniq -c
```

Observed: `8 result: FAILED`, `72 result: ok`.

## Acceptance

- [ ] Binary and discrete drivers are conditioned on their support with
      probability weights, not on `m` random draws.
- [ ] `a_dominant_binary_driver_does_not_saturate_to_one` passes 100/100.
- [ ] A dominant binary driver's first-order index is stable across runs
      (bounded spread, not a coin flip).
- [ ] Optional: `full_sensitivity_analysis` accepts a seed.

## Context

Found while clearing the migration ratchet (#20), which had prevented the
test suite from running in CI at all since 2026-08-07. This test was
introduced inside that window, so its 10% failure rate has never been
visible.
