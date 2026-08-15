# 34 — Provider heterogeneity for platform agents, via an OpenRouter gateway credential

**Status:** investigation / not yet a proposal · **Author:** design session
**Depends on:** `AGENT_CREDENTIAL_MODEL.md` (accepted 2026-08-03),
`AGENT_CREDENTIAL_MODEL_ROLLOUT.md` (P0–P3 shipped),
`SPEC_28_UNIFIED_CREDENTIAL_PATH.md` (P5, landed),
`docs/plans/PLATFORM_ECONOMICS.md` (v1 shipped 2026-08-12)

**Question being answered:** system agents are now owned by `abw-system` and
funded from the credential store, but they can only actually reach one provider.
Should we give `abw-system` an **OpenRouter key as a gateway credential**, so
system agents can run on *heterogeneous* providers — with a configurable default
model per agent (Anthropic, GLM, …)?

---

## 0. TL;DR

1. **The ask is provider heterogeneity, not portability and not cost-shifting.**
   One credential that reaches many models, so each system agent can run on the
   substrate its job actually needs. Call it **C**, and keep it separate from two
   adjacent features it is routinely confused with (§1).

2. **C is a strong idea and the platform cannot do it today.** Not "needs work" —
   cannot. The Anthropic path hardcodes `key_for("anthropic")`, there is no
   model-name translation, the model ladder never executes outside one handler,
   and — the structural one — **there are two separate provider registries with
   different provider sets**, so an agent's reachable substrate depends on which
   code path invoked it. `openai` works for dreaming but not execution; `glm`
   works for execution but not dreaming (§3.3).

3. **Modelled margin, portfolio right-sizing rather than a single swap:**
   uniform-Sonnet fleet **+$11.12/Mtok** → heterogeneous fleet **+$16.10/Mtok**
   (+45%). The p5 of the heterogeneous case (+$11.30) sits *above the mean* of the
   status quo, so the conclusion survives its own error bars (§5).

4. **You were right and I was wrong about unit cost economics.** Per-episode
   `cost_usd` is wired, per-model, and persisted — that is what the green circle
   shows. What is *not* wired is cost → **price** coupling: `execution_fee` never
   reads cost, provider, or model. Correction and consequences in §4.

5. **The one hard blocker specific to C:** the rate card knows **eight Anthropic
   model strings and nothing else** (`registry.rs:382-394`, `_ => 3.0`). Every
   GLM / DeepSeek / Kimi / Gemini run would be recorded at $3/Mtok against a real
   ~$0.9. **The economics surface would argue against heterogeneity precisely
   when heterogeneity is working.** Fix the rate card before, not after (§4.3).

6. **Governance finding:** because `execution_fee` is token-based, a weaker model
   that fails a structured-output contract and retries produces *more* billable
   tokens. Margin therefore **rises** with retry inflation — Sobol confirms a
   positive 15% variance contribution. The platform is insulated from its own
   quality regressions and users pay for them. Visible in your screenshot: two
   `FAILURE` runs, $0.93 of recorded cost, charged (§5.3).

---

## 1. Three features, routinely conflated

Funding today is a pure function of the agent (`api_server.rs:5241-5247`):

```rust
pub(crate) fn funding_principal_for(agent: &Agent) -> Option<String> {
    if is_platform_funded(&agent.tier) { Some("abw-system".to_string()) }
    else { agent.owner_id.clone() }
}
```

| | What changes | Funding principal | Blast radius |
|---|---|---|---|
| **A — invoker-funded BYOK** | *who pays* | `f(agent)` → `f(agent, invoker, policy)` | High: pricing, royalty, liability, eval |
| **B — vendor portability** | ability to *switch* vendor wholesale | unchanged | Low |
| **C — provider heterogeneity** ← **the ask** | *how many substrates are reachable at once* | unchanged | Low–medium |

C is not a credential feature at all. The store already expresses it:
`(abw-system, openrouter, '*')` is one row. **C is a model-resolution feature.**
That is where all the work is, and it is why §3 is the long section.

### 1.1 Why the gateway framing is right

Going heterogeneous *natively* means eight vendor accounts, eight credential
rows, eight billing relationships, eight rate-limit regimes, eight sets of
quirks. `provider_base_url` (`multi_model_executor.rs:54-70`) already anticipates
seven. OpenRouter collapses that to **one credential and one base URL**, and
throws in provider-level fallback — which is P4 graceful degradation, a
fast-follow that has now missed two rollouts.

For a platform whose differentiator is *composition* rather than inference, that
trade is clearly correct: heterogeneity becomes a config property of an agent
card instead of an integration project per vendor.

### 1.2 What C buys, concretely

Today **74 of 100 curated cards run Sonnet** and 22 run Haiku, by declaration
rather than by fit (`jq` census of `capabilities.model`). The fleet does
genuinely different work:

| Work | Agents | Substrate it actually needs |
|---|---|---|
| Structured extraction | `ontologist`, evaluators, scanners | small, fast, reliable JSON |
| Narration / prose | `dream_narrator`, `simops_narrator` | mid-tier |
| Reasoning / synthesis | `fermi`, `xaman_ek`, advisors, oracles | frontier |
| Coordination / routing | coordinators, `keeper`, `navigator` | small–mid |
| Long-context ingestion | research agents | Gemini / Kimi class |

C is what lets that table become real. It also unlocks capability the platform
cannot reach at all today: long-context models, non-English strength (GLM), cheap
reasoning (DeepSeek).

---

## 2. Where we stand (verified)

| Fact | Evidence |
|---|---|
| Store is `(principal_id, provider, scope)`; one `*` default + N per-agent | `migrations/171_agent_credentials.sql` |
| Executors are credential-stateless; keys arrive per-execution | `credentials.rs`, `SPEC_28` §4.4 |
| Platform-funded = `system` ∪ `curated` = **100 cards** (not 15) | `credentials.rs:179-181` |
| 15 cards are `tier: "system"`; 85 `curated`, owned by the admin account | card census; `migrations/111_…` |
| Declared models: 74 Sonnet, 22 Haiku, 2 `gpt-4o-mini`, 1 Ollama, 1 `deterministic`. **No Opus.** | `jq` census |
| Ladder rungs: 190 anthropic, 92 openrouter, 4 openai, 1 ollama | rung census |
| OpenRouter is a wired OpenAI-compatible provider and store-resolvable | `multi_model_executor.rs:58`, `funding_parity_tests.rs:207` |
| `execution_fee = max(1, tokens/1000)` +10% gas — **model- and provider-blind** | `gas.rs:135-140` |
| Per-episode `cost_usd` **is** per-model and persisted (since 2026-08-12) | `api_server.rs:5942`, `PLATFORM_ECONOMICS.md` §3 |

---

## 3. Why C is impossible today — six blockers

Each is a defect independent of this spec.

1. **The Anthropic path cannot be pointed anywhere else.**
   `llm_executor.rs:365` and `tool_executor.rs:113` call `key_for("anthropic")`
   *literally*; `tool_executor.rs:417` stamps `provider: Some("anthropic")`.
   97/100 cards declare `anthropic`. **This is the blocking defect** — the
   busiest execution path is hard-wired to one vendor, so "heterogeneous" is
   unreachable for ~all traffic.

2. **No model-name translation.** `ModelRung.model` is copied verbatim to the
   wire (`agent_card.rs:399` → `tool_executor.rs:159`). `claude-sonnet-4-6`
   404s at OpenRouter; `anthropic/claude-sonnet-4-6` 404s at Anthropic. The UI
   already offers both conventions (`handlers/agents.rs:938`), so cards can
   already contain unreconcilable ids.

3. **There are TWO provider registries and they support different providers.**
   This is the structural blocker for heterogeneity, and it is worse than a
   missing arm.

   | | Registry 1 — `ProviderType` | Registry 2 — `OPENAI_COMPATIBLE_PROVIDERS` |
   |---|---|---|
   | Defined at | `agent-bestiary/memory/src/llm.rs:198` | `multi_model_executor.rs:73` |
   | Used by | consolidation / dreaming (`build_extraction_llm`, `consolidation.rs:54`) | agent execution (`execute_agent`, tool loops) |
   | anthropic, mistral, qwen, openrouter, deepseek, kimi | ✅ | ✅ |
   | **openai** | ✅ | ❌ |
   | **glm** | ❌ | ✅ |
   | ollama | ❌ | ✅ |
   | gemini | ❌ | ❌ (despite `GEMINI_API_KEY` bootstrapped at `api_server.rs:2049`) |

   So **an agent's reachable providers depend on which code path invoked it.**
   `ontologist` (`provider: "openai"`) works when the dream cycle calls it and
   fails with `Unknown provider: openai` via `execute_agent`. Symmetrically,
   setting any agent to **`glm` works for execution but silently disables
   dreaming** — `build_extraction_llm` returns `None` and consolidation falls
   back to pattern-based extraction without an error.

   This is the same class of defect SPEC_28 closed one level up: there, *funding*
   depended on the shape of the output; here, *capability* depends on the caller.
   Adding a provider currently means editing two lists in two crates, and they
   have already drifted in both directions.

4. **The model ladder never executes.** `cognition_tier` is `None` at every
   `ExecutionContext` construction site except `rabble_workspace.rs:261`, so
   `apply_tier_resolution` (`agent_card.rs:386`) is dead on the API, MCP, eval,
   workspace and delegation paths. **The mechanism C assumes is not running.**
   Compounding it, `resolve_agent_card` (`api_server.rs:5513-5522`) overwrites
   `provider`/`model` from the DB row but does *not* bridge `model_ladder` — so
   ladder and effective model come from two different sources of truth.

5. **`openrouter/free` is not a real model id**, and it is the free rung in 92
   of 97 ladders. It survives only as a price-table constant
   (`registry.rs:391`). Any request resolving to it 400s.

6. **No graceful degradation, and status codes are discarded**
   (`multi_model_executor.rs:223-229`, `tool_executor.rs:743-749`). 401, 429,
   quota and 5xx are indistinguishable. `min_tier` / `min_provider_class` /
   `capability_gates` are never read at execution time.

### 3.1 Behavioural divergence is a C-specific problem

The same card on two substrates is **two different agents**:

- `LLMExecutor::build_system_prompt` (`llm_executor.rs:65-89`) prepends a 7-rule
  `HELPFULNESS_PREAMBLE`. The OpenAI-compatible paths
  (`multi_model_executor.rs:124-136`, `tool_executor.rs:443-447`) do **not**, and
  use different fallback prompts. `execute_openai_compatible:130-136` force-appends
  its own JSON envelope instruction that the Anthropic path skips.
- `extended_thinking` and `top_k` exist only on `ClaudeRequest`
  (`llm_executor.rs:445-463`) and are silently dropped, while the forced
  `temperature = 1.0` they imply is **kept** (`agent_card.rs:426-432`).

For A this is a fairness problem. For **C it is a correctness problem**, because
heterogeneity is the *point* — you will be running the same agent on multiple
substrates deliberately and comparing results. **Prompt and sampling
construction must be unified across executor paths before heterogeneity is
switched on**, or every A/B you run measures the executor branch rather than the
model.

---

## 4. Correction: unit cost economics *are* wired

I got this wrong in the first pass and your screenshot is the evidence.

**What is wired.** `episodes.cost_usd` is computed per execution from
`(provider, model, tokens)` via `registry::calculate_cost` and persisted
(`api_server.rs:5942`). `PLATFORM_ECONOMICS.md` v1 shipped 2026-08-12 with an
admin surface at `GET /api/admin/economics/platform`, attribution by the funding
principal *recorded at execution time*, a `cost_basis` block on every response,
an uncollapsible caveat banner, and a mutation-tested smoke suite. The
`$0.616272` in the green circle is a real per-model estimate, not a placeholder.
`economics.rs:16-43` is candid about its own error bars. This is better
instrumented than most platforms at this stage.

**What is not wired: cost → price.** `execution_fee(tokens)` (`gas.rs:135`) takes
one argument. It never reads `cost_usd`, `provider`, or `model`. So cost is
*observed* but never *acted on*: no per-execution margin, no pricing response, no
refusal on an unprofitable run. Revenue and cost meet exactly once, in an admin
report, at an assumed 1.5¢/credit.

The precise gap is **reconciliation, not observation.** My earlier framing
("not wired") was wrong; "measured but inert" is right.

### 4.1 Three biases in the number in the green circle

1. **No input/output split** — `tokens_used` is one number; Sonnet is $3 in /
   $15 out. At a 20% output share the true blended rate is ~$5.4/Mtok, so
   `$0.616272` is likely **~1.8× understated**. Your own doc already flags this
   as the top remaining error (`PLATFORM_ECONOMICS.md` §4.1) — agreed, and it is
   also the cheapest to fix, since both providers return the split.
2. **`_ => 3.0` fallback** — any model string not in the eight-arm match prices
   at $3/Mtok. A Haiku id with a different suffix is **12× overstated**.
3. **`provider_used` is a heuristic on the model string**
   (`api_server.rs:5968-5984`), even though `AgentMetadata.provider` already
   carries the authoritative value. A Claude model served *via* OpenRouter is
   labelled `anthropic`; an OpenRouter-namespaced Claude id is labelled
   `openrouter`. **Provider attribution is wrong for exactly the proxied case C
   introduces.**

Also note both rows in your screenshot are `FAILURE` and still carry cost — and
are still charged. Failures burn real money at full price. See §5.3.

### 4.2 The actual requirement: cost per *resolved forecast*

The requirement is not margin. It is **cost per agent execution, summable across
every execution that contributed to a forecast, so it can be divided into a
resolved Brier score.** Dollars per unit of accuracy — the only metric that says
whether a substrate is worth what it costs. Three gaps block it:

1. **Per-execution cost is wrong for non-Anthropic models** (§4.3). This is the
   live case, not a hypothetical — see §4.5.
2. **There is no correlation id from an episode to a forecast.** Already
   documented, honestly, in `migrations/193_route_provenance_outcomes.sql`:
   *"`episodes` (which carries the route tags) and `forecast_agent_claims` (which
   carries the quantitative claim, and via `forecast_id` the Shapley credit) are
   written by the same execution but share no correlation id. The episode is
   persisted by the execution handler; the claim is written by a `tokio::spawn`
   in the multiplier hook. Neither knows the other's primary key."* The join is a
   heuristic on `(agent_id, driver)` within a time window. **A heuristic join is
   not a costing basis** — it will silently attribute one forecast's spend to
   another.
3. **Delegated executions are not folded in.** Sub-agent tokens never reach the
   parent's `tokens_used` (`tool_executor.rs:412`), and the delegation path has no
   charge call. For compound agents — which is how forecast research actually runs
   — the recorded cost is the orchestrator's own tokens only.

So cost-per-Brier-point is **not computable today**, and the missing piece is a
correlation id, not a cost model.

> **Status: gaps 1 and 2 implemented.** Migration 195 adds
> `forecast_agent_claims.episode_id` — the follow-up mig-193 explicitly asked
> for — minted by the handler *before* the claim hook is spawned, because the
> two writes race and the claim usually lands first. `route_outcomes` now joins
> exactly when the id is present and falls back to the mig-193 window for
> historical rows, reporting which via a new `join_method` column; the four
> dependent views inherit the fix untouched. New view
> `forecast_cost_attribution` gives cost per forecast and `usd_per_brier_point`,
> counting only spend that has **both** a measured cost basis (mig-194) and an
> exact join, and reporting `unattributed_cost_usd` / `unlinked_claims` rather
> than hiding them.
>
> **The subtle bug this surfaced:** `apply_agent_multipliers` writes one claim
> **per driver prefix**, so a naive forecast→claim→episode join multiplies an
> execution's cost by its driver count — inflating exactly the broad-coverage
> agents that cost most, while looking entirely plausible. The view
> de-duplicates to `DISTINCT (forecast_id, episode_id)` first.
> `scripts/smoke_cost_attribution.sh` pins it: mutation-tested by removing the
> DISTINCT, which reports $0.27 against a true $0.09 and is caught by DEDUP-001.
>
> **Status: gap 3 implemented.** It was worse than "tokens aren't folded in":
> the delegation tools ran the child, read its `reasoning` and `evidence`, and
> **discarded the whole `AgentOutput`**. No episode was written, so a delegated
> run's cost did not exist — it was absent, not mis-attributed.
>
> Migration 198 adds `episodes.parent_episode_id`, and each delegated run now
> writes its **own** episode via the shared `agent_output_to_episode`
> constructor (moved to `src/episodes.rs` so lib and bin share one). Chosen over
> folding tokens into the caller because folding makes the parent's total right
> and destroys the attribution — you could not say which member cost what, nor
> credit or pay it. Since per-agent attribution *is* the marketplace premise,
> each agent keeps its own row.
>
> Consequence to internalise: **a compound execution's cost is the sum over the
> tree, never the root row.** `forecast_cost_attribution` walks it with a
> `WITH RECURSIVE` descent, because a delegated child has no claim of its own
> and can only reach a forecast through its nearest claiming ancestor.
>
> Two bugs caught by testing rather than review, both of which would have
> shipped:
> 1. A naive forecast→claim→episode join multiplies cost by driver count
>    (claims fan out one row per driver). Mutation-tested: removing the
>    `DISTINCT` reports $0.27 against a true $0.09.
> 2. Appending a column to `route_outcomes` **breaks the second boot.**
>    `run_migrations()` keeps no applied-state table — every file re-runs every
>    boot — so migration 193 recreates that view without the new column and
>    Postgres refuses: *"cannot drop columns from view"*. 197 therefore keeps
>    193's column list byte-identical and changes only the JOIN. The smoke test
>    now replays the whole sequence three times.
>
> **Still unpriced.** Delegated spend is now *measurable*; no charge is raised
> for it. Whether it should be is the open pricing question — measuring first is
> deliberate.

### 4.2.1 Why this matters more for C than for A

For A, imperfect cost data makes pricing hard. For C, it makes **evaluation
impossible** — you cannot tell whether heterogeneity worked.

### 4.3 The blocker: the rate card is Anthropic-only

`registry.rs:382-394` in full: eight Anthropic model strings, `openrouter/free`
(not a real id), and `_ => 3.0`. **Zero real non-Anthropic models.**

Consequence under C:

| Model | Real blended | Recorded | Error |
|---|---|---|---|
| GLM-4.6 | ~$0.9/Mtok | $3.0 | **3.3× overstated** |
| DeepSeek-V3 | ~$0.5/Mtok | $3.0 | **6× overstated** |
| Kimi / Gemini Flash | ~$0.4/Mtok | $3.0 | **7× overstated** |
| Haiku (matched) | ~$0.6/Mtok | $0.25 | 2.4× understated |

So after moving `ontologist` to GLM, the Economics tab would show its cost
*rising* against Haiku and roughly flat against Sonnet. **The instrument would
report failure while the change succeeded**, and the natural response would be to
revert. Combined with §4.1.3, the economics surface breaks exactly when
heterogeneity lands.

**Requirement:** make the rate card a **data table keyed on `(provider, model)`
with input/output rates**, seeded from config rather than a `match` arm, with an
explicit `unknown_model` bucket that is *counted and surfaced* rather than
silently defaulted to $3. That is a small change and it is the difference between
C being measurable and C being a matter of opinion.

> **Status: implemented.** `src/agent_backend/rate_card.rs` — rate table keyed
> `(provider, model)` with separate input/output rates, prefix matching for dated
> snapshots, proxy-aware pricing (`openrouter:anthropic/…` resolves to the
> upstream vendor's row plus uplift), `RATE_CARD_PATH` JSON override so prices
> change without a deploy, and a `CostBasis` on every estimate. 16 unit tests,
> including regressions pinning the DeepSeek and Haiku-4.5 bugs. `AgentOutput::cost()`
> is now the single pricing entry point; `registry::calculate_cost` delegates to
> it so the two tables cannot drift again. Migration 194 persists
> `input_tokens`, `output_tokens`, `cost_basis`, `cost_rate_key` so cost is a
> **derived, correctable** quantity. `provider_used` now reads the authoritative
> `AgentMetadata.provider` instead of guessing from the model string.

### 4.5 This is live, not hypothetical — worked from a real execution

`efra_critical_factor` runs on **DeepSeek**. Two executions in its history:

| Recorded | Implied tokens at the $3/Mtok default | Real DeepSeek blended (~$0.44/Mtok) | Error |
|---|---|---|---|
| `$0.616272` | 205,424 | ~$0.090 | **~6.9× overstated** |
| `$0.311628` | 103,876 | ~$0.046 | **~6.9× overstated** |

DeepSeek matches no arm in `calculate_cost`, so it falls to `_ => 3.0` —
Anthropic Sonnet's rate. The Economics tab is currently reporting a DeepSeek
agent as if it ran on Sonnet. Note the two error directions compound: the
$3 default overstates DeepSeek ~6.9×, while the missing input/output split
understates real Anthropic runs ~1.8×. **Cross-provider cost comparison is
currently not just imprecise, it is directionally wrong** — which is exactly the
comparison heterogeneity requires.

(`provider_used` does resolve correctly here — `deepseek-chat` hits the
`starts_with("deepseek")` arm at `api_server.rs:5977`. The heuristic breaks for
proxied models, not this one.)

Both runs are `FAILURE`, and both were charged. See §5.3.

---

## 5. Economics of heterogeneity

### 5.1 Method

Monte Carlo, 10k iterations (`fermi_execute_fpl`). Margin per **1M tokens of
useful work**, so retry inflation is charged against the case honestly. Revenue
= `1000 credits × 1.10 gas × credit_usd`. Costs blended input/output at list
price. Sonnet $5.4/Mtok blended; cheap tier $0.9; OpenRouter fee 5.5%.

Stated assumptions, not measurements: `credit_usd ~ tri(0.010, 0.015, 0.020)`;
`downgrade_share ~ tri(0.30, 0.55, 0.75)`; `retry_inflation ~ tri(1.00, 1.25, 1.70)`.
The input/output split does not exist in the codebase, so `output_share` is
assumed throughout. Decision-grade, not accounting-grade.

### 5.2 Result

| Scenario | Mean | p5 | p95 |
|---|---|---|---|
| **Uniform Sonnet fleet (status quo)** | **+$11.12** | +$7.40 | +$14.82 |
| **Heterogeneous, right-sized via OpenRouter** | **+$16.10** | +$11.30 | +$21.28 |

**+$4.98/Mtok, +45%.** The heterogeneous p5 (+$11.30) exceeds the status-quo
*mean* (+$11.12) — the conclusion survives its own error bars, which is a
stronger claim than the point estimate.

Sobol: `credit_usd` 70%, `retry_inflation` 15%, `downgrade_share` 11%. Note what
this says — **the realised price of a credit dominates margin more than every
substrate decision combined.** Credit packaging is a bigger lever than provider
choice, and it is orthogonal to this spec.

For reference, the two adjacent features: vendor-swap-everything-to-GLM (B)
models at +$15.50, and invoker-BYOK at full credit price (A) at +$16.50. **C
reaches essentially A's ceiling without shifting who pays.** Separately, Anthropic
prompt caching on the stable system-agent prefix models at **+$1.32/Mtok** for no
vendor change at all — 26% of the heterogeneity gain, and it composes with it.

### 5.3 The retry externality — a governance finding

`retry_inflation`'s Sobol contribution is **positive**. Because `execution_fee`
is token-based, a weaker model that fails a structured-output contract and
retries produces *more billable tokens*, so margin **rises** with degradation.
Break-even would require the cheap model's blended rate to exceed ~$16.5/Mtok;
nothing in the cheap tier approaches that.

**Under current pricing, quality regression is not a margin risk — it is a
margin bonus, paid by users.**

This is visible in your screenshot: two `FAILURE` runs, `$0.616272` and
`$0.311628` of recorded cost, both charged. And it lands hardest on exactly the
agents heterogeneity threatens: **17 of 96 cards bypass the tool loop on a
structured-output contract** (`SPEC_28` §2), and 4.5 avg iterations on
`efra_critical_factor` shows how quickly a tool loop multiplies tokens.

Consequences to accept deliberately:

- The platform has **no economic signal** telling it a substrate downgrade went
  badly. The margin number will look better.
- Therefore the guardrail must be **eval-side, not economics-side**: contract
  compliance rate and retry rate per `(agent, provider, model)`, alarmed
  independently of cost. Structurally, `eval_signals` is the right home.
- Charging users for platform-chosen substrate failures is a defensible policy
  only if it is a *decision*. Right now it is an emergent property of a fee
  function that predates multi-provider execution.

---

## 6. Risks specific to C

### 6.1 Eval integrity — partition the fleet

Some system agents produce **user deliverables**; others produce
**platform-wide invariants**. Only the first group is safely heterogeneous.

| Agent | Output | Substrate-elastic? |
|---|---|---|
| `coherence_evaluator` | `pairwise_coherence`, `coherence_evaluations` | **No — cross-agent comparability** |
| `ontologist` | `entities`, `semantic_rules` in every agent's KG | **No — KG consistency over time** |
| `keeper`, `navigator`, `stripe_billing` | governance / billing | **No — correctness** |
| `fermi`, `xaman_ek`, `dream_narrator`, `naturalist` | user-facing work | Yes |

`ontologist` is the sharpest case: it writes into *every* agent's knowledge
graph. If extraction runs on GLM this month and Sonnet next, `semantic_rules`
acquires a substrate-dependent seam that no consumer can see. Rules are
long-lived; the seam outlives the experiment.

**Requirement: declare `substrate_pinned` vs `substrate_elastic` on the card**,
and stamp `(provider, model)` on every KG write so a seam is at least
attributable after the fact.

### 6.2 Delegation inherits funding *and* substrate

`tools_legacy.rs:4513-4520`:

```rust
let context = ExecutionContext {
    ...
    // Delegated child inherits the parent execution's funding.
    credentials: ctx.credentials.clone(),
};
```

Inheritance is by **parent execution, not child policy.** Under C the child also
inherits nothing useful about substrate — `cognition_tier: None` at 4517, so the
child's ladder never resolves and it falls to its card default. So a
heterogeneity policy applied at the entry point **silently stops at the first
delegation boundary**, and compound agents (`dream_coordinator` → `ontologist` +
`dream_narrator`) are exactly where system-agent work concentrates.

Also, today, in the current direction: a platform-funded curated agent delegating
to a *user-owned* agent runs that user's agent on `abw-system`'s key. And the
delegation path is **unpriced** — no charge call in `execute_execute_agent` /
`execute_delegate_to_agent`, and sub-agent tokens are not folded into the
parent's `tokens_used` (`tool_executor.rs:412`). So delegated substrate spend is
invisible to both the economics surface and the fee.

**Requirement: re-resolve credentials *and* substrate policy at every delegation
boundary.** Inheritance should be the explicit exception.

### 6.3 One gateway in front of everything

OpenRouter would sit in front of `coherence_evaluator`, `keeper` and
`stripe_billing`. Given §6.1 pins those to platform substrate anyway, the clean
resolution is: **pinned agents go direct to Anthropic; elastic agents go via the
gateway.** That also preserves prompt caching (+$1.32/Mtok) on the long stable
prompts of the agents that run most often, and keeps the governance path off the
extra hop.

### 6.4 What C does *not* buy

Enterprise BYOK. If the driver is data residency or procurement, OpenRouter is an
additional data processor and makes it worse, not better. Keep the provider axis
open — the store already has it — but do not market C as compliance BYOK.

---

## 7. Recommendation

**Do C. Do not start A.** B is a side effect of C, free once C lands.

| Phase | Content | Why |
|---|---|---|
| **C0 — make substrate real** | Fix `openai` dispatch (§3.3 — `ontologist` is broken *now*); add `provider_model_id(provider, model)` translation inside `apply_tier_resolution`; replace `openrouter/free` with a real id; thread `cognition_tier` into the main execution paths; bridge `model_ladder` in `resolve_agent_card`; preserve HTTP status in `ExecutionError`. | Six live defects. Nothing works without them. |
| **C1 — make substrate measurable** | Rate card → `(provider, model)` table with input/output rates and a surfaced `unknown_model` bucket; capture the input/output token split from both APIs; make `provider_used` read `AgentMetadata.provider` instead of guessing from the model string. | §4.3. Without this you cannot tell whether C worked, and the instrument will point the wrong way. |
| **C2 — unify executor behaviour** | One prompt-construction and sampling path across Anthropic and OpenAI-compatible executors (§3.1). | Otherwise every substrate A/B measures the executor branch. |
| **C3 — partition the fleet** | `substrate_pinned` / `substrate_elastic` on cards; pinned → direct Anthropic + prompt caching; elastic → OpenRouter gateway. Stamp `(provider, model)` on KG writes. | §6.1, §6.3. Also banks the +$1.32 caching gain. |
| **C4 — heterogeneity, per agent, behind a flag** | `(abw-system, openrouter, '*')`; per-agent model defaults; move one elastic agent at a time, **gated on contract-compliance and retry rate, not on cost** (§5.3). | The actual feature. Incremental and reversible. |
| **C5 — delegation correctness** | Re-resolve credentials + substrate at delegation boundaries; fold sub-agent tokens into cost accounting. | §6.2 — a latent billing and policy hole today. |
| **Later — A** | Only if enterprise demand for control (residency, committed spend) is real, and only after cost→price coupling exists. | §1. C captures the margin; A is a different product. |

**Sequencing logic:** C1 before C4 is the non-obvious constraint. Every instinct
says ship the substrate switch first and instrument after — but the instrument is
currently *biased against the change*, by 3–7× on exactly the models you'd move
to. Shipping C4 first means the Economics tab tells you to revert something that
worked.

---

## 8. Open questions

1. **Scope:** heterogeneity for `tier == "system"` (15 agents) or all
   platform-funded (100)? Note `is_platform_funded` includes `curated`, but the
   royalty gate excludes only `system` (`gas.rs:280`) — the two predicates
   already disagree, and 85 curated agents pay 85% royalty to the admin account.
   Scoping to `system` keeps them aligned.
2. **Retry externality:** is charging users for platform-chosen substrate
   failures acceptable as an explicit policy? If not, the fee function needs a
   failed-execution rule, independent of heterogeneity (§5.3).
3. **Quality floor:** what measured regression on the eval set is acceptable for
   platform-funded runs, given users cannot see the substrate? This is a
   product-values question, and §5.3 means economics will not raise its hand.
4. **`ontologist` seam:** accept a substrate-dependent seam in `semantic_rules`,
   or pin extraction permanently? (Recommend pin — §6.1.)
5. **Is the rate card config or code?** Provider prices change monthly;
   `registry.rs` requires a deploy. Related: `PLATFORM_ECONOMICS.md` §4.2 already
   flags this as rate-card drift.

---

## Appendix — margin model

Status quo baseline:

```fpl
question "Uniform Sonnet fleet, blended input/output pricing, margin per Mtok of useful work"

driver credit_usd continuous {
    distribution: triangular(0.010, 0.015, 0.020)
    unit: "USD per credit"
    rationale: "CREDIT_TIERS span 2.0c to 1.0c; economics.rs defaults to a blended 1.5c"
}

model: 1100 * credit_usd - 5.4

simulate 20000 iterations
```

Heterogeneous fleet, with retry inflation charged against the case:

```fpl
question "Heterogeneous fleet: margin per Mtok of USEFUL work, right-sizing system agents via OpenRouter"

driver credit_usd continuous {
    distribution: triangular(0.010, 0.015, 0.020)
    unit: "USD per credit"
    rationale: "blended realised credit price"
}

driver downgrade_share continuous {
    distribution: triangular(0.30, 0.55, 0.75)
    unit: "ratio"
    rationale: "share of fleet token volume whose work can move off Sonnet without unacceptable quality loss"
}

driver retry_inflation continuous {
    distribution: triangular(1.00, 1.25, 1.70)
    unit: "multiplier"
    rationale: "extra token volume from failed structured-output contracts on weaker models; 17 of 96 cards demand strict JSON"
}

model: 1100 * credit_usd * ((1 - downgrade_share) + downgrade_share * retry_inflation) - ((1 - downgrade_share) * 5.4 + downgrade_share * retry_inflation * 0.9 * 1.055)

simulate 20000 iterations
```

Prompt-caching variant multiplies the Sonnet input term by
`(1 - cached_input_share + 0.1 * cached_input_share)` with
`cached_input_share ~ triangular(0.35, 0.65, 0.85)`.
