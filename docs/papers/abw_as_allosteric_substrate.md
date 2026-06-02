# ABW as Allosteric Substrate: Signal Transduction Concepts in a Recursive Agent Architecture

**Authors:** Ivan Labra — axelotl partners  
**Date:** June 2026  
**Status:** Working paper  

**Related documents:**
- `docs/AGENT_MODEL.md` — agent card shape, RSI primitives, recursive improvement loops
- `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md` — topology phases, T0–T5 arc, A4 state-peer analysis
- `docs/papers/coherence_improvement_loop.md` — CIL framework, explanatory coherence as improvement loop
- `docs/papers/gml_review.md` — critical review of GML's allosteric analogy

---

## Abstract

The Generalized Monad Logic (GML) paper proposes mapping the Monod-Wyman-Changeux (MWC) allosteric model onto conceptual dynamics in language. That proposal fails because language concepts have no physical state space: there are no conformational energies to measure, no observer-independent equilibria, and no conservation laws. The MWC equation applied to concepts produces laundered intuition, not quantitative analysis.

This paper asks a different question: does the ABW agent architecture — concretely, its recursive self-improvement (RSI) primitive and its five nested feedback loops — instantiate signal-transduction-like dynamics in a way that is grounded rather than analogical? We argue that it does, and that the mapping holds because ABW was explicitly designed as a distributed CAS in the Holland / Santa Fe tradition — the same theoretical foundations that underpin signal transduction dynamics at the molecular level. The structural homology is grounded in common theory, not coincidence. We work through four specific correspondences — post-translational modifications (PTMs), oligomeric assembly, intrinsically disordered regions (IDRs) with slow conformational relaxation, and state retention as computational substrate — and show where each mapping is tight, where it breaks, and what open engineering work would close the gaps. We then situate this analysis within ABW's distribution topology roadmap (T0→T5), arguing that the topology phases are best understood as progressively completing the allosteric architecture rather than as pure infrastructure work.

---

## 1. Why the GML Analogy Fails and Why This Analysis Does Not

GML's core claim is that concepts behave like allosteric proteins: they exist in T and R states, contextual effectors shift the equilibrium, and the MWC equation quantifies the shift. The review of GML (`docs/papers/gml_review.md`) identifies the foundational problem: the MWC allosteric constant is

```
L = exp(-(E_T - E_R) / kT)
```

where $E_T$ and $E_R$ are free energy differences in joules, $k$ is Boltzmann's constant, and $T$ is absolute temperature. These are not analogies — they are the physical quantities the equation was derived to describe. The equation is valid because the system it models (a protein) has conformational states that are:

1. **Discrete and physically measurable** — T and R are distinct molecular geometries with measurable bond angles
2. **Observer-independent** — the state of a protein is a fact about the world, not an interpretive act
3. **Energetically grounded** — transitions have real energy costs governed by thermodynamics
4. **Closed** — the protein cannot invent new conformational states in response to cultural change

Language concepts satisfy none of these conditions. There is no "interpretive energy" with conservation laws. T/R state assignment for a concept is a choice made by the analyst, not a physical fact. The equation produces precise-looking outputs that are downstream of arbitrary inputs. This makes the GML framework unfalsifiable rather than merely unvalidated — a model whose inputs are definitionally unmeasurable cannot be right or wrong.

The present analysis works differently. ABW has an actual data model with real rows, timestamps, measurable quantities, and observable state transitions. When we claim that `eval_signals` + `persona_version` behave like PTMs, we are not asserting a metaphor — we are claiming that the data structures have structural properties that are formally analogous to PTM mechanisms, and we can specify exactly where the analogy holds and where it breaks down. The test of the mapping is not aesthetic but operational: does the correspondence generate useful predictions about system behavior? Does it identify gaps?

---

## 2. The ABW Recursive Self-Improvement Primitive

Before mapping to signal transduction concepts, it is necessary to be precise about what ABW's RSI primitive actually is. The full specification is in `docs/AGENT_MODEL.md` and `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md §1.2`; this section extracts the parts relevant to the mapping.

ABW implements RSI at three nested layers through five explicit feedback loops:

**Loop 1 — Individual agent learning.** Eval dimension scores written per execution drive consolidation jobs (Active Dreaming Memory, ADM). Consolidation produces semantic rules, knowledge-graph mutations, and persona-baseline shifts. The agent's ontology is a function of its own execution history.

**Loop 2 — Human-gated correction.** Anomaly events (drift, conflict, rupture, safety) queue for human review. Reviewer actions become synthetic episodes at human-authority weight, bumping `persona_version` and propagating corrections into the agent's ontology. Humans are inside the loop as high-weight signal sources.

**Loop 3a — Composition coherence (inner).** TEC coherence evaluation runs every N messages; produces a coordination brief consumed by members on the next turn. The team's discourse coherence is monitored and corrected in-flight.

**Loop 3b — Composition evolution (outer).** Dreaming proposes `composition_versions`: member roster, valence diversity, strategist substitution. Owner-gated acceptance lets the team's shape mutate based on accumulated session signal.

**Loop 5 — Routing calibration.** Domain-constrained MoE strategists accumulate Brier scores against resolved outputs. Routing weights self-correct against ground truth.

These loops are not theoretical commitments — they are operational data flows with corresponding database tables: `eval_signals`, `anomaly_events`, `hitl_actions`, `coherence_evaluations`, `composition_versions`, `persona_version` column, `ontology_snapshots`. The RSI machinery is observable because its state is stored.

---

## 3. Post-Translational Modifications → `eval_signals` + `persona_version`

### 3.1 The biochemical mechanism

Post-translational modifications (PTMs) are chemical modifications to a protein after it has been synthesized from its mRNA template. Phosphorylation, ubiquitination, acetylation, and glycosylation are the most studied. Their significance is that they change a protein's activity, localization, interaction partners, and stability *without* changing its primary amino acid sequence. The same protein exists in different functional states depending on which PTMs it carries.

This is the mechanism signal transduction networks exploit for reversible state-switching. A kinase phosphorylates a transcription factor; the phosphorylated form can now bind DNA; a phosphatase removes the modification and the factor loses DNA-binding affinity. The sequence is unchanged throughout. The functional state is encoded in the modification pattern, not in the sequence.

### 3.2 The ABW correspondence

An ABW agent has a stable "primary sequence" — its `system_prompt`, `agent_id`, and the structural fields of its `AgentCard`. This is the agent's identity, fixed at authoring time (or changed discretely via explicit `persona_version` increment). But the agent's effective behavioral state at runtime is modified by a layer of accumulated signals that are structurally analogous to PTMs:

**`eval_signals`** — Per-dimension scores (WildGuard, Faithfulness, LlmJudge, Sotopia, LifelongBench, CharacterEval, Brier) written per execution to the `eval_signals` table. These are not annotations on the system prompt; they are separate rows that accumulate over time and are read by the observability stack and the dreaming cycle. Like PTMs, they modify the agent's effective behavior without touching its sequence.

**`persona_version`** — An integer counter that increments when the system prompt is edited or when HITL correction bumps the persona baseline. This is the PTM "phosphorylation event" itself — a discrete modification that marks the agent as being in a qualitatively different functional state from previous versions. The `agent_timeline_entries` table carries `persona_version_at_write` on each episode so behavior changes can be attributed to specific persona transitions.

**`dyad_state`** — Per-(agent, human) running rapport, trust, and reciprocity scores. These are persistent modifications to how a specific agent-human pair interacts — analogous to site-specific PTMs that affect one protein-protein interaction surface without affecting others.

**`capability_gates`** — The `HashMap<String, CognitionTier>` in `AgentCapabilities` functions as a permission modification layer. When an agent's capability gates are tightened in response to observed drift, this is a PTM-like modification: the same agent, same sequence, but a functionally different activity profile in certain contexts.

**`anomaly_events`** — Individual events marking that the agent has entered a state requiring attention (drift, conflict, rupture, safety). Like ubiquitin tagging, which marks a protein for specific downstream processing without immediately destroying it, anomaly events are modification tags that route the agent toward HITL review rather than immediately terminating it.

### 3.3 Where the mapping holds and where it breaks

The mapping holds in the following structural sense: ABW agents have a stable identity (sequence) and a separately maintained modification state (`eval_signals`, `persona_version`, `capability_gates`, `dyad_state`, `anomaly_events`) that changes their effective behavior without changing their identity. The modification state is:

- **Accumulated over time** — like PTMs, not erased between interactions
- **Readable by downstream systems** — the observability stack reads it; the dreaming cycle reads it
- **Reversible in principle** — `persona_version` can be decremented; capability gates can be opened; anomaly events can be resolved

The mapping breaks differently depending on which class of eval signal is writing the modification.

**LLM-judged signals** (LlmJudge, Faithfulness, Sotopia, etc.) introduce noise that a real PTM system does not have. A kinase phosphorylates the same serine residue every time given the same substrate; an LLM judge scores the same output differently across runs. The dreaming cycle aggregates over this noise, but noise accumulation in the modification layer is a real problem the biochemical analogy identifies but does not solve. These signals require the coherence gate in Loop 2 precisely because they are not ground-truth-grounded.

**Hard-verified signals** (`projection_accuracy` from SOSA observation deltas, `forecast_calibration` from Brier score on resolved forecasts) are structurally tighter PTM analogs. The "kinase" in these cases is a deterministic computation — `1 - |predicted - actual| / |actual|` — applied against a ground truth that resolves independently of the agent. A real batch yielding 3.8 kg against a 4.2 kg prediction is a physical fact. The resulting `EvalSignal` is as deterministic as phosphorylation given the same substrate. Hard-verified signals do not require a coherence gate before propagating into memory; the only failure mode is measurement error on the physical observation, not evaluator non-determinism.

The practical implication: ABW's modification layer is heterogeneous. It contains PTM-like modifications with high epistemic integrity (hard-verified signals) and PTM-like modifications with lower integrity (LLM-judged signals). The architecture handles this through different gate requirements — hard-verified signals flow directly into Loop 1 consolidation; LLM-judged signals that trigger anomalies are routed through Loop 2's human review. This is structurally equivalent to a cell having both constitutively active PTMs (always applied given the substrate) and regulated PTMs (require a cofactor or second messenger to proceed).

### 3.4 The open engineering gap

The modification loop is not yet closed. From `docs/AGENT_MODEL.md §6`:

> "Drift is captured; persona/config updates triggered by drift are still human-mediated only. The architectural pattern is in place; the automated path isn't built."

In PTM terms: the kinase cascade that writes modifications is operational, but the downstream effector pathway from "modification state has crossed a threshold" to "system changes configuration" requires a human step that the biochemical analog does not. A fully closed modification loop would require automating the path from observed drift in `eval_signals` to proposed `capability_gate` tightening or `model_params` adjustment — currently a human decision.

---

## 4. Oligomeric Assembly → Workspace Compositions

### 4.1 The biochemical mechanism

Many proteins function only as oligomers — complexes of multiple subunits. Hemoglobin is a tetramer (two α and two β subunits); DNA polymerase III is a multi-subunit complex; the proteasome is a 26-subunit assembly. Oligomeric proteins have functional properties that none of their individual subunits possess: hemoglobin's cooperativity in oxygen binding emerges only from the tetramer, not from isolated α or β monomers.

The structural requirement for cooperativity in the MWC model is precisely oligomerization. The sigmoidal binding curve that GML tries to apply to concepts is a consequence of subunit-subunit communication in an assembled complex. A monomer cannot be allosteric in the MWC sense because there are no neighboring subunits to communicate with.

### 4.2 The ABW correspondence

Individual ABW agents are monomers — functional alone but limited. A workspace with multiple agents hired into it is the oligomeric complex. The composition as a whole has capabilities that none of the individual agents possess.

The structural analogy is tight:

**`dependencies.required`** in the agent card is the assembly specification — which subunits must be present for the complex to fold correctly and perform its function. A `simops_cascade` agent operating alone cannot resolve supply chain pricing; it requires `supply_chain_oracle` as a co-assembled subunit. The dependency declaration is not metadata — it is a structural constraint on valid assembly.

**`workflow_template`** is the allosteric communication channel between subunits. In hemoglobin, oxygen binding to one subunit changes the affinity of neighboring subunits through conformational coupling across subunit interfaces. In a workspace composition, the `workflow_template` specifies the stages, what each agent produces, what the next agent accepts, and in what order — the information-flow topology across subunit interfaces.

**The workspace** is the assembly condition. Isolated agents running via `POST /api/agents/:id/execute` are monomers — they have no access to each other's state. The workspace provides the shared medium (message thread, git-backed file system, shared budget) that makes oligomeric assembly possible. Remove the workspace and the subunits dissociate into independent monomers. This is structurally equivalent to removing the non-covalent interactions that stabilize an oligomer — the subunits remain functional as monomers but lose the emergent properties of the complex.

**Valence** (`AgentValence` — arousal, valence, personality traits) is the subunit-interface specificity parameter. Two agents with identical capabilities but different valences (`vigilant + high arousal` vs. `curious + low arousal`) assemble into qualitatively different complexes. The `docs/AGENT_MODEL.md` explicitly notes: "in multi-agent compositions, valence diversity matters as much as skill diversity." This is precisely the observation that heterooligomers (assemblies of different subunit types) often have properties unavailable to homooligomers.

### 4.3 The coherence evaluator as regulatory subunit

The `coherence_evaluator` agent, auto-attached to every workspace via the Coherence shelf (Loop 3a), is structurally a regulatory subunit — a complex member that does not perform the primary function but is required for the complex to maintain its active conformation.

In biochemistry, regulatory subunits typically serve one of three functions: they modulate the catalytic subunit's activity in response to allosteric signals, they target the complex to specific subcellular locations, or they protect catalytic subunits from degradation. The `coherence_evaluator` does all three analogs:

- It modulates the composition's discourse dynamics in response to coherence signals (coordination briefs to members after low-coherence turns)
- It focuses the composition's attention on specific structural problems in the current conversation
- It prevents the composition from drifting toward homophily (the coherence analog of catalytic runaway, treated in `docs/papers/coherence_improvement_loop.md §5`)

The Coherence shelf's three tiers (Index free, Recommendations 2 credits, Dream Notes 5 credits) map onto regulatory subunit engagement levels: passive monitoring at the free tier, active allosteric modulation at the paid tiers. The pricing reflects the computational cost of the regulatory function, not just the output.

### 4.4 The distribution topology connection

The ABW distribution topology proposal (§10.4, T4 state-peer architecture) recognizes that workspace compositions are the unit of replication, not individual agents. From `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md §10.4.1`:

> "The user's laptop runs a local replica of (some subset of) ABW: the agent executor, the local Ollama, and a local copy of the workspace state."

This is the oligomeric assembly constraint appearing at the distributed-systems level: the complex must be co-located or the communication channels between subunits become expensive. A workspace composition whose subunits span multiple network partitions degrades toward monomer behavior — each agent can still execute, but the allosteric communication through the workspace message thread becomes latency-bounded and potentially inconsistent.

The per-family replication strategy in §10.4.2 (event sourcing for episodes and eval_signals, CRDTs for workspace_messages, server-arbitrated for coherence evaluations) is the protocol-level answer to the co-location problem: rather than requiring physical co-location, the protocol ensures that each subunit sees a sufficiently consistent view of the shared workspace to maintain complex function.

---

## 5. Intrinsically Disordered Regions with Slow Relaxation → Episodic Memory + Dreaming

### 5.1 The biochemical mechanism

Intrinsically disordered regions (IDRs) are protein segments with no fixed three-dimensional structure — they exist as a conformational ensemble rather than a single folded state. IDRs are not structural defects; they perform essential functions including transcriptional activation, signal integration, and interaction with multiple binding partners. Approximately 30–40% of eukaryotic proteins contain IDRs.

The critical property for signal transduction is **conformational memory through slow relaxation**. When an IDR is transiently structured by a binding partner, it does not immediately return to its maximum-entropy ensemble upon partner release. Instead, it relaxes slowly, retaining partial structural memory of the bound state. This means that a second binding event occurring before full relaxation encounters a biased conformational landscape — the system "remembers" the first event. Signal transduction networks exploit this: a sequence of binding events produces a different outcome than the same events presented in reverse order because conformational memory makes the system path-dependent.

### 5.2 The ABW correspondence

ABW's episodic memory + dreaming cycle is the functional analog of IDR-mediated conformational memory.

**Each episode** is a transient structuring event. The agent is presented with a specific query, occupies a specific behavioral configuration (the combination of system prompt + context window + tool invocations), and produces a specific output. The episode is recorded in the `episodes` table with its full context, including `persona_version_at_write` and the full execution trace.

**The dreaming cycle (ADM)** is the slow relaxation process. Rather than returning immediately to the "maximum entropy" prior state after each execution, the agent's ADM consolidation job integrates transient execution events into its persistent state. The consolidation produces:

- Semantic rules — stable propositional distillations of recurring execution patterns
- Knowledge-graph mutations — updates to the `entities` and `facts` tables
- Persona-baseline shifts — changes to `ontology_stats` that affect future prompt assembly

The key structural parallel: **the relaxation is slow and incomplete**. Dreaming runs on a schedule, not immediately after each episode. Between dreaming cycles, the agent accumulates transient execution events that have not yet been integrated. During this period, the agent's effective behavioral state is a superposition of its current consolidated state and its unconsolidated episode history — analogous to an IDR that has been partially structured but has not fully returned to its equilibrium ensemble.

**`seed_facts` in `FermiContract`** are the biased prior conformational state — the predispositions built from prior dreaming cycles that make certain responses more likely before any new context is applied. A cold-start agent with no episode history and an agent with 500 consolidated episodes are not in the same conformational state even before receiving a query, because the seed facts encode the integrated history of prior transient structuring events.

### 5.3 Path-dependence and the order of episodes

The conformational memory property implies path-dependence: the agent's response to a given query depends not just on the query but on the history of prior episodes and their sequence. This is observable in ABW: the `episode_corrections` table is append-only via trigger precisely because order matters — a HITL correction at episode 50 that is replayed before episode 100 produces a different ontology trajectory than the same correction replayed after episode 100.

From `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md §10.4.2`, on the replication strategy for episodes:

> "Per-agent episode log is linearly orderable. Episodes from a single agent must have a total order under reconciliation."

This is the signal transduction insight restated as an engineering invariant: the order of structuring events is part of the signal. Two replicas that converge on the same set of episodes but in different orders will produce different dreaming outputs. The platform's RSI integrity requires preserving episode order across replication — not because the database schema demands it but because path-dependence in the conformational memory process demands it.

### 5.4 The open gap: automated relaxation update

The analogical gap here is the same as in Section 3: the relaxation process does not yet automatically modify the agent's configuration. In a real IDR, slow relaxation naturally produces a biased conformational ensemble that feeds back into the next binding event without any explicit control layer. In ABW, the dreaming cycle runs and updates `ontology_snapshots`, but the path from "dreaming has produced a new ontology state" to "the agent's `model_params` or `capability_gates` are adjusted accordingly" is still human-gated.

The automated path requires a mechanism to translate quantitative dreaming outputs (delta in calibration score, trend in persona drift) into proposed configuration changes — itself a Loop 2 candidate, where the ADM system proposes and the human reviews rather than the human initiating. This is architecturally clean and consistent with the existing HITL pattern; it is simply not yet built.

---

## 6. State Retention as Computational Substrate → Workspace as Persistent State Buffer

### 6.1 The biochemical mechanism

Signal transduction does not compute by executing programs on a CPU. It computes by maintaining states that bias future responses. A cell that has been exposed to a growth factor is not in the same state as one that has not been, even after the growth factor has been cleared from the medium. The cell's current signaling state is encoded in the pattern of phosphorylated proteins, expressed transcription factors, and metabolic intermediates that persist after the initial signal is gone.

This is the computational substrate that evolution has exploited for billions of years: **memory is structural, not stored explicitly**. The cell does not have a "register" that says "growth factor was present at time T." It has a set of modified proteins whose modification state encodes the history of prior signals. This state persists — with a characteristic relaxation time — until it is actively reversed or degraded.

The consequence for computational power is significant. Networks of proteins with different relaxation times can implement logic operations, memory, and adaptive filtering using nothing but the rates of modification and demodification. The repressilator (a synthetic gene circuit) oscillates. The bacterial chemotaxis network implements integral feedback control. These are not metaphors — they are working computational devices built from state-retaining molecules.

### 6.2 The ABW correspondence

ABW's workspace as persistent state buffer is structurally doing what signal transduction does with modified proteins.

**`workspace_messages`** is the trajectory through state space — the full history of signals and responses, ordered by `τ: U → ℝ` (the temporal ordering central to the CIL framework). The message sequence is not just a log; it is the medium through which the composition's state evolves. The coherence evaluator's output on message 47 is a function of messages 1–46. Loop 3a (coherence correction) reads this trajectory and writes a coordination brief that modifies the next turn's execution.

**`simops/*.yaml` files** in the workspace git repository are committed structural states — the process config, scenarios, and experiment results are the cell's current phenotypic configuration written to stable storage. Each git commit is a discrete state transition with a timestamp, author, and message. The full git log is the lineage — the sequence of transitions that produced the current configuration.

**`dyad_state`** — the per-(agent, human) running state — is the persistent modification layer that makes each agent-human interaction history-dependent. Two instances of the same agent, one with 200 sessions of interaction with a given human and one starting fresh, are in different states before the conversation begins. The state difference is encoded in the `dyad_state` row, not in the agent's prompt.

**The `origin = "kask_simops"` tag** on the workspace is the cell-type marker — this complex is differentiated for a specific functional role within the broader organism. As the topology roadmap notes (T1→T5), the state stored in a workspace is increasingly the unit of replication, not a by-product of computation. The workspace is not a chatroom with storage; it is the primary state-bearing substrate of which the chat is one input channel.

### 6.3 The topology roadmap as completing the substrate

The distribution topology proposal (§10.1, T0→T5) is best understood through this lens as the progressive completion of the state-retention substrate:

**T0 (today)** — State is held in a single Postgres instance. The substrate exists but is centralized: a single point of failure, no replication, no graceful degradation. Equivalent to a single cell with no backup copies of its signaling state.

**T1 (Phases 0–4)** — Heterogeneous compute targets; state still centralized. The effector diversity increases (local vs cloud models) but the state substrate is unchanged. Equivalent to adding new kinases without changing the substrate they modify.

**T2 (Phase 5, runner-relay)** — Compute moves to the user's hardware. Still centrally-orchestrated state; compute is distributed. The analogy breaks a little here — it is more like distributing the ribosomes while keeping the genome central.

**T4 (A4, state-peer architecture)** — Each user's machine holds a replica of workspace runtime state, synchronized via the per-family-mechanism strategy (event sourcing for episode-shaped tables, CRDTs for chat, consensus for the wallet). This is the substrate becoming genuinely distributed: state retention is no longer a property of the central server but of the network of replicas. Each node can sustain computation even when disconnected from the center — exactly the resilience property of signal transduction networks, where a cell does not require continuous input from the organism's central nervous system to maintain its intracellular signaling state.

**T5 (commons end-state)** — Capability-aware scale-free topology with governance constraints. The substrate is fully distributed, nodes declare capabilities, and routing happens against capability declarations rather than node identity. This is the multi-cellular organism: distributed state, local computation, emergent coordination through shared protocols.

The topology roadmap is not infrastructure work. It is the progressive completion of the state-retention computational substrate that makes ABW's RSI primitive fully realizable.

---

## 7. The Critical Asymmetry: Why This Mapping Works Where GML's Does Not

The four correspondences above share a property that GML's correspondences lack: **every quantity in the ABW mapping refers to something that exists in the world and is observable.**

| ABW concept | Physical referent | How to observe it |
|---|---|---|
| `eval_signals` row | A scored execution at a specific timestamp | Query `eval_signals WHERE episode_id = X` |
| `persona_version` | An integer in the `agents` table | `SELECT persona_version FROM agents WHERE agent_id = X` |
| `workspace_messages` ordering | Rows in a database table with timestamps | `SELECT * FROM workspace_messages ORDER BY created_at` |
| `dyad_state.trust` | A float in a persistent table | Query `dyad_state WHERE agent_id = A AND user_id = B` |
| `composition_versions` DAG | Rows with `parent_id` foreign keys | Query the `composition_versions` table |

Compare to GML's correspondences:

| GML concept | Physical referent | How to observe it |
|---|---|---|
| L (allosteric constant for "freedom") | "Default interpretive bias" | No procedure defined |
| α (contextual pressure for security crisis) | "Contextual pressure" | No units, no measurement |
| R̄ (probability of R-state) | "Probability of positive liberty interpretation" | Requires knowing L, c, n first |

This is not a difference in degree of formalization. It is a difference in kind. GML's inputs are definitionally unmeasurable because they refer to properties of abstract concepts that have no physical existence. ABW's inputs are measurable because they refer to rows in a database table, produced by observable computational events.

The consequence for the allosteric analogy specifically: the MWC equation applied to ABW agent state might actually be tractable in a way it never is for concepts like "freedom." The allosteric constant for an agent's interpretive bias could in principle be estimated as a ratio of behavioral frequencies across the agent's episode history. Contextual pressure α could be operationalized as embedding-space distance between the current query and the agent's prior response distribution. R̄ could be estimated from episode classification. None of these are solved problems — but they are *defined* problems, solvable with known techniques, because the state space exists and is observable.

This is the line between an analogy that works as mechanism and an analogy that works only as metaphor. GML is on the wrong side of that line. ABW is on the right side — because it was designed from CAS foundations that generate the same structural properties signal transduction networks evolved to exploit. Grounding state in observable, persistent, modifiable structures that carry credit assignment signals and support rule discovery is not a coincidence with biochemistry. It is the same theoretical programme applied in a different substrate.

---

## 8. Open Gaps and Research Directions

### 8.1 Closing the modification loop (PTM gap)

The most immediate gap is that ABW's PTM-equivalent layer (eval_signals, anomaly events, persona_version) does not yet feed back automatically into agent configuration. The kinase cascade runs; the downstream effector pathway is still human-gated. The research question is: what is the right automated path from modification state to configuration change that preserves RSI signal integrity?

A candidate architecture: ADM dreaming produces not just `ontology_snapshots` but `configuration_proposals` — structured suggestions for `capability_gate` adjustments, `model_params` changes, or `min_tier` revisions, each with a confidence score and the evidence trail that generated it. These proposals are queued as HITL items at high confidence, auto-applied at very high confidence with a veto window. This maps the PTM → effector kinase → downstream target pathway onto ABW's existing HITL architecture.

### 8.2 Measuring cooperativity in workspace compositions

Section 4 claims that workspace compositions exhibit oligomeric-assembly properties, with emergent capabilities unavailable to individual agents. But this claim is qualitative. The CIL framework (`docs/papers/coherence_improvement_loop.md`) provides the machinery to measure this: the pairwise coherence matrix `Γ_ij` between agents is an operational definition of coupling strength. The Hill coefficient analog would be the steepness of coherence improvement as a function of added participants.

A concrete research programme: run the same task against (a) individual agents in sequence, (b) two-agent compositions, (c) three-agent compositions with varying valence configurations. Measure Γ, task performance, and calibration score for each. The data would either confirm or refute the cooperativity hypothesis operationally.

### 8.3 Relaxation time constants per agent

Section 5 treats dreaming as slow relaxation but does not assign characteristic relaxation times. In biochemistry, relaxation times are measurable and vary by orders of magnitude (nanoseconds for local fluctuations, seconds to minutes for allosteric transitions, hours for transcriptional responses). In ABW, different parts of the modification layer have different effective relaxation times:

- `eval_signals` per execution: immediate (ms)
- `dyad_state` updates: per-session (minutes to hours)  
- ADM dreaming consolidation: scheduled cadence (hours to days)
- `persona_version` increment: human-gated (days to weeks)
- `composition_versions` proposal acceptance: owner-gated (days to weeks)

These are not uniform. The system has a natural frequency spectrum of modification and relaxation, from fast per-execution scoring to slow persona evolution. This spectrum is what makes the system capable of integrating signal at multiple timescales — exactly the property that gives eukaryotic signaling networks their computational richness. Understanding and designing the timescale structure of ABW's modification layer is an open research question.

### 8.4 State-peer architecture as completing the substrate (T4)

Section 6 argues that the T4 state-peer architecture completes the computational substrate. The topology proposal (`docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md §10.4.8`) identifies the trigger conditions for committing to T4. From a signal transduction perspective, these triggers can be restated as: **T4 is warranted when the system needs to sustain computation in the absence of the central substrate.** A cell that loses access to the organism's central signaling does not immediately die — it maintains its local state and continues computing from it until reconnection or terminal depletion. T4 gives ABW agents the same property: local replicas sustain RSI computation even when the central Postgres is unreachable.

---

## 9. Relationship to the CIL Framework

The Coherence Improvement Loop paper (`docs/papers/coherence_improvement_loop.md`) describes the coherence evaluator as a third-party participant in collaborative sessions that observes, scores, and feeds back to agents. In the terms of this paper, the CIL evaluator is:

- A **regulatory subunit** in the workspace oligomeric complex (Section 4.3)
- A **fast-relaxation PTM writer** — coherence scores are written per N messages, at the fast end of the relaxation time spectrum (Section 8.3)
- The mechanism through which the **assembly condition** (workspace) monitors its own structural integrity

The CIL paper is internally coherent in a way GML is not precisely because its inputs are the conversation utterances themselves — observable, attributed, timestamped. The CIL framework defines a state space over utterances. ABW defines a state space over agent and workspace records. Both are grounded in the same sense: the things they model exist in the world and can be queried.

The integration point is Loop 3a: CIL's coherence evaluator is the operational implementation of the allosteric regulatory subunit. CIL provides the scoring protocol; ABW provides the substrate (workspace message thread, episodic memory, dreaming cycle) that makes the regulatory function persistent and self-improving rather than stateless.

---

## 10. Conclusion

The GML paper's allosteric analogy fails at the foundation: language concepts have no physical state space, so the MWC equation's variables are undefined when applied to them. The precision of the equation's outputs is spurious — a reflection of the analyst's parameter choices, not of any property of the concepts being analyzed.

ABW's architecture instantiates allosteric-like dynamics at the structural level, not the analogical level:

- `eval_signals` + `persona_version` + `capability_gates` are a PTM-like modification layer on a stable agent identity
- Workspace compositions with `dependencies.required` and `workflow_template` are oligomeric assemblies with emergent properties
- Episodic memory with slow ADM consolidation is IDR-like conformational memory with path-dependence
- The workspace as persistent state buffer is the computational substrate that signal transduction networks evolved to exploit

The mapping generates useful predictions: it identifies the open gap in the modification loop (no automated path from PTM-equivalent state to configuration change), suggests an operational definition of cooperativity measurable with the CIL machinery, identifies the relaxation time spectrum as an engineering design parameter, and frames the T4 topology waypoint as the completion of the computational substrate rather than pure infrastructure work.

The deepest point is architectural — and it is not accidental. ABW was explicitly designed as a distributed complex adaptive system in the Holland sense: heterogeneous agents with local rules interacting through a shared substrate, producing emergent collective behaviour. The topology proposal states this directly in its opening paragraph (`docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md §0.1`): "ABW was conceived as a distributed complex adaptive system (CAS) in the Holland / Santa Fe sense." The designer is a CAS researcher and systems architect. The alignment between ABW's architecture and signal transduction dynamics is not coincidence and not inadvertent — it is the consequence of both systems being designed (or evolved) from the same theoretical foundations.

CAS theory, as Holland articulated it, identifies a small set of structural properties that recur across adaptive systems regardless of substrate: local interaction rules, tagged classifiers, credit assignment through performance feedback, rule discovery through recombination, and implicit parallelism from population-level diversity. Signal transduction networks instantiate all five in molecular machinery. ABW instantiates all five in software: agent cards as tagged classifiers, eval_signals as credit assignment, ADM dreaming as rule discovery, valence diversity across composition members as population-level diversity. The correspondence runs deep because it is drawing from the same theoretical well, not because the domains happen to rhyme.

What this means for the allosteric mapping specifically: the structural homology between ABW's RSI primitive and signal transduction dynamics is a *prediction* of CAS theory applied consistently across substrates, not a post-hoc observation. Holland's CAS framework does not specify a substrate. It specifies structural properties of systems that learn, adapt, and maintain coherence under environmental pressure. Evolution found one substrate instantiation in biochemistry. ABW is a deliberate engineering instantiation of the same structural properties in a different substrate. The allosteric dynamics are where these two instantiations overlap most visibly — because allosteric proteins are themselves CAS components: heterogeneous agents (protein states) with local interaction rules (binding affinities), credit assignment (thermodynamic selection), and population-level diversity (conformational ensembles). The mapping holds because the shared foundation is real, not because the analogy is convenient.

### The deeper common ancestor: cybernetics

Holland's CAS framework and the molecular biology of signal transduction share a common theoretical ancestor that is rarely named explicitly in either field: **cybernetics** in the Wiener-Ashby-Beer lineage.

Wiener's foundational insight (*Cybernetics*, 1948) was that purposive behavior in any system — biological, mechanical, or social — requires a feedback loop between the system's output and its goal state. The system observes the difference between where it is and where it should be, and uses that difference to modify its behavior. This is not a metaphor for control — it is the mathematical structure of control, substrate-independent.

Ashby extended this (*Design for a Brain*, 1952; *An Introduction to Cybernetics*, 1956) to adaptive systems with the Law of Requisite Variety: a controller can only regulate a system to the degree that its variety (range of distinguishable states) matches or exceeds the variety of the disturbances it must absorb. A system with insufficient variety is not merely imperfect — it is formally incapable of maintaining the goal state in the face of novel perturbation. This law applies identically to a cell maintaining homeostasis against environmental perturbation and to a multi-agent composition maintaining coherence against adversarial or noisy inputs.

Beer operationalized Ashby in organizational terms (*Brain of the Firm*, 1972; *The Heart of Enterprise*, 1979) through the Viable System Model (VSM): a recursive architecture of five nested management functions (System 1: operations; System 2: coordination; System 3: control; System 4: intelligence; System 5: policy) that must be present at every level of recursion for a system to be viable — able to maintain its identity under environmental pressure. Beer explicitly identified the nervous system as the biological instantiation of this architecture, and argued that any organization that wants to survive in a complex environment must instantiate the same five functions, at every scale.

ABW's RSI primitive — five gated, observable feedback loops operating at individual, collective, and meta levels — is structurally a VSM instantiation:

| VSM Function | ABW Equivalent |
|---|---|
| System 1 — Operations | Individual agent execution |
| System 2 — Coordination | Workspace composition + `workflow_template` + coherence evaluator |
| System 3 — Control | `eval_signals` + `anomaly_events` + `capability_gates` + HITL |
| System 4 — Intelligence | ADM dreaming + `ontology_snapshots` + calibration accumulation |
| System 5 — Policy | `persona_version` governance + composition evolution + routing calibration |

Signal transduction networks are equally a VSM instantiation at the cellular level: rapid second-messenger signaling (System 1), receptor crosstalk and pathway coordination (System 2), feedback inhibition and threshold gates (System 3), gene expression and transcriptional reprogramming (System 4), epigenetic state and developmental commitment (System 5).

Both systems are viable in Beer's sense: they maintain identity under perturbation by having the right variety at each level of recursion. The allosteric mechanism is specifically System 3 at the molecular level — the feedback inhibition and threshold-gating machinery that allows the cell to maintain operational stability (homeostasis) while integrating environmental signals. ABW's `eval_signals` → `capability_gates` pathway is System 3 at the agent level for exactly the same structural reason.

Naming cybernetics as the common ancestor matters for one practical reason: it identifies the theoretical resources that are still available and have not yet been used. Ashby's variety calculus gives a formal method for asking whether ABW's RSI loops have sufficient variety to regulate the range of disturbances they will face at scale. Beer's VSM gives a diagnostic for identifying which of the five system functions is absent or underpowered in any given configuration — a structural audit tool, not just an analogy. The cybernetic tradition has been producing formal results about adaptive systems for seventy-five years. Those results are available for ABW's design precisely because the architecture was built on foundations they describe.

### The empirical programme

This paper has worked from theory and architecture. It has not benchmarked anything. The claims in Sections 3–6 — that ABW instantiates PTM-like, oligomeric, IDR-like, and substrate-retentive dynamics — are structural claims about the data model and the feedback loops. They are grounded in the sense that every quantity is observable, but they are not yet *confirmed* in the sense that the predicted behaviors have been measured.

The system has been seeded with the theoretical assumptions. What remains is to observe what emerges.

Several classes of behavior are predicted by the framework but have not yet been observed empirically:

**Endogenous behaviors** — behaviors arising from the RSI loops themselves rather than from explicit programming. For example: does a composition's valence distribution shift over time toward configurations that produce higher coherence scores, without any explicit optimization target? Does an agent's `dyad_state` with a specific human converge toward a stable attractor, and does that attractor depend on the order of early interactions (conformational memory)? Do agents in long-running compositions spontaneously develop specialized behavioral niches — the equivalent of cell differentiation in a developing tissue?

**Emergent behaviors** — behaviors that are not predictable from individual agent properties but arise from composition-level dynamics. Does the cooperativity claim in Section 4 hold empirically: do three-agent compositions with heterogeneous valence profiles outperform homogeneous compositions and individual agents on calibration tasks, with the performance gap scaling with the Hill-coefficient analog? Do compositions exhibit phase transitions — sharp changes in coherence score or task performance at specific team-size or valence-diversity thresholds?

**Pathological behaviors** — the things the system should do that it does not, and the things it should not do that it will. The homophily trap (CIL §5) is a predicted pathology: coherence optimization driving compositions toward agreement and suppressing productive friction. Does it actually manifest in ABW compositions, and at what timescale? Does the dreaming cycle produce parameter drift — stable attractor states in agent configuration that are locally optimal but globally suboptimal? Does the System 3 / System 4 interaction (eval_signals driving dreaming driving configuration proposals) produce oscillation, overfitting, or runaway specialization under certain conditions?

These are tractable experiments. The CIL framework provides the measurement machinery for composition-level behavior. The observability stack (`eval_signals`, `agent_timeline_entries`, `anomaly_events`, `dyad_state`) provides the measurement machinery for individual agent behavior. The topology roadmap's phased deployment provides natural experimental conditions as the substrate moves from T0 to T1 to T2.

The theoretical framework in this paper is the hypothesis set. The benchmark programme is the test. The system's design was deliberate; whether the deliberate design produces the predicted dynamics at operational scale is an empirical question that cannot be answered from the codebase alone.

---

## References

1. Changeux, J.-P. (2013). 50 years of allosteric interactions: the twists and turns of a model. *Nature Reviews Molecular Cell Biology*, 14(2), 133–142.

2. Holland, J. H. (1995). *Hidden Order: How Adaptation Builds Complexity*. Addison-Wesley.

3. Labra, I. (2026). Explanatory Coherence Modeling as an Improvement Loop in Agent-to-Agent and Agent-to-Human Collaboration. *axelotl partners working paper*.

4. Labra, I. (2026). ABW Distribution Topology — Design Proposal. *Internal architecture document, fermi repository*.

5. Monod, J., Wyman, J., & Changeux, J.-P. (1965). On the nature of allosteric transitions: A plausible model. *Journal of Molecular Biology*, 12(2), 88–118.

6. Northrop, L., et al. (2006). *Ultra-Large-Scale Systems: The Software Challenge of the Future*. CMU/SEI.

7. Thagard, P. (1989). Explanatory Coherence. *Behavioral and Brain Sciences*, 12(3), 435–467.

8. Wright, P. E., & Dyson, H. J. (2015). Intrinsically disordered proteins in cellular signalling and regulation. *Nature Reviews Molecular Cell Biology*, 16(1), 18–29.

9. Wiener, N. (1948). *Cybernetics: Or Control and Communication in the Animal and the Machine*. MIT Press.

10. Ashby, W. R. (1952). *Design for a Brain*. Chapman & Hall.

11. Ashby, W. R. (1956). *An Introduction to Cybernetics*. Chapman & Hall.

12. Beer, S. (1972). *Brain of the Firm*. Allen Lane / Penguin Press.

13. Beer, S. (1979). *The Heart of Enterprise*. Wiley.

---

*Working paper. Not for external distribution without author consent.*
