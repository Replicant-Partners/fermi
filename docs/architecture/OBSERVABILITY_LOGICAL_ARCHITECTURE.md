# Social Agent Observability Platform — Logical Architecture

**Audience:** Technical leads, architects, senior contributors  
**Companion to:** `OBSERVABILITY_ARCHITECTURE_SPEC.md` (physical implementation detail)  
**Purpose:** Defines the system in logical terms — what the components *are*,
what domain concepts they own, and how they interact — independent of language,
crate boundaries, or SQL schema particulars.

---

## 1. System Overview

The Social Agent Observability Platform is a **closed-loop behavioral monitoring
system** for AI agents. It continuously evaluates agent behavior across multiple
dimensions, builds a longitudinal record of how an agent's behavior evolves over
time, surfaces anomalies to human reviewers, and — when reviewers intervene —
feeds corrective signals back into the agent's behavioral baseline through a
coherence-gated memory write.

The system is organized into four logical planes. Each plane transforms the
outputs of the plane below it into richer, more actionable information.

```mermaid
graph TD
    A["Plane A · Agent & Memory\nFoundation"]
    B["Plane B · Evaluator Registry\nMulti-dimensional scoring"]
    C["Plane C · Longitudinal Observer\nTrend · Drift · Anomaly"]
    D["Plane D · Human Interface\nHITL · Intervention · Feedback"]

    A -->|EpisodeBundle| B
    B -->|AggregatedSignal| C
    C -->|AnomalyEvent| D
    D -->|SyntheticEpisode + Correction| A

    style A fill:#2d4a3e,color:#cfe8cf,stroke:#4a7c59
    style B fill:#2d3a4a,color:#cfe0ef,stroke:#4a6a8c
    style C fill:#4a3a2d,color:#efe0cf,stroke:#8c6a4a
    style D fill:#4a2d3a,color:#efcfe0,stroke:#8c4a6a
```

The feedback arrow from Plane D back to Plane A is the defining feature of the
system: it is not a one-way monitoring pipeline but a **recursive improvement
loop** in which human review decisions are re-injected as high-authority
behavioral evidence.

---

## 2. Domain Ontology (Entity-Relationship)

The following ER diagram expresses the domain model — the *concepts* and their
relationships — not the physical schema columns.

```mermaid
erDiagram
    AGENT {
        id agent_id
        string name
        string system_prompt
        int persona_version
        enum tier
    }

    EPISODE {
        id episode_id
        string query
        string response
        enum provenance
        float authority_weight
        int persona_version_at_write
    }

    DYAD {
        id dyad_id
        float rapport
        float trust
        float reciprocity
        int episode_count
    }

    EVAL_RUN {
        id run_id
        json aggregated_signal
        bool prefilter_blocked
    }

    EVAL_SIGNAL {
        id signal_id
        string evaluator_name
        string dimension
        float score
        float confidence
    }

    TIMELINE_ENTRY {
        id entry_id
        json dim_scores
        float drift_norm
        list anomaly_flags
        enum provenance
    }

    ANOMALY_EVENT {
        id event_id
        enum kind
        enum severity
        json payload
        bool requires_review
        datetime resolved_at
    }

    HITL_ACTION {
        id action_id
        string reviewer_id
        enum action
        json score_overrides
    }

    EPISODE_CORRECTION {
        id correction_id
        enum scope
        enum classification
        string correction_text
        float authority_weight
        json coherence_check
        json minimum_update_set
    }

    SYNTHETIC_EPISODE {
        id episode_id
        string corrected_response
        float authority_weight
        enum provenance
    }

    TWO_REVIEWER_REQUEST {
        id request_id
        string first_reviewer_id
        string second_reviewer_id
        enum status
        json encoded_intervention
    }

    AGENT ||--o{ EPISODE : "executes"
    AGENT ||--o{ DYAD : "participates in"
    AGENT ||--o{ EVAL_RUN : "is evaluated by"
    AGENT ||--o{ TIMELINE_ENTRY : "has timeline"
    AGENT ||--o{ ANOMALY_EVENT : "triggers"

    EPISODE }o--|| DYAD : "belongs to"
    EPISODE ||--o{ EVAL_SIGNAL : "scored by"
    EPISODE ||--o{ TIMELINE_ENTRY : "projected into"
    EPISODE ||--o{ EPISODE_CORRECTION : "corrected by"

    EVAL_RUN ||--o{ EVAL_SIGNAL : "contains"
    EVAL_RUN ||--o{ TIMELINE_ENTRY : "feeds"

    TIMELINE_ENTRY ||--o{ ANOMALY_EVENT : "triggers"

    ANOMALY_EVENT ||--o{ HITL_ACTION : "reviewed via"
    ANOMALY_EVENT ||--o| TWO_REVIEWER_REQUEST : "escalated to"

    HITL_ACTION ||--o| EPISODE_CORRECTION : "produces"

    EPISODE_CORRECTION ||--o| SYNTHETIC_EPISODE : "creates"
    SYNTHETIC_EPISODE }o--|| AGENT : "re-injected into"
    EPISODE_CORRECTION }o--|| AGENT : "may bump persona version of"
```

### Key ontological relationships

| Relationship | Meaning |
|---|---|
| `AGENT` → `EPISODE` | An agent's execution history. Every interaction produces an episode. |
| `EPISODE` → `DYAD` | Episodes between the same agent and human are grouped into a dyad, which tracks relational dynamics over time. |
| `EPISODE` → `EVAL_SIGNAL` | One episode may be scored by multiple evaluators across multiple dimensions. |
| `EVAL_SIGNAL` → `TIMELINE_ENTRY` | Scores are projected into a denormalized timeline entry, the primary read surface for trend analysis. |
| `TIMELINE_ENTRY` → `ANOMALY_EVENT` | When the timeline reveals drift, conflict, rupture, or safety issues, an anomaly event is raised. |
| `ANOMALY_EVENT` → `HITL_ACTION` | A human reviewer acts on the anomaly — approving, relabelling, or intervening. |
| `HITL_ACTION` → `EPISODE_CORRECTION` | An intervention produces an immutable correction record. |
| `EPISODE_CORRECTION` → `SYNTHETIC_EPISODE` | The correction is materialized as a new high-authority episode that feeds back into the agent's behavioral baseline. |
| `AGENT.persona_version` | Increments on system-prompt edits and agent-wide interventions, creating version boundaries that the drift monitor tracks across. |

---

## 3. Logical Module Map

The following diagram shows the logical modules of the system and their
dependency relationships. Each logical module is named on the left; its
implementing Rust crate is shown on the right.

```mermaid
graph LR
    subgraph Foundation ["Plane A — Foundation"]
        AM["Agent Model\n(identity, persona version,\ncapability gates)"]
        EM["Episode Store\n(provenance, authority weight,\ndyad identity)"]
        EB["Episode Bundle\n(normalized evaluator input)"]
        AM --- EM
        EM --> EB
    end

    subgraph Registry ["Plane B — Evaluator Registry"]
        PF["Pre-filter Tier\n(serial, can short-circuit)"]
        DT["Dimensional Tier\n(concurrent, multi-score)"]
        AG["Aggregator\n(confidence-weighted mean,\nconflict detection)"]
        PF --> DT
        DT --> AG
    end

    subgraph Observer ["Plane C — Longitudinal Observer"]
        SC["Episode Scorer\n(inline timeline writer)"]
        DM["Persona Drift Monitor\n(cosine distance,\nthreshold classification)"]
        ST["Social Tracker\n(EWMA rapport/trust/reciprocity,\nrupture detection)"]
        AD["Anomaly Detector\n(drift · conflict · rupture · safety)"]
        TA["Trend Analyser\n(on-demand window statistics)"]
        BW["Background Worker\n(two-pass incremental scan)"]
        SC --> DM
        SC --> ST
        DM --> AD
        ST --> AD
        BW --> DM
        BW --> AD
        BW --> TA
    end

    subgraph HITL ["Plane D — Human Interface"]
        OB["Observatory UI\n(timeline · anomalies · dyads)"]
        RQ["Review Queue\n(pending anomalies)"]
        IE["Intervention Encoder\n(scope · classification · authority)"]
        CG["Coherence Gate\n(TEC settling, Γ(C) threshold)"]
        TW["Two-Write Memory\n(synthetic episode + annotation)"]
        TR["Two-Reviewer Consensus\n(agent-wide escalation)"]
        OB --> RQ
        RQ --> IE
        IE --> CG
        CG --> TW
        CG --> TR
        TR --> TW
    end

    EB --> PF
    EB --> DT
    AG --> SC
    AD -->|AnomalyEvent| RQ
    TW -->|SyntheticEpisode| EM
    TW -->|Correction| EM
    TW -->|Bump persona version| AM
```

### Logical → physical crate mapping

| Logical Module | Rust Crate | Key Types |
|---|---|---|
| Agent Model, Episode Store, Episode Bundle | `agent-bestiary-memory` | `Agent`, `Episode`, `EpisodeBundle`, `MemoryStore` |
| Pre-filter Tier, Dimensional Tier, Aggregator | `agent-bestiary-evaluators` | `EvalModel`, `EvalTier`, `EvaluatorRegistry`, `Aggregator`, `AggregatedSignal` |
| Episode Scorer, Drift Monitor, Social Tracker, Anomaly Detector, Trend Analyser, Background Worker | `agent-bestiary-observability` | `EpisodeScorer`, `PersonaDriftMonitor`, `SocialInteractionTracker`, `AnomalyDetector`, `TrendAnalyzer`, `ObservabilityWorker` |
| Intervention Encoder, Coherence Gate, Two-Write Memory | `agent-bestiary-coherence-gate` | `InterventionEncoder`, `CoherenceGate`, `TwoWriteMemory` |
| Observatory UI, Review Queue (HTTP handlers) | `fermi` (application) | `src/handlers/observatory.rs` |
| LLM Judge, Brier Lookup (production EvalModel adapters) | `fermi` (application) | `src/handlers/eval_judge.rs`, `src/handlers/eval_brier.rs` |

---

## 4. Interaction Diagrams

### 4.1 Eval pipeline → evaluator registry → timeline write (hot path)

This is the primary data ingestion path. It runs synchronously for every eval
case and ends with a non-blocking background worker spawn.

```mermaid
sequenceDiagram
    actor User
    participant EP as Eval Pipeline<br/>(fermi handler)
    participant EX as Agent Executor
    participant ES as Episode Store
    participant EB as Episode Bundle
    participant REG as Evaluator Registry
    participant PF as Pre-filter Tier
    participant DT as Dimensional Tier
    participant AGG as Aggregator
    participant SC as Episode Scorer<br/>(inline writer)
    participant BW as Background Worker

    User->>EP: POST /api/agents/:id/eval
    EP->>EX: execute(agent, query)
    EX-->>EP: AgentOutput { response, tokens }

    EP->>ES: store_episode(provenance=auto_pass, authority_weight=0.5,<br/>dyad_id, persona_version_at_write)
    ES-->>EP: episode_id

    EP->>EB: from_parts(episode, agent, transcript, goal_spec)
    EB-->>EP: EpisodeBundle

    EP->>REG: run(bundle)

    REG->>PF: evaluate(bundle) [serial]
    alt pre-filter blocks
        PF-->>REG: score < 0.5 → short-circuit
        REG-->>EP: RegistryOutcome { prefilter_blocked=true }
    else pre-filter passes
        PF-->>REG: EvalResult (safety/grounding OK)
        REG->>DT: evaluate(bundle) [concurrent: judge, brier, sotopia, ...]
        DT-->>REG: Vec<EvalResult>
        REG->>AGG: aggregate(results)
        AGG-->>REG: AggregatedSignal { per_dimension, conflicts, flags }
        REG-->>EP: RegistryOutcome { signal: AggregatedSignal }
    end

    EP->>ES: bulk_insert eval_signals (per evaluator × dimension)
    EP->>ES: update eval_runs (aggregated_signal, conflict_flags, prefilter_blocked)

    EP->>SC: write_inline(episode, signal, run_id)
    SC->>ES: insert agent_timeline_entries<br/>(dim_scores, provenance, persona_version, dyad_id)

    EP-->>EP: tokio::spawn
    EP->>BW: scan_agent(agent_id) [non-blocking]
    Note over BW: runs asynchronously — see §4.2

    EP-->>User: EvalRunResult { run_id, aggregated_signal, case_results }
```

---

### 4.2 Background worker scan (drift + anomaly detection)

The worker runs after the hot path completes. It is also triggered on-demand
via the observatory UI's "Trigger Scan" button.

```mermaid
sequenceDiagram
    participant BW as Background Worker
    participant ES as Episode Store
    participant DM as Persona Drift Monitor
    participant ST as Social Tracker
    participant AD as Anomaly Detector

    BW->>ES: get_agent_observability_state(agent_id)
    ES-->>BW: ObservabilityState { last_scanned_entry_id, ... }

    BW->>ES: list_timeline_entries_since(last_scanned_entry_id, batch=200)
    ES-->>BW: Vec<TimelineEntry> [oldest-first]

    loop Pass 1 — Drift computation
        BW->>DM: compute(prev_persona_version, curr_persona_version, recent_norms)
        Note over DM: cosine_similarity(mean_embedding_vN, mean_embedding_vN+1)<br/>drift_norm = 1.0 − cosine_similarity
        DM-->>BW: DriftVector { norm, anomalous }
        alt drift is anomalous
            BW->>ES: update_timeline_entry(drift_norm, flags += "drift:anomalous")
        else not anomalous
            BW->>ES: update_timeline_entry(drift_norm)
        end
    end

    BW->>ES: re-fetch updated entries [Pass 1 flags now visible]
    ES-->>BW: Vec<TimelineEntry> [refreshed]

    loop Pass 2 — Anomaly detection
        BW->>AD: detect_in_window(agent_id, entries)
        Note over AD: Safety   → any entry with safety:* flag<br/>Drift    → any entry with drift:anomalous flag<br/>Conflict → same conflict:<dim> in N consecutive entries<br/>Rupture  → any entry with rupture:<dyad_id> flag
        AD-->>BW: Vec<DetectedAnomaly>
        loop for each anomaly
            BW->>ES: insert anomaly_events (kind, severity, payload, requires_review=true)
        end
    end

    BW->>ES: upsert agent_observability_state<br/>(last_scanned_entry_id, counters, timestamps)
```

---

### 4.3 HITL action → coherence gate → two-write feedback loop

This diagram covers the full intervention path from a reviewer's action in the
observatory UI to the memory write that closes the improvement loop. It shows
both the `episode`/`dyad` path (immediate write) and the `agent_wide` path
(two-reviewer consensus required).

```mermaid
sequenceDiagram
    actor R1 as Reviewer 1
    actor R2 as Reviewer 2
    participant RQ as Review Queue API
    participant IE as Intervention Encoder
    participant CG as Coherence Gate<br/>(TEC Settling Engine)
    participant TR as Two-Reviewer<br/>Consensus Store
    participant TW as Two-Write Memory
    participant ES as Episode Store
    participant AM as Agent Model

    R1->>RQ: POST /hitl/:event_id/action<br/>{ action: "intervene", scope, classification,<br/>correction_text, dimension, justification }

    RQ->>IE: encode(InterventionRequest)
    Note over IE: Validates: agent_wide requires classification + correction_text<br/>Stamps: authority_weight=1.0, provenance=HumanCorrected<br/>Sets: gate_is_synchronous = (scope == AgentWide)
    IE-->>RQ: EncodedIntervention

    RQ->>CG: check(encoded)
    Note over CG: Builds 2-utterance TEC system:<br/>U0 = existing agent response<br/>U1 = proposed correction<br/>Relation: Contradicts(U0, U1)<br/>Runs SettlingEngine → computes Γ(C)

    alt AgentWide AND Γ(C) < 0.5
        CG-->>RQ: Err(Blocked { gamma, tensions })
        RQ-->>R1: HTTP 422 — coherence gate blocked
    else Episode/Dyad OR Γ(C) ≥ 0.5
        CG-->>RQ: GateOutcome { verdict, gamma, principle_scores,<br/>tensions, minimum_update_set }

        alt scope == AgentWide
            RQ->>TR: create_two_reviewer_request<br/>(encoded_intervention as JSONB, first_reviewer_id)
            TR-->>RQ: request_id
            RQ-->>R1: HTTP 200 — awaiting_second_reviewer { request_id }

            R2->>RQ: POST /hitl/consensus/:request_id { approved: true }
            Note over RQ: Enforces: second_reviewer ≠ first_reviewer
            RQ->>CG: re-check(encoded) [fresh gate run]
            CG-->>RQ: GateOutcome
            RQ->>TW: execute(encoded, gate_outcome, original_episode)
        else scope == Episode or Dyad
            RQ->>TW: execute(encoded, gate_outcome, original_episode)
        end

        TW->>ES: store_episode(SyntheticEpisode)<br/>provenance=SyntheticCorrection, authority_weight=1.0
        ES-->>TW: synthetic_episode_id

        TW->>ES: create_episode_correction<br/>(coherence_check, minimum_update_set,<br/>tensions_flagged, synthetic_episode_id)
        ES-->>TW: correction_id

        alt scope == AgentWide
            TW->>AM: bump_persona_version(agent_id)
            AM-->>TW: new_persona_version
        end

        TW-->>RQ: TwoWriteReceipt { correction_id, synthetic_episode_id,<br/>persona_version_bumped, new_persona_version }

        RQ->>ES: resolve_anomaly_event(event_id)
        RQ-->>R1: HTTP 200 — intervention_complete { correction_id, synthetic_episode_id, gate }
    end
```

---

### 4.4 Two-reviewer consensus flow (agent-wide scope detail)

This diagram isolates the two-reviewer workflow to show the state machine of
a `two_reviewer_requests` record across its lifecycle.

```mermaid
sequenceDiagram
    actor R1 as Reviewer 1
    actor R2 as Reviewer 2
    participant API as Observatory API
    participant TR as Consensus Store
    participant TW as Two-Write Memory
    participant ES as Episode Store
    participant AM as Agent Model

    Note over TR: Precondition: Coherence gate approved (Γ(C) ≥ 0.5)

    R1->>API: POST /hitl/:event_id/action { scope: "agent_wide", ... }
    API->>TR: create_two_reviewer_request<br/>{ status: pending, encoded_intervention: JSONB,<br/>first_reviewer_id, first_reviewed_at }
    TR-->>API: request_id
    Note over TR: Unique partial index: one pending request per anomaly
    API-->>R1: { status: awaiting_second_reviewer, request_id }

    Note over R2: R2 independently reviews the pending request

    alt R2 rejects
        R2->>API: POST /hitl/consensus/:request_id { approved: false }
        API->>TR: update status = rejected
        TR-->>API: ok
        API-->>R2: { status: rejected }
        Note over TR: Anomaly remains open in the HITL queue
    else R2 approves
        R2->>API: POST /hitl/consensus/:request_id { approved: true }
        Note over API: Enforces R2 ≠ R1
        API->>TR: deserialize stored EncodedIntervention
        API->>TW: execute(encoded, gate_outcome, original_episode)

        TW->>ES: Write 1 — store synthetic episode<br/>(SyntheticCorrection, authority_weight=1.0)
        TW->>ES: Write 2 — create episode_correction<br/>(coherence_check, minimum_update_set)
        TW->>AM: bump_persona_version(agent_id)
        AM-->>TW: new_persona_version
        TW-->>API: TwoWriteReceipt

        API->>TR: update status = approved<br/>{ correction_id, synthetic_episode_id,<br/>second_reviewer_id, second_reviewed_at }
        API->>ES: resolve_anomaly_event
        API-->>R2: { status: intervention_complete, correction_id,<br/>synthetic_episode_id, new_persona_version }
    end
```

---

## 5. State Machine — Anomaly Event Lifecycle

An anomaly event passes through a well-defined set of states from detection to
resolution. This state machine is the operational contract between Plane C
(which creates events) and Plane D (which resolves them).

```mermaid
stateDiagram-v2
    [*] --> Detected : AnomalyDetector fires\n(requires_review = true)

    Detected --> Approved : Reviewer approves\n(hitl_action: approve)
    Detected --> Relabelled : Reviewer corrects scores\n(hitl_action: relabel)
    Detected --> InterventionPending : Reviewer intervenes\n(Episode / Dyad scope)
    Detected --> AwaitingSecondReviewer : Reviewer intervenes\n(AgentWide scope)

    AwaitingSecondReviewer --> Rejected : Second reviewer rejects
    AwaitingSecondReviewer --> InterventionPending : Second reviewer approves
    Rejected --> Detected : Anomaly re-enters queue\n(resolved_at remains null)

    InterventionPending --> Intervened : TwoWriteMemory executes\n(synthetic episode + correction)

    Approved --> [*] : resolved_at set
    Relabelled --> [*] : resolved_at set
    Intervened --> [*] : resolved_at set
```

---

## 6. Logical Data Flow — End to End

This diagram shows how a single agent execution event flows through the entire
system and loops back as a corrective signal.

```mermaid
flowchart TD
    EX["Agent Execution\n(query → response)"]
    EP["Episode\n(provenance · authority_weight\ndyad_id · persona_version_at_write)"]
    BND["Episode Bundle\n(transcript · agent card · goal spec)"]

    subgraph Eval ["Evaluator Registry"]
        PF2["Pre-filter Tier\n(WildGuard · Faithfulness)"]
        DT2["Dimensional Tier\n(Sotopia · CharacterEval\nLifelongBench · Brier)"]
        SIG["AggregatedSignal\n(per-dimension means · conflicts · flags)"]
        PF2 -->|pass| DT2
        PF2 -->|block| SIG
        DT2 --> SIG
    end

    subgraph Longitudinal ["Longitudinal Observer"]
        TL["Timeline Entry\n(dim_scores · drift_norm · anomaly_flags)"]
        DS["Dyad State\n(rapport · trust · reciprocity)"]
        AE["Anomaly Events\n(drift · conflict · rupture · safety)"]
        TL --> AE
        DS --> AE
    end

    subgraph Human ["Human Interface"]
        HQ["HITL Review Queue"]
        HA["Reviewer Decision\n(approve · relabel · intervene)"]
        GC["Coherence Gate\nΓ(C) check"]
        TRC["Two-Reviewer Consensus\n(agent_wide only)"]
        HQ --> HA
        HA --> GC
        GC -->|agent_wide| TRC
        TRC -->|approved| TW2
        GC -->|episode / dyad| TW2
    end

    subgraph Feedback ["Memory Feedback"]
        TW2["Two-Write Memory"]
        SE["Synthetic Episode\n(SyntheticCorrection · authority_weight=1.0)"]
        CR["Correction Record\n(coherence_check · minimum_update_set)"]
        PV["Persona Version Bump\n(agent_wide only)"]
        TW2 --> SE
        TW2 --> CR
        TW2 -->|agent_wide| PV
    end

    EX --> EP
    EP --> BND
    BND --> Eval
    SIG --> TL
    SIG --> DS
    AE --> HQ
    SE -->|re-injected into memory| EP
    PV -->|new baseline for drift monitor| TL

    style Eval fill:#1a2a3a,color:#cde,stroke:#4a7aaa
    style Longitudinal fill:#2a1a0a,color:#edc,stroke:#aa7a4a
    style Human fill:#1a0a2a,color:#dce,stroke:#7a4aaa
    style Feedback fill:#0a2a1a,color:#ced,stroke:#4aaa7a
```

---

## 7. Module Responsibilities Summary

| Logical Module | Owns | Consumes | Produces |
|---|---|---|---|
| **Agent Model** | Agent identity, `persona_version`, capability gates | — | Agent metadata for all planes |
| **Episode Store** | Episode history, provenance, authority weight, dyad identity | Agent execution output | Episodes, EpisodeBundles, MemoryStore R/W |
| **Episode Bundle** | Normalized evaluator input contract | Raw episode + agent card | `EpisodeBundle` for the registry |
| **Pre-filter Tier** | Safety + grounding checks | `EpisodeBundle` | `EvalResult` or short-circuit signal |
| **Dimensional Tier** | Multi-dimensional behavioral scoring | `EpisodeBundle` | `Vec<EvalResult>` |
| **Aggregator** | Confidence-weighted mean, conflict detection | `Vec<EvalResult>` | `AggregatedSignal` |
| **Episode Scorer** | Inline timeline write | `AggregatedSignal` + episode | `TimelineEntry` (hot path) |
| **Persona Drift Monitor** | Cross-version embedding distance | Episode embeddings per persona version | `DriftVector` (norm, anomalous flag) |
| **Social Tracker** | Per-dyad relational state (rapport/trust/reciprocity) | `AggregatedSignal` per dyad | Updated `DyadState`, rupture flags |
| **Anomaly Detector** | Four anomaly kinds (drift/conflict/rupture/safety) | `Vec<TimelineEntry>` | `Vec<DetectedAnomaly>` |
| **Trend Analyser** | On-demand window statistics per dimension | `Vec<TimelineEntry>` | `TrendReport` |
| **Background Worker** | Two-pass incremental scan, checkpoint management | Timeline entries since last scan | Updated drift fields, `AnomalyEvent` rows, checkpoint |
| **Observatory UI** | Dashboard and HITL queue rendering | Timeline, dyad, anomaly, trend APIs | Human-readable views |
| **Intervention Encoder** | Validation and stamping of reviewer intent | Raw `InterventionRequest` | `EncodedIntervention` (authority_weight=1.0) |
| **Coherence Gate** | TEC constraint satisfaction, Γ(C) threshold | `EncodedIntervention` | `GateOutcome` (approved/settled/blocked) |
| **Two-Reviewer Consensus** | Four-eyes enforcement for agent-wide scope | `EncodedIntervention` from first reviewer | Confirmed or rejected consensus record |
| **Two-Write Memory** | Synthetic episode + immutable correction annotation | `EncodedIntervention` + `GateOutcome` | `SyntheticEpisode`, `EpisodeCorrection`, optional persona version bump |
