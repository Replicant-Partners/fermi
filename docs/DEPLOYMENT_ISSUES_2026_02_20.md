# Deployment Issues — 2026-02-20

> **Context:** Deployed Phases 1-4 (four-pillar UX, SSE, creature hub, rabble follows, journals).
> This document captures every issue found during post-deploy audit.
> Priority: 🔴 breaks functionality, 🟡 degraded experience, 🟢 cosmetic/minor.

---

## 🔴 Issue 1: `my/rabbles` response missing `creator_id` field

**Where:** Backend `src/handlers/creatures/swarms.rs` → hosting entries  
**Impact:** Rabbles tab → Hosting list crashes on parse  

The hosting entries from `GET /api/my/rabbles` don't include `creator_id`.
The Flutter `_RabbleWithCreatures.fromJson()` calls `SwarmEvent.fromJson()` which
requires `creator_id` as a non-nullable field. Parse fails → hosting tab is empty.

The participating entries have `host_id` (not `creator_id`) — same problem.

**Fix (backend):** Add `creator_id` to both hosting and participating entries:

```rust
// In hosting entry (line ~155):
"creator_id": &user_id,  // The current user IS the creator

// In participating entry (line ~193):
"creator_id": row.get::<String, _>("creator_id"),
```

Also add missing fields that `SwarmEvent.fromJson` expects:
- `h3_cell` (hosting entries have it in the SQL but not in the JSON)
- `funding_mode` (participating entries missing it)

**Fix (Flutter alternative):** Make `_RabbleWithCreatures.fromJson()` not go through `SwarmEvent.fromJson()` — parse directly with only the fields we need. This is more resilient.

---

## 🔴 Issue 2: `my/rabbles` participating entries use `host_id` not `creator_id`

**Where:** Backend `src/handlers/creatures/swarms.rs` line ~203  
**Impact:** Participating rabble cards can't determine host vs participant role  

The participating entries use `host_id` and `host_display_name` instead of the
standard `creator_id` / `creator_display_name` that `SwarmEvent.fromJson` expects.

**Fix:** Either:
1. Rename `host_id` → `creator_id` in the backend response, OR
2. Handle both field names in `SwarmEvent.fromJson`:
   ```dart
   creatorId: json["creator_id"] as String? ?? json["host_id"] as String? ?? "",
   ```

---

## 🟡 Issue 3: Mixed API URL patterns (relative vs baseUrl)

**Where:** Flutter `lib/services/api_client.dart`  
**Impact:** Works in PWA (same origin) but breaks if API is on a different domain  

75 API calls use relative paths (`Uri.parse('/api/...')`), 37 use `$baseUrl`.
All new methods use `$baseUrl` (correct). Old methods don't.

When the Flutter web app is served from `rabble.world` and the API is at
`agent-bestiary.world`, the relative paths will fail.

**Fix:** Sed-replace all `Uri.parse('/api/` to `Uri.parse('$baseUrl/api/` in api_client.dart.
Low risk — `baseUrl` is always set.

---

## 🟡 Issue 4: `my_creatures` array in hosting entries — field name mismatch

**Where:** Backend `swarms.rs` builds `my_creatures` array with specific field names  
**Impact:** Creature avatars may not render in rabble cards  

The backend builds `my_creatures` entries with:
```json
{ "creature_id": "...", "specimen_name": "...", "species_group": "...", "asset_path": "...", "data_source": "...", "creature_state": "..." }
```

The Flutter `_MyCreatureInRabble.fromJson` expects the same field names — **this matches**.
But need to verify `asset_path` produces a valid `imageUrl` (may need `https://agent-bestiary.world` prefix).

**Status:** Likely works but needs visual verification.

---

## 🟡 Issue 5: SSE creature stream CORS headers

**Where:** Backend `src/handlers/streams.rs` → `creature_stream_handler`  
**Impact:** SSE connections from `rabble.world` to `agent-bestiary.world` may be blocked  

SSE streams require CORS headers for cross-origin connections. The existing
workspace SSE and activity feed SSE work because they're same-origin. The creature
stream may fail if the Flutter PWA is served from a different origin.

**Fix:** Verify the global CORS layer in `api_server.rs` covers SSE `text/event-stream` responses. Check that `Access-Control-Allow-Origin` includes the rabble.world domain.

---

## 🟡 Issue 6: Friendship flow — can't send friend request from creature detail hub

**Where:** Flutter `lib/screens/creature/creature_screen.dart` → Social section  
**Impact:** User said "hopefully I can finally friend somebody" — no send-request button  

The Social section shows friend count and pending requests but has no button to
**send** a friendship request. The backend endpoint exists (`POST /api/creature-friendships`)
but the Flutter UI doesn't expose it from the creature detail hub.

**Fix:** Add a "Befriend" button in the social section when viewing someone else's creature.
Needs: creature picker (which of MY creatures sends the request?) + confirmation.

The existing `friendship_request_card.dart` widget handles the incoming side.
Need a new `send_friendship_request_sheet.dart` or add it inline.

---

## 🟡 Issue 7: `getActivityFeed` returns list directly, `getFeed` returns map

**Where:** Flutter `api_client.dart` — two different feed methods  
**Impact:** JournalsScreen uses the wrong one or the shapes mismatch  

- `getActivityFeed()` returns `List<Map<String, dynamic>>` (calls `/api/feed/events`)
- `getFeed()` returns `Map<String, dynamic>` with `events` key (calls `/api/feed/events`)
- `JournalsScreen._loadActivityFeed()` calls `getActivityFeed()` — returns a list ✅

But `ExploreScreen._load()` calls `getFeed()` which returns `{ events: [...], has_more: bool }`.

**Status:** Both work correctly for their callers. No fix needed — just noting the inconsistency for future cleanup.

---

## 🟡 Issue 8: Journals flights tab — `flight.locationName` / `flight.durationSeconds` access pattern

**Where:** Flutter `lib/screens/journals_screen.dart` → `_FlightTile`  
**Impact:** May crash if Flight model doesn't expose these as direct properties  

The `_FlightTile` accesses `flight.locationName`, `flight.durationSeconds`,
`flight.endedAt`, `flight.startedAt` directly. Need to verify the `Flight` model
exposes these fields.

**Fix:** Check `lib/models/flight.dart` field names match what `_FlightTile` uses.

---

## 🟢 Issue 9: Unused import warnings in new screens

**Where:** Flutter `rabbles_screen.dart`, `journals_screen.dart`  
**Impact:** Clean code only, no functionality impact  

- `rabbles_screen.dart`: unused import `creature/creature_screen.dart`
- `journals_screen.dart`: may have unused `http` import after SSE refactor

**Fix:** Remove unused imports.

---

## 🟢 Issue 10: Explore map — `onLongPress` save location may not work on mobile web

**Where:** Flutter `lib/screens/explore_screen.dart` → `_buildMapView`  
**Impact:** Long-press gesture may conflict with flutter_map pan on mobile browsers  

The `onLongPress` handler on `MapOptions` to trigger the save location dialog
may not fire reliably on mobile web because long-press is also used for map panning.

**Fix:** Consider adding a dedicated "Drop Pin" button in the map controls instead of
relying solely on long-press.

---

## 🟢 Issue 11: `explore_screen_patched.dart` — stale broken file

**Where:** `lib/screens/explore_screen_patched.dart`  
**Impact:** 5 compile errors in `flutter analyze` (doesn't affect build/runtime)  

This is a stale partial file that was never completed. It has missing methods,
undefined types, and a syntax error. It's not imported anywhere.

**Fix:** Delete the file.

---

## Fix Priority Order

### Immediate (before user testing)
1. **Issue 1+2** — Fix `my/rabbles` response to include `creator_id` + all required `SwarmEvent` fields
2. **Issue 6** — Add befriend button to creature detail social section

### Soon (before wider rollout)
3. **Issue 3** — Standardize API URL patterns to use `baseUrl`
4. **Issue 5** — Verify SSE CORS headers
5. **Issue 8** — Verify Flight model field names

### Cleanup (non-blocking)
6. **Issue 4** — Verify creature avatar URLs in rabble cards
7. **Issue 9** — Remove unused imports
8. **Issue 10** — Add "Drop Pin" button alternative
9. **Issue 11** — Delete `explore_screen_patched.dart`

---

## How to verify fixes

```bash
# Backend — run after fixing issues 1+2:
cd /home/ilabra/fermi
cargo build 2>&1 | grep "^error"  # should be empty

# Flutter — run after fixing issues 3+6+8+9:
cd /home/ilabra/rabble
/home/ilabra/flutter/bin/flutter analyze 2>&1 | grep "error" | grep -v "explore_screen_patched"
# should be empty

# Rebuild and deploy:
/home/ilabra/flutter/bin/flutter build web --release
rm -rf /home/ilabra/fermi/rabble-web/*
cp -r build/web/* /home/ilabra/fermi/rabble-web/
cd /home/ilabra/fermi
git add -A && git commit -m "fix: deployment issues from 2026-02-20 audit"
git push origin main
```
