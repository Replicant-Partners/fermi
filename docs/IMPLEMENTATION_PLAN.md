# Implementation Plan — Rabble Flutter PWA

> **Generated from:** `docs/UX_AUDIT.md` with inline feedback on all 10 clarifying questions
> **Date:** 2026-02-14
> **Platform:** Flutter (PWA) — this IS the web experience
> **Backend:** Rust/Axum API server (fermi)

---

## Decisions Lock-In

These are the resolved design decisions driving every task below.

| # | Decision | Impact |
|---|----------|--------|
| D1 | Four pillars = four bottom nav tabs (🐾 Creatures, 👥 Rabbles, 🌍 Environment, 📓 Journals) | Navigation restructure |
| D2 | Creature Detail is the universal hub — reachable from every screen; creature carries its interaction affordances | Every creature reference is a tappable link |
| D3 | Following rabbles = active notifications (new creatures arrive, rabble starts, etc.) | Need `rabble_follows` table with notification triggers, not passive bookmarks |
| D4 | Peek does NOT auto-follow | Explicit follow action required |
| D5 | Two viewpoint modes: "My Location" (device GPS) + "Through [Creature]'s Eyes" (creature position) | Environment tab has a view-source toggle |
| D6 | Hex grid serves both rabble boundary placement AND spatial awareness; dynamic boundary moves with anchor creature during joint flights | Live spatial computation; hex boundary is not static |
| D7 | Rich flight path reports: metrics + species-you-might-have-met (taxonomic waypoint lookup) + plan for agentic hooks | Data model must support waypoint enrichment |
| D8 | Flight paths are shareable/exportable; plan for agentic intelligence from waypoint context | Export as image/link; design agentic hook points |
| D9 | SSE push everywhere — reuse chat SSE pattern for all live views. No polling. | New SSE streams for creature state and environment updates |
| D10 | Flutter first. Flutter PWA IS the web. Deprioritize separate web platform cleanup. | All effort on Flutter; web cleanup is future work |

---

## Architecture: SSE Plumbing (Decision D9)

This is foundational — every phase depends on it.

### Current State

The backend already has SSE infrastructure:
- `GET /api/workspaces/:id/stream` — chat messages (workspace SSE)
- `GET /api/feed/stream` — activity feed (global SSE)
- Both use `tokio::sync::broadcast` channels in `AppState`

### Target State

Three SSE stream types, all built on the same broadcast pattern:

```
┌─────────────────────────────────────────────────────────────┐
│                    SSE Stream Architecture                    │
├──────────────────┬──────────────────┬───────────────────────┤
│ Creature Stream  │ Rabble Stream    │ Environment Stream    │
│ /api/creatures/  │ /api/rabble/     │ /api/environment/     │
│   :id/stream     │   :id/stream     │   stream?lat=&lng=    │
├──────────────────┼──────────────────┼───────────────────────┤
│ Events:          │ Events:          │ Events:               │
│ • state_changed  │ • creature_joined│ • nearby_rabble       │
│ • location_update│ • creature_left  │ • nearby_creature     │
│ • flight_started │ • chat_message   │ • boundary_update     │
│ • flight_ended   │ • rabble_moved   │ • creature_entered    │
│ • friend_request │ • host_action    │ • creature_left_area  │
│ • friend_accepted│ • rabble_ended   │                       │
│ • entered_rabble │                  │                       │
│ • left_rabble    │                  │                       │
└──────────────────┴──────────────────┴───────────────────────┘
```

### Backend Changes Required

**In `api_server.rs` — add broadcast channels to `AppState`:**

```rust
pub(crate) struct AppState {
    // ... existing fields ...
    pub(crate) ws_broadcast: broadcast::Sender<WorkspaceEvent>,       // existing
    pub(crate) rabble_broadcast: broadcast::Sender<RabbleEvent>,      // existing
    pub(crate) creature_broadcast: broadcast::Sender<CreatureEvent>,  // NEW
    pub(crate) env_broadcast: broadcast::Sender<EnvironmentEvent>,    // NEW
}

pub(crate) struct CreatureEvent {
    pub creature_id: Uuid,
    pub event_type: String,   // "state_changed", "location_update", etc.
    pub payload: serde_json::Value,
}

pub(crate) struct EnvironmentEvent {
    pub h3_cell: String,      // events are spatially scoped
    pub event_type: String,
    pub payload: serde_json::Value,
}
```

**New handler file: `src/handlers/streams.rs`**

```rust
// GET /api/creatures/:id/stream
pub async fn creature_stream_handler(...) -> Sse<impl Stream<Item = ...>>

// GET /api/rabble/:id/stream  (extend existing rabble_chat SSE)
// Already partially exists — add non-chat events

// GET /api/environment/stream?lat=X&lng=Y&radius=Z
pub async fn environment_stream_handler(...) -> Sse<impl Stream<Item = ...>>
```

**Emit events from existing handlers:**

Every handler that mutates creature state (`fly_handler`, `perch_handler`, `tether_handler`, `join_swarm_handler`, `end_flight_handler`, `push_telemetry_handler`, friendship handlers) gains a `state.creature_broadcast.send(...)` call at the end of its success path.

---

## Phase 0 — Unblock (1-2 days)

Critical path blockers. Nothing else starts until these are green.

### 0.1 Verify Migration 090 on Production

**Type:** Ops
**Effort:** 15 minutes

```sql
-- Run on production database:
SELECT column_name FROM information_schema.columns
WHERE table_name = 'users' AND column_name = 'social_visibility';

SELECT routine_name FROM information_schema.routines
WHERE routine_schema = 'public'
AND routine_name IN (
  'get_pending_friendship_requests',
  'get_creature_friends',
  'get_pending_creature_invites',
  'get_activity_feed',
  'get_creatures_met_in_rabble'
);
```

**If missing:** Re-run migration 090. Check `migrations/` directory for the file.

### 0.2 Verify PostGIS on Production

**Type:** Ops
**Effort:** 15 minutes

```sql
SELECT PostGIS_Version();
```

**If missing:** `CREATE EXTENSION IF NOT EXISTS postgis;` — requires superuser on Neon. May need to enable via Neon dashboard.

### 0.3 Deploy Notification Column Fix

**Type:** Deploy
**Effort:** 30 minutes

The six handlers fixed in `ux-social-improvements.md` (`notification_type` → `type`, `body` → `message`) need to reach production. Standard deploy.

### 0.4 Fix Creature Picker in Flutter Join Flow

**Type:** Flutter
**Effort:** 1 hour

In the Flutter rabble join flow, the creature list API call is missing `?owner_id=<user_id>`. Add the parameter from auth state so only the user's own creatures appear in the picker.

**Files:** Find the Flutter screen that calls `GET /api/creatures` during the join-rabble flow. Add `owner_id` query parameter.

---

## Phase 1 — SSE Foundation + Creature Hub (5-7 days)

The Creature Detail Card is the centrepiece. SSE is its nervous system.

### 1.1 Backend: Creature SSE Stream

**Type:** Backend (Rust)
**Effort:** 1 day
**File:** `src/handlers/streams.rs` (new)

Create `GET /api/creatures/:id/stream` endpoint:
- Auth required (must own the creature or be in the same rabble)
- Subscribes to `creature_broadcast` channel, filters by `creature_id`
- Events: `state_changed`, `location_update`, `flight_started`, `flight_ended`, `friend_request`, `friend_accepted`, `entered_rabble`, `left_rabble`

Wire broadcast sends into existing mutation handlers:
- `src/handlers/creatures/state.rs` — fly, perch, tether, untether, end_flight, push_telemetry, join_swarm
- `src/handlers/social.rs` — creature friendship accept
- `src/handlers/creatures/identity.rs` — transfer

### 1.2 Flutter: Bottom Nav Restructure (D1)

**Type:** Flutter
**Effort:** 0.5 day

Replace current tab structure with four pillar tabs:

```
┌────────┬────────┬────────────┬──────────┐
│   🐾   │   👥   │     🌍     │    📓    │
│Creatures│Rabbles │Environment │ Journals │
└────────┴────────┴────────────┴──────────┘
```

Each tab is a shell with a placeholder screen. We'll fill them in phases 1-4.

### 1.3 Flutter: Creature Collection Grid (Pillar 1 root screen)

**Type:** Flutter
**Effort:** 1 day

The 🐾 Creatures tab shows a card grid of the user's creatures.

Each card shows:
- Creature image (from `/api/creatures/:id/image`)
- Specimen name
- State badge: flying ⚡ / perched 🪺 / tethered 📡 / roosting 💤
- Rabble name chip (tappable → rabble detail)
- Friend count
- Location pin with name
- Cognition level star

Data source: `GET /api/creatures?owner_id=<me>`

Cards are tappable → creature detail hub (1.4).

### 1.4 Flutter: Creature Detail Hub Card (D2)

**Type:** Flutter
**Effort:** 2 days

The most important screen in the app. Sections:

**Header:**
- Full-width creature image
- Name, species, level
- State badge (live-updated via SSE)

**State Section:**
- Current state with icon (⚡ Flying / 🪺 Perched / 📡 Tethered)
- Location name (live-updated)
- Flight duration (if active)

**Rabble Section** (shown when `rabble_id` is non-null):
- Rabble name (tappable → rabble detail)
- Role badge: "Hosting" / "Participating" / "Anchor"
- Creature count in rabble
- [Peek 👁] [Chat 💬] action buttons

**Social Section:**
- Friend count (tappable → friends list)
- Pending request badge (tappable → accept/decline sheet)
- Befriend suggestions (from last rabble recap)

**Journal Section:**
- Total flights · Total locations
- Last activity summary line (from feed)
- [View full activity →] link

**Mini-Map Section:**
- Embedded map showing creature's current position
- Breadcrumb trail (last N telemetry points)
- Tappable → full explore view centred on creature

**Actions Section** (contextual based on state):
- Flying: [End Flight] [Peek Rabble] [View Track]
- Perched: [Fly] [Move to Rabble] [Transfer]
- Tethered: [Untether] [View Track] [Peek Rabble]
- General: [Friends] [Journal] [Settings]

### 1.5 Flutter: SSE Integration on Creature Detail

**Type:** Flutter
**Effort:** 1 day

Connect the creature detail screen to `GET /api/creatures/:id/stream`:
- On screen open: establish SSE connection
- On `state_changed` event: update state badge, location, flight info
- On `location_update` event: move pin on mini-map, extend breadcrumb
- On `friend_request` event: increment pending badge, show toast
- On `entered_rabble` / `left_rabble`: update rabble section
- On screen close: disconnect SSE

Use `EventSource` or `http` SSE client in Flutter. Match the pattern already used for chat.

### 1.6 Flutter: Universal Creature Links (D2)

**Type:** Flutter
**Effort:** 0.5 day

Create a `CreatureLink` widget that renders a creature's avatar + name as a tappable chip. Use it everywhere a creature is referenced:
- Rabble member lists
- Friend lists
- Activity feed events
- Notification items
- Map pins (tap → hub card)
- Journal entries

Navigation: `Navigator.push(..., CreatureDetailScreen(creatureId: id))`

---

## Phase 2 — Rabble Pillar (3-4 days)

### 2.1 Flutter: Rabble List with Host/Participant Split

**Type:** Flutter
**Effort:** 1 day

The 👥 Rabbles tab calls `GET /api/my/rabbles` and renders two sections:

**"Hosting" section:**
- Rabble cards with name, location, creature count
- Inline creature avatar row (from `my_creatures` array — each avatar is a `CreatureLink`)
- Last activity timestamp
- Status badge (active / scheduled / completed)

**"Participating" section:**
- Same card layout
- Host name shown instead of "hosting" badge
- My creature avatars highlighted

Ordered by `last_activity_at DESC` (backend already does this).

### 2.2 Backend: Rabble Follows with Notifications (D3)

**Type:** Backend
**Effort:** 1 day
**Files:** New migration, `src/handlers/social.rs` additions

**Migration:**

```sql
CREATE TABLE IF NOT EXISTS rabble_follows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL REFERENCES users(user_id),
    swarm_id UUID NOT NULL REFERENCES swarm_events(swarm_id),
    notify_on_join BOOLEAN DEFAULT TRUE,
    notify_on_start BOOLEAN DEFAULT TRUE,
    notify_on_end BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(user_id, swarm_id)
);
```

**Endpoints:**
- `POST /api/rabbles/:id/follow` — follow with notification preferences
- `DELETE /api/rabbles/:id/follow` — unfollow
- `GET /api/my/rabbles` — add `following: [...]` array to response

**Notification triggers** (add to existing handlers):
- When a creature joins a followed rabble → notify follower (if `notify_on_join`)
- When a followed rabble starts → notify follower (if `notify_on_start`)
- When a followed rabble ends → notify follower (if `notify_on_end`)

### 2.3 Flutter: Following Tab

**Type:** Flutter
**Effort:** 0.5 day

Third section in the Rabbles tab: "Following"
- Cards show rabble name, location, creature count
- No `my_creatures` row (you have none there — that's the point)
- [Join] button (opens creature picker → join flow)
- [Unfollow] button
- Notification preference toggles (join/start/end)

### 2.4 Flutter: Rabble Detail + SSE

**Type:** Flutter
**Effort:** 1 day

Rabble detail screen (reached from rabble card, creature hub, or notification):
- Header: rabble name, location, status, host name
- Member list: creature avatars (each is a `CreatureLink`)
- Chat: connected to existing rabble SSE stream
- Map: rabble boundary circle + creature pins
- Actions: [Add Creature] (if hosting), [Follow], [Leave], [Peek AR 👁]

### 2.5 Backend: Rabble Move with Lat/Lng

**Type:** Backend
**Effort:** 0.5 day
**File:** `src/handlers/creatures/swarms.rs`

Add `center_lat`, `center_lng`, `location_name`, `h3_cell` to `UpdateSwarmRequest`. Only the creator can move. Emit a `rabble_moved` event on the rabble broadcast. Return `moved_from` in the response for potential undo.

---

## Phase 3 — Environment Pillar (5-7 days)

### 3.1 Backend: Environment SSE Stream

**Type:** Backend
**Effort:** 1 day
**File:** `src/handlers/streams.rs`

`GET /api/environment/stream?lat=X&lng=Y&radius=Z`

Subscribes to `env_broadcast`, filters by spatial proximity (H3 cell matching or distance check). Events:
- `nearby_rabble` — a rabble starts/moves into range
- `nearby_creature` — a creature enters the area (respecting visibility)
- `boundary_update` — a dynamic rabble boundary moves (anchor creature movement)
- `creature_entered_area` — your creature crosses into a rabble area
- `creature_left_area` — your creature leaves a rabble area

Emit env events from: `push_telemetry_handler`, `fly_handler`, `join_swarm_handler`, `create_swarm_handler`, rabble move handler.

### 3.2 Flutter: Explore Map View (Environment tab root)

**Type:** Flutter
**Effort:** 2 days

The 🌍 Environment tab opens to a map-first view.

**View Source Toggle (D5):**

```
┌──────────────────────────────────────┐
│  📍 My Location  │  👁 Luna's Eyes   │
│     [active]     │                   │
└──────────────────────────────────────┘
```

- "My Location" — map centres on device GPS, nearby endpoint uses device coords
- "[Creature]'s Eyes" — dropdown to select a creature → map centres on that creature's last known position → nearby endpoint uses creature coords

**Map Layers:**
- My Creatures: avatar pins (each is a `CreatureLink` — tap → creature hub)
- My Rabbles: circles with radius (hosted = solid border, participating = dashed)
- Nearby Rabbles: semi-transparent circles (from `/api/dashboard/nearby`)
- Favourite Locations: star pins (Phase 3.4)

**Live Updates via Environment SSE:**
- New rabble appears nearby → pin fades in
- Creature moves → pin animates to new position
- Rabble boundary moves (anchor creature in joint flight) → circle animates

**Prominent AR FAB:**
- Floating action button in bottom-right: [👁 AR]
- Opens AR viewer with current map context

### 3.3 Flutter: Creature Track Visualisation (D9 — live via SSE)

**Type:** Flutter
**Effort:** 1 day

When viewing through a creature's eyes (D5), or from the creature hub mini-map:
- Polyline from `GET /api/creatures/:id/track` rendered on map
- Pulsing dot at current position
- Breadcrumb timestamps on hover/tap
- Nearby rabble circles visible
- Distance to nearest rabble shown

Live updates: as `location_update` events arrive via creature SSE, extend the polyline in real time.

### 3.4 Backend + Flutter: Saved Locations / Favourites

**Type:** Backend + Flutter
**Effort:** 1 day

**Migration:**

```sql
CREATE TABLE IF NOT EXISTS saved_locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL REFERENCES users(user_id),
    name TEXT NOT NULL,
    lat DOUBLE PRECISION NOT NULL,
    lng DOUBLE PRECISION NOT NULL,
    radius_meters INT DEFAULT 500,
    h3_cell TEXT,
    source TEXT DEFAULT 'pin',  -- 'pin', 'rabble', 'creature_waypoint'
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Endpoints:**
- `POST /api/locations` — save a location
- `GET /api/locations` — list my saved locations
- `DELETE /api/locations/:id` — remove
- `PATCH /api/locations/:id` — rename, adjust radius

**Flutter UI:**
- Long-press on map → "Save this location" (drop pin)
- Saved locations appear as ⭐ pins on explore map
- Saved locations list accessible from explore tab header
- "Move Creature Here" action on saved location → fly creature to that area

### 3.5 Flutter: AR Viewer Promotion

**Type:** Flutter
**Effort:** 0.5 day

Make AR viewer a first-class entry point:
1. FAB on explore map: [👁 AR] → opens AR with current map context
2. [Peek 👁] button on every rabble card everywhere in the app
3. QR scan → AR viewer with rabble context pre-loaded
4. Proximity notification (from environment SSE) → tap → AR viewer

### 3.6 Backend: Hex Grid with Dynamic Boundaries (D6)

**Type:** Backend
**Effort:** 1 day
**File:** `src/handlers/dashboard/mod.rs` or new `src/handlers/hex.rs`

`GET /api/map/hexes?lat=X&lng=Y&radius=Z&resolution=8`

Returns H3 cells as GeoJSON FeatureCollection:

```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "properties": {
        "h3_cell": "882a100d63fffff",
        "rabble_id": "uuid-or-null",
        "rabble_name": "Bug Collectors",
        "creature_count": 5,
        "is_dynamic": true
      },
      "geometry": {
        "type": "Polygon",
        "coordinates": [[[lng, lat], ...]]
      }
    }
  ]
}
```

For dynamic rabble boundaries (D6 — "the area moves with the anchor creature"):
- When anchor creature pushes telemetry, recalculate the rabble's `h3_cell` based on anchor position
- Emit `boundary_update` event on environment broadcast
- Flutter map animates the hex boundary to new position

### 3.7 Backend: Nearby Creatures Endpoint

**Type:** Backend
**Effort:** 0.5 day

`GET /api/dashboard/nearby-creatures?lat=X&lng=Y&radius=Z`

Spatial query on creature positions (from latest telemetry or flight location), filtered by:
- `visibility = 'public'` or creature owner is a contact
- Not the requesting user's own creatures
- Within radius (PostGIS `ST_DWithin`)

Returns creature summaries (id, name, species, image, distance, rabble name if any).

---

## Phase 4 — Journals Pillar (4-5 days)

### 4.1 Flutter: Journals Tab Shell

**Type:** Flutter
**Effort:** 0.5 day

The 📓 Journals tab with four sub-tabs:

```
┌──────────┬──────────┬──────────┬──────────┐
│ Activity │ Reports  │ Flights  │ All Bugs │
└──────────┴──────────┴──────────┴──────────┘
```

### 4.2 Flutter: Activity Stream (live via SSE)

**Type:** Flutter
**Effort:** 1 day

The "Activity" sub-tab renders `GET /api/feed/events` as a timeline.

Initial load: paginated fetch.
Live updates: connect to `GET /api/feed/stream` (existing SSE endpoint). New events prepend to the timeline with an animation.

Each event card shows:
- Timestamp
- Event icon (creature joined, friend made, rabble started, etc.)
- Narrative text
- Creature avatars involved (each is a `CreatureLink`)
- Tappable → context-appropriate detail screen

### 4.3 Backend: Per-Creature Activity Endpoint

**Type:** Backend
**Effort:** 0.5 day
**File:** `src/handlers/social.rs` or `src/handlers/creatures/query.rs`

`GET /api/creatures/:id/activity?limit=20&offset=0`

Filters the activity feed to events referencing a specific creature. Same response shape as `/api/feed/events` but scoped. This powers the "journal section" on the creature detail hub.

### 4.4 Flutter: Reports Sub-View

**Type:** Flutter
**Effort:** 0.5 day

List of completed rabbles from `GET /api/my/rabbles` (filter `status = 'completed'`).

Each card shows:
- Rabble name, dates, location
- Creature you sent
- [View Recap] button → calls `GET /api/rabble/:id/recap/:creature_id`
- Recap screen shows: creatures met, friend suggestions, narrative summary (when available)

### 4.5 Backend: Stitched Flight Path as GeoJSON (D7, D8)

**Type:** Backend
**Effort:** 1 day
**File:** `src/handlers/creatures/query.rs` or new `src/handlers/flights.rs`

`GET /api/creatures/:id/flight-path/:flight_id`

Takes raw telemetry points from the flight and returns:

```json
{
  "flight_id": "...",
  "creature_id": "...",
  "started_at": "...",
  "ended_at": "...",
  "geojson": {
    "type": "Feature",
    "geometry": {
      "type": "LineString",
      "coordinates": [[lng, lat, timestamp], ...]
    },
    "properties": {
      "total_distance_meters": 4230,
      "duration_seconds": 7200,
      "average_speed_mps": 0.59,
      "rabbles_crossed": ["uuid1", "uuid2"],
      "rabble_time_seconds": { "uuid1": 3600, "uuid2": 1800 },
      "waypoints": [
        {
          "lat": 51.54,
          "lng": -0.14,
          "timestamp": "...",
          "h3_cell": "...",
          "nearby_rabble": "uuid1",
          "enrichment_hook": "waypoint_context"
        }
      ]
    }
  },
  "share_url": "/flights/abc123"
}
```

**Waypoint enrichment hooks (D7 — plan, don't build yet):**

Each waypoint has an `enrichment_hook` field. Future agentic loops will:
1. Take the waypoint's lat/lng + h3_cell
2. Query GBIF for species recorded at that location
3. Run a synthesis agent: "What species might this creature have encountered at this waypoint?"
4. Store enrichment results back on the waypoint

For now, just include the hook point in the data model. The `enrichment_hook` field is a marker for Phase 6+.

### 4.6 Flutter: Flight Paths Sub-View (D8)

**Type:** Flutter
**Effort:** 1.5 days

List of flights per creature from `GET /api/creatures/:id/flights`.

Each card shows:
- Date, duration, distance
- Mini-map preview (static polyline)
- Rabbles crossed count
- Creature name (`CreatureLink`)

Tap → full-screen flight detail:
- Full map with polyline from GeoJSON endpoint (4.5)
- Playback scrubber (animate a dot along the path)
- Waypoint markers (tappable → future enrichment)
- Rabble areas crossed (circles on map)
- Stats panel: distance, speed, time, rabbles

**Share button (D8):**
- Export as image (screenshot the map + stats overlay)
- Share URL (from `share_url` in response — needs a simple public page or deep link)

### 4.7 Backend: Narrative Rabble Recap via Agent

**Type:** Backend
**Effort:** 0.5 day
**File:** `src/handlers/creatures/swarms.rs` or lifecycle handler

When a rabble's status changes to `completed`:
1. Trigger the `swarm_host` agent (same pattern as `trigger_swarm_host_welcome`)
2. Prompt: "Summarise what happened in [Rabble Name]. [N] creatures participated. They were: [list]. The rabble lasted [duration] at [location]."
3. Store the narrative in `activity_events` or a new `rabble_reports` table
4. The recap endpoint (`GET /api/rabble/:id/recap/:creature_id`) returns this narrative alongside the structured data

### 4.8 Flutter: All Creatures Activity ("All Bugs" sub-tab)

**Type:** Flutter
**Effort:** 0.5 day

Aggregate view: `GET /api/feed/events` but rendered grouped by creature.

Each creature section:
- Creature header (`CreatureLink` with avatar, name, state badge)
- Last 3-5 events for that creature
- [View all →] link → creature hub → journal section

---

## Phase 5 — Polish & Agentic Hooks (3-5 days)

### 5.1 Flutter: Notification Centre (D3)

**Type:** Flutter
**Effort:** 1 day

Notification bell in the app bar (all tabs). Tapping opens a notification drawer.

Notification types:
- Creature friend request (tap → creature hub → accept/decline)
- Rabble started/ended (tap → rabble detail)
- Creature joined followed rabble (tap → rabble detail)
- Creature entered rabble area (tap → creature hub)
- Transfer received (tap → creature hub)

Backed by `GET /api/notifications?limit=20` + `PUT /api/notifications/read-all`.

Live count update: piggyback on existing SSE streams or add a lightweight `/api/notifications/stream`.

### 5.2 Flutter: Onboarding Flow

**Type:** Flutter
**Effort:** 1 day

First-time user experience:
1. Sign in (Google/GitHub — already implemented)
2. "Mint your first creature" — species picker → name → art generation
3. Brief tour of four tabs (overlay hints)
4. "Find a rabble nearby" or "Create your own rabble"

### 5.3 Backend: Waypoint Taxonomic Lookup Hook (D7 — first pass)

**Type:** Backend
**Effort:** 1 day

`GET /api/waypoints/:h3_cell/species`

Queries GBIF occurrence API for species recorded within the H3 cell's area:
```
GET https://api.gbif.org/v1/occurrence/search?
    decimalLatitude=X&decimalLongitude=Y&radius=500
    &kingdomKey=1 (Animalia)
    &classKey=216 (Insecta, or relevant class)
    &limit=20
```

Returns species list with common names, images, and "could have met" narrative potential.

This is the first agentic hook. It doesn't run automatically yet — it's called on demand when a user views a flight path waypoint.

### 5.4 Backend: Flight Path Share Endpoint (D8)

**Type:** Backend
**Effort:** 0.5 day

`GET /flights/:share_id` — public page showing a flight path map + stats.

Generate `share_id` when a user taps "Share" on a flight. Store in a `flight_shares` table with the pre-computed GeoJSON. The public page is a minimal standalone HTML template (or a Flutter deep link if PWA handles it).

### 5.5 Flutter: Dynamic Hex Boundary Rendering (D6)

**Type:** Flutter
**Effort:** 1 day

On the explore map, render hex boundaries from `/api/map/hexes`:
- Static rabble hexes: solid fill with rabble colour
- Dynamic rabble hexes (joint flight): animated boundary that moves as `boundary_update` SSE events arrive
- Favourite location hexes: dashed outline
- Hex tap → rabble detail or "Create rabble here" depending on occupancy

---

## Phase 6 — Future: Agentic Intelligence (not scheduled)

These are the hooks we've designed for but won't build yet:

### 6.1 Waypoint Context Agent

An agent loop that enriches flight path waypoints with:
- Species recorded at that location (GBIF)
- Habitat type (land use data)
- Weather at time of flight
- Other creatures that were nearby at that time

Runs asynchronously after a flight ends. Stores enrichment on the waypoint. Surfaces in the flight path detail view as tappable waypoint cards.

### 6.2 Rabble Intelligence Agent

Post-rabble agent that analyses:
- Which creatures interacted most
- Species diversity in the rabble
- Geographic range covered
- "If you liked this rabble, you might like..." — recommendation engine based on species overlap, location proximity, and social graph
- Shareable "Rabble Report Card" with visualisations

### 6.3 Predictive Flight Suggestions

An agent that looks at a creature's flight history, current location, and nearby rabbles to suggest:
- "Luna could join Bug Collectors — 3 of her friends are there"
- "There's a new rabble 200m away with 2 Painted Ladies"
- "Based on your flight patterns, you might enjoy the Hampstead Heath corridor"

### 6.4 Cross-Creature Knowledge Synthesis

When multiple creatures from different owners share a rabble, the system can:
- Compare flight paths for common waypoints
- Identify species diversity patterns across overlapping territories
- Generate "field guide" entries for locations based on aggregated creature intelligence

---

## Timeline Summary

```
Week 1:  Phase 0 (unblock) + Phase 1 (SSE foundation + creature hub)
         ├── Day 1-2: Verify migrations, deploy fixes, fix creature picker
         ├── Day 2-3: Backend creature SSE stream + broadcast wiring
         ├── Day 3: Bottom nav restructure + creature collection grid
         └── Day 4-5: Creature detail hub card + SSE integration

Week 2:  Phase 1 (finish) + Phase 2 (rabbles)
         ├── Day 1: Universal creature links widget
         ├── Day 1-2: Rabble list with host/participant split
         ├── Day 2-3: Backend rabble follows with notifications
         ├── Day 3: Following tab in Flutter
         └── Day 4-5: Rabble detail screen + rabble move endpoint

Week 3:  Phase 3 (environment)
         ├── Day 1: Backend environment SSE stream
         ├── Day 1-3: Explore map view with dual viewpoint toggle
         ├── Day 3: Creature track visualisation (live via SSE)
         ├── Day 4: Saved locations backend + Flutter UI
         └── Day 5: AR viewer promotion + nearby creatures endpoint

Week 4:  Phase 3 (finish) + Phase 4 (journals)
         ├── Day 1: Hex grid backend + dynamic boundaries
         ├── Day 1: Journals tab shell + activity stream (SSE)
         ├── Day 2: Per-creature activity endpoint + reports sub-view
         ├── Day 3-4: Stitched flight path GeoJSON + flight paths sub-view
         └── Day 5: Narrative rabble recap agent + all-bugs view

Week 5:  Phase 5 (polish)
         ├── Day 1: Notification centre
         ├── Day 2: Onboarding flow
         ├── Day 3: Waypoint taxonomic lookup hook
         ├── Day 4: Flight path sharing
         └── Day 5: Dynamic hex boundary rendering
```

**Total estimated effort:** 5 weeks for one developer, ~3 weeks with two (Flutter + Backend in parallel)

**Critical path:** Phase 0 → Phase 1.1 (SSE backend) → Phase 1.4 (creature hub) → everything else can parallelise.

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Migration 090 not applied on prod | Medium | 🔴 Blocks all social features | Phase 0.1 — verify first |
| PostGIS not available on Neon | Low | 🔴 Blocks all spatial queries | Phase 0.2 — verify; Neon supports PostGIS |
| SSE connections overwhelm server | Low | 🟡 Performance degradation | Use broadcast channels (already O(1) per send); add connection limits |
| Flutter PWA SSE support | Low | 🟡 May need EventSource polyfill | Standard browser API; Flutter web supports it |
| GBIF API rate limits | Medium | 🟢 Degrades waypoint enrichment | Cache responses by H3 cell; respect rate limits |
| Dynamic hex boundaries at scale | Medium | 🟡 Compute-intensive with many moving rabbles | Only recompute for rabbles with active joint flights; batch updates |

---

*Plan generated from UX Audit feedback. Ready to begin Phase 0.*