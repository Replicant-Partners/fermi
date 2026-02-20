# Sprint: Creature Card & UX Coherence Fixes

> **Date:** 2026-02-20
> **Source:** Owner feedback after first deploy of four-pillar UX
> **Theme:** Cognitive coherence — the creature card is the hub, everything flows from it

---

## Core Problem Statement

> "The most concerning thing is cognitive dissonance in the app where it's hard
> to reconcile the rabble and the current state of the creature."

The creature detail screen has accreted sections without a clear information
hierarchy. The user can't glance at a creature and immediately understand:
**where is it, what is it doing, and who is it with?**

---

## Feedback Decomposition

### F1. Creature Card Layout — Simplify & Re-layer

**What was said:**
> "We need to simplify and re-lay out the creature card."
> "The creature card has two social sections now that are confusing — one is for
> rabbles, the other for friends."
> "I don't see the point of chat there."
> "The map location is buried."
> "There seems now to be a duplicate log vs Journal info."

**Analysis:**

The current creature detail has this section order:
```
1. Hero image + field notes overlay
2. Name + species badge + cognition pill
3. Presence chips (rabble/tethered/flying)
4. ─── RABBLE section ───  (new — role badge, chat/peek buttons)
5. ─── SOCIAL section ───  (new — friend count, pending requests)
6. ─── JOURNAL section ─── (new — flights, locations, duration)
7. Config (collapsible)
8. Actions row (fly, perch, tether, etc.)
9. Tabs: [Live] | [Log]
```

**Problems identified:**
- Rabble section + Social section = two separate boxes that feel like two social features. They're actually **context** (where am I) vs **relationships** (who are my friends). They need merging or clear visual differentiation.
- Chat button on creature card makes no sense — chat belongs to the rabble, not the creature. Remove it.
- Peek button is placeholder with no implementation. Remove it.
- Map/location is hidden — you have to look at the presence chip or the Journal section to figure out WHERE the creature is. Map should be prominent.
- Log tab duplicates Journal section stats. The Journal section shows flights/locations, and the Log tab shows flight history — feels redundant.

**Proposed new layout:**

```
1. Hero image + field notes overlay (keep)
2. Name + species badge + level (keep)
3. ─── STATE + LOCATION (merged, prominent) ──────────
   ⚡ Flying · 📍 Camden Market, London · ⏱ 2h 14m
   [Mini-map showing current position]        ← PROMOTED to top
   ────────────────────────────────────────────
4. ─── RABBLE (only if in one) ────────────────────────
   🏠 "Bug Collectors" — Hosting · 3 creatures
   → Tap row to open rabble
   [👁 Peek]  [💬 Chat]                      ← KEEP these — they're useful shortcuts
   ────────────────────────────────────────────
5. ─── FRIENDS ────────────────────────────────────────
   👫 3 friends · 🔔 1 pending request
   [+ Befriend] button (if viewing someone else's creature)
   ────────────────────────────────────────────
6. Actions row (contextual: Fly / Perch / Tether / Transfer)
7. ─── JOURNAL (collapsed by default) ────────────────
   📊 12 flights · 7 locations
   [Expand →] opens full journal / log
   ────────────────────────────────────────────
8. Config (collapsed, rarely used — gear icon)
```

**Key changes:**
- **Map promoted** from buried/absent → right below the state line
- **Rabble section** keeps Chat/Peek buttons — these are useful shortcuts into the rabble context
- **Legacy bottom panel removed** — the `[Live] | [Log]` TabBar + TabBarView at the bottom is the confusing part. The Live tab is a second map/telemetry view that duplicates the promoted mini-map. The Log tab duplicates the Journal section. Both go away.
- **Social section** renamed to **Friends** — clear single purpose
- **Befriend button** added for viewing other people's creatures
- **Journal** collapsed by default — tap to expand into full log (replaces Log tab)
- **Config** collapsed behind a gear icon, not a section
- **Single scroll** — no more NestedScrollView with tabs. Just a clean vertical layout.

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — remove TabController, NestedScrollView, and Live/Log tabs. Convert to single-scroll with promoted map + expandable journal.
- Keep `_HubActionButton` (Chat/Peek) on rabble section
- Add befriend flow to Friends section
- Promote mini-map into state section
- `rabble/lib/screens/creature/creature_live.dart` — functionality absorbed into state+map section
- `rabble/lib/screens/creature/creature_history.dart` — becomes the expanded Journal content

---

### F2. Creature State Coherence — Most Recent State on Open

**What was said:**
> "When I click on a creature card it needs most recent state."
> "Duplicate presences in the flight animation and rabble chats."

**Analysis:**

The creature is rendered from cached data. When a creature moves between states
(perched → flying → in rabble), stale data persists across screens. This causes:
- Creature showing as "in rabble" on one screen, "flying" on another
- Duplicate creature avatars in the flight animation (old state + new state)
- Rabble chat showing creatures that have already left

**Fix:**
1. **Force-refresh on creature detail open** — always call `GET /api/creatures/:id`
   when opening the creature detail screen, show shimmer skeleton while loading.
   The current code does this (`_load()` in `initState`) but the collection grid
   may be showing stale data while the detail screen loads fresh data.

2. **Invalidate creature cache on SSE events** — when a `state_changed` or
   `entered_rabble` / `left_rabble` event arrives, mark the creature as stale
   in the collection screen so it refreshes next time it's visible.

3. **Single source of truth for creature state** — the backend's
   `creature_state` field (from `creature_state` table) is the authoritative
   state. The Flutter app should use ONLY this, not try to infer state from
   flight records or presence fields.

**Files to change:**
- `rabble/lib/screens/collection_screen.dart` — refresh on return from detail
- `rabble/lib/screens/creature/creature_screen.dart` — show skeleton while loading
- `rabble/lib/widgets/flock_viz.dart` — deduplicate creatures by ID

---

### F3. Click-Through to Rabble

**What was said:**
> "Be able to click through to the rabble where it is (whether hosted or not)."

**Analysis:**

The rabble section on the creature card has a "Chat" button that tries to
navigate to `RabbleChatScreen` but requires fetching the full `SwarmEvent` first
(awkward API call). The whole section should just be a tappable row that takes
you to the rabble.

**Fix:**
- Make the entire rabble section a single tappable `InkWell`
- On tap: navigate to `RabbleChatScreen` by first fetching the swarm via
  `getSwarm(creature.rabbleId)` → navigate
- Remove Chat/Peek buttons entirely
- Show chevron `>` indicator to signal it's tappable

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — simplify rabble section

---

### F4. Befriending

**What was said:**
> "Hopefully I can finally friend somebody."
> "Be able to friend it."

**Analysis:**

The backend supports creature-to-creature friendships:
- `POST /api/creature-friendships` — send request (requires `from_creature_id`, `to_creature_id`)
- `POST /api/creature-friendships/:id/accept` — accept
- `POST /api/creature-friendships/:id/decline` — decline

The Flutter app has `friendship_request_card.dart` for handling incoming requests
but NO UI to **send** a request. The flow should be:

1. View someone else's creature (from rabble, map, or activity feed)
2. See Friends section with `[+ Befriend]` button
3. Tap → pick which of MY creatures sends the request
4. Confirm → sends request → shows "Request sent" toast

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — add befriend button
  (when viewing a creature you don't own)
- New: `rabble/lib/widgets/send_friendship_sheet.dart` — creature picker +
  confirmation bottom sheet

---

### F5. Explore Map — Remove Polling Feel

**What was said:**
> "The map in explore view feels like it's polling — we should get rid of that."

**Analysis:**

The explore screen has a 30-second polling timer (`_startPolling`) that refreshes
all data. This causes:
- Map flicker as pins are removed and re-added
- Visible loading states during refresh
- Wasted API calls for data that hasn't changed

**Fix:**
1. **Remove the 30-second polling timer** (`_pollTimer` in explore_screen.dart)
2. **Rely on SSE creature streams** for live position updates — already wired
3. **Pull-to-refresh** for manual map refresh (already exists via RefreshIndicator
   in feed view, needs to work in map view too)
4. **Map controls refresh button** already exists in top-right — keep that as the
   manual refresh mechanism

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — remove `_startPolling()` and
  `_pollTimer`, keep SSE-based updates

---

### F6. AR Viewer as Spatial View in Rabble Chat

**What was said:**
> "The AR viewer which is up — the experience of scanning the invite QR code
> with that AR viewer in the map view is something that should replace the
> current spatial view on top of the rabble chat screen."

**Analysis:**

Currently the rabble chat has:
- Left panel: MiniMap (static OpenStreetMap with creature dots)
- Right panel: FlockViz (2D boids animation)

The owner's vision: replace the split panel's map side (or the whole split panel)
with the AR viewer experience. When you're in a rabble, you should see creatures
through your camera, not on a flat map.

The QR scan → AR viewer flow already works beautifully. Making it the default
spatial view in rabble context would be a significant UX upgrade.

**Decision: Option D — toggle Map ↔ AR in the split panel. CONFIRMED.**

The owner's vision:

> "The experience of scanning the invite QR code delivers an interesting and cool
> experience. The intent was to be able to show up to a location and have this kind
> of magic viewer that allowed you to meet the AR creatures (in anticipation of Glasses).
> I want that same experience you get with the scanning of the bar code to be something
> I can see and toggle from the split panel view."

**What this means concretely:**

The split panel in rabble chat currently shows:
- Left: MiniMap (OpenStreetMap with creature dots)
- Right: FlockViz (2D boids animation)

With Option D:
- Left: **Map ↔ AR toggle** — MiniMap by default, tap 👁 to switch to camera-through
  AR view (same experience as the QR scan AR portal)
- Right: FlockViz boids (keep — "the split panel is great")
- Map should show **real-time tracks** (already wired via SSE in Phase 2)

**Fallbacks:**
- Desktop web (no camera) → MiniMap only, AR toggle hidden
- Camera permission denied → MiniMap with "Enable camera for AR" prompt
- Low battery → stay on MiniMap

**The AR experience to replicate:**
The QR code scanning flow currently opens `ArViewerScreen.portal()` which shows
creatures overlaid on the camera feed at the rabble's GPS coordinates. The goal
is to make this same renderer available as a panel in the split view — not a
full-screen takeover, but an inline camera feed with creatures overlaid.

This may require:
- A new `ArPanel` widget that wraps the AR camera feed in a constrained box
  (not full-screen like `ArViewerScreen`)
- Or: using the existing `Camera` widget with creature overlay painters

**Files to change:**
- `rabble/lib/screens/rabble_chat.dart` — add AR/Map toggle button on left panel,
  swap MiniMap ↔ ArPanel based on toggle state
- `rabble/lib/widgets/ar_panel.dart` — NEW: inline AR camera with creature overlay
  (extracted from `ar_viewer.dart` rendering logic, constrained to panel size)
- `rabble/lib/widgets/split_panel.dart` — no changes needed (just swap child)

---

### F18. Flock Dynamics — Reynolds Host-Only

**What was said:**
> "We had much richer flock dynamics but those are being lost in the noise so we
> will table them and reintroduce them in a better context. The one set of flight
> dynamics we do have is the Reynolds stuff and that should only be currently
> available to a host — to the host of the rabble (to be able to make the creatures
> fly in coordinated movement)."

**Analysis:**

The app has two flock dynamics systems:
1. **Reynolds boids** (separation/cohesion/alignment) — in `FlockViz` and the
   `SwarmEngine`/`RingAttractor` simulation. These produce coordinated flock movement.
2. **Onto4MAT formation algorithms** — purchasable from the swarm algorithm marketplace
   (`swarm_algorithms.rs`). These are richer: V-formation, echelon, line, wedge, etc.

The richer formations are getting lost in the noise. Decision: **table them for now**.

**What stays active:**
- Reynolds boids (FlockViz) — visible in the split panel right side
- Reynolds parameter control (separation/cohesion/alignment sliders) — **host only**
- The `FlightDynamics` widget that exposes these sliders should only appear for
  the rabble creator, not for participants

**What gets hidden/tabled:**
- Onto4MAT formation marketplace
- Advanced formation specs
- `swarm_algorithms.rs` endpoints (keep in code, hide from UI)
- Formation algorithm selector in rabble settings

**Files to change:**
- `rabble/lib/screens/rabble_chat.dart` — only show `FlightDynamics` sliders
  when `swarm.creatorId == auth.userId` (host check)
- `rabble/lib/widgets/flight_dynamics.dart` — no code changes, just gated by host check
- Hide formation algorithm UI from rabble settings/marketplace (if exposed anywhere)

---

### F19. AR QR Experience as Split Panel Toggle

**What was said:**
> "I want that same experience you get with the scanning of the bar code to be
> something I can see and toggle from the split panel view."

**Analysis:**

This is the implementation detail of F6. The QR scan flow currently:
1. User scans QR code → resolves to a `swarm_id`
2. Opens `ArViewerScreen.portal()` with the swarm's creatures
3. Renders creatures in 3D space using the camera feed + GPS position
4. User sees virtual creatures overlaid on the real world

The key question: can this renderer work in a **panel** (200px tall, half-screen
wide) rather than full-screen?

**Technical considerations:**
- Camera preview widget (`Camera` from the `camera` package) can be sized to any box
- AR creature overlay uses `CustomPainter` — works at any size
- GPS positioning logic is independent of screen size
- The `PortalCreature` model and rendering pipeline are reusable

**Implementation approach:**
1. Extract the camera + overlay rendering from `ArViewerScreen` into an `ArPanel` widget
2. `ArPanel` takes: `swarm`, `creatures`, `currentUserId`, `lat`, `lng`
3. Wrap in a `ClipRRect` to fit the split panel dimensions
4. Add a 👁 toggle button in the split panel header (left side)
5. Toggle swaps `MiniMap` ↔ `ArPanel`

**Files to change:**
- `rabble/lib/widgets/ar_panel.dart` — NEW: constrained AR camera + creature overlay
- `rabble/lib/screens/ar_viewer.dart` — extract shared rendering logic into reusable painters
- `rabble/lib/screens/rabble_chat.dart` — toggle button + panel swap

---

### F7. Duplicate Log vs Journal Info

**What was said:**
> "There seems now to be a duplicate log vs Journal info."
> "The Journal should be expandable to log with click-through recreations which
> are still deferred features."

**Analysis:**

Currently the creature detail has:
- **Journal hub section** — shows total flights, locations, active flight duration
- **Log tab** (tab 2) — shows full flight history + version history

These overlap. The journal section is a summary, the log tab is the detail.
But having both makes the screen feel redundant.

**Fix:**
- **Remove the Log tab entirely** — the creature detail becomes single-scroll, no tabs
- **Journal section** becomes expandable:
  - Collapsed: "12 flights · 7 locations" + "Expand →"
  - Expanded: full flight list (what Log tab currently shows)
  - Each flight is tappable → future flight detail/recreation screen
- **Live tab functionality** (telemetry map, track viz) moves into the
  State+Location section's mini-map, which can expand to full screen

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — remove TabController,
  collapse into single scroll with expandable Journal
- `rabble/lib/screens/creature/creature_history.dart` — becomes the
  expanded Journal content

---

## Priority Order for Sprint

### Must (before user testing round 2)
1. **F1** — Creature card simplification + re-layout
2. **F4** — Add befriend button + send friendship flow
3. **F2** — Force-refresh creature state + deduplicate presences
4. **F3** — Click-through to rabble (tappable row)

### Should (same sprint if time)
5. **F5** — Remove polling from explore map
6. **F7** — Merge Log tab into expandable Journal section

### Discuss (design decision needed)
7. **F6** — AR viewer as spatial view in rabble chat (needs owner decision on option A/B/C/D)

---

## How These Fixes Map to Files

| File | Fixes |
|------|-------|
| `rabble/lib/screens/creature/creature_screen.dart` | F1, F2, F3, F4, F7 |
| `rabble/lib/screens/collection_screen.dart` | F2 |
| `rabble/lib/screens/explore_screen.dart` | F5 |
| `rabble/lib/screens/rabble_chat.dart` | F6 |
| `rabble/lib/widgets/flock_viz.dart` | F2 |
| `rabble/lib/widgets/send_friendship_sheet.dart` | F4 (new file) |
| `rabble/lib/widgets/creature_link.dart` | F3 (minor) |
| `rabble/lib/models/creature.dart` | F2 (no changes, just noting) |

The creature_screen.dart is the epicentre — 5 of 7 fixes touch it.
That file should be rewritten as a clean single-scroll layout, not patched.

---

### F8. Rabble Page — Ordering, Context & Quick Actions

**What was said:**
> "The rabble page is fine but it should be most recent activity at the top."
> "The cards need more context — I need to know what my host creature is, and
> what creatures I have in the rabble."
> "I need to be able to edit and configure the rabble."
> "I need to be able to invite creatures and friends directly from the view
> with as little effort as possible."

**Analysis:**

The RabblesScreen Hosting/Joined/Following tabs work structurally but the cards
are too sparse and the actions require too many taps.

**Current state of rabble cards:**
- Anchor creature avatar + name + status badge
- Creature count + location
- Host name (joined only)
- `my_creatures` CreatureLink row (when data available)
- Tap → rabble chat

**What's missing:**
1. **Sort order** — cards are ordered by creation, not by last activity. Most
   active rabble should be at the top. The backend `my/rabbles` already returns
   `last_activity_at` — just need to sort by it client-side.

2. **Host creature prominence** — when hosting, the card should show which of
   YOUR creatures is the anchor/host. The `my_creatures` row exists but doesn't
   call out the anchor. Need: "⚓ Luna is anchoring" or similar.

3. **My creatures in this rabble** — the `my_creatures` row may not be populating
   because of Issue 1/2 (`creator_id` missing from `my/rabbles`). Once that's
   fixed, each card should clearly show: "Your creatures: Luna, Atlas, Morpho".

4. **Edit/configure** — hosting cards need a ⚙ gear icon that opens a settings
   sheet: rename, change visibility, adjust radius, set walk-in price, end rabble.
   Backend already supports `PATCH /api/swarms/:id`.

5. **Quick invite** — cards need a `[+ Invite]` button that opens the invite sheet
   (already exists in `rabble_chat.dart` as `_InviteSheet`). Extract it to a
   shared widget so it can be triggered from the rabble card without entering chat.

6. **Quick add creature** — cards need a `[+ Add Creature]` button that opens
   the creature picker (already exists in `rabble_chat.dart` as
   `_addCreatureToRabble`). Extract to shared widget.

**Proposed enhanced rabble card:**

```
┌──────────────────────────────────────────────┐
│ [Anchor Avatar]  Bug Collectors     ● active │
│                  📍 Camden Market             │
│                  🐛 5 creatures · 3 people   │
│──────────────────────────────────────────────│
│ ⚓ Luna (hosting)  ·  Atlas  ·  Morpho       │ ← my creatures row
│──────────────────────────────────────────────│
│ [+ Creature]  [+ Invite]  [⚙ Edit]  [→ Open]│ ← quick actions
└──────────────────────────────────────────────┘
```

For joined rabbles:
```
┌──────────────────────────────────────────────┐
│ [Anchor Avatar]  Bug Collectors     ● active │
│                  📍 Camden Market             │
│                  🐛 5 creatures · Host: @alex │
│──────────────────────────────────────────────│
│ Your creature: Atlas                         │
│──────────────────────────────────────────────│
│ [+ Add Creature]  [→ Open]                   │
└──────────────────────────────────────────────┘
```

**Files to change:**
- `rabble/lib/screens/rabbles_screen.dart` — sort by `last_activity_at`, enhance
  `_MyRabbleCard` with edit/invite/add-creature actions, highlight anchor creature
- Extract `_InviteSheet` from `rabble_chat.dart` → `rabble/lib/widgets/invite_sheet.dart`
- Extract creature picker from `rabble_chat.dart` → `rabble/lib/widgets/creature_picker_sheet.dart`

---

### F20. "My Location" Button Should Zoom to GPS Position

**What was said:**
> "When I click on My Location in the map view it should — if I have allowed the
> app to — zoom to me."

**Analysis:**

The "My Location" button in the viewpoint toggle currently sets `_viewpoint = 'my_location'`
and updates `_viewpointCenter` to the device GPS position. But the map doesn't
actually **animate** to that position — it only affects the `initialCenter` which
is set once on build. Subsequent taps don't move the map.

**Fix:**
1. Add a `MapController` to the `FlutterMap` widget in `ExploreScreen`
2. When "My Location" is tapped:
   - Request/refresh GPS position via `LocationService.refreshPosition()`
   - Animate the map to the new position: `mapController.move(LatLng(lat, lng), 15)`
   - If location permission not granted, prompt with `LocationService.requestLocation()`
3. Add a dedicated "locate me" FAB or use the existing "My Location" toggle as the trigger

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — add `MapController`, wire into
  `_switchViewpoint('my_location')` to call `mapController.move()`, request
  location permission if needed

---

### F21. One Host Per Rabble — Enforce + Show Prominently

**What was said:**
> "There can only be one host per rabble — that needs fixing."
> "On the rabble chat and rabble cards that rabble host needs to be obvious."

**Analysis:**

Two issues here:

**21a. Enforcement:**
The backend currently allows multiple creatures from the same user to join a rabble, and the "host" concept is derived from `creator_id` on the `swarm_events` table. But there's no enforced constraint that says "only one creature can be the anchor/host creature." The `anchor_creature_id` field exists but:
- It's set during `host_rabble_handler` (the creature that creates the rabble)
- It can be transferred via `transfer_anchor_handler`
- But nothing prevents the same user from adding multiple creatures, making it unclear which one is "the host creature"

**Fix (backend):**
- The host is the USER who created the rabble (`creator_id`). Their anchor creature (`anchor_creature_id`) is the host creature.
- Clarify that "host" = the creator user, "anchor" = the specific creature anchoring the rabble
- Ensure the anchor creature is always shown first in member lists

**21b. Prominence on UI:**
On rabble cards and rabble chat, the host should be immediately obvious:
- Rabble card: "Hosted by @username" with the anchor creature avatar
- Rabble chat app bar: host name/avatar visible
- Member list: host creature first with a crown/star badge

**Fix (Flutter):**
- `RabblesScreen` cards: already show host name for "joined" rabbles — also show for hosting rabbles ("You're hosting with Luna ⚓")
- `RabbleChatScreen` app bar: add host name + anchor creature mini avatar
- Member list: sort host creature first, add 👑 badge

**Files to change:**
- `rabble/lib/screens/rabbles_screen.dart` — host creature prominence on cards
- `rabble/lib/screens/rabble_chat.dart` — host info in app bar
- `rabble/lib/widgets/creature_link.dart` — optional crown/host badge via `trailing`

---

### F22. Rabble Description

**What was said:**
> "It would be nice to have a rabble description."

**Analysis:**

The backend already supports `description` on `swarm_events`:
- `CreateSwarmRequest` has `description: Option<String>`
- `UpdateSwarmRequest` has `description: Option<String>`
- The field is stored and returned in API responses
- The `SwarmEvent` Flutter model already has `description` field

But the Flutter UI never shows or lets you set it:
- `host_rabble_handler` doesn't prompt for description
- Rabble cards don't show description
- Rabble chat doesn't show description
- There's no way to edit it after creation

**Fix (Flutter):**
- Host rabble flow: add optional description field to the host dialog
- Rabble cards: show description below name (truncated, 2 lines max)
- Rabble chat: show description in an info section below the app bar
- Edit rabble sheet (F8): include description field

**Files to change:**
- `rabble/lib/screens/rabbles_screen.dart` — show description on cards
- `rabble/lib/screens/rabble_chat.dart` — show description below app bar
- `rabble/lib/screens/creature/creature_screen.dart` — if hosting, show description in rabble hub section
- `rabble/lib/widgets/creature_picker.dart` or host flow — add description input when creating rabble

---

### F23. Duplicate Journal Card on Creature Detail

**What was said:**
> (Screenshot shows two Journal sections — one hub section and one expandable at the bottom)

**Analysis:**

The creature detail screen now has TWO journal-like sections:
1. The `_HubSection` Journal card (from Phase 1 hub additions) — shows flights + locations
2. The expandable Journal with `CreatureHistory` (from F7 sprint fix) — also shows flights + locations

These are redundant. The F7 expandable journal was meant to REPLACE the hub section, but both survived.

**Fix:**
- Remove the first Journal `_HubSection` (the static one with just stats)
- Keep only the F7 expandable Journal that shows stats collapsed and CreatureHistory expanded
- The expandable version already has flights + locations count + expand arrow

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — remove the original `_HubSection` Journal block (the one between Friends and Config sections)

---

### F24. Configuration Card Should Be Above Rabble Card

**What was said:**
> "The configuration card should be above the rabble card and at the top of the creature."

**Analysis:**

Current creature detail order:
1. Hero image
2. Name + species + level
3. Presence chips
4. Rabble section
5. Friends section
6. Journal section (duplicate)
7. Config (collapsible)
8. Actions (Social/Movement/Economy pills)
9. Journal (expandable — duplicate)

The Config section is buried. The user wants it near the top — it contains the creature's settings that you'd check/change before doing anything else.

**Proposed new order:**
1. Hero image
2. Name + species + level
3. Presence chips
4. **Config** (collapsible, near top)
5. Rabble section (if in one)
6. Friends section
7. Actions (in card UI)
8. Journal (expandable)

**Files to change:**
- `rabble/lib/screens/creature/creature_screen.dart` — move `CreatureConfig` widget above the Rabble `_HubSection`

---

### F25. Movement Pills Should Be in Card UI

**What was said:**
> "The movement pills should be in similar card UI."

**Analysis:**

Currently the action buttons (Social: Join, Movement: Hop/Expedition/Tether, Economy: Gift/List) are rendered as loose pill buttons with section labels. They look like a different design language than the hub sections (Rabble, Friends, Journal) which use `_HubSection` cards.

**Fix:**
- Wrap the actions in `_HubSection` style cards:
  - "Movement" section: Hop, Expedition, Tether
  - "Social" section: Join/Find Rabble (see F26)
  - "Economy" section: Gift, List
- Or: wrap the entire `CreatureActions` widget in a single `_HubSection` card with title "Actions"

**Files to change:**
- `rabble/lib/screens/creature/creature_actions.dart` — wrap in card-style container
- Or: `rabble/lib/screens/creature/creature_screen.dart` — wrap `CreatureActions` in `_HubSection`

---

### F26. "Join" Should Be "Find a Rabble" + End Rabble Semantics

**What was said:**
> "The join rabble should be more like 'find a rabble to join'. If you are hosting
> the rabble and you join a different rabble you effectively finish yours — we will
> need explicit end rabble/transfer ownership of rabble semantics."

**Analysis:**

Two issues:

**26a. Label change:**
The "Join (free-2cr)" button implies you know which rabble to join. Better: "Find a Rabble" which opens the map or nearby rabbles list, allowing discovery.

**26b. Hosting conflict semantics:**
If you're hosting rabble A and join rabble B with your anchor creature, rabble A is effectively abandoned. The system should:
1. Warn: "Your creature Luna is anchoring 'Party time's rabble'. Moving Luna to another rabble will end your current rabble."
2. Offer: "End rabble" or "Transfer anchor to [other creature]" before proceeding
3. Or: prevent the anchor creature from leaving (force transfer first)

**Backend consideration:**
The `join_swarm_handler` already auto-ends active flights. But it doesn't check if the creature is an anchor. Need to add a check:
- If `creature_id == swarm.anchor_creature_id` for any active swarm → block join or force transfer.

**Files to change:**
- `rabble/lib/screens/creature/creature_actions.dart` — rename "Join" → "Find a Rabble", link to explore map
- `fermi/src/handlers/creatures/state.rs` — in `join_swarm_handler`, check if creature is anchor of another rabble, warn/block
- New: `rabble/lib/widgets/end_rabble_sheet.dart` — confirmation sheet for ending/transferring rabble

---

### F27. Rabble Cards Need Mine vs External Creature Counts

**What was said:**
> "I would like to know how many creatures I have there that are mine as well
> (e.g. 5 creatures are mine, n are external)."

**Analysis:**

Rabble cards currently show total creature count ("5 creatures") but don't distinguish
between the user's own creatures and others. The `my_creatures` array is available
from the `my/rabbles` endpoint — just need to render the split.

**Fix:**
- Instead of "🐛 5 creatures", show: "🐛 5 creatures (2 mine, 3 others)" or
  "🐛 2 mine · 3 others"
- Use the `my_creatures` array length for "mine" count
- Subtract from total `creature_count` for "others"

**Files to change:**
- `rabble/lib/screens/rabbles_screen.dart` — update creature count display in `_MyRabbleCard`

---

### F28. Creature Tray Icon/Switch Lost in Rabble Chat

**What was said:**
> "I've lost in the rabble chat the creature icon switch when I switch creatures to chat."
> (Screenshot shows "Join with a creature" prompt but no creature tray with switchable avatars)

**Analysis:**

The creature tray at the top of the rabble chat shows the user's creatures in this rabble
as tappable avatars. When the user has no creatures in the rabble, it shows "Join with a creature".

The issue from the screenshot: the user IS in the rabble (messages show "hermanito" sending)
but the tray shows "Join with a creature" and "0 members". This suggests:
1. The `_loadMyCreatures` call failed silently, or
2. The creature-to-rabble matching logic (`c.rabbleId == widget.swarm.swarmId`) doesn't match
   because the creature's `rabbleId` field isn't populated in the list endpoint
3. The SSE creature context banner correctly shows "You're peeking" because it falls through
   to the empty-creatures path

**Fix:**
- Add error logging to `_loadMyCreatures` in rabble_chat.dart
- Cross-check: does the creature list endpoint return `rabble_id` for creatures in swarms?
- If not, use the `my/rabbles` endpoint's `my_creatures` array to determine which creatures
  are in this specific rabble
- Alternative: when entering rabble chat from a creature's Chat button, pass the creature ID
  so the tray pre-selects it

**Files to change:**
- `rabble/lib/screens/rabble_chat.dart` — add error logging to `_loadMyCreatures`,
  consider alternative creature-matching strategy
- `rabble/lib/screens/creature/creature_screen.dart` — pass creature ID when navigating to rabble chat

---

### F29. User Search Cannot Find Known Users — 🔴 BLOCKER

**What was said:**
> "I still can't find users through the add users UI in my profile system that I know
> are in the system. This is preventing sharing and socialization and testing of these
> features. Please unblock."

**Analysis:**

The `GET /api/users/search?q=term` endpoint searches by `display_name ILIKE` and `email ILIKE` only.
Many users may have:
- No `display_name` set (NULL) — they'd be invisible to search
- A `user_id` format like `google-oauth2|123456` that nobody would search for
- A GitHub username that's their known identity but isn't searched

**Fix (APPLIED):**
Backend search expanded to match: `display_name`, `email`, `user_id`, `github_username`,
`google_id`, AND `creature specimen_name` (so you can find someone by their creature's name).

Also: if no `display_name`, show github_username or email prefix as the display name.
And: return `creature_count` and `creature_names` in search results.

Flutter: error feedback added (was silently swallowing search failures).

**Files changed:**
- `fermi/src/handlers/users.rs` — expanded search query
- `rabble/lib/screens/profile_screen.dart` — error feedback on search

**Status: FIX APPLIED — needs deploy to verify.**

---

### F14. Remove Environment Feed View — Redundant with Journals

**What was said:**
> "Actually the activity screen in environment is redundant given journal pages —
> which look interesting."

**Analysis:**

The Environment tab currently has two modes:
- **Feed view** (default) — chronological activity feed, identical to Journals → Activity tab
- **Map view** — spatial map with creature pins, rabble circles, viewpoint toggle

The feed view is a straight duplicate of what Journals already does. It adds
cognitive load ("where do I find my activity? Two places?") and the feed has no
spatial relevance — it's just a list of events that happens to live in the
Environment tab for historical reasons.

**Fix:**
- **Remove the feed view entirely** from ExploreScreen
- Environment tab opens DIRECTLY to the map (combines with F13)
- Remove the list toggle button from map controls
- The feed/activity content lives exclusively in Journals → Activity tab
- Keep the map controls: refresh, layer toggles, viewpoint toggle, AR FAB

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — remove `_buildFeedView()`, remove
  `_mapMode` toggle entirely, remove `_filter`, remove `_loadMore`, remove
  `_scrollController` scroll listener. The screen IS the map.

---

### F15. All Bugs Semantics + Friend Activity in Journals

**What was said:**
> "What are the semantics of 'All Bugs' tab — that's all my bugs or all bugs I
> have visibility to?"
> "The environment list view is the only place I can see friends' activity —
> that feels strange. That should be in journals, filtered for friends or for mine."

**Analysis:**

**All Bugs tab semantics:**
Currently "All Bugs" shows the user's OWN creatures grouped by creature, with
their recent events. The name is ambiguous — it could mean:
- A) All MY creatures (current behaviour)
- B) All creatures I can see (mine + friends + public)

**Decision needed:** Is this a "my menagerie journal" or a "world view"?

**Recommendation:** Rename to **"My Creatures"** and keep it as the per-creature
grouped view of YOUR activity. This is the creature-centric journal.

**Friend activity:**
Currently the only place to see friend/contact activity is the Environment feed
(which we're removing in F14). This needs to move to Journals.

**Fix — Journals tab restructure:**

```
┌────────────┬────────────┬──────────────┬──────────────┐
│  Activity  │  Friends   │ My Creatures │   Flights    │
└────────────┴────────────┴──────────────┴──────────────┘
```

- **Activity** — YOUR events (creature flew, joined rabble, etc.) + SSE live
- **Friends** — events from contacts/friends' creatures (filtered from the same
  `/api/feed/events` endpoint which already includes `is_contact` / `is_friend_creature` flags)
- **My Creatures** — (renamed from "All Bugs") grouped-by-creature view
- **Flights** — flight history (keep as is)
- **Reports tab removed** — completed rabble recaps move into the Flights tab
  as a "Recaps" section, or into My Creatures as per-creature recaps

**Files to change:**
- `rabble/lib/screens/journals_screen.dart` — rename "All Bugs" → "My Creatures",
  add "Friends" tab filtered on `is_contact || is_friend_creature`, remove
  Reports tab (merge into Flights or My Creatures)

---

### F16. WhatsApp-Style Chat Layout

**What was said:**
> "In the chats I think I would like it to be that messages from my creatures are
> on one side and messages from others on the other side — WhatsApp-style chat layout."

**Analysis:**

Currently all messages in the rabble chat are left-aligned in a flat list. There's
no visual distinction between "my messages" and "their messages". This makes it
hard to follow conversations.

**Fix:**
- My creature's messages: **right-aligned**, darker background (`RabbleTheme.bg3`)
- Other creatures' messages: **left-aligned**, standard background (`RabbleTheme.bg2`)
- System/narrator messages: **centred**, subtle background
- Add avatar + creature name above each message bubble (or on first message in a group)
- Group consecutive messages from the same creature (no repeated avatar)

The `ChatPanel` widget already has `activeCreatureId` — it knows which creature
is "me". It also receives `sender_id` and `creature_id` per message. The data
is there, just needs layout changes.

**Files to change:**
- `rabble/lib/widgets/chat_panel.dart` — switch from flat list to
  `Align(alignment: isMe ? Alignment.centerRight : Alignment.centerLeft)` per
  message bubble. Add bubble shape styling.

---

### F17. Creature Handle Colors — Differentiate Mine from Others

**What was said:**
> "My creatures should have a different color handle — currently all are yellow.
> Consider the green we use as a contrast color."

**Analysis:**

In the rabble chat, creature names (handles) above messages are all rendered in
`RabbleTheme.amber` regardless of ownership. This makes it impossible to quickly
scan and see "which messages are mine?"

**Fix:**
- **My creature handles**: `RabbleTheme.mint` (the green accent — already used
  for active/positive states throughout the app)
- **Other creature handles**: `RabbleTheme.amber` (keep current)
- **System/narrator handles**: `RabbleTheme.violet`

This pairs with F16 — left/right alignment + colour coding makes the chat
immediately scannable.

**Files to change:**
- `rabble/lib/widgets/chat_panel.dart` — colour creature name based on
  `message.senderId == currentUserId` or `message.creatureId == activeCreatureId`

---


**What was said:**
> "There are users in the system that I am trying to add as friends and I cannot —
> perhaps it's because they were legacy users — and getting user-id 500 errors.
> This needs to be fixed. I need to be able to invite and add friends and those
> friends need to do the same — this has to be in place for soft launch."

**Analysis:**

The social layer has two tiers:
1. **Contacts** (user-to-user): `POST /api/contacts` — requires the target `user_id` to exist in the `users` table
2. **Creature friendships** (creature-to-creature): `POST /api/creature-friendships` — requires both creatures to exist + owners to be different users

The 500 errors are likely caused by:

**Root cause candidates:**
1. **Legacy users with NULL user_id** — Migration 004 created users with `user_id TEXT PRIMARY KEY`, but migration 004b added a partial unique index excluding legacy users (`WHERE auth_provider != 'legacy'`). Migration 093 added a proper UNIQUE constraint. Legacy users may have auth_provider values that don't match the CHECK constraint, or their user_id format may be incompatible.

2. **Contacts table FK violations** — The `contacts` table has `user_id` and `contact_id` columns. If these reference `users(user_id)` via FK, and the target user doesn't have a proper `users` row, the INSERT fails with a 500.

3. **Creature ownership mismatch** — If a creature's `owner_id` points to a legacy user string that doesn't match the authenticated user's ID format (e.g., Ethereum address vs Google OAuth ID), the ownership check passes but downstream queries fail.

4. **Notification INSERT failure** — After a successful friendship request, the handler tries to INSERT a notification for the target user. If the target user's `user_id` violates a NOT NULL or FK constraint on the `notifications` table, the whole request fails with 500 even though the friendship was created.

**Investigation steps:**
```bash
# Check what users exist and their auth_providers
psql -c "SELECT user_id, email, auth_provider, display_name FROM users ORDER BY created_at"

# Check if contacts table has FK constraints
psql -c "SELECT conname, conrelid::regclass, confrelid::regclass FROM pg_constraint WHERE conrelid = 'contacts'::regclass"

# Check for any NULL user_ids in creatures
psql -c "SELECT creature_id, owner_id FROM creatures WHERE owner_id NOT IN (SELECT user_id FROM users WHERE user_id IS NOT NULL)"

# Look at recent 500s in logs
# Check server logs for the exact SQL error
```

**Fixes needed:**
1. **Defensive contact handler** — if `users` row doesn't exist for a legacy user, return a clear error ("This user needs to sign in with the new auth system first") instead of 500.

2. **Friendship handler** — catch notification INSERT failures gracefully (don't let notification failure kill the friendship request). Wrap the notification INSERT in a try/catch that logs but doesn't propagate.

3. **Legacy user migration** — ensure all users in the system have valid `user_id` entries. May need a backfill script.

4. **User lookup endpoint** — `GET /api/users/search?q=<name>` so the Flutter app can find users to befriend (currently there's no user discovery).

**Files to change:**
- `fermi/src/handlers/social.rs` — defensive error handling in `add_contact_handler` and `send_friendship_request_handler`
- `fermi/src/handlers/social.rs` — wrap notification INSERTs in error-swallowing blocks
- New: `scripts/fix_legacy_users.sql` — backfill script
- Potentially: new user search/discovery endpoint

**This is the #1 blocker for soft launch. Without working friendships, the social layer is broken.**

---

### F12. Map Visual Distinction — Rabbles vs Creatures

**What was said:**
> "In the map view — need better visual distinction between rabbles and creatures."

**Analysis:**

Currently both creature pins and rabble markers are similarly sized (40-56px) with
similar visual weight. Rabbles use an amber square with a groups icon, creatures use
a circular avatar. At a glance it's hard to distinguish them, especially when
zoomed out.

**Fix:**
- **Rabble markers**: Make larger (60px), use the radius circle as the primary visual (already added in Phase 3). Add the rabble NAME as a label below the marker, not just the creature count.
- **Creature pins**: Keep current size (40px circular avatar) but add a subtle state-colored ring: mint for tethered/live, amber for flying, grey for perched.
- **Colour coding**: Rabbles = amber/gold family. Creatures = species-colored (already the case). Make the distinction more prominent.
- **Z-ordering**: Creatures render ON TOP of rabble circles, so they're never hidden.

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — adjust marker sizes, add name labels to rabble markers, z-order creature pins above rabbles

---

### F13. Map as Default View for Environment Tab

**What was said:**
> "The map should be the default view for environment."

**Positive feedback noted:**
> "The through the bug's eyes feature is great."
> "I like the ability to join from map — that's great."

**Analysis:**

Currently the Environment tab defaults to the feed/list view (`_mapMode = false`).
The user has to tap the map button to switch. The map IS the environment — it should
be the first thing you see.

**Fix:**
- Change `_mapMode` default from `false` to `true` in `ExploreScreen`
- Keep the list/feed view accessible via the list button in map controls
- The feed becomes the secondary view, not the primary

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — change line `bool _mapMode = false;` to `bool _mapMode = true;`

One-line fix. High impact.

---

### F9. Environment Activity Feed — Remove Polling + Creature Context on Rabble Click-Through

**What was said:**
> "The activity screen in environment is great — but it's also polling and should
> be real-time."
> "When I click through into rabbles I have no idea which creature I've clicked
> in with — I need that context to not be discombobulated."

**Analysis:**

Two related problems:

**9a. Polling in the explore feed:**
The ExploreScreen has a 30-second `_pollTimer` that refetches everything (feed
events, swarms, creature positions). This causes visible refreshes and wasted
API calls. Same issue as F5 but specifically about the activity feed in the
explore tab, not just the map.

The existing `ActivityFeedWidget` already has SSE support (connects to
`/api/feed/stream`). The explore screen should use this widget or the same SSE
pattern instead of its own polling timer.

**Fix:**
1. Remove `_pollTimer` from `explore_screen.dart` entirely
2. Wire SSE stream for feed events (same pattern as `ActivityFeedWidget`)
3. Keep manual refresh button in map view for on-demand reload
4. Creature pin positions already update via `CreatureStreamService` SSE — no polling needed

**9b. Creature context when clicking into a rabble:**
When you tap a rabble on the map or in the feed, you enter `RabbleChatScreen`
with NO indication of which of YOUR creatures is in that rabble. You're dropped
into a chat and have to figure out your context.

The `RabbleChatScreen` already has a creature tray that shows your creatures,
but it loads asynchronously and doesn't prominently show "You're here as Luna".

**Fix:**
1. When navigating to a rabble from the explore screen, pass the user's creature
   context: which creature was tapped, or which creature is in this rabble.
2. `RabbleChatScreen` should show a prominent banner at the top:
   "You're in this rabble as **Luna** 🦋" (with creature avatar)
3. If the user has NO creature in this rabble, show:
   "You're peeking — [Join with a creature]"
4. Pre-select the relevant creature in the creature tray when entering

**Files to change:**
- `rabble/lib/screens/explore_screen.dart` — remove `_pollTimer`, wire SSE, pass
  creature context when navigating to rabble
- `rabble/lib/screens/rabble_chat.dart` — accept optional `activeCreatureId` param,
  show "You're here as [creature]" banner prominently

---

### F10. Profile Page — White Background / Missing Theme

**What was said:**
> "Profile page is white and needs theming."

**Analysis:**

The `ProfileScreen` was moved from a bottom nav tab to the account menu. It
likely uses a default `Scaffold` background without applying `RabbleTheme` colors.
The rest of the app uses the dark theme (`RabbleTheme.bg0` / `bg1`), so a white
profile page looks jarring and broken.

**Fix:**
- Apply `RabbleTheme.bg0` as scaffold background
- Ensure all text uses `RabbleTheme.fg0` / `fg1` / `fg2` (not default black)
- Check Card colors use `RabbleTheme.bg2` not default white
- Check any `TextField` / `InputDecoration` uses themed borders and fill colors
- Audit any hardcoded `Colors.white` backgrounds

**Files to change:**
- `rabble/lib/screens/profile_screen.dart` — apply dark theme consistently

---

## Updated Priority Order for Sprint

### 🔴 SOFT LAUNCH BLOCKERS
1. **F11** — Fix friendship 500 errors ✅ DONE
2. **F4** — Add befriend button + send friendship flow ✅ DONE
3. **F29** — User search cannot find known users ✅ FIX APPLIED (needs deploy)

### Must (before user testing round 3)
4. **F23** — Remove duplicate Journal card on creature detail
5. **F24** — Move Config above Rabble card on creature detail
6. **F25** — Movement pills in card UI style
7. **F27** — Rabble cards: mine vs external creature counts
8. **F28** — Fix creature tray in rabble chat (creatures not loading)
9. **F26** — "Join" → "Find a Rabble" + end rabble semantics + anchor guard

### Completed this sprint
10. **F1** — Creature card rewrite (single scroll, no tabs) ✅ DONE
11. **F2** — Force-refresh + shimmer ✅ DONE
12. **F5** — Remove polling ✅ DONE
13. **F7** — Expandable journal ✅ DONE
14. **F8** — Rabble card enhancements (sort, quick actions) ✅ DONE
15. **F9b** — "You're here as Luna" banner ✅ DONE
16. **F10** — Profile dark theming ✅ DONE
17. **F12** — Map visual distinction ✅ DONE
18. **F13 + F14 + F20** — Map-only explore, GPS zoom ✅ DONE
19. **F15** — Journals restructure ✅ DONE
20. **F16 + F17** — WhatsApp chat + handle colours ✅ DONE
21. **F21** — Host prominence ✅ DONE
22. **F22** — Rabble description ✅ DONE

### Should (next sprint)
23. **F3** — Full tappable rabble row (Chat/Peek work, row tap deferred)
24. **F6 + F18 + F19** — AR panel toggle, Reynolds host-only, ArPanel widget

### Design Notes (for future sprint)
- **Reports** are a type of rich activity event (flight recap, rabble summary) with a rich payload, not a separate tab. Design the payload shape and delivery mechanism in a design session.
- **F26 backend** — Anchor creature guard: prevent anchor from leaving hosted rabble without transfer/end. Needs explicit "End Rabble" and "Transfer Anchor" flows.

### ✅ Positive feedback (keep/protect these)
- "Through the bug's eyes" viewpoint toggle — loved
- "Join from map" flow — great UX
- Journals tabs — look interesting
- Rabble page structure + cards — great
- Map view overall — looking good
- Split panel (map + flock animation) — great, especially with real-time tracks
- WhatsApp-style chat layout — clean
- Host prominence on cards — clear
- Creature context banner ("You're here as Luna") — helpful

---

## Updated File Map

| File | Fixes |
|------|-------|
| **BACKEND** | |
| `fermi/src/handlers/social.rs` | F11 (defensive error handling, notification isolation) |
| `fermi/scripts/fix_legacy_users.sql` | F11 (new — backfill legacy users) |
| **FLUTTER** | |
| `rabble/lib/screens/creature/creature_screen.dart` | F1✅, F2✅, F3, F4✅, F7✅, F23, F24, F25 |
| `rabble/lib/screens/creature/creature_actions.dart` | F25 (card UI), F26 (rename Join → Find a Rabble) |
| `rabble/lib/screens/creature/creature_live.dart` | F1✅ (absorbed into main screen) |
| `rabble/lib/screens/creature/creature_history.dart` | F7✅ (becomes expandable journal content) |
| `rabble/lib/screens/collection_screen.dart` | F2✅ |
| `rabble/lib/screens/rabbles_screen.dart` | F8✅, F21✅, F22✅, F27 |
| `rabble/lib/screens/rabble_chat.dart` | F6, F8, F9b✅, F18, F19, F28 |
| `rabble/lib/screens/profile_screen.dart` | F10✅, F29✅ |
| `rabble/lib/widgets/ar_panel.dart` | F6, F19 (new — inline AR camera panel for split view) |
| `rabble/lib/widgets/flight_dynamics.dart` | F18 (gated to host-only, no code changes — just caller gating) |
| `rabble/lib/screens/explore_screen.dart` | F5✅, F9a✅, F12✅, F13✅, F14✅, F20✅ |
| `rabble/lib/screens/journals_screen.dart` | F15✅ |
| `fermi/src/handlers/users.rs` | F29✅ (expanded search fields) |
| `fermi/src/handlers/creatures/state.rs` | F26 (anchor creature guard in join_swarm) |
| `rabble/lib/widgets/end_rabble_sheet.dart` | F26 (new — end rabble / transfer anchor confirmation) |
| `rabble/lib/widgets/chat_panel.dart` | F16, F17 (WhatsApp layout, creature handle colours: mine=mint, others=amber) |
| `rabble/lib/widgets/flock_viz.dart` | F2 |
| `rabble/lib/widgets/send_friendship_sheet.dart` | F4 (new file) |
| `rabble/lib/widgets/invite_sheet.dart` | F8 (extracted from rabble_chat.dart) |
| `rabble/lib/widgets/creature_picker_sheet.dart` | F8 (extracted from rabble_chat.dart) |