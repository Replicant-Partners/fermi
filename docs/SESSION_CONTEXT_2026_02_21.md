# Session Context — 2026-02-21

> **Purpose:** Everything needed to resume work in the next session.
> **Last commit (fermi):** `1e5482c` — "Push notifications infrastructure + governance backend"
> **Last commit (rabble):** `98148b7` — "Governance MVP: Block + Eject + Report"
> **Branch:** `main` (both repos synced with origin)
> **Previous session:** `docs/SESSION_CONTEXT_2026_02_20.md`

---

## What Happened This Session

Massive session — core social loop went from broken to functional. Two real users (Ivan + Alena) tested across devices.

### Major Achievements

1. **Creature = Persona model enforced globally**
   - `rabble_role` derived from `anchor_creature_id`, not `creator_id`
   - `isHosting` uses `isAnchor` flag, not `creature_state`
   - One creature = one host. User manages by proxy.

2. **Rabble lifecycle works end-to-end**
   - Mint → perch → host → land in chat → other user discovers → joins → chat → leave/end
   - Host flow auto-navigates to rabble chat with `initialCreatureId`
   - Creature state update synchronous (fast UPSERT), version history in background

3. **Explicit leave/end semantics**
   - "Leave" button in creature tray (explicit, not implicit via hop)
   - "End Rabble" for anchor creatures (with transfer option)
   - Hop/fly preserves rabble membership (new flight inherits `swarm_id`)
   - Outside-radius hop warns and confirms leave

4. **Chat identity — creature as persona**
   - Messages grouped by creature (not user)
   - Avatar on every message
   - Active persona persisted per rabble in SharedPreferences
   - Entry flow carries `initialCreatureId`

5. **Member count fixed**
   - `creature_state.rabble_id` is now the source of truth (not `creature_flights`)
   - Members endpoint + flock_history both query creature_state
   - Reconciled `swarm_events.creature_count` from creature_state

6. **Rabbles screen redesigned**
   - 4 tabs: Discover | Hosting | Joined | Following
   - Follow toggle (🔭 scope icon) on every card
   - "Host" FAB with full flow: creature picker → location → config → chat
   - Edit rabble name from card

7. **Governance MVP shipped**
   - Block: creature-level + user-level escalation (private, blocked party never knows)
   - Eject: host removes creature from rabble (24h cooldown or permanent)
   - Report: reason picker, context snapshot, admin review queue
   - Block checks in join_swarm + friendship_request flows
   - Overflow menu on non-owned creature cards

8. **Notifications with deep linking**
   - Friendship requests: Accept/Decline action buttons
   - Creature invites: Accept/Decline action buttons
   - Tap any notification → navigates to creature card or rabble chat
   - Notifications now include `metadata` JSONB with IDs for deep linking

9. **Friends list on creature card**
   - Shows actual friends with avatar, name, "Met in [rabble]" context
   - Tap friend → navigate to their creature card

10. **Push notification infrastructure**
    - Database: `push_subscriptions` + `push_config` tables
    - Backend: subscribe/unsubscribe endpoints, `notify_user()` helper
    - Tickle push pattern (no ECE encryption needed)

11. **Performance optimizations**
    - Migration 096: 15 targeted indexes for hot-path queries
    - Parallel API calls in creature screen `_load()`
    - Debounced SSE reload (500ms coalesce)

12. **Bugs fixed**
    - User search 500: `SELECT DISTINCT` + `ORDER BY` conflict
    - Join rabble 500: `team_members.member_id` not `user_id`
    - Rabble chat 502: members endpoint was creator-only + wrong data
    - Flock viz showing historical creatures (missing `ended_at IS NULL`)
    - Fly handler updating ended flight instead of creating new one
    - End rabble not clearing creature_state

---

## Integrity Issue — AR Portal Viewer

### Problem

The AR portal viewer in rabble chat:
1. **Only returns the current user's creatures**, not all members of the swarm
2. The AR view button in rabble chat currently opens a camera shot that blanks out and doesn't project properly

### Root Cause

The `flock_history_handler` was fixed to use `creature_state.rabble_id` as source of truth, but the `ArViewerScreen.portal()` constructor may be filtering by user or passing incomplete creature data.

### What Needs To Happen

1. **AR portal should show ALL creatures in the rabble** (same data as flock viz / members)
2. **The AR button in rabble chat should use the portal viewer** (spatial creatures projected via AR), not the generic camera view
3. The `_launchAR()` method in `rabble_chat.dart` fetches flock data and builds `PortalCreature` list — verify it's not filtering by user
4. The `ArViewerScreen.portal()` should receive the complete creature list and project them spatially

### Files to Check

| File | What to check |
|------|--------------|
| `rabble/lib/screens/rabble_chat.dart` → `_launchAR()` | Does it pass all creatures or just mine? |
| `rabble/lib/screens/ar_viewer.dart` → `ArViewerScreen.portal()` | Does it filter by userId internally? |
| `fermi/src/handlers/rabble_workspace.rs` → `flock_history_handler` | Already fixed to use creature_state — verify it returns all members |
| `rabble/lib/models/portal_creature.dart` → `PortalCreature.fromJson()` | Does it filter by userId? |

### Design Change

The AR button in rabble chat (`_launchAR`) should:
- Open the portal viewer showing ALL creatures in the rabble
- Project them spatially using the flock dynamics data
- This replaces the current generic camera view
- Same link/action as the QR scan join flow (portal view, not camera)

---

## Plan — Next Session

### Priority 1: Fix AR Portal (Integrity)

1. Trace `_launchAR()` in rabble_chat.dart — ensure all rabble creatures are passed
2. Check `PortalCreature.fromJson()` for userId filtering
3. Verify `flock_history_handler` returns all members (should be fixed already)
4. Replace AR camera view with portal viewer in rabble chat
5. Test: all creatures visible in AR, not just mine

### Priority 2: Push Notifications (Go Live)

1. Generate VAPID keys: `npx web-push generate-vapid-keys`
2. Set as Vercel env vars: `VAPID_PUBLIC_KEY`, `VAPID_PRIVATE_KEY`
3. Add push event handler to service worker
4. Flutter: request notification permission + subscribe to push
5. Migrate existing `INSERT INTO notifications` calls to `notify_user()`
6. Test: close app → receive push when someone joins rabble

### Priority 3: Test & Polish

1. Test governance: block creature, block user, eject from rabble, report
2. Test follow toggle on rabble cards
3. Test notification deep linking (friendship accept → creature card)
4. Test rabble lifecycle: create → discover → join → chat → leave → end
5. Fix any remaining member count drift
6. Error message cleanup (no raw SQL/stack traces to users)

### Priority 4: Deferred Features (Design Docs Ready)

| Feature | Doc | Effort |
|---------|-----|--------|
| Rich Media Chat | `docs/DESIGN_RICH_MEDIA_CHAT.md` | ~12-15h |
| Gift-as-Invite | `docs/DESIGN_GIFT_AS_INVITE.md` | ~16-20h |
| Governance (remaining) | `docs/DESIGN_GOVERNANCE.md` | ~2h (chat filtering, admin review) |

---

## Project State Report

### Architecture

```
rabble.world (PWA)
├── Flutter Web (rabble/) — 4-pillar UX
│   ├── 🐾 Creatures — collection, detail, actions, friends
│   ├── 👥 Rabbles — Discover/Hosting/Joined/Following
│   ├── 🌍 Environment — Map, explore, AR
│   └── 📓 Journals — Activity, flights
│
├── Rust API (fermi/) — Vercel serverless
│   ├── Creatures — CRUD, flights, state, tethering
│   ├── Rabbles — host, join, leave, end, flock
│   ├── Social — friendships, invites, co-presence
│   ├── Governance — block, eject, report
│   ├── Push — subscribe, notify, tickle
│   ├── Chat — messages, narrator, mentions
│   └── Auth — JWT, API keys, OAuth
│
└── Neon PostgreSQL — 98 migrations applied
    ├── creatures, creature_state, creature_versions
    ├── swarm_events, creature_flights
    ├── creature_friendships, creature_blocks, user_blocks
    ├── rabble_messages, rabble_follows, rabble_ejections
    ├── push_subscriptions, push_config
    ├── notifications, reports
    └── activity_events, rabble_co_presence
```

### Codebase Metrics (approx)

- **Rust backend:** ~50K lines across handlers, auth, memory
- **Flutter app:** ~18K lines across screens, widgets, services, models
- **Migrations:** 98 SQL files
- **Design docs:** 3 (Rich Media, Gift-as-Invite, Governance)

### Key Design Decisions This Session

| Decision | Rationale |
|----------|-----------|
| `creature_state` = source of truth for membership | Flights get ended/created by various flows; creature_state is always explicitly updated |
| Explicit leave, not implicit | Hopping outside radius warns + confirms. No silent removal. |
| creature_state UPSERT synchronous, record_transition async | Fast response (no 503 timeout) while preserving version history |
| Tickle push pattern | Avoids complex ECE encryption; service worker fetches from API |
| Block is private | Blocked party never knows — prevents retaliation |
| Eject with cooldown | 24h default, permanent option. Host moderation without permanent damage |
| Follow = favourites | Not tabs, a toggle on every card. Future: notification preferences |

---

## Commit History (this session)

### Fermi (25+ commits)

```
1e5482c Push notifications infrastructure + governance backend
6bbea2e Governance MVP: Block + Eject + Report — full backend + Flutter UI
9d346e4 Friends list + notification deep linking + metadata on notifications
1220966 docs: Governance design — Block + Eject + Report for MVP
17cd61a docs: Gift-as-Invite design — creature gifting, campaigns, AR drops
3fd6706 docs: Rich Media Chat design document
5d48d17 Fix member count: use creature_state as source of truth
1988960 build: rebuild Flutter web — notification actions + binoculars follow icon
5b6447f build: rebuild Flutter web — Rabbles 4 tabs + follow toggle
9582889 Fix end rabble: clear creature_state for all creatures
8f61064 build: rebuild Flutter web — unified rabble list with filter chips
d84f7a3 Fix join rabble 500: team_members column is member_id, not user_id
a0779d6 build: rebuild Flutter web — Nearby tab for rabble discovery
8835af4 build: rebuild Flutter web — Leave button always visible in tray
b64a192 Fix 0 members: fly handler was updating ended flight
9d0a509 Explicit leave rabble + movement preserves membership
140dad1 build: rebuild Flutter web — hop outside rabble warns + confirms leave
3fd5ab9 build: rebuild Flutter web — avatar on every message, members retry
20d0253 Fix rabble chat 502: members endpoint was creator-only + wrong data
d10bf14 Fix user search 500: replace DISTINCT with GROUP BY
9e85162 Fix 503 timeout: fast inline creature_state UPSERT
d02d64e build: rebuild Flutter web — MVP rabble flow, host→chat, edit name
a032293 Fix flock viz + chat identity + creature card layout
2c8e06c Creature = persona: backend auto-detect fix + members endpoint
db11598 Sprint F23-F28 + one creature = one host global pattern
7641b66 Perf: migration 096 indexes + LATERAL joins for list_creatures
```

### Rabble (15+ commits)

```
98148b7 Governance MVP: Block + Eject + Report
6899051 Friends list on creature card + notification deep linking
53c7666 Notifications: Accept/Decline for friendships + invites
72a71cb Rabbles: 4 tabs (Discover/Hosting/Joined/Following) + follow toggle
7b7e038 Rabbles: unified list with filter chips (reverted to tabs)
0541715 Chat: avatar on every message + retry members on empty
6cb2875 Explicit leave rabble + movement preserves membership
f0bdbbd Hop/expedition outside rabble area warns + confirms leave
947bf50 Fix Leave button: always visible in tray
1183676 Chat identity + creature card layout + flock fix
055adaa Creature = persona: entry flow, persisted identity, chat attribution
ac3c1b1 MVP rabble flow: host→chat, Add Rabble FAB, Edit name
1decd26 Sprint F23-F28: one creature = one host
```

---

## Key Files Reference

| File | What it is |
|------|-----------|
| **DESIGN DOCS** | |
| `docs/DESIGN_RICH_MEDIA_CHAT.md` | Images, video, audio, polls — ~12-15h |
| `docs/DESIGN_GIFT_AS_INVITE.md` | Creature gifting, campaigns, AR drops — ~16-20h |
| `docs/DESIGN_GOVERNANCE.md` | Block, eject, report — MVP done, chat filtering remaining |
| **BACKEND — NEW** | |
| `src/handlers/governance.rs` | Block/eject/report handlers + helper functions |
| `src/handlers/push.rs` | Push subscription + delivery + notify_user() |
| `migrations/097_governance.sql` | creature_blocks, user_blocks, rabble_ejections, reports |
| `migrations/098_push_subscriptions.sql` | push_subscriptions, push_config |
| **BACKEND — MODIFIED** | |
| `src/handlers/creatures/state.rs` | join_swarm + host_rabble: fast state UPSERT, ejection/block checks |
| `src/handlers/creatures/swarms.rs` | end_rabble + leave_rabble handlers |
| `src/handlers/creatures/query.rs` | rabble_role from anchor, is_anchor in list |
| `src/handlers/rabble_chat.rs` | Members from creature_state, auto-detect with ended_at IS NULL |
| `src/handlers/rabble_workspace.rs` | flock_history from creature_state |
| `src/handlers/social.rs` | Block check in friendships, metadata on notifications |
| `src/handlers/profile.rs` | Notifications return metadata field |
| **FLUTTER — KEY CHANGES** | |
| `lib/screens/rabble_chat.dart` | Persona: initialCreatureId, SharedPreferences, leave button, member count |
| `lib/screens/rabbles_screen.dart` | 4 tabs, follow toggle, host FAB, edit sheet |
| `lib/screens/notifications_screen.dart` | Accept/Decline actions, deep linking |
| `lib/screens/creature/creature_screen.dart` | Friends list, block/report menu, card layout |
| `lib/screens/creature/creature_actions.dart` | End Rabble, leave radius warning, host→chat nav |
| `lib/widgets/chat_panel.dart` | Group by creature, avatar every message |
| `lib/widgets/end_rabble_sheet.dart` | End/transfer rabble bottom sheet |
| `lib/models/creature.dart` | isHosting from isAnchor |

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

# 4. AR portal issue — start here
grep -n "_launchAR\|PortalCreature\|portal" rabble/lib/screens/rabble_chat.dart

# 5. Push notifications — next
npx web-push generate-vapid-keys  # generate VAPID keys
vercel env add VAPID_PUBLIC_KEY   # set in Vercel
vercel env add VAPID_PRIVATE_KEY

# 6. Build + deploy cycle
cd /home/ilabra/rabble
/home/ilabra/flutter/bin/flutter build web --release
rm -rf /home/ilabra/fermi/rabble-web/* && cp -r build/web/* /home/ilabra/fermi/rabble-web/
cd /home/ilabra/fermi && git add -A && git commit -m "build: ..." && git push origin main
```

---

## Known Issues

### Must Fix (Next Session)

- 🔴 **AR portal only shows user's own creatures** — should show all rabble members
- 🔴 **AR button in rabble chat opens blank camera** — should open portal viewer with spatial creatures
- 🟡 **Push notifications not delivering** — infrastructure built, VAPID keys + service worker needed
- 🟡 **Existing notification code** uses raw INSERT — should migrate to `notify_user()` for push

### Open (Lower Priority)

- 🟡 `my/rabbles` response may not include `creator_id` — Flutter fallback handles it
- 🟡 SSE CORS headers untested for cross-origin (same-origin PWA works)
- 🟡 Stale test files reference `CreatureDetailScreen` (renamed to `CreatureScreen`)
- 🟢 Unused import warnings in some screens
- 🟢 `creature_count` on swarm_events can drift — needs periodic reconciliation

---

## Database State

```
# After this session:
# Migrations applied: 001-098 (including 094 rabble_follows, 096 perf indexes, 097 governance, 098 push)
# Active rabbles: ~8
# Creatures: 68
# Users: 2 active testers (Ivan + Alena)
# Friendships: 5 (some accepted via new notification UI)
# Push subscriptions: 0 (not yet configured)
# Reports: 0
# Blocks: 0
```

---

**Status:** Active Development 🚀
**Next Milestone:** AR portal fix + Push notifications go-live
**Session Duration:** ~8 hours
**Commits:** 40+ across both repos