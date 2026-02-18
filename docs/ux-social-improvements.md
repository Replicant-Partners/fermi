# Social Features — UX Improvement Plan

> Generated from `things-to-do.md` audit + codebase review.
> Each item is mapped to its backend status, bugs found/fixed, and remaining frontend work.

---

## 🐛 Critical Bug Fixed: Notifications Were Silently Failing

**Root cause:** The `notifications` table (migration 021) has columns `type` and `message`,
but every handler added after the social layer (migration 090) was inserting into
`notification_type` and `body` — columns that **don't exist**.

Because these inserts were wrapped in `let _ = ...` (fire-and-forget), the errors were
swallowed. This means **no notifications were ever created** for:

- Friendship requests (`friendship_request`)
- Friendship accepts (`friendship_accepted`)
- Creature invites / "come fly with me" (`creature_invite`)
- Creature gifts / transfers (`creature_gift`)
- Rabble invites (`rabble_invite`)
- Credit transfers (`credit_transfer`)

**Files fixed:**
- `src/handlers/social.rs` — 3 INSERT statements
- `src/handlers/creatures/identity.rs` — 1 INSERT statement
- `src/handlers/rabble_chat.rs` — 1 INSERT statement
- `src/handlers/wallet.rs` — 1 INSERT statement

All changed from `notification_type` → `type`, `body` → `message`.

**Impact:** This is very likely the reason you "can't see how to friend creatures" — the
notification that tells the other user about the friendship request was never created. The
API calls work, but the other user was never informed.

---

## 🐛 Probable Cause of "userid 500 errors on trying to do stuff with other users"

Several candidates identified:

1. **Notification column mismatch (fixed above)** — while these are `let _ =`, any
   middleware or logging that intercepts the SQL error could surface it as a 500 in logs.

2. **Missing `social_visibility` column** — Migration 090 does
   `ALTER TABLE users ADD COLUMN IF NOT EXISTS social_visibility TEXT`. If this migration
   hasn't been applied to the production database, any query that references
   `users.social_visibility` (friendship listings, activity feed, recap) will 500.

   **Action:** Verify migration 090 has been applied:
   ```sql
   SELECT column_name FROM information_schema.columns
   WHERE table_name = 'users' AND column_name = 'social_visibility';
   ```

3. **Missing SQL functions** — The social handlers call PostgreSQL functions like
   `get_pending_friendship_requests()`, `get_creature_friends()`,
   `get_pending_creature_invites()`, and `get_activity_feed()`. These are all defined in
   migration 090. If the migration failed partway through, some functions may not exist.

   **Action:** Verify functions exist:
   ```sql
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

4. **PostGIS dependency** — The dashboard spatial queries (migration 089) use
   `ST_Distance`, `ST_MakePoint`, and `ST_DWithin`. If PostGIS isn't installed, these
   functions will 500. The social layer itself doesn't use PostGIS, but the dashboard
   creature/rabble views do.

---

## Things-to-do Item Mapping

### 1. "Rabble screen has to show latest activity at the top"

| Aspect | Status |
|--------|--------|
| Backend | ✅ New `GET /api/my/rabbles` endpoint added — orders by `last_activity_at DESC` (uses `GREATEST(created_at, latest activity_event)`) |
| Backend | ✅ Existing `GET /api/feed/events` already returns events in `created_at DESC` order |
| Frontend | ❌ Flutter rabble list screen needs to call `/api/my/rabbles` instead of `/api/swarms` and display the `last_activity_at` timestamp |

**New endpoint:** `GET /api/my/rabbles`
Returns `{ hosting: [...], participating: [...] }` with `last_activity_at` on each entry.

---

### 2. "Needs to be a distinction of rabbles I host vs rabbles I'm a member of"

| Aspect | Status |
|--------|--------|
| Backend | ✅ New `GET /api/my/rabbles` splits response into `hosting` and `participating` arrays |
| Backend | Each entry has a `role` field: `"host"` or `"participant"` |
| Frontend | ❌ Needs two-tab or two-section UI: "My Rabbles" / "Participating In" |

**Response shape:**
```json
{
  "hosting": [{ "swarm_id": "...", "role": "host", "my_creatures": [...], ... }],
  "hosting_count": 3,
  "participating": [{ "swarm_id": "...", "role": "participant", "host_display_name": "...", ... }],
  "participating_count": 5,
  "total": 8
}
```

---

### 3. "Need to understand which creatures I have in which rabbles"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `GET /api/my/rabbles` includes `my_creatures` array on every rabble entry |
| Backend | ✅ `GET /api/dashboard/creatures` already returns `rabble_id`, `rabble_name`, `state`, `in_rabble_area` |
| Frontend | ❌ Display creature avatars on each rabble card; creature detail should link to its rabble |

Each `my_creatures` entry:
```json
{
  "creature_id": "uuid",
  "specimen_name": "Luna",
  "species_group": "butterfly",
  "asset_path": "/api/creatures/uuid/image",
  "data_source": "device",
  "creature_state": "flying"
}
```

---

### 4. "Should be able to add a specialist creature to a rabble I host"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `POST /api/swarms/:id/join` already works — host can join their own creatures |
| Backend | The handler already detects `creator_id == user_id` and skips payment logic |
| Frontend | ❌ Needs a "Add creature" button on the host's rabble detail view that shows a picker of owned creatures not already in the rabble |

**Flow:**
1. Host views their rabble detail
2. Taps "Add Creature +"
3. `GET /api/creatures?owner_id=<me>&status=active` filtered to exclude creatures already in this rabble
4. Selects creature → `POST /api/swarms/:rabble_id/join` with `{ creature_id: "..." }`

---

### 5. "The peek function from nearby is great — but let's go to the AR viewer for that rabble!"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `GET /api/dashboard/nearby?lat=X&lng=Y` returns nearby rabbles with `distance_meters` and `user_in_area` |
| Frontend | ❌ "Peek" button on nearby rabble cards should deep-link to the AR viewer with the rabble context pre-loaded |

**Suggested flow:**
- Nearby rabble card → "Peek 👁️" button → AR viewer opens at rabble coordinates
- AR viewer URL/route: `/ar/:rabble_id` or equivalent deep link

---

### 6. "When I can join the rabble I need to join from a list of my creatures — this is broken"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `GET /api/creatures?owner_id=<user_id>&status=active` works — the `owner_id` filter is properly implemented in `list_creatures_handler` |
| Backend | ✅ `POST /api/swarms/:id/join` expects `{ creature_id, contribution? }` |
| Bug likely | The Flutter client may not be passing `owner_id` to the creature list API, or the auth token isn't providing the correct `user_id` |
| Frontend | ❌ Debug: log the user_id from auth state and verify it matches what's sent to `/api/creatures?owner_id=X` |

**Debug steps:**
1. In the Flutter join flow, log `authState.userId` before calling the creatures API
2. Verify the API call includes `?owner_id=<that_user_id>`
3. Check the response — if it returns creatures from other users, the filter isn't being applied
4. If it returns empty, the user_id format may be wrong (check for URL encoding issues)

---

### 7. "The AR viewer in explore is buried in the map — it needs to be the magic portal viewer"

| Aspect | Status |
|--------|--------|
| Backend | N/A (purely frontend UX) |
| Frontend | ❌ AR viewer should be a first-class entry point, triggered by: |

**Trigger points (priority order):**
1. **QR code / sticker scan** → immediate AR viewer with rabble context
2. **Proximity notification** → "You're near [Rabble Name]! Peek inside?" → AR viewer
3. **Tethered creature crosses rabble boundary** → push notification → AR viewer
4. **Rabble card anywhere in the app** → "Peek" button → AR viewer
5. **Explore tab** → prominent AR button (not buried in map)

The `qr_token` field on swarms already supports QR-based entry:
`GET /api/swarms/join-by-qr/:token` → returns the rabble context for AR.

---

### 8. "No way to see how my tethered bug is moving through space"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `GET /api/creatures/:id/track?since=ISO8601&limit=N` returns telemetry points |
| Backend | ✅ `POST /api/creatures/:id/push-telemetry` records device location points |
| Backend | ✅ Active flight info now included in creature detail card (`active_flight.data_source`, `active_flight.location_name`) |
| Frontend | ❌ Needs a real-time track visualization: |

**Suggested UI:**
- Creature detail card for tethered creatures shows a mini-map with breadcrumb trail
- "View Track" button opens full-screen map with:
  - Creature's path (polyline from telemetry points)
  - Nearby rabble areas (circles on map from `/api/dashboard/nearby`)
  - Current position (pulsing dot)
  - Distance to nearest rabble

---

### 9. "Broken data relationship — if I'm hosting a rabble I can take it to another rabble"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `PATCH /api/swarms/:id` supports updating rabble properties (creator-only) |
| Backend | ❌ No location-move endpoint yet — need `PATCH /api/swarms/:id` to accept `center_lat`, `center_lng`, `location_name` with confirmation |
| Frontend | ❌ Needs a confirmation dialog: "This will move your rabble to [New Location]. Are you sure?" |

**Needed backend addition:**
Add `center_lat`, `center_lng`, `location_name`, `h3_cell` to `UpdateSwarmRequest` in
`src/handlers/creatures/swarms.rs`. Include a `moved_from` field in the response so the
client can offer an "undo" / "go back" option.

**Suggested flow:**
1. Host opens rabble settings → "Move Rabble"
2. Map picker or current-location button
3. Confirmation: "⚠️ Moving [Rabble Name] from [Old Location] to [New Location]. Participants will see the new location. Move?"
4. Success: "Rabble moved! [Undo within 5 minutes]"

---

### 10. "Creature detail card needs to show current state"

| Aspect | Status |
|--------|--------|
| Backend | ✅ `GET /api/creatures/:id` now returns enriched social context: |
| | `social.friend_count` — number of accepted creature friendships |
| | `social.pending_friend_requests` — inbound pending requests |
| | `social.rabble_role` — `"host"` / `"anchor"` / `"participant"` / `null` |
| | `social.is_tethered` — boolean (active flight from device) |
| | `social.is_anchor` — boolean (is the anchor creature of its rabble) |
| | `active_flight.flight_id` — current flight UUID |
| | `active_flight.data_source` — `"device"` (tethered) or `"synthetic"` |
| | `active_flight.started_at` — when flight began |
| | `active_flight.location_name` — last known location |
| | `owner_display_name` — respects social_visibility setting |
| Backend | ✅ Existing fields: `creature_state`, `rabble_id`, `rabble_name`, `presence`, `visibility` |
| Frontend | ❌ Creature detail card UI needs to display: |

**Suggested card layout:**

```
┌────────────────────────────────────────┐
│  [Creature Image]                      │
│  Luna the Painted Lady                 │
│  Vanessa cardui · butterfly            │
│                                        │
│  ⚡ Status: Flying (tethered)          │
│  📍 Location: Camden Market, London    │
│  🏠 Rabble: "Bug Collectors" (host)    │  ← tappable → rabble peek/detail
│  👫 3 friends · 1 pending request      │  ← tappable → friends list
│                                        │
│  [Peek Rabble 👁️] [Chat 💬] [Friends] │
│                                        │
│  Context warnings:                     │
│  ⚠️ "You're hosting this rabble"       │
│  or "Join chat" / "Join rabble"        │
└────────────────────────────────────────┘
```

**Tappable elements:**
- Rabble name → peek AR view → join/chat with context-appropriate CTA
- Friends count → creature friends list
- Pending requests badge → accept/decline UI

---

### 11. "userid 500 errors on trying to do stuff with other users"

| Aspect | Status |
|--------|--------|
| Backend | ✅ Notification column mismatch fixed (see top of document) |
| Backend | ⚠️ Migration 090 must be verified as applied |
| Backend | ⚠️ PostGIS availability must be verified for dashboard queries |
| Frontend | ❌ Add error boundary / toast that surfaces the actual error message from the API |

**Immediate debug action:**
```bash
# Check server logs for the actual 500 error:
# Look for lines containing "INTERNAL_SERVER_ERROR" or SQL errors
grep -i "error\|500\|column.*does not exist\|function.*does not exist" /var/log/fermi/*.log
```

---

## New API Endpoints Summary

| Method | Path | Purpose | Auth |
|--------|------|---------|------|
| GET | `/api/my/rabbles` | My rabbles split by host/participant with creature placement | ✅ |
| GET | `/api/creatures/:id` | Now includes `social` and `active_flight` context blocks | Public |
| GET | `/api/creatures/:id/friends` | List creature's accepted friends | Public |
| GET | `/api/creature-friendships/pending` | Pending inbound friendship requests for my creatures | ✅ |
| POST | `/api/creature-friendships` | Send friendship request (creature-to-creature) | ✅ |
| POST | `/api/creature-friendships/:id/accept` | Accept a pending friendship | ✅ |
| POST | `/api/creature-invites` | "Come fly with me" invite | ✅ |
| GET | `/api/creature-invites/pending` | Pending inbound invites for my creatures | ✅ |
| GET | `/api/rabble/:id/recap/:creature_id` | Post-rabble recap: who you met, friend suggestions | ✅ |
| GET | `/api/feed/events` | Activity feed (paginated, relationship-annotated) | ✅ |
| GET | `/api/feed/stream` | Activity feed SSE stream (real-time) | ✅ |

---

## Social UX Flow: How to Friend a Creature

The full flow, now that notifications are fixed:

1. **Discovery** — User sees another creature in:
   - A rabble they're participating in (rabble member list)
   - The rabble recap screen after a rabble ends
   - The nearby/explore view
   - Another user's profile

2. **Initiate** — Tap the creature → creature detail card → "Befriend" button
   ```
   POST /api/creature-friendships
   { "from_creature_id": "<my creature>", "to_creature_id": "<their creature>", "met_in_rabble": "<optional>" }
   ```

3. **Notify** — The other user now receives a notification (✅ fixed):
   ```
   "[Luna] wants to be friends!"
   "[Luna] met your creature [Atlas] and wants to befriend it"
   ```

4. **Accept** — Other user sees notification → taps → pending requests view → accept/decline
   ```
   POST /api/creature-friendships/:id/accept
   ```

5. **Confirmation** — Both users see activity feed event: "Luna and Atlas are now friends!"

---

## Social UX Flow: How to Follow a User (Contact)

This is the Layer 1 (user-to-user) flow:

1. **Discovery** — Find user via:
   - `GET /api/users/search?q=<name>` — search by display name
   - `GET /api/users/:id/profile` — view their public profile
   - Creature detail card → owner name (if visibility is `public`)

2. **Follow** — Add as contact:
   ```
   POST /api/contacts
   { "contact_id": "<their_user_id>", "nickname": "optional" }
   ```

3. **Effect** — Their activity appears in your feed; they gain access through the
   "invited door" when joining your rabbles (cheaper/free entry).

---

## Priority Order for Frontend Work

1. **🔴 P0 — Fix creature list in join flow** (item 6) — likely just a missing `owner_id` param
2. **🔴 P0 — Verify migration 090 is applied** (item 11) — run the SQL checks above
3. **🟡 P1 — Creature detail card with social context** (item 10) — use new `social` block
4. **🟡 P1 — My Rabbles split view** (items 1, 2, 3) — call `GET /api/my/rabbles`
5. **🟡 P1 — Surface AR viewer / peek** (items 5, 7) — deep-link from rabble cards
6. **🟢 P2 — Tether track visualization** (item 8) — use `/api/creatures/:id/track`
7. **🟢 P2 — Rabble move warnings** (item 9) — needs small backend addition + frontend dialog
8. **🟢 P2 — Add specialist creature to hosted rabble** (item 4) — creature picker + join API