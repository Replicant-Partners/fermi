# AR Spatial Suite

Place, map, and animate augmented reality assets in physical space. Three agents, six tools, one hexagonal grid.

---

## Overview

The AR Spatial Suite is an agentic toolkit for authoring AR experiences at real-world locations. It produces structured placement records, grid maps, and choreography sequences that any AR-capable client (WebXR app, mobile app, or display glasses) can consume.

The suite is built on **H3** (Uber's open-source hexagonal grid system), which divides Earth's surface into hierarchical hexagonal cells. H3 is free, runs offline as pure math, and delivers sub-meter precision with microsecond lookups.

### Why H3?

| Feature | H3 | What3Words | Geohash |
|---------|-----|-----------|---------|
| Cost | Free (Apache 2.0) | ~0.3p per lookup | Free |
| Offline | Yes (pure math) | No (API required) | Yes |
| Precision | 0.3m (res 15) | 3m fixed | 3.7cm (len 12) |
| Grid shape | Hexagonal | Square | Rectangle |
| Neighbor uniformity | 6 equidistant | 8 (diagonals differ) | 8 (diagonals differ) |
| Rust crate | `h3o` | None | `geohash` |

Hexagons have uniform neighbor distances in all directions (no diagonal penalty), making them ideal for smooth spatial animation and movement tracking.

---

## The Three Agents

### ar_beacon --- "Drop it here"

The placement agent. Takes a description and a location, generates a visual asset, and anchors it to an H3 cell with a time-to-live.

**What it does:**
1. Generates an AR asset image via `generate_image` (Gemini)
2. Resolves the location to an H3 cell using `h3_resolve`
3. Stores the beacon in the database via `create_beacon`
4. Returns the beacon record with a public asset URL

**Example:**
```
@ar_beacon Drop a glowing blue portal outside my office at 51.5074, -0.1278 for 24 hours
```

**Key concepts:**
- **TTL (Time-to-Live):** Every beacon expires. Options: 1 hour, 24 hours (default), 7 days, 30 days, custom.
- **Decay styles:** `fade` (opacity decreases), `dissolve` (particles), `instant` (hard cutoff), `loop_decay` (animation slows).
- **Orientation:** Azimuth (compass bearing), elevation (vertical tilt), billboard mode (always face viewer).
- **Interaction triggers:** `on_gaze`, `on_tap`, `on_proximity`, `on_dwell`.

### ar_cartographer --- "Name this space"

The mapping agent. Turns a physical space into a named, addressable grid.

**What it does:**
1. Takes a center point + radius
2. Generates a hexagonal grid at the chosen H3 resolution
3. Names the cells (directional, functional, or landmark-based)
4. Groups cells into zones
5. Persists the grid map via `save_grid_map`

**Example:**
```
@ar_cartographer Create a grid map for my gallery centered at 51.5074, -0.1278 with a 30-meter radius. The entrance is south, main exhibition north, cafe east.
```

**Key concepts:**
- **Quadrants:** Named H3 cells. "entrance-foyer", "gallery-north", "stage-center".
- **Zones:** Groups of quadrants. "exhibition-wing" = [gallery-north, gallery-south, gallery-east].
- **Resolution:** Default 12 (~9m hexes). Use 13 for ~3m, 14 for ~1m, 15 for ~30cm.
- **Templates:** Gallery, festival, office, retail, park, conference.

### ar_choreographer --- "Make it move"

The animation agent. Turns static beacons into spatial performances.

**What it does:**
1. Takes a beacon and a motion description
2. Compiles the motion into a choreography record
3. Supports macro (between cells) and micro (within a cell) motion
4. Writes the choreography to the database

**Example:**
```
@ar_choreographer Make the portal beacon bounce up and down in entrance-foyer, then slowly drift to gallery-north over 5 minutes
```

**Key concepts:**
- **Macro motion:** Asset moves between H3 cells. Path traversal, migration, patrol loops.
- **Micro motion:** XYZ animation within a single cell. The "dancing dot". X/Y = horizontal (-1 to 1), Z = vertical (0 = ground, 1 = ~3m).
- **10 built-in actions:** bounce, orbit, hover, pulse, wander, spiral, figure_eight, breathe, swarm, wave.
- **Combined mode:** Macro path + micro action at each stop.
- **Triggers:** immediate, scheduled, on_gaze, on_proximity, on_tap, beacon_expiry, no_viewers.

---

## Platform Tools

Six tools power the AR suite. These are available to all agents in the tool registry.

### h3_resolve

Convert between GPS coordinates and H3 cell IDs. Five operations:

| Operation | Input | Output |
|-----------|-------|--------|
| `gps_to_h3` | lat, lng, resolution | H3 cell ID + center coordinates |
| `h3_to_gps` | H3 cell ID | Center lat/lng |
| `neighbors` | H3 cell ID | 6 adjacent cells |
| `distance` | Two H3 cell IDs | Grid distance (hop count) |
| `grid_disk` | Center + k rings | All cells within radius |

**Resolution guide:**

| Resolution | Edge length | Area | Use case |
|-----------|-------------|------|----------|
| 9 | ~174m | ~0.1 km2 | District / campus overview |
| 10 | ~66m | ~15,000 m2 | Building / block |
| 11 | ~25m | ~2,000 m2 | Room cluster |
| 12 (default) | ~9m | ~300 m2 | Room scale |
| 13 | ~3.5m | ~100 m2 | Desk / doorway |
| 14 | ~1.3m | ~30 m2 | Sub-meter |
| 15 | ~0.5m | ~9 m2 | Maximum precision |

### geocode

Convert a street address to GPS coordinates via OpenStreetMap Nominatim. Free, no API key required. Returns up to 3 results ranked by relevance.

```json
{
  "address": "Tate Modern, London"
}
```

### create_beacon

Create an AR beacon in the database. Resolves GPS to an H3 cell, stores the placement record, and returns a public asset URL.

### query_beacons

Find beacons near a location. Computes the H3 grid disk at the given radius and queries all matching cells. Filters by expiry time. Used by renderers and the agents themselves.

### save_grid_map

Persist a named spatial grid to the database. Stores center, resolution, radius, quadrant names, and zone groupings.

---

## Public API Endpoints

These endpoints require no authentication. They are designed for AR client apps.

### `GET /api/beacons/nearby?lat=X&lng=Y&radius=3&resolution=12`

Discover active public beacons near a location. Returns beacon records with asset URLs, orientation, TTL, and interaction triggers.

**Query parameters:**
- `lat`, `lng` (or `h3_cell`) --- location center
- `radius` --- search radius in H3 rings (default: 3, max: 10)
- `resolution` --- H3 resolution (default: 12)

### `GET /api/beacons/:beacon_id`

Get a single beacon record. Public beacons only.

### `GET /api/beacons/:beacon_id/asset`

Serve the beacon's asset file (image, model, video). Content-Type is inferred from the file extension. Responses are cached for 1 hour.

### `GET /api/grid-maps/:map_id`

Get a grid map definition with all quadrants and zones.

---

## How-To: Digital Graffiti

The flagship use case. Place AR art at real-world locations with a built-in expiry date. Think street art, but in augmented reality.

### Step 1: Create a Workspace

Go to Dashboard > New Workspace. Name it "AR Graffiti" or similar.

### Step 2: Hire the Agents

Add `ar_beacon` to your workspace. Optionally add `ar_cartographer` if you want named locations, and `ar_choreographer` if you want movement.

### Step 3: Place Your First Beacon

```
@ar_beacon Place a neon "HELLO WORLD" sign floating at eye level
at 51.5074, -0.1278, facing south, for 24 hours.
Billboard mode, pulse on gaze.
```

The agent will:
1. Generate the neon sign image via Gemini
2. Save it to workspace files at `ar_assets/beacon_xxx.png`
3. Resolve 51.5074, -0.1278 to H3 cell `8c2a1072b59ffff` at resolution 12
4. Store the beacon with 24-hour TTL, south-facing orientation, billboard mode
5. Return the beacon ID and public asset URL

### Step 4: Verify

Call the public API to confirm your beacon exists:

```
GET /api/beacons/nearby?lat=51.5074&lng=-0.1278&radius=1
```

You should see your beacon in the response with its asset URL.

### Step 5: Make It Move (Optional)

```
@ar_choreographer Make the HELLO WORLD sign pulse slowly ---
scale between 0.8 and 1.2, once per second. Add a gentle
hover wobble at 1.5m height.
```

### Step 6: Create a Trail (Optional)

```
@ar_beacon Create a trail of 5 golden arrows from 51.5074, -0.1278
to 51.5080, -0.1270, spaced evenly, each with a 48-hour TTL
and fade decay.
```

### Step 7: Watch It Expire

After 24 hours, the beacon's TTL expires. If you set `fade` decay, it will gradually become transparent over the final 20% of its lifetime. Then it's gone. Like real graffiti in the rain.

---

## How-To: Venue Mapping

Set up a persistent AR installation for an event or space.

### Step 1: Define the Grid

```
@ar_cartographer Create a grid map called "Spring Exhibition"
centered at 51.5074, -0.1278, radius 50 meters, resolution 12.
Name the quadrants: entrance is south, main gallery north,
sculpture garden east, gift shop southwest.
```

### Step 2: Place Beacons by Name

Once the grid exists, ar_beacon can use quadrant names instead of coordinates:

```
@ar_beacon Drop a rotating sculpture preview at the entrance-south
quadrant, 7-day TTL, on_proximity trigger.
```

### Step 3: Define Zones

```
@ar_cartographer Add an "exhibition" zone containing
gallery-north, gallery-northeast, and gallery-east.
Add a "commercial" zone with gift-shop and cafe.
```

### Step 4: Choreograph a Tour

```
@ar_choreographer Create a guided tour animation: a floating
golden arrow starts at entrance-south, pauses for 10 seconds,
drifts to gallery-north (30 seconds), pauses, continues to
gallery-east (30 seconds), then loops back. Ease-in-out
transitions, hover at 2m height.
```

---

## Architecture Notes

### Data Flow

```
User message
    |
    v
AR Agent (LLM)
    |
    +-- h3_resolve (pure math, no network)
    +-- geocode (Nominatim, 1 req/s)
    +-- generate_image (Gemini API)
    +-- create_beacon (DB write)
    +-- save_grid_map (DB write)
    +-- write_workspace_file (git write)
    |
    v
Database (ar_beacons, ar_grid_maps, ar_choreographies)
    |
    v
Public API (/api/beacons/nearby, /api/beacons/:id/asset)
    |
    v
AR Client (WebXR, mobile app, display glasses)
```

### Database Tables

- **ar_beacons**: Placement records. H3 cell, asset path, orientation, TTL, tags, interaction triggers.
- **ar_choreographies**: Motion sequences. References a beacon. Macro steps, micro keyframes, triggers.
- **ar_grid_maps**: Named spatial grids. Center, resolution, radius, quadrant names, zone groupings.

### H3 Cell Storage

H3 cell IDs are stored as TEXT (15-char hex strings like `8c2a1072b59ffff`). The `idx_ar_beacons_h3` index enables fast lookup. Spatial queries compute the H3 grid disk client-side, then use `WHERE h3_cell IN (...)` for database lookup. This is efficient because H3 math is microsecond-fast and the resulting cell set is small (a 3-ring disk = 37 cells).

### Future: Client Renderer

The agents produce structured data. A future AR client will:
1. Poll `GET /api/beacons/nearby` based on device GPS
2. Fetch assets from `GET /api/beacons/:id/asset`
3. Render assets at the specified H3 cell center coordinates
4. Apply orientation (azimuth, elevation, billboard)
5. Play choreography sequences (interpolate keyframes)
6. Handle interaction triggers (gaze tracking, proximity, tap)
7. Respect TTL and decay styles

The platform is the spatial content management system. The glasses are the display.
