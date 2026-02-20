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

**This is a design decision to discuss, not an immediate fix.**

Options:
- **A)** Replace MiniMap in split panel with AR camera feed
- **B)** Replace the entire split panel with AR viewer (full width)
- **C)** Make the split panel default to AR left + FlockViz right
- **D)** Add a toggle: Map ↔ AR in the split panel

**Considerations:**
- AR requires camera permission — can't be default for everyone
- AR doesn't work on desktop web — needs fallback
- The FlockViz boids are beloved — keep them
- Battery/CPU impact of always-on AR camera

**Recommendation for discussion:** Option D (toggle) with AR as the promoted
option. The 👁 button in the split panel switches MiniMap ↔ AR camera feed.
First-time users get MiniMap, but a prominent "Switch to AR" button encourages
trying it.

**Files to change (if going with D):**
- `rabble/lib/screens/rabble_chat.dart` — add AR/Map toggle to split panel
- `rabble/lib/widgets/split_panel.dart` — no changes needed (just swap child)

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

## Updated Priority Order for Sprint

### Must (before user testing round 2)
1. **F1** — Creature card simplification (remove legacy Live/Log tabs, promote map, single scroll)
2. **F4** — Add befriend button + send friendship flow
3. **F2** — Force-refresh creature state + deduplicate presences
4. **F3** — Click-through to rabble (tappable row, keep Chat/Peek)
5. **F8** — Rabble card enhancements (sort by activity, host creature, quick actions)

### Should (same sprint if time)
6. **F5** — Remove polling from explore map
7. **F7** — Merge Log tab into expandable Journal section (part of F1)

### Discuss (design decision needed)
8. **F6** — AR viewer as spatial view in rabble chat (needs owner decision on option A/B/C/D)

---

## Updated File Map

| File | Fixes |
|------|-------|
| `rabble/lib/screens/creature/creature_screen.dart` | F1, F2, F3, F4, F7 |
| `rabble/lib/screens/creature/creature_live.dart` | F1 (absorbed into main screen) |
| `rabble/lib/screens/creature/creature_history.dart` | F7 (becomes expandable journal content) |
| `rabble/lib/screens/collection_screen.dart` | F2 |
| `rabble/lib/screens/rabbles_screen.dart` | F8 |
| `rabble/lib/screens/rabble_chat.dart` | F6, F8 (extract invite + creature picker) |
| `rabble/lib/screens/explore_screen.dart` | F5 |
| `rabble/lib/widgets/flock_viz.dart` | F2 |
| `rabble/lib/widgets/send_friendship_sheet.dart` | F4 (new file) |
| `rabble/lib/widgets/invite_sheet.dart` | F8 (extracted from rabble_chat.dart) |
| `rabble/lib/widgets/creature_picker_sheet.dart` | F8 (extracted from rabble_chat.dart) |