# Rabble Design Notes

> Living design document for UX decisions, state constraints, and future work.
> Last updated: 2026-02-18 — Social layer implementation complete (backend + Flutter).

---

## Creature ↔ Rabble Relationship Model

### Core constraint

A **creature** can only exist in **one rabble at a time** and has **one location at a time**.

```
User (owner)
 └── has many Creatures
       └── each creature is in 0..1 Rabble (current)
       └── each creature has 0..1 Location (current)

Rabble (swarm_event)
 └── has many Creatures (from different owners)
```

- A **user** participates in multiple rabbles simultaneously — via different creatures.
- A **creature** is 1:1 with its current rabble. Moving it to a new rabble removes it from the old one.
- Historical membership is preserved in `creature_flights` / `creature_versions`, but the live state is singular.

### Implication for UI

When the user "joins" a rabble with a creature that's already in another rabble, this is a **relocation**, not a dual-membership. The UI must:

1. Show where the creature currently is before the move.
2. Use "Move here" language, not "Join" or "Add".
3. Confirm the relocation with a dialog naming the source rabble.
4. Make it clear there's no charge for adding creatures to your own rabble ("Free — your rabble").

---

## Chat Multiplexing

### Current state

The user can have **multiple rabbles open** (one per creature), but the chat screen is single-instance — navigating to a rabble replaces the current view.

### Target UX

- After moving a creature to a rabble, the creature tray should **auto-select** the newly arrived creature so you can immediately chat as that creature.
- The user should be able to **switch between rabble chats** without losing scroll position or draft messages. This could be:
  - A tab bar at the top of the chat (if few rabbles), or
  - A rabble switcher drawer / bottom sheet.
- Each rabble chat maintains its own SSE stream / poll independently.

### Post-move flow

When a creature is moved to a new rabble:

1. `joinSwarm` API call completes.
2. Reload the creature list for the target rabble (`_loadMyCreatures`).
3. Auto-set `_activeCreatureId` to the moved creature.
4. The creature should appear in the tray immediately, highlighted.
5. The chat panel should be ready — the creature can talk right away.

If the user was viewing the **source** rabble's chat at the time, they should see a system message like "[Creature] has left the rabble" and the creature should disappear from that tray.

---

## Nearby Search — Radius Tuning

The radius slider on the Nearby tab is tuned for **social density**:

- **Range**: 50m → 2km
- **Default**: 500m
- **Steps**: ~50m increments (39 divisions)

The reasoning: rabbles are place-based social events. A 50km radius defeats the purpose — you want to discover things you could walk to, not things across the city. If we find users need wider search for discovery (e.g., planning), we can add a separate "Explore map" view with wider range, but the dashboard Nearby tab should stay tight.

---

## Social Layer — IMPLEMENTED (Migration 090)

### Creature-mediated relationships ✅

The social graph is **creature-first**:

- **Befriend a creature**: ✅ `POST /api/creature-friendships` — symmetric, canonical ordering (`creature_a < creature_b`), tracks where they met (`met_in_rabble`).
- **Follow an owner**: Existing contacts system (`POST /api/contacts`) — asymmetric user-to-user follow. Separate from creature friendships.
- **Privacy model**: ✅ `social_visibility` column on users — `public` | `creature-only` | `private`. All social queries respect visibility: creature-only hides owner name, private hides from search. Update via `PUT /api/users/social-visibility`.

### Two-tier invite model ✅

| Tier | Endpoint | Who | Layer |
|------|----------|-----|-------|
| **Social invite** | `POST /api/rabble/:id/invite` | User → User | Layer 1 (config) |
| **Creature invite** | `POST /api/creature-invites` | Creature → Creature | Layer 2 (action) |

Creature invites ("come fly with me") require the from_creature to be actively in a rabble. Invites expire after 24 hours. Accepting an invite also grants rabble visibility via `object_shares`.

### Discovery surfaces ✅

- **In-rabble members list**: `GET /api/rabble/:id/members` — see all creatures.
- **Post-rabble recap**: ✅ `GET /api/rabble/:id/recap/:creature_id` — "You met..." screen with befriend actions, overlap duration, friendship status.
- **QR scan**: Already exists (`/api/rabble/join/:qr_token`).
- **Share links**: `rabble.world/join/[token]` for non-users.

### Activity feed ✅

- **Paginated**: `GET /api/feed/events?before=...&limit=50` — with relationship context annotations (`is_own_creature`, `is_contact`, `is_friend_creature`).
- **SSE stream**: `GET /api/feed/stream?since=...` — push new events as they arrive, backfill on reconnect.
- **Event types**: `creature_minted`, `creature_flew`, `rabble_created`, `rabble_joined`, `friendship_requested`, `friendship_accepted`, `creature_invited`, `creature_invite_accepted`, + more.
- **Context annotations**: Each event tagged with relationship to the viewing user for visual priority (own > friend > contact > other).

### Co-presence tracking ✅

`rabble_co_presence` table records which creatures were present together in a rabble, with join/leave timestamps and overlap duration. Drives the recap screen and friend suggestions.

### API endpoints (15 new handlers in `social.rs`)

| Method | Route | Purpose |
|--------|-------|---------|
| `POST` | `/api/creature-friendships` | Send friendship request |
| `GET` | `/api/creature-friendships/pending` | List pending requests for my creatures |
| `POST` | `/api/creature-friendships/:id/accept` | Accept friendship |
| `POST` | `/api/creature-friendships/:id/decline` | Decline friendship |
| `DELETE` | `/api/creature-friendships/:id` | Unfriend |
| `GET` | `/api/creatures/:id/friends` | List creature's friends |
| `POST` | `/api/creature-invites` | "Come fly with me" |
| `GET` | `/api/creature-invites/pending` | List pending invites |
| `POST` | `/api/creature-invites/:id/accept` | Accept invite |
| `POST` | `/api/creature-invites/:id/decline` | Decline invite |
| `GET` | `/api/rabble/:id/recap/:creature_id` | Post-rabble recap |
| `POST` | `/api/rabble/:id/co-presence` | Record co-presence |
| `PUT` | `/api/users/social-visibility` | Update visibility |
| `GET` | `/api/feed/events` | Paginated activity feed |
| `GET` | `/api/feed/stream` | SSE activity feed |

### Data model (Migration 090)

```sql
-- Creature-to-creature friendships (symmetric, canonical order)
creature_friendships (id, creature_a, creature_b, initiated_by, status, met_in_rabble, met_at, ...)
-- status: pending | accepted | declined | blocked
-- CHECK (creature_a < creature_b) enforces canonical ordering

-- Creature invites ("come fly with me")
creature_invites (id, from_creature_id, to_creature_id, rabble_id, status, message, expires_at, ...)
-- status: pending | accepted | declined | expired
-- 24-hour expiry, unique pending per creature-pair-rabble

-- Activity events (SSE feed)
activity_events (id, actor_user_id, actor_creature_id, event_type, rabble_id, target_creature_id, title, body, metadata, created_at)

-- Co-presence tracking
rabble_co_presence (id, rabble_id, creature_id, owner_id, joined_at, left_at, overlap_seconds)

-- User social visibility
users.social_visibility TEXT DEFAULT 'public' -- public | creature-only | private
```

### Flutter widgets (implemented)

| Widget | File | Purpose |
|--------|------|---------|
| `RabbleRecapScreen` | `screens/rabble_recap.dart` | Post-rabble "You met" with befriend buttons |
| `CreatureInviteSheet` | `widgets/creature_invite_sheet.dart` | "Come fly with me" bottom sheet |
| `FriendshipRequestCard` | `widgets/friendship_request_card.dart` | Accept/decline pending requests |
| `ActivityFeedWidget` | `widgets/activity_feed.dart` | SSE-powered feed with context badges |

### External invitation (future)

The invite flow needs to be frictionless:

1. Generate a share link (rabble or creature profile).
2. Non-users land on a web preview → sign up → land directly in the rabble.
3. Existing users tap the link → open app → join immediately.

---

## Explore / Activity Feed — IMPLEMENTED

### Problems (resolved)

- ~~Feed polls every 30s but replaces the whole list, losing scroll position.~~ → SSE stream prepends new events.
- ~~Events are context-free.~~ → Each event annotated with `is_own_creature`, `is_contact`, `is_friend_creature`.
- ~~Data goes stale between polls.~~ → SSE push with backfill on reconnect.

### Implementation

- **SSE stream** (`GET /api/feed/stream?since=...`): ✅ Push new events, prepend at top without disrupting scroll. Backfill missed events on reconnect.
- **Context annotations**: ✅ `get_activity_feed()` SQL function joins against `contacts`, `creature_friendships`, `creature_state` to tag each event.
- **Visual priority**: ✅ `ActivityFeedWidget` highlights events by relationship: amber (own), mint (friend), sky (contact), muted (other).
- **Paginated fallback**: `GET /api/feed/events?before=...&limit=50` for initial load and infinite scroll.

### Remaining work

- [ ] Wire `ActivityFeedWidget` into `explore_screen.dart` (replace old polling feed)
- [ ] Wire `RabbleRecapScreen` navigation from rabble completion / leave flow
- [ ] Wire `CreatureInviteSheet` into `rabble_chat.dart` action bar
- [ ] Wire `FriendshipRequestCard` into notifications screen
- [ ] Call `recordCoPresence()` from `join_swarm_handler` (backend hook)
- [ ] Call `update_co_presence_departure()` from leave handler (backend hook)
- [ ] Upgrade SSE from poll-based (5s) to broadcast channel when volume justifies it

---

## Changelog

| Date | Author | Notes |
|------|--------|-------|
| 2026-02-17 | Session | Initial design notes from dashboard redesign discussion |
| 2026-02-18 | Session | Social layer fully implemented: migration 090, 15 API handlers, 5 Flutter widgets. Friendships, invites, recap, activity feed SSE all working. Wiring into existing screens is next. |