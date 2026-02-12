# Rabble: A Society of Drone Trainers

## The Story

A butterfly collector in Mexico City opens Rabble on her phone. She mints a Monarch butterfly --- *Danaus plexippus* --- names it Citlali, and takes it flying through Chapultepec Park. Her phone records GPS breadcrumbs: latitude, longitude, heading, timestamp. Citlali swoops between trees, follows the path along the lake, catches a thermal and spirals upward. Twenty minutes, four hundred path samples.

She doesn't know it, but she just trained a firefighting drone.

---

## How It Works

When Citlali's flight ends, the Rabble app calls `PUT /api/flights/:id/end` with the recorded path samples. If the creature owner has opted in to SOSA data sharing (a single toggle in the creature settings), a bridge fires:

1. **Path samples become universal sensor observations.** Each `{lat, lng, heading, t}` breadcrumb is converted into W3C SSN/SOSA observations --- the same vocabulary used by weather stations, industrial sensors, and scientific instruments. Longitude becomes `onto4mat:xLocation`. Latitude becomes `onto4mat:yLocation`. Heading becomes `onto4mat:hasHeading`. Speed is derived via haversine from consecutive samples and recorded as `onto4mat:hasSpeed`.

2. **An observation analyst interprets the data.** A fire-and-forget agent execution runs the `observation_analyst` against the flight's SOSA observations. This agent understands the SSN/SOSA vocabulary and produces a structured analysis --- an episode with an embedding vector that captures the *meaning* of the flight pattern.

3. **The embedding enters the shared experience space.** Through the ADM consolidation cycle (dreaming), flight patterns are clustered, rules are extracted, and the agent's knowledge graph evolves. Over hundreds of flights from dozens of users, structural patterns emerge: "declining energy + tight spiral = thermal exploitation", "high speed + straight heading = transit between waypoints", "cluster of agents + low separation = formation behavior."

4. **Drones download experience, not models.** A Crazyflie drone controller calls `GET /api/observe/sessions/:id/experience` and receives a lookup table of embedding vectors paired with the actions and contexts that produced them. At runtime, the drone computes the embedding of its current sensor state, finds the nearest neighbor in the table, and executes the associated action. No neural network. No training pipeline. Just pattern matching against the accumulated wisdom of a thousand butterfly flights.

The butterfly collector's Monarch, swooping through Chapultepec, traced the same evasive pattern a drone needs when navigating through smoke between burning trees. The AR creature and the physical drone operate in different domains but produce structurally identical telemetry. The SOSA layer makes them commensurable. The embedding space makes them transferable.

---

## The Onto4MAT Bridge

The connection isn't metaphorical. It's ontological.

The Onto4MAT ontology (arxiv 2203.12955) defines the formal vocabulary for multi-agent swarm behavior: Agent, Team, Formation, Environment, Mission, Actions. Each agent has `hasSpeed`, `hasHeading`, `hasEnergy`, `hasDistanceToGoal`. Each team has `hasAlignmentWithTeam`, `hasCohesionWithTeam`, `hasTeamSeparation`.

A Rabble creature in flight is an Onto4MAT Agent. A rabble gathering is a Team. A swarm of AR dragonflies orbiting a park fountain is a Formation. The flight recording captures exactly the data properties that drone swarm controllers need.

By mapping Rabble's flight telemetry through SOSA into the same observable properties that the `swarm_coordinator` agent uses for real drone telemetry, we create a single embedding space where AR creature behavior and physical drone behavior are represented in the same coordinates. A Monarch butterfly's evasive spiral and a Crazyflie's obstacle avoidance maneuver, if structurally similar, will cluster together.

### Cross-Domain Transfer

This is not limited to drones and butterflies. The SOSA observation layer is domain-agnostic:

| Domain | Platform | Observable Properties | Transfer Value |
|--------|----------|----------------------|----------------|
| AR creatures | Rabble phone app | position, heading, speed, energy | Evasive patterns, navigation, formation |
| Drone swarms | Crazyflie/Crazyswarm2 | position, heading, speed, energy, formation metrics | Direct operational use |
| Weather stations | IoT sensors | temperature, humidity, pressure, wind | Environmental context for flight planning |
| Greenhouses | Soil/air sensors | moisture, CO2, light | Resource management patterns |
| Wearables | Health devices | heart rate, motion, location | Human-in-the-loop coordination |

A greenhouse sensor detecting declining soil moisture and a drone detecting declining battery both produce the same structural pattern: *resource depletion requiring corrective action*. In the embedding space, these patterns cluster regardless of domain. The drone can learn from the greenhouse's recovery strategy (irrigate = recharge) because the *shape* of the problem is the same.

---

## The Economic Model: Data Asset Rights

Here is where the story becomes an economy.

Every Rabble user who opts their creature into SOSA sharing is contributing a data asset --- structured observations that, in aggregate, create intelligence that has market value. The AKP (Agent Knowledge Protocol) establishes the framework for valuing and distributing the returns from this intelligence.

### Who Owns What

| Asset | Owner | How It's Created | Value |
|-------|-------|-----------------|-------|
| Raw flight path | Creature owner (user) | Flying their creature | Low individually, high in aggregate |
| SOSA observations | Creature owner (user) | Auto-generated from flight path (opt-in) | Structured, interoperable, machine-readable |
| Agent episode + embedding | Platform (observation_analyst) | Agent execution on SOSA data | Interpretive layer, pattern recognition |
| Consolidated knowledge (rules, entities) | Agent owner | ADM dreaming cycle | Compounding intellectual property |
| Experience lookup table | Platform + contributors | Aggregated embeddings from all opted-in flights | Operational intelligence for downstream consumers |

### The Consent Gate

Nothing flows without consent. The AKP roadmap mandates opt-in at every boundary:

- **`sosa_opt_in`** (boolean, default `false`) on each creature. The user must explicitly enable SOSA data sharing before any flight data leaves Rabble's domain.
- **Agent interaction policies** (migration 049) control whether an agent's knowledge can be shared, with whom, and under what terms.
- **Embedding marketplace listings** require explicit creation --- nobody's embeddings are exposed without deliberate action.

The default state is private. Every creature, every agent, every embedding starts locked. Sharing is an affirmative choice with visible consequences.

### Revenue Flows

When the experience lookup table has operational value --- when a drone company downloads it to improve their swarm controllers --- the economic flows trace back through the system:

```
Drone company purchases experience access
    |
    |--> Platform: marketplace transaction fee (15%)
    |      + gas fees on API calls (credits)
    |
    |--> Observation analyst agent owner: execution royalties
    |      (the agent that interpreted the raw data)
    |
    |--> Embedding marketplace: listing fees + match fees
    |      (the marketplace that brokered access)
    |
    |--> Creature owners: ???  <-- THIS IS THE DESIGN QUESTION
```

The open question --- and the critical one --- is how value flows back to the butterfly collector in Chapultepec. She generated the raw data. Without her flight, the embedding doesn't exist. But she didn't create the interpretation, the consolidation, or the cross-domain transfer. She contributed a data point; the system created intelligence.

### Modeling the Distribution

Three models, not mutually exclusive:

**Model A: Data Dividend**
Every creature with `sosa_opt_in = true` earns a fractional share of downstream revenue proportional to the number of observations contributed. Simple, transparent, but doesn't reward quality --- a lazy straight-line flight earns the same as a complex evasive pattern.

**Model B: Embedding Royalty**
When a specific embedding is matched in the marketplace (cosine similarity > threshold), the observations that contributed most to that embedding's formation are identified, and the creature owners who generated those observations receive a royalty. Rewards quality and uniqueness --- novel flight patterns that fill gaps in the embedding space earn more.

**Model C: Community Pool**
Users who opt in join a community pool. Pool revenue is distributed based on contribution metrics: observation count, pattern diversity, temporal coverage, geographic spread. The pool self-governs via AKP group contracts. This is the most aligned with Beckstrom's Law --- the network is only valuable if participants gain more than it costs them.

### What We Can Model Now

The credit system already tracks every transaction. We can instrument the data flow:

| Event | tx_type | Credits | Who Pays |
|-------|---------|---------|----------|
| Creature flight recording | `creature_flight` | 3 | User |
| SOSA observation generation | (free, auto) | 0 | Platform |
| Observation analyst execution | `observation_ingest` | 1 | Platform (subsidized) |
| Experience table download | `swarm_telemetry_ingest` | 1+ | Consumer |
| Embedding marketplace match | `marketplace_match_purchase` | variable | Consumer |
| Royalty payout to contributor | `marketplace_match_payout` | variable | Platform (to user) |

The `credit_ledger` is append-only. Every flow is auditable. When we introduce the distribution model, we add new `tx_type` values (`sosa_data_dividend`, `sosa_embedding_royalty`, `sosa_pool_distribution`) and the economic loop closes.

---

## The Firefighting Scenario

To make this concrete:

**Phase 1: Data Generation (Now)**
- 500 Rabble users fly creatures in parks, forests, urban areas
- 200 opt in to SOSA sharing
- Each user flies 10x/month, 400 samples/flight
- Monthly: 200 users x 10 flights x 400 samples = 800,000 SOSA observations
- The observation analyst processes these into ~2,000 episodes with embeddings

**Phase 2: Pattern Emergence (3-6 months)**
- ADM consolidation clusters flight patterns
- Rules emerge: thermal exploitation, obstacle avoidance, formation maintenance, energy-aware routing
- The experience table grows to 10,000+ embedding vectors with associated actions
- A `swarm_coordinator` agent trained on real drone telemetry finds that 30% of its embedding space overlaps with Rabble-derived embeddings

**Phase 3: Operational Value (6-12 months)**
- A wildfire response team uses Crazyflie drones for reconnaissance
- Their drones download the experience table: 10,000 embeddings covering navigation patterns in forested terrain
- At runtime, a drone entering a smoke corridor computes its current state embedding
- Nearest neighbor: an AR Monarch butterfly's evasive spiral through dense tree canopy in Chapultepec
- The drone executes the same pattern --- tight turn, altitude gain, heading correction --- and navigates through
- The response team didn't train a model. They borrowed a butterfly's instincts.

**Phase 4: Economic Return (12+ months)**
- The wildfire team's experience purchases flow through the marketplace
- Platform takes 15% transaction fee
- Observation analyst owner earns execution royalties
- Rabble users who contributed the most structurally valuable flights receive data dividends
- The butterfly collector in Mexico City earns credits for flights she took months ago
- She uses those credits to mint more creatures, fly more flights, generate more data
- The flywheel turns

---

## Community Resilience

The deeper story isn't about drones or butterflies. It's about communities building collective intelligence that protects them.

A neighborhood in fire-prone Southern California forms a Rabble community. They fly creatures through their local terrain --- the canyon behind the school, the ridge where fires approach, the evacuation routes through the hills. Their flights map the landscape in a way that satellite imagery cannot: at human scale, at walking speed, with the intuition of people who live there.

When those flight patterns are bridged to SOSA and consolidated into the experience space, they become navigational intelligence for drones that will one day patrol those same canyons, monitor those same ridges, guide evacuations along those same routes.

The community's creatures --- their digital companions --- become the teachers of the machines that protect them. The butterfly they flew through the canyon last Saturday is, in a very real sense, showing a drone how to fly that canyon when it matters.

And because the economic model traces value back to contributors, the community that generates the most useful intelligence earns the most from it. Resilience pays. Local knowledge has market value. The people most at risk from wildfire are also the people best positioned to generate the intelligence that mitigates it.

This is the promise: **your companions build the intelligence of your community's resilience.**

---

## Technical Pattern Summary

### The Pipeline

```
Rabble Flight
  |
  | PUT /api/flights/:id/end  { path_samples: [{lat, lng, heading, t}, ...] }
  |
  v
SOSA Bridge (fire-and-forget, opt-in gated)
  |
  | 1. Check creature.sosa_opt_in (AKP consent)
  | 2. Get/create sosa_platform for creature
  | 3. Create observation_session for flight
  | 4. Convert each sample to SOSA observations:
  |    - onto4mat:xLocation (lng, deg)
  |    - onto4mat:yLocation (lat, deg)
  |    - onto4mat:hasHeading (heading, deg)
  |    - onto4mat:hasSpeed (haversine-derived, m/s)
  | 5. Close observation session
  |
  v
Auto-Execute (observation_analyst agent)
  |
  | Interprets SOSA observations
  | Produces episode + embedding
  |
  v
ADM Consolidation (dreaming)
  |
  | Clusters episodes, extracts rules
  | Knowledge graph evolves
  |
  v
Experience Export
  |
  | GET /api/observe/sessions/:id/experience
  | Returns [{embedding, action, query, status}, ...]
  |
  v
Device Runtime (drone/robot)
  |
  | Compute current state embedding
  | Nearest-neighbor lookup
  | Execute associated action
```

### Key Design Decisions

1. **SOSA as universal layer.** W3C SSN/SOSA provides domain-agnostic sensor vocabulary. Any platform that emits SOSA observations can contribute to and benefit from the shared experience space.

2. **Embeddings over models.** Nearest-neighbor lookup on embedding vectors instead of trained neural networks. No training pipeline, no GPU requirements, works on microcontrollers. Each new observation enriches the space immediately.

3. **Opt-in by default.** `sosa_opt_in = false` on every creature. The consent gate is checked before any data bridge fires. No silent data collection.

4. **Fire-and-forget execution.** The SOSA bridge runs in a `tokio::spawn` after the flight API response is sent. The user gets instant flight confirmation; the intelligence pipeline runs asynchronously.

5. **Append-only economics.** Every credit movement is recorded in the `credit_ledger`. When distribution models are implemented, the audit trail is already complete.

### Database Schema

```
creatures
  +-- sosa_opt_in (boolean, default false)

sosa_platforms
  +-- platform_id, owner_id, name, platform_type

sosa_sensors
  +-- sensor_id, platform_id, observable_property, unit

observation_sessions
  +-- session_id, owner_id, platform_id, name, status

sosa_observations
  +-- observation_id, session_id, platform_id
  +-- observable_property, feature_of_interest
  +-- result_value, result_unit, phenomenon_time

swarm_sessions
  +-- session_id, owner_id, name, formation_type, mission_type

swarm_telemetry
  +-- telemetry_id, session_id, agent_label
  +-- x_location, y_location, z_location, heading, speed, energy
  +-- team_alignment, team_cohesion, team_separation

episodes (existing)
  +-- embedding (1024D pgvector)
  +-- agent_id (observation_analyst or swarm_coordinator)
```

### API Endpoints

| Endpoint | Purpose |
|----------|---------|
| `PUT /api/creatures/:id/sosa-opt-in` | Toggle SOSA data sharing (AKP consent) |
| `POST /api/observe/sessions` | Create observation session (2cr) |
| `POST /api/observe/sessions/:id/observations` | Ingest SOSA observations (1cr/batch) |
| `GET /api/observe/sessions/:id/observations` | Query observations (filterable) |
| `GET /api/observe/sessions/:id/summary` | Aggregated metrics |
| `GET /api/observe/sessions/:id/experience` | Embedding lookup table for devices |
| `POST /api/swarm/sessions` | Create drone telemetry session (2cr) |
| `POST /api/swarm/sessions/:id/telemetry` | Ingest Onto4MAT telemetry (1cr/batch) |
| `GET /api/swarm/sessions/:id/experience` | Drone experience lookup table |

### Agents

| Agent | Role |
|-------|------|
| `swarm_coordinator` | Onto4MAT drone telemetry analysis, formation detection, anomaly flagging |
| `observation_analyst` | Domain-agnostic SSN/SOSA interpretation, cross-domain pattern recognition |

---

## What Comes Next

1. **MQTT + real-time streaming.** Replace batch REST ingestion with embedded MQTT broker (rumqttd) and WebSocket streaming. Enables live telemetry dashboards and real-time formation detection.

2. **Distribution model implementation.** Choose and implement Model A, B, or C for tracing value back to data contributors. Add `tx_type` values to credit_ledger.

3. **Swarm coordinator workspace tool.** Let the `swarm_coordinator` be hired into workspaces where it can analyze telemetry in conversation, recommend formations, and coordinate with other agents.

4. **Cross-domain coherence evaluation.** Use the coherence engine to evaluate whether knowledge transferred between domains (AR creature -> drone) maintains explanatory coherence. Does the butterfly's evasive pattern *actually* make sense for a drone in smoke?

5. **GNN layer (Xaman Ek).** When enough SOSA observations flow through the system, the graph neural network layer can learn the topology of cross-domain transfer --- predicting which creature flights will be most valuable for which operational domains before the transfer happens.

---

*Your companion traces a path through the park. A drone follows it through the fire.*
