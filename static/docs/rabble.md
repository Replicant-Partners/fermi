# Rabble.world — AR Insect Menagerie

Rabble is a loosely coupled AR creature app powered by the Agent Bestiary backend. Users select real insect species from the GBIF (Global Biodiversity Information Facility), mint unique specimen images, fly them at real-world locations using the AR Spatial Suite, collect them, and gather in social swarms.

## Architecture

```
rabble.world (Flutter)          agent-bestiary.world (Axum)
┌─────────────────┐             ┌──────────────────────┐
│  AR Viewer       │◄───REST───►│  /api/creatures       │
│  Species Browser │            │  /api/swarms          │
│  Collection UI   │            │  /api/collections     │
│  Swarm Map       │            │  /api/beacons/nearby  │
└─────────────────┘             │                      │
                                │  Agents:             │
                                │  - species_resolver   │
                                │  - specimen_minter    │
                                │  - rabble_curator     │
                                │                      │
                                │  Tools:              │
                                │  - gbif_species_search│
                                │  - mint_creature      │
                                │  - h3_resolve         │
                                │  - create_beacon      │
                                │  - generate_image     │
                                └──────────────────────┘
```

**Shared infrastructure**: Auth (Google/GitHub OAuth), credits, workspace git, H3 spatial tools, image generation (Gemini).

**Separate**: Flutter frontend, Stripe account, revenue attribution via `tx_type` tagging.

## Species

Rabble uses GBIF as its taxonomic backbone. All species data is real — scientific names, taxonomy, reference images, conservation status.

### Launch Species

**Butterflies** (10):
- Vanessa atalanta (Red Admiral)
- Papilio machaon (Old World Swallowtail)
- Morpho menelaus (Blue Morpho)
- Danaus plexippus (Monarch)
- Gonepteryx rhamni (Common Brimstone)
- Aglais io (European Peacock)
- Pieris brassicae (Large White)
- Lycaena phlaeas (Small Copper)
- Argynnis paphia (Silver-washed Fritillary)
- Anthocharis cardamines (Orange Tip)

**Dragonflies** (10):
- Anax imperator (Emperor Dragonfly)
- Calopteryx virgo (Beautiful Demoiselle)
- Sympetrum striolatum (Common Darter)
- Aeshna cyanea (Southern Hawker)
- Libellula depressa (Broad-bodied Chaser)
- Ischnura elegans (Blue-tailed Damselfly)
- Orthetrum cancellatum (Black-tailed Skimmer)
- Cordulegaster boltonii (Golden-ringed Dragonfly)
- Pyrrhosoma nymphula (Large Red Damselfly)
- Erythromma najas (Red-eyed Damselfly)

## Agents

### species_resolver

Searches GBIF for insect species data. Returns taxonomy, common names, media references, and conservation context.

**Tool**: `gbif_species_search`
- Search by common or scientific name
- Direct lookup by GBIF key
- Filters to Insecta (highertaxonKey=216)
- Returns media references for image generation

### specimen_minter

Generates unique specimen images using controlled variation. Each minted creature gets a unique name, variation notes, and a naturalist illustration-style card image.

**Tool**: `mint_creature`
- Stores specimen in `creatures` table
- Links to GBIF species key
- Records asset paths for card image and flight silhouette
- Generates specimen name if not provided

**Image pipeline**: species_resolver (GBIF reference) -> generate_image (Gemini, naturalist style) -> mint_creature (store record)

### rabble_curator

Compound agent orchestrating collections, flight logging, swarm coordination, and economics. Delegates to species_resolver and specimen_minter.

**Workflow**:
```
User Request
    |
    v
[species_resolver] -- GBIF lookup
    |
    v
[generate_image] -- Gemini illustration
    |
    v
[specimen_minter] -- Store creature
    |
    v
[create_beacon] -- Place at location
```

## Tools

| Tool | Description | Workspace? |
|------|-------------|------------|
| `gbif_species_search` | Search GBIF species database | No |
| `mint_creature` | Store minted creature record | Yes |
| `h3_resolve` | GPS to H3 cell operations | No |
| `create_beacon` | Place AR beacon at H3 cell | Yes |
| `generate_image` | Gemini text-to-image | Yes |
| `edit_image` | Gemini image-to-image | Yes |

## API Endpoints

### Public (no auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/creatures` | Browse minted creatures |
| GET | `/api/creatures/:id` | Creature detail + data card |
| GET | `/api/creatures/:id/flights` | Flight history |
| GET | `/api/swarms` | Browse upcoming swarm events |
| GET | `/api/swarms/:id` | Swarm detail |

**Query parameters** for `/api/creatures`:
- `species_group` — filter by group (butterfly, dragonfly)
- `scientific_name` — partial match (ILIKE)
- `owner_id` — filter by owner
- `limit` / `offset` — pagination (default 20, max 100)

**Query parameters** for `/api/swarms`:
- `status` — scheduled, active, completed (default: scheduled + active)
- `h3_cell` — filter by location
- `species_filter` — filter by species group
- `limit` — max results (default 20, max 50)

### Authenticated

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/collections` | User's creature collections |

## Database Tables

### creatures
Minted specimen records with GBIF reference, asset paths, and flight stats.

### creature_flights
Flight log — every time a creature is flown at a location. Links to beacons and swarm events.

### swarm_events
Coordinated multi-user creature gatherings with time windows, species filters, and participant tracking.

### creature_collections
Named groupings of creatures owned by a user.

## Economics

### Transaction Types

| tx_type | Credits | Description |
|---------|---------|-------------|
| `creature_mint` | 5 | Mint a new creature specimen |
| `creature_flight` | 3 | Fly a creature at a location |
| `swarm_create` | 5 | Create a swarm event |
| `swarm_join` | 1 | Join an existing swarm |
| `gbif_contribution` | — | Portion of mint fee attributed to GBIF |
| `rabble_platform_fee` | — | Rabble's share of transaction fees |

### Revenue Split (per mint)
- 1 credit: GBIF contribution
- 1 credit: Image generation (Gemini)
- 1 credit: Gas fee (ABW infrastructure)
- 2 credits: Platform fee (Rabble)

### Revenue Split (per flight)
- 1 credit: Beacon creation (ABW infrastructure)
- 1 credit: Gas fee
- 1 credit: Platform fee (Rabble)

## Flutter Client (Planned)

Android-first, iOS-second, web-third. Key screens:

1. **Species Browser** — Searchable grid of available species from GBIF
2. **Mint Studio** — Preview + generate unique specimen images
3. **AR Viewer** — WebXR/ARCore rendering of creatures at beacon locations
4. **Collection** — User's minted creatures with data cards and stats
5. **Swarm Map** — Map view of upcoming and active swarm events
6. **Flight Log** — Timeline of creature flights with location history

## GBIF Integration

The [Global Biodiversity Information Facility](https://www.gbif.org/) provides free, open access to biodiversity data. Rabble uses:

- **Species Search API**: `/v1/species/search` — taxonomy, common names
- **Species Detail API**: `/v1/species/{key}` — full taxonomic record
- **Media API**: `/v1/species/{key}/media` — reference images for generation

No API key required. Rate limit: reasonable use (User-Agent header included).

A portion of every mint fee is tagged as `gbif_contribution` for future donation to GBIF's mission.
