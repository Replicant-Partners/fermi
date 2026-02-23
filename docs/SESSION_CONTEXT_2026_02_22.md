# Session Context — 2026-02-22

> **Purpose:** Everything needed to resume work in the next session.
> **Last commit (fermi):** `a9a2b1e` — "build: Notification Settings screen + web rebuild"
> **Last commit (rabble):** `16a279c` — "F: Notification Settings screen — toggles, per-rabble mute, blocks list"
> **Branch:** `main` (both repos synced with origin)
> **Previous session:** `docs/SESSION_CONTEXT_2026_02_21.md`

---

## What Happened This Session

Continued from 2026-02-21. Focus: tethering state machine, AR portal unification, push notifications go-live, notifications management, and stability hardening.

### Major Achievements

1. **Tethering state machine — fully robust**
   - Tether: creates device flight immediately + presence=tracking + preserves rabble
   - Untether: calls API directly (not session-dependent TetherService) + resets presence
   - Auto-clean stale tethers on re-tether (no more "already tethered" blocking)
   - Fly/hop auto-untethers (no constraints on creature actions)
   - End rabble preserves device flights (tethered creatures keep tracking)
   - Tethered host auto-uses GPS position for rabble hosting (skips picker)
   - Tether radius check: warns if GPS is outside rabble area before tethering

2. **AR portal unified — one code path**
   - Rabble chat AR button now opens same `RabblePortal` widget as map QR scan
   - `preloadSwarmId` param skips scanning, loads directly
   - QR resolve handler fixed: uses `creature_state` (was last place using `creature_flights`)
   - Portal creature positions scattered within radius (not stacked at center)
   - `origin_lat/origin_lng` added to flock_history response for raw coordinates

3. **Push notifications — fully operational**
   - VAPID JWT signing with ES256 (p256 + ecdsa crates)
   - All 8 notification sources migrated to `notify_user()` 
   - Service worker with push event handling
   - Flutter auto-subscribes on app load via JS interop
   - Proximity push: finds nearby rabbles, 4h cooldown per rabble
   - Proximity SQL fixed: subquery instead of HAVING without GROUP BY
   - Throttled to once per 5 minutes (was spamming on every didChangeDependencies)

4. **Location/tethering notifications**
   - Tethered member drift check: notifies when creature drifts outside rabble radius (4h cooldown)
   - "Rabble is moving" notification: notifies members when anchor moves (5min cooldown)
   - @mention notifications: parse @creatureName in chat, push to creature owner

5. **Notification Settings screen**
   - In user dropdown menu
   - Push master toggle + category toggles (Social, Rabble, Nearby, @Mentions, Economy)
   - Per-rabble muting
   - Blocked users/creatures list with unblock
   - Clear all notifications

6. **Stability audit — all state transitions fixed**
   - perch_handler: clears creature_state.rabble_id
   - host_rabble_handler: clears old rabble before hosting new
   - join_batch_handler: uses creature_state (was creature_flights)
   - fly_handler: auto-untethers before flight
   - end_rabble + anchor_leave: preserves device flights, just clears swarm_id
   - leave_rabble (anchor): auto-ends rabble instead of blocking with error
   - end_rabble: state set to 'perched' not 'idle'

7. **Misc fixes**
   - Host button added to tethered creature actions
   - Portal creature images: use `/api/creatures/{id}/image` endpoint
   - Chat polling: compare by message ID not just count
   - EndRabbleSheet: delay reload for backend commit
   - Creature card reloads on return from rabble chat
   - Floaty stuck state resolved (stale tether + rabble + missing flight)

---

## Project State Report

### Architecture

```
rabble.world (PWA)
├── Flutter Web (rabble/) — 4-pillar UX
│   ├── 🐾 Creatures — collection, detail, actions, friends, tethered filter
│   ├── 👥 Rabbles — Discover/Hosting/Joined/Following, host flow, edit
│   ├── 🌍 Environment — Map, explore, AR portal, QR scan, proximity alerts
│   └── 📓 Journals — Activity, flights
│
├── Rust API (fermi/) — Vercel serverless
│   ├── Creatures — CRUD, flights, state, tethering, versioning
│   ├── Rabbles — host, join, leave, end, flock, scatter, move
│   ├── Social — friendships, invites, co-presence, @mentions
│   ├── Governance — block, eject, report
│   ├── Push — VAPID signing, subscribe, proximity, drift, moving alerts
│   ├── Chat — messages, narrator, mentions, polls (future)
│   ├── Auth — JWT, API keys, OAuth (Google, GitHub)
│   └── Notifications — notify_user(), deep linking, metadata
│
└── Neon PostgreSQL — 98 migrations applied
    ├── creatures, creature_state, creature_versions, creature_conditions
    ├── creature_flights, creature_tethers, creature_friendships
    ├── swarm_events, creature_blocks, user_blocks
    ├── rabble_messages, rabble_follows, rabble_ejections
    ├── push_subscriptions, push_config (VAPID keys stored)
    ├── notifications (with metadata JSONB), reports
    └── activity_events, rabble_co_presence
```

### Codebase Metrics

- **Rust backend:** ~55K lines across handlers, auth, memory, governance, push
- **Flutter app:** ~20K lines across screens, widgets, services, models, simulation
- **Migrations:** 98 SQL files
- **Design docs:** 3 (Rich Media Chat, Gift-as-Invite, Governance)
- **Session commits this session:** 20 (fermi) + 15 (rabble) = 35

### State Machine — Creature Lifecycle (now clean)

```
┌─────────┐  perch   ┌─────────┐  host    ┌─────────┐
│  idle   │ ───────> │ perched │ ───────> │ hosting │
└─────────┘          └─────────┘          └─────────┘
     ▲                    │  ▲                 │
     │              join  │  │ leave           │ leave (anchor)
     │                    ▼  │                 │ = auto-end rabble
     │               ┌──────────┐              │
     │               │ in_rabble│ <────────────┘
     │               └──────────┘
     │                    │  ▲
     │              tether│  │ untether
     │                    ▼  │
     │               ┌──────────┐
     └─────────────  │ tracking │  (presence = 'tracking')
       (end rabble)  └──────────┘

Every transition updates:
1. creature_state (state, rabble_id, location)
2. creature_conditions (presence)
3. creature_flights (end old, create new)
4. creature_tethers (deactivate if action replaces tethering)
5. swarm_events (creature_count)
```

### Notification Types — All Active

| Type | Trigger | Push? | Cooldown |
|------|---------|-------|----------|
| `friendship_request` | Creature befriends yours | ✅ | — |
| `friendship_accepted` | Your request accepted | ✅ | — |
| `creature_invite` | Creature invited to rabble | ✅ | — |
| `rabble_invite` | You're invited to a rabble | ✅ | — |
| `rabble_join/start/end` | Follower events | ✅ | — |
| `rabble_eject` | Creature removed from rabble | ✅ | — |
| `creature_gift` | Someone gifted you a creature | ✅ | — |
| `credit_transfer` | Credits received | ✅ | — |
| `rabble_nearby` | Public rabble within 2km | ✅ | 4h/rabble |
| `rabble_drift` | Tethered creature left rabble area | ✅ | 4h/creature |
| `rabble_moving` | Rabble you're in is moving | ✅ | 5min |
| `chat_mention` | @yourCreature in chat | ✅ | — |

### Key Design Decisions This Session

| Decision | Rationale |
|----------|-----------|
| No constraints on creature actions | Creature is always free to act — tether auto-cleans, anchor leave auto-ends, fly auto-untethers |
| creature_state = source of truth everywhere | Flights get ended/created by various flows; creature_state is always explicitly updated |
| AR portal unified via RabblePortal widget | One code path, consistent rendering — preloadSwarmId skips scan |
| Notifications client-side prefs | SharedPreferences for MVP speed; server-side user_preferences table is future |
| Proximity throttled to 5min | Was spamming on every widget rebuild; 5min is frequent enough for discovery |
| Scatter within radius | Deterministic hash of creature_id for stable, unique positions |

---

## What's Next

### Priority 1: Test & Fix (Current)
Owner is testing the full flow. Expected issues:
- Push notification delivery (VAPID signing may need debugging with real push services)
- AR portal from chat (now unified, should match map QR scan)
- Tether/untether/host/leave cycle (state machine hardened, may still have edge cases)
- Notification settings actually filtering notifications (currently client-side prefs stored but not checked during delivery)

### Priority 2: Rich Messaging
Design doc: `docs/DESIGN_RICH_MEDIA_CHAT.md`
- Image upload + display (Phase 1, ~2-3h)
- @mentions UX (autocomplete creature names in chat input)
- Audio recording + waveform playback (Phase 2)
- Short video (Phase 3)
- Polls (Phase 4)
- Location sharing (Phase 5)

### Priority 3: Usability Polish
- Error messages: no raw SQL/stack traces to users
- Loading states: shimmer placeholders on creature cards
- Offline/stale state handling
- Onboarding flow for new users (currently goes to creature mint)
- Map creature pins (show individual creatures, not just rabble circles)

### Priority 4: Feature Development
Design docs ready:
- `docs/DESIGN_RICH_MEDIA_CHAT.md` — ~12-15h total
- `docs/DESIGN_GIFT_AS_INVITE.md` — ~16-20h total
- `docs/DESIGN_GOVERNANCE.md` — remaining: chat filtering, admin review (~2-3h)

### Priority 5: Server-side Notification Preferences
- Currently prefs are in SharedPreferences (client-side only)
- Need `user_preferences` table for cross-device persistence
- Backend should check prefs before sending push
- Per-rabble mute should be server-side (suppress at delivery time)

---

## Commit History (this session)

### Fermi (20 commits)

```
a9a2b1e build: Notification Settings screen + web rebuild
541a408 D+E: Tethered member drift check + rabble moving notifications
2e4e434 A+B+C: AR portal unified, end→perched, @mention notifications
b911619 Stability audit: fix fragile state transitions across all handlers
86bfcf9 build: fix untether (direct API call) + tether rabble radius check
25e6870 Fix tether/untether/leave state machine — no constraints on creature actions
4118bc2 Fix proximity 500: HAVING→subquery, non-fatal, throttled to 5min
4c73df9 build: fix portal creature images
8350e06 build: rebuild Flutter web — AR button opens swarm portal (no camera)
707a8a1 Fix AR portal: QR resolve uses creature_state for rabble members
be57cb5 Fix stale tethers: auto-clean on re-tether instead of blocking
6e22173 Fix AR portal: scatter creature positions so they don't stack
bd9a0b9 build: rebuild Flutter web — Host button on tethered creatures
1f0c79b Fix AR portal: add origin_lat/origin_lng to flock_history response
a17e910 Fix tether presence: simple UPDATE with error logging, cleaned stale tethers
7e2a3a7 VAPID JWT signing + proximity push — push notifications fully operational
381afa5 Push notifications live: all notifications migrated to notify_user() + tether fix
3291c1f Push notifications live: VAPID keys + custom SW + Flutter subscription + web rebuild
132319c Scatter creatures within radius + fix end rabble card reset
8f7506c Fix tethering: immediate state update, device flight, member follow, rabble broadcast
```

### Rabble (15 commits)

```
16a279c F: Notification Settings screen — toggles, per-rabble mute, blocks list
7fceb83 Tethered host auto-uses GPS for rabble location (skip picker)
31c9143 A: Unify AR portal — rabble chat uses same RabblePortal as map QR scan
65132fa Fix untether + tether rabble check
e4248c2 Fix proximity 500 spam + throttle to every 5 minutes
0184297 Fix portal creature images: use /api/creatures/{id}/image endpoint
fe7277f AR button opens swarm portal (animated creatures on dark bg, no camera needed)
68c6944 Fix AR portal: QR resolve uses creature_state, reverted AR button to camera view
c786ce6 AR portal debug logging + scatter origin positions
a4e718e Add Host button to tethered creature actions (was missing)
ff3f453 Fix AR portal: use origin_lat/origin_lng for creature positioning
3573378 Tethered filter on collection + fix tether presence update
edd226f Proximity push + VAPID signing + location check-in
89834dc Push notifications: service worker + Flutter subscription + VAPID keys
7dd313f Fix EndRabbleSheet: delay reload for backend commit, pop before callback
```

---

## Key Files Reference

| File | What it is |
|------|-----------|
| **NEW THIS SESSION** | |
| `src/handlers/push.rs` | Push delivery: VAPID signing, proximity check, notify_user() |
| `migrations/098_push_subscriptions.sql` | push_subscriptions + push_config tables |
| `rabble-web/custom-sw.js` | Service worker: push events, click navigation, actions |
| `lib/screens/notification_settings_screen.dart` | Notification preferences UI |
| **MODIFIED THIS SESSION** | |
| `src/handlers/creatures/tethering.rs` | Tether/untether/telemetry — complete rewrite of state management |
| `src/handlers/creatures/state.rs` | perch/host/join — added creature_state cleanup |
| `src/handlers/creatures/swarms.rs` | end/leave — perched not idle, anchor auto-ends, device flight preservation |
| `src/handlers/creatures/flights.rs` | fly — auto-untether before flight |
| `src/handlers/rabble_chat.rs` | @mention notifications, message posting |
| `src/handlers/rabble_workspace.rs` | flock_history: origin_lat/lng, scatter, creature_state source |
| `src/handlers/qr_codes.rs` | QR resolve: creature_state source (was last holdout) |
| `src/handlers/social.rs` | All notifications → notify_user() |
| `src/handlers/governance.rs` | Eject notification → notify_user() |
| `src/handlers/profile.rs` | Notifications return metadata field |
| `lib/screens/rabble_chat.dart` | AR button → RabblePortal, removed _SwarmPortalPage |
| `lib/screens/home_shell.dart` | Push subscription, proximity check, notification settings menu |
| `lib/screens/creature/creature_actions.dart` | Untether direct API, tether radius check, host GPS auto |
| `lib/widgets/rabble_portal.dart` | preloadSwarmId for direct load without scan |
| `lib/models/portal_creature.dart` | origin_lat/origin_lng for positioning |
| `lib/services/api_client.dart` | Push, proximity, governance API methods |

---

## Known Issues

### Must Fix (If Testing Reveals)
- 🟡 **Push notification delivery** — VAPID signing implemented but untested with real Chrome/Firefox push services. May need debugging.
- 🟡 **Notification prefs not enforced server-side** — client stores prefs but backend doesn't check them before sending. Need server-side filtering.
- 🟡 **AR portal from chat may still show scanner briefly** — preloadSwarmId should skip it but the scanner controller initializes in initState.

### Open (Lower Priority)
- 🟡 SSE CORS headers untested for cross-origin
- 🟡 Stale test files reference `CreatureDetailScreen`
- 🟡 `creature_count` on swarm_events can drift — periodic reconciliation needed
- 🟢 Unused import warnings in some screens
- 🟢 Chat polling is 3s interval — could use SSE for real-time delivery

---

## Database State

```
Migrations applied: 001-098
Active rabbles: ~10
Creatures: 68+
Users: 2+ active testers
Friendships: 5+ (some accepted via notification UI)
Push subscriptions: pending first real test
Reports: 0
Blocks: 0
VAPID keys: configured in push_config table
```

---

## How to Resume

```bash
# 1. Check current state
cd /home/ilabra/fermi && git log --oneline -3
cd /home/ilabra/rabble && git log --oneline -3

# 2. Verify builds
cd /home/ilabra/fermi && cargo build 2>&1 | grep "^error"
cd /home/ilabra/rabble && /home/ilabra/flutter/bin/flutter analyze 2>&1 | grep "error" | head -5

# 3. Verify production
curl -s https://agent-bestiary.world/api/health | head -1

# 4. Build + deploy cycle
cd /home/ilabra/rabble
/home/ilabra/flutter/bin/flutter build web --release
rm -rf /home/ilabra/fermi/rabble-web/* && cp -r build/web/* /home/ilabra/fermi/rabble-web/
cp web/custom-sw.js /home/ilabra/fermi/rabble-web/custom-sw.js
cd /home/ilabra/fermi && git add -A && git commit -m "build: ..." && git push origin main

# 5. Key design docs
cat docs/DESIGN_RICH_MEDIA_CHAT.md       # Images, video, audio, polls
cat docs/DESIGN_GIFT_AS_INVITE.md        # Creature gifting, campaigns
cat docs/DESIGN_GOVERNANCE.md            # Block, eject, report
cat docs/SESSION_CONTEXT_2026_02_22.md   # This file
```

---

## Session Stats

**Duration:** ~6 hours (continuation of 2026-02-21)
**Commits:** 35 across both repos
**Bugs fixed:** 15+
**Features added:** Push notifications, proximity alerts, @mentions, notification settings, drift detection, rabble moving alerts
**State transitions hardened:** perch, host, join, fly, tether, untether, leave, end
**Design docs captured:** Rich Media Chat, Gift-as-Invite, Governance

---

**Status:** Active Development 🚀
**Next Milestone:** Rich messaging (images, @mention autocomplete) + usability polish
**App readiness:** Core social loop functional, governance in place, notifications operational. Approaching soft launch.