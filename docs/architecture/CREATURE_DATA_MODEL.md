# Creature Data Model — Clean End State

## Design Intent

A creature is a **versioned wiki page**. Each state transition creates a new version.
The current version is the live state. The history is immutable and flows into ABW
(agent episodic memory, bitemporal). The loosely coupled modules (SOSA, tether,
devices, formations, flocking) attach to state contexts — they are never gatekeepers
of the core lifecycle.

### Governing Principles (from STATE_OF_PROJECT_FEB12.md)

1. **Credits are a flow, not a balance** — every state transition creates flow through agents
2. **Agents get paid to think, not to parrot** — state transitions dispatch to agents (episodes created); reads serve what agents already produced
3. **If the agent won't learn from it, don't invoke the agent** — display is infrastructure, cognition is agent work

### Creature-Specific Corollaries

- The creature **owns its social conditions** (walk_in_price, visibility) — a location is just coordinates
- A creature can only be **in one place** — if it joins a rabble, it inherits the rabble's location
- **Flight is a first-class state**, not a gap between perches — it has its own agentic workflow, telemetry, and workspace context
- Every state transition is an **agent invocation** — agents learn, credits flow, episodes accumulate
- The creature's identity (species, art, owner) is **write-once**; everything else is versioned state

---

## State Machine

```
                    ┌──────────────┐
                    │              │
         ┌────────►│     FLY      │◄────────┐
         │         │              │         │
         │         └──────┬───────┘         │
         │                │                 │
         │           lands at               │
         │           destination            │
         │                │                 │
         │                ▼                 │
    fly to new    ┌──────────────┐    fly to new
    location      │              │    location
         │        │  PERCH:SOLO  │         │
         │        │              │         │
         │        └───┬─────┬───┘         │
         │            │     │              │
         ├────────────┘     │              │
         │                  │ join         │
         │                  ▼              │
         │           ┌──────────────┐      │
         │           │              │      │
         └───────────│ PERCH:RABBLE │──────┘
                     │              │
                     └──────────────┘
```

Three states. Every arrow is a state transition that:
1. Creates a new **creature_version** (immutable)
2. Dispatches to **agents** (episode created, credits charged)
3. Updates the **current_state** pointer

### State Definitions

| State | Location | Association | Workspace Context |
|-------|----------|-------------|-------------------|
| `PERCH:SOLO` | Own coordinates (lat, lng, h3_cell) | None — defines join conditions | Personal menagerie workspace |
| `PERCH:RABBLE` | Inherits rabble location | Member of one rabble | Rabble workspace (shared) |
| `FLY` | In transit (from → to) | Temporarily dissociated | Flight workspace (navigator, flight_coordinator active) |

### State Transitions

| From | To | Trigger | Agents Invoked | Credits |
|------|----|---------|----------------|---------|
| *(initial)* | `PERCH:SOLO` | `perch` (first placement) | naturalist (field notes), navigator (location context) | 2cr |
| `PERCH:SOLO` | `FLY` | `fly` | flight_coordinator (plan), navigator (route) | 1cr |
| `PERCH:RABBLE` | `FLY` | `fly` (leave rabble) | flight_coordinator, navigator, swarm_host (departure notice) | 1cr |
| `FLY` | `PERCH:SOLO` | `land` (no rabble at destination) | navigator (arrival context) | 0cr (included in fly) |
| `FLY` | `PERCH:RABBLE` | `land` + `join` (rabble at destination) | swarm_host (welcome), keeper (log) | walk_in_price or 1cr |
| `PERCH:SOLO` | `PERCH:RABBLE` | `join` (rabble at same location) | swarm_host (welcome), keeper (log) | walk_in_price or 1cr |
| `PERCH:RABBLE` | `PERCH:SOLO` | `leave` | swarm_host (farewell), keeper (log) | 0cr |

---

## Data Model

### Core Tables (the wiki page)

#### `creatures` — Identity (write-once)

These fields are set at mint and rarely change. This is the creature's "page header."

```
creature_id         UUID PK
owner_id            TEXT FK → users
scientific_name     TEXT NOT NULL
species_group       TEXT NOT NULL        -- butterfly, dragonfly, beetle, bee, ...
gbif_key            INTEGER
taxonomy            JSONB                -- full GBIF taxonomy
specimen_name       TEXT                 -- display name (owner can rename)
variation_notes     TEXT                 -- naturalist's field journal entry
asset_path          TEXT                 -- generated art reference
created_at          TIMESTAMPTZ
```

#### `creature_state` — Current State (mutable, single row per creature)

The "current version" pointer. Always exactly one row per creature.

```
creature_id         UUID PK FK → creatures
state               TEXT NOT NULL CHECK (state IN ('perch_solo', 'fly', 'perch_rabble'))
location_lat        DOUBLE PRECISION     -- current coordinates (own or inherited from rabble)
location_lng        DOUBLE PRECISION
h3_cell             TEXT                 -- H3 index for spatial queries
rabble_id           UUID FK → swarm_events  -- NULL when solo or flying
workspace_id        UUID FK → teams      -- active workspace context
version_id          UUID FK → creature_versions  -- pointer to current version
updated_at          TIMESTAMPTZ
```

#### `creature_conditions` — Social Attributes (owner-defined)

What the creature defines about how others can interact with it.
Lives on the creature, not the location — "the food truck decides its menu, not the parking spot."

```
creature_id         UUID PK FK → creatures
visibility          TEXT DEFAULT 'public' CHECK (visibility IN ('public', 'contacts_only', 'private'))
walk_in_price       INTEGER              -- NULL = private, 0 = free, N = cover charge
sosa_opt_in         BOOLEAN DEFAULT false
active_modules      TEXT[]               -- ['tether', 'sosa', 'flock'] — which loose couplings are active
updated_at          TIMESTAMPTZ
```

### Versioned History (the immutable backend)

#### `creature_versions` — Every State Transition

Each row is a fact. The creature's "edit history." This is the primary source for
agent episodic memory and bitemporal queries.

```
version_id          UUID PK
creature_id         UUID FK → creatures
version_number      INTEGER NOT NULL     -- monotonically increasing per creature
state               TEXT NOT NULL        -- state AFTER this transition
previous_state      TEXT                 -- state BEFORE (NULL for initial perch)

-- Location at this version
location_lat        DOUBLE PRECISION
location_lng        DOUBLE PRECISION
h3_cell             TEXT
rabble_id           UUID                 -- if joining/in a rabble

-- Transition metadata
transition_type     TEXT NOT NULL        -- 'perch', 'fly', 'land', 'join', 'leave'
triggered_by        TEXT NOT NULL        -- user_id who initiated

-- Agent work product
episode_ids         UUID[]               -- episodes created by agents during this transition
workspace_id        UUID                 -- workspace context active during transition

-- Bitemporal
valid_from          TIMESTAMPTZ NOT NULL DEFAULT NOW()  -- when this state became true in the world
recorded_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()  -- when we recorded it (transaction time)

-- Immutable
metadata            JSONB                -- transition-specific data (flight plan, join conditions, etc.)
```

**Indexes:**
```sql
CREATE INDEX idx_cv_creature_version ON creature_versions(creature_id, version_number DESC);
CREATE INDEX idx_cv_creature_valid ON creature_versions(creature_id, valid_from DESC);
CREATE INDEX idx_cv_state ON creature_versions(state);
CREATE INDEX idx_cv_rabble ON creature_versions(rabble_id) WHERE rabble_id IS NOT NULL;
```

### Flight Context (attached to FLY state versions)

#### `flight_telemetry` — Observations During Flight

Replaces the current overloaded `creature_flights` table. Each row is a telemetry
observation attached to a specific version (which is always a FLY-state version).

```
telemetry_id        UUID PK
version_id          UUID FK → creature_versions  -- the FLY version this belongs to
creature_id         UUID FK → creatures           -- denormalized for query efficiency

-- Position
lat                 DOUBLE PRECISION
lng                 DOUBLE PRECISION
altitude_m          DOUBLE PRECISION
heading             DOUBLE PRECISION

-- Source
data_source         TEXT DEFAULT 'app'   -- 'app', 'gps_tracker', 'meshtastic', 'manual'
device_id           UUID FK → creature_devices   -- NULL if from app

-- Temporal
observed_at         TIMESTAMPTZ NOT NULL  -- when this position was observed
recorded_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
```

**Index:**
```sql
CREATE INDEX idx_ft_version ON flight_telemetry(version_id, observed_at);
CREATE INDEX idx_ft_creature ON flight_telemetry(creature_id, observed_at DESC);
```

### Rabble Context (attached to PERCH:RABBLE state)

The existing `swarm_events`, `rabble_messages`, `swarm_sub_flocks` tables are already
correctly modeled — they describe the rabble, not the creature. No changes needed.

The **join** between creature and rabble is the `creature_state.rabble_id` FK plus the
`creature_versions` row recording the join transition.

### Art & Animation (loosely coupled, identity-adjacent)

```
creature_images          — unchanged (binary storage, one per creature)
creature_animation_layers — unchanged (body/left_wing/right_wing layers)
```

These attach to identity, not state. Art doesn't change when the creature flies.

### Loosely Coupled Modules

These modules activate **in context** — when `creature_conditions.active_modules`
includes them. They read/write their own tables and attach to versions or states.

#### Tether Module (live GPS tracking)

```
creature_tethers    — unchanged (tether_id, creature_id, tether_type, active)
telemetry_points    — unchanged (point_id, tether_id, lat, lng, recorded_at)
```

Activates when `'tether' IN active_modules`. Tether telemetry flows into
`flight_telemetry` via the tether's data_source. The tether module is a
**producer** of telemetry, not a separate state.

#### SOSA Module (W3C observation framework)

```
sosa_platforms          — unchanged
sosa_observations       — unchanged
observation_sessions    — unchanged
```

Activates when `creature_conditions.sosa_opt_in = true`. SOSA observations
are created during FLY state transitions and attached to the version's episode.
This is the **high-value loose coupling** — real telemetry flowing into agent
episodic memory for embedding generation.

#### Device Module (hardware pairing)

```
creature_devices    — unchanged (device_id, creature_id, device_type, is_active)
```

Devices produce telemetry. They don't gate state transitions.

#### Formation Module (swarm algorithms)

```
swarm_algorithms    — unchanged (algorithm_id, formation_spec, cost_credits)
swarm_activations   — unchanged (activation_id, algorithm_id, user_id, swarm_id)
swarm_sub_flocks    — unchanged (sub_flock_id, swarm_id, formation_algorithm_id)
```

Activates in PERCH:RABBLE context when the rabble has purchased formations.
The formation module is a **consumer** of creature positions, producing
coordinated movement patterns that the AR viewer renders.

---

## Agent Coupling

Every state transition dispatches to agents. This is how agents learn and credits flow.

### System Agents Per Context

| Context | Agents | What They Learn |
|---------|--------|-----------------|
| **Menagerie** (personal workspace) | naturalist, keeper | Species knowledge, owner behavior patterns |
| **Rabble** (shared workspace) | swarm_host, keeper, navigator, naturalist, rabble_anchor_manager, rabble_lifecycle_coordinator, flight_coordinator | Group dynamics, location patterns, social economics |
| **Flight** (transition workspace) | flight_coordinator, navigator | Route optimization, telemetry interpretation |

### Credit Flow Per Transition

```
User initiates transition (e.g. "fly")
  │
  ├─► charge_gas(user_wallet, fly_fee)           — user pays
  │
  ├─► dispatch_rabble_action(workspace, agent, action, query)
  │     │
  │     ├─► agent executes → episode created      — agent learns
  │     ├─► charge_and_distribute()               — 10% platform, 90% split among agents
  │     └─► episode → ADM → consolidation → KG    — knowledge grows
  │
  ├─► create creature_version                     — immutable history
  │
  └─► update creature_state                       — current state pointer moves
```

### Valence

Each agent carries valence metadata (primary_affect, arousal, personality_traits).
State transitions modulate valence:

- **perch** (arrival, settling): low arousal, high valence — contentment
- **fly** (movement, exploration): high arousal, positive valence — excitement
- **join** (social, belonging): medium arousal, high valence — connection
- **leave** (departure, change): medium arousal, mixed valence — bittersweet

The workspace context inherits the combined valence of its active agents,
influencing response tone and narrative style. The dream narrator uses
accumulated valence to color consolidation narratives.

### Economic Hooks

Every agent invocation MUST:

1. **Create an episode** — the agent learned something
2. **Charge execution_fee** — credits flow through the agent
3. **Record in credit_ledger** — append-only audit trail
4. **Produce embeddings** — the episode enters the embedding space

Every read of agent-produced data MUST:

1. **Charge platform_read** — platform earns, demand signal recorded
2. **NOT invoke the agent** — the agent already got paid when it thought

The **read-to-execute ratio** per agent reveals durable value: high ratio means
the agent produces knowledge people keep coming back to read.

---

## Handler Decomposition

`creatures.rs` (5,264 lines) splits into modules aligned with the state machine:

```
src/handlers/
  creature/
    mod.rs              — re-exports, shared types (Creature, CreatureState)
    identity.rs         — mint, update_name, generate_art, animate
    state.rs            — perch, fly, land, join, leave (the state machine)
    conditions.rs       — set_visibility, set_walk_in_price, toggle_modules
    history.rs          — list_versions, get_version, flight_telemetry
    query.rs            — list_creatures, get_creature, search, visible_flights
```

Each module is small, focused, and has clear agent coupling:

| Module | Agent Invocations | Platform Reads | Free |
|--------|-------------------|----------------|------|
| `identity.rs` | mint (naturalist), generate_art (via Gemini tool) | — | get creature detail |
| `state.rs` | perch/fly/join/leave (multiple agents per transition) | — | — |
| `conditions.rs` | — (owner config, no agent work) | — | all (owner manages own creature) |
| `history.rs` | — | version list, telemetry queries | — |
| `query.rs` | — | visible_flights | creature list, creature detail |

---

## Flutter Widget Mapping

The creature_screen tabs map directly to the state machine:

### Actions Tab

Renders **different widgets based on `creature_state.state`**:

| State | Primary Actions | Secondary Actions |
|-------|----------------|-------------------|
| `perch_solo` | Fly, Set Conditions | Toggle Modules, Tether |
| `perch_rabble` | Fly (leave), Chat, Flock | View Rabble, Toggle Modules |
| `fly` | — (in transit, view telemetry) | Cancel Flight |

No more inferring state from `_hasActiveFlight` or `flights.any(endedAt == null)`.
The state is declarative: `creature_state.state` tells you exactly what to render.

### Live Tab

Renders **active modules for current state context**:

| State | Live Content |
|-------|-------------|
| `perch_solo` + tether | GPS track, telemetry stream |
| `perch_rabble` | Rabble chat, member list, flock positions |
| `fly` | Route progress, telemetry, flight plan |
| Any + SOSA | Observation stream |

### Log Tab

Renders `creature_versions` as a timeline. Each version expands to show:
- State transition details
- Agent episodes produced
- Telemetry summary (for fly versions)
- Credits spent

---

## Migration Strategy

Evolution, not restart. The existing data is valuable.

### Phase 1: Add New Tables Alongside Old

```sql
-- creature_state: derived from current creatures + creature_flights
CREATE TABLE creature_state ( ... );

-- creature_versions: backfilled from creature_flights history
CREATE TABLE creature_versions ( ... );

-- creature_conditions: extracted from creatures table
CREATE TABLE creature_conditions ( ... );

-- flight_telemetry: migrated from creature_flights.path_samples
CREATE TABLE flight_telemetry ( ... );
```

### Phase 2: Dual-Write

Both old and new tables get written during transitions. Handlers read from new tables.
Old tables become read-only shadows.

### Phase 3: Handler Decomposition

Split `creatures.rs` into the module structure above. Each module reads/writes
the new tables. Old columns on `creatures` and `creature_flights` become
computed views or are dropped.

### Phase 4: Drop Old Columns

Remove redundant columns from `creatures` (presence, visibility, walk_in_price, etc.)
and `creature_flights` (they become `creature_versions` + `flight_telemetry`).

### Tables to Drop (actually dead)

- `swarm_sessions` — Onto4MAT academic feature, zero queries
- `swarm_telemetry` — Onto4MAT academic feature, zero queries
- `ar_beacons` — deferred AR feature, never populated
- `sosa_sensors` — never written to
- `creature_collections` — no UI, replaced by the version history model

---

## What This Enables

1. **Clean state-driven UI** — Flutter reads `creature_state.state` and renders the right widgets. No inference from flight records.

2. **Full audit trail** — Every state transition is a `creature_version`. Bitemporal queries: "where was this creature at time T?" is a simple version lookup.

3. **Agent learning from every transition** — Every version links to episodes. The agent's episodic memory grows with every creature action. Consolidation (dreaming) turns this into KG entities and embeddings.

4. **Loose coupling preserved** — SOSA, tether, devices, formations all keep their tables. They activate in context, produce telemetry or observations, and feed into the episodic/embedding pipeline. They never block the core state machine.

5. **Credit flow visibility** — Every transition charges gas, distributes to agents, records in ledger. The read-to-execute ratio per agent measures durable value. Platform reads on version history create demand signals.

6. **Rabble as social context, not creature state** — The rabble defines the group. The creature defines its conditions. Joining is a state transition, not a mutation of the creature record. Leaving is equally clean.
