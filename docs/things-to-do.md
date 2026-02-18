# Things To Do — Social & Rabble UX

> See [ux-social-improvements.md](./ux-social-improvements.md) for the full implementation plan, API mapping, and flow diagrams.

---

## 1. Rabble screen has to show latest activity at the top
**Status: ✅ Backend done** — New `GET /api/my/rabbles` endpoint orders by `last_activity_at DESC`.
Frontend needs to call this endpoint and display the timestamp.

## 2. Needs to be a distinction of rabbles I host vs rabbles I'm a member of
**Status: ✅ Backend done** — `GET /api/my/rabbles` returns `{ hosting: [...], participating: [...] }` with a `role` field on each entry (`"host"` or `"participant"`).
Frontend needs a two-tab or two-section layout.

## 3. Need to understand which creatures I have in which rabbles
**Status: ✅ Backend done** — `GET /api/my/rabbles` includes a `my_creatures` array on every rabble entry (creature_id, specimen_name, species_group, asset_path, data_source, creature_state). Also available via `GET /api/dashboard/creatures` with `rabble_id` and `rabble_name` per creature.
Frontend needs to display creature avatars on each rabble card.

## 4. I should be able to add a specialist creature (e.g. a shopping assistant tied to a location and knowledge about the product) to a rabble I host
**Status: ✅ Backend ready** — `POST /api/swarms/:id/join` already works for the host joining their own creatures (skips payment logic when `creator_id == user_id`).
Frontend needs an "Add Creature +" button on the host's rabble detail view with a creature picker filtered to owned creatures not already in the rabble.

## 5. The peek function from nearby is great — but let's go to the AR viewer for that rabble!
**Status: ⚠️ Backend ready, frontend UX needed** — `GET /api/dashboard/nearby?lat=X&lng=Y` returns nearby rabbles with `distance_meters` and `user_in_area`. The AR viewer exists but needs to be deep-linked from the "Peek" button on nearby rabble cards.
Frontend: "Peek 👁️" on nearby card → AR viewer at rabble coordinates.

## 6. When I can join the rabble I need to join from a list of my creatures — this is broken currently, maybe because the user ID isn't there to provide the correct filtered view
**Status: 🔴 Likely frontend bug** — Backend `GET /api/creatures?owner_id=<user_id>&status=active` works correctly. The `owner_id` query param is properly implemented in `list_creatures_handler`. The Flutter client likely isn't passing the `owner_id` parameter or the auth token isn't providing the correct `user_id`.
**Debug:** Log `authState.userId` in the join flow and verify it matches the `?owner_id=` param sent to the API.

## 7. The AR viewer in explore is buried in the map — it's great, it needs to be the magic portal viewer (Harry Potter-like experience whenever you come across a rabble)
**Status: ❌ Frontend UX redesign needed** — AR viewer should be a first-class entry point triggered by:
1. QR code / sticker scan → immediate AR viewer with rabble context
2. Proximity notification → "You're near [Rabble Name]! Peek inside?" → AR viewer
3. Tethered creature crosses rabble boundary → push notification → AR viewer
4. Rabble card anywhere in the app → "Peek" button → AR viewer
5. Explore tab → prominent AR button (not buried in map)

The `qr_token` field on swarms already supports QR-based entry via `GET /api/swarms/join-by-qr/:token`.

## 8. There is no way to see how my tethered bug is moving through space — this is bad. I want to see my waypoint tracks as I am changing positions relative to rabble locations near me
**Status: ✅ Backend ready** — `GET /api/creatures/:id/track?since=ISO8601&limit=N` returns telemetry points. `POST /api/creatures/:id/push-telemetry` records device location. Creature detail card now includes `active_flight.data_source` and `active_flight.location_name`.
Frontend needs a real-time track visualization: mini-map with breadcrumb trail, nearby rabble area circles, current position pulsing dot.

## 9. Broken data relationship — if I'm hosting a rabble I can take my rabble to another rabble, but that means I need a warning that I'm leaving my spot
**Status: ⚠️ Backend needs small addition** — `PATCH /api/swarms/:id` exists for updating rabble properties (creator-only), but doesn't yet support `center_lat`/`center_lng`/`location_name` changes. Need to add location fields to `UpdateSwarmRequest` with a `moved_from` field in the response.
Frontend needs a confirmation dialog: "⚠️ This will move your rabble from [Old] to [New]. Are you sure?" with an undo option.

## 10. The creature detail card needs to show me the current state of my creature
**Status: ✅ Backend done** — `GET /api/creatures/:id` now returns enriched context:
- `social.friend_count` — number of accepted creature friendships
- `social.pending_friend_requests` — inbound pending requests
- `social.rabble_role` — `"host"` / `"anchor"` / `"participant"` / `null`
- `social.is_tethered` — boolean (active flight from device)
- `social.is_anchor` — boolean (is the anchor creature of its rabble)
- `active_flight.flight_id`, `active_flight.data_source`, `active_flight.started_at`, `active_flight.location_name`
- `owner_display_name` — respects `social_visibility` setting
- Existing: `creature_state`, `rabble_id`, `rabble_name`, `presence`, `visibility`

Frontend: every rabble card should be tappable → peek AR view → join with context-appropriate CTA and warnings ("You're hosting", "Join chat", "Join rabble").

## 11. userid 500 errors on trying to do stuff with other users — need to figure this out
**Status: 🐛 Root cause found and partially fixed**
- **Fixed:** Notification column name mismatch — all social/identity/chat/wallet handlers were inserting into `notification_type` and `body` columns, but the actual table (migration 021) has `type` and `message`. This meant **zero notifications were ever created** for friendship requests, creature invites, gifts, or rabble invites. Fixed in `social.rs`, `identity.rs`, `rabble_chat.rs`, `wallet.rs`.
- **To verify:** Migration 090 (social layer) must be applied — check for `social_visibility` column on `users` table and existence of `get_pending_friendship_requests` function.
- **To verify:** PostGIS availability for dashboard spatial queries.

---

## Priority Order

| Priority | Item | What's needed |
|----------|------|---------------|
| 🔴 P0 | #6 — Creature list in join flow | Debug `owner_id` param in Flutter client |
| 🔴 P0 | #11 — 500 errors | Verify migration 090 applied; notification fix deployed |
| 🟡 P1 | #10 — Creature detail card | Frontend: use new `social` + `active_flight` blocks |
| 🟡 P1 | #1, #2, #3 — My Rabbles view | Frontend: call `GET /api/my/rabbles`, two-section layout |
| 🟡 P1 | #5, #7 — AR viewer surfacing | Frontend: deep-link from rabble cards + proximity triggers |
| 🟢 P2 | #8 — Tether track visualization | Frontend: map with polyline from `/api/creatures/:id/track` |
| 🟢 P2 | #9 — Rabble move warnings | Backend: add location fields to update; Frontend: confirm dialog |
| 🟢 P2 | #4 — Add specialist creature | Frontend: creature picker + existing join API |