# BayesOps: Executive Summary for Business Partners

**What this is:** A plain-language explanation of a technical capability we are building,
why it matters, and where we are.

---

## The Problem in Plain Language

When we run a forecast — "what will this cultivation batch yield?" or "will this drug pass
its trial?" — we make assumptions about probability. We might say: "based on our experience,
there's roughly a 40% chance this batch yields over 5 kg." That number — 40% — comes from
somewhere. Right now, it mostly comes from a person's judgment.

That judgment is valuable. But it has a problem: **it doesn't automatically get more precise
as we run more batches.** The tenth batch doesn't make the eleventh forecast more accurate
unless someone manually updates their assumption. And it carries no signal about how confident
we should be — a 40% estimate based on 3 batches and a 40% estimate based on 300 batches look
identical in our system today.

BayesOps fixes this.

---

## The Core Idea: Let the Data Set the Numbers

Think of it like a weather forecasting service that starts from climatological averages but
updates its predictions as more sensors come online. On day one, it says "70% chance of rain
because it usually rains in March." By year three, with a hundred local weather stations, it
says "67% chance of rain because that's what our sensors in your specific microclimate have
recorded over 847 comparable days." The second forecast is both more accurate *and* more
honest about its uncertainty — it knows exactly how much data backs it up.

BayesOps does this for our forecasting system. Instead of a person typing in "40% chance of
exceeding 5 kg yield," the system:

1. Reads the history of actual batch outcomes
2. Computes the right probability distribution mathematically
3. Automatically makes the estimate wider (more uncertain) when data is sparse, and narrower
   (more confident) as data accumulates
4. Feeds that computed estimate into the forecast model as if a very careful analyst had done
   the work

---

## Why the Uncertainty Width Matters

This is the part that's easy to miss but critically important.

Consider two statements:
- "We expect 4.8 kg yield, somewhere between 3 and 7 kg" (based on 5 batches)
- "We expect 4.8 kg yield, somewhere between 4.2 and 5.4 kg" (based on 80 batches)

Both say the same expected value. But the second statement carries a fundamentally different
message for a business decision. If you're planning production capacity, the first range means
you might need to plan for wildly different scenarios. The second means you can commit with
confidence.

Current system: both statements would be represented identically — just the number 4.8 —
because the uncertainty isn't being tracked.

BayesOps system: the width of the uncertainty range automatically reflects how much evidence
you actually have. After 5 batches, the range is wide. After 80, it's tight. **The forecast
becomes more useful as you accumulate operational data, automatically.**

---

## The "What-If" Capability

Beyond just tracking uncertainty, BayesOps enables a qualitatively different kind of question.

Today, a SimOps operator can ask: *"What will this process yield?"*

With BayesOps fully deployed, they can ask: *"What would yield be if I ran at 160 kWh LED
instead of our usual 120 kWh — given everything we've learned from 30 real runs?"*

This is the difference between looking at average historical outcomes and running a
model-informed scenario analysis. The system has learned the relationship between inputs
(lighting, temperature, nutrients) and outputs (yield, carbon, cost) from real operational
data, and can extrapolate that relationship to conditions you haven't tried yet.

**Business analogy:** it's the difference between a financial model that says "our revenue
last year was $X" and one that says "if we increase sales headcount by 20%, revenue should
increase by Y% based on our observed conversion rates at different team sizes." The second
requires building and validating a model of the relationship, not just reading historical
averages.

---

## What We Have Now vs. What We're Building

### What exists today

The forecasting system (Fermi) already runs sophisticated probabilistic simulations. An
analyst specifies what they believe about each input — "lighting efficiency follows this
distribution, yield loss from contamination follows that one" — and the system runs tens of
thousands of simulations to produce a full probability distribution over outcomes. This is
genuinely good. It's better than point estimates. The Brier scoring system already measures
forecast accuracy over time.

The gap is in the *inputs to the model*: the analyst's beliefs. They are manually entered,
don't automatically update from operational data, and don't carry calibrated uncertainty.

We have also just deployed (June 2026) a system called the Projection Scoring Evaluator that
automatically measures how accurate our process models are: every time a real batch completes,
it compares what we predicted to what actually happened and records the error. This is the
measurement infrastructure that BayesOps will read from.

### What BayesOps builds on top

BayesOps is the layer that takes that measurement history and turns it into better probability
estimates for the next forecast. It has two parts:

**Simple fitting (Phase 1):** Given a history of batch outcomes, compute the right probability
distribution with calibrated uncertainty. This is mathematically straightforward but not
currently automated. A 1-week engineering task that requires no changes to any existing
forecasting infrastructure.

**Conditional fitting (Phases 2–3):** Given a history of batch outcomes *and their input
conditions*, build a model that can predict yield at input conditions you haven't tried.
This enables the what-if scenario modeling. More sophisticated, but still architecturally
clean — it builds a new capability without altering existing tools.

**Language integration (Phase 5):** Allow forecasters to write `data_driven("yield")` in a
forecast model instead of `Beta(0.41, 0.59)`. The system resolves `data_driven("yield")`
automatically by looking up the fitted distribution from your operational history. This is
the final step where the manual work of updating estimates disappears entirely.

---

## The Timeline

We are currently at **Gate 0**: the measurement infrastructure (Projection Scoring Evaluator)
needs to be deployed and accumulate its first real batch cycle. This is operational work,
not new engineering.

After that:

| Phase | What it delivers | Estimated duration |
|---|---|---|
| Phase 1 | Automated uncertainty-calibrated base rate estimates | 1 week |
| Phase 2–3 | What-if scenario modeling from fitted input-output relationships | 3 weeks |
| Phase 4 | SimOps process model selection informed by historical accuracy | 1 week |
| Phase 5 | `data_driven()` in forecast language — manual entry replaced | 1 week |

**Total: approximately 6–7 weeks of engineering work, sequenced over roughly 2–3 months
to allow validation at each stage.**

The critical path dependency is real operational data. Phases 1–4 become more useful as
more batches complete. Phase 5 is where the system becomes largely self-updating.

---

## Why This Matters Commercially

Three customer-facing properties improve:

**1. Forecast accuracy improves automatically.** Today, a customer who runs 50 batches
gets the same forecast quality as a customer who just started, unless an analyst manually
updates the model. With BayesOps, accuracy improves as a direct function of operational
volume. This makes the platform more valuable the longer customers use it — a genuine
network effect within each customer's own data.

**2. Decisions become defensible.** "We recommend running at 145 kWh because our model,
fitted to 40 real runs, gives a 73% probability of exceeding the target yield at that
setting — with a 90% confidence interval between 68% and 78%" is a different kind of
recommendation than "we think 145 kWh is a good setting." The first is auditable, traceable
to data, and carries an honest statement of uncertainty.

**3. The platform learns from itself.** Each batch that completes improves the next forecast.
This creates a compounding advantage for customers who use the system operationally over time
versus those who use it episodically for one-off analysis. It is the difference between a
tool and a platform that gets smarter with use.

---

## What We Are Not Claiming

It is important to be precise about what BayesOps does and does not do.

**It does not train AI model weights.** It updates probability estimates from data using
statistical mathematics that has been standard in science and engineering for decades. This
is not machine learning in the deep-learning sense — it is Bayesian inference, a
well-understood and auditable method.

**It does not replace domain expertise.** The system learns the relationship between inputs
and outputs from operational data. If the operational conditions change significantly —
a new strain, a new process step, a new facility — the historical data may not apply and
an expert must evaluate whether the model's learned relationships still hold.

**It does not produce certainty.** It produces *calibrated uncertainty* — probability
distributions whose width honestly reflects how much evidence you have. A system with
5 data points will give wide ranges. That is correct behavior, not a failure.

**The quality of the output depends on the quality of the input data.** Systematic
measurement errors in batch observations will produce systematically biased posteriors.
Garbage in, garbage out — but the uncertainty width will signal when something is wrong,
because poor data produces wide, inconsistent distributions.

---

## Summary in One Paragraph

BayesOps is the layer that connects our operational batch history to our forecasting models.
Today, a human manually estimates probabilities and enters them into forecasts. After BayesOps,
the system reads the history of real batch outcomes, computes the right probability distribution
with uncertainty proportional to how much data exists, and feeds those computed estimates
automatically into forecasts. The more batches a customer runs, the more accurate and confident
their forecasts become — automatically, without analyst intervention. The advanced capability
allows the system to answer "what would happen if we changed this input?" — not just "what
has happened historically" — by learning the input-to-output relationship from operational data.
The engineering work is approximately 6–7 weeks, sequenced to validate at each stage against
real operational data before proceeding.

---

*Document prepared June 2026. Technical specification: `docs/specs/14_BAYESOPS_SPEC.md`.*
