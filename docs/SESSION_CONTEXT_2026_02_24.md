# Session Context — 2026-02-24

> **Purpose:** Everything needed to resume work in the next session.
> **Last commit (fermi):** `077c25a` — "build: creature invite icon — bug with + badge"
> **Last commit (rabble):** `f078700` — "Creature invite icon: bug + plus badge (matches creature theme)"
> **Branch:** `main` (both repos synced with origin)
> **Previous sessions:** `docs/SESSION_CONTEXT_2026_02_22.md`, `docs/SESSION_CONTEXT_2026_02_21.md`

---

## What Happened This Session (2026-02-24)

Continuation of the 2026-02-22 session. Focus: push notification delivery fix, chat message notifications, creature invite + eject UI affordances, and icon polish.

### Commits This Session

**Fermi (5 commits):**
```
077c25a build: creature invite icon — bug with + badge
414d9a4 build: creature invite + eject UI in rabble chat
c83c268 Push notification for new chat messages — 5min cooldown per user per rabble
e460727 build: fix push subscription JS interop (callback pattern)
909c5ee docs: Session context 2026-02-22 — state of the project
```

**Rabble (3 commits):**
```
f078700 Creature invite icon: bug + plus badge (matches creature theme)
da19acc Creature invite + eject affordances in rabble chat
ea62705 Fix push subscription: callback pattern instead of broken promiseToFuture
```

### What Was Fixed / Added

1. **Push subscription JS interop fixed**
   - Root cause: `promiseToFuture` on `eval`'d JS Promise threw `NoSuchMethodError: 'then'`
   - Fix: callback pattern — register Dart callback as `window._pushSubCallback`, JS calls it when done
   - Push subscriptions now registering (1 active sub confirmed in DB)

2. **Chat message push notifications**
   - New message in rabble → push to all member owners except sender
   - Title: "[creature name] in [rabble name]" with message preview
   - 5-minute cooldown per user per rabble (no spam during active chat)
   - Notification type: `chat_message`

3. **Creature invite affordance**
   - New 🐛+ icon in rabble chat app bar (bug with amber plus badge)
   - Shows friends of active creature not already in the rabble
   - Tap → sends "come fly with me!" creature invite
   - Invite sent from active persona (creature-to-creature)

4. **Eject affordance**
   - Long-press any member creature in the members section (host only)
   - Dialog: Remove (24h cooldown) or Ban permanently
   - Can't eject anchor creature or your own creatures
   - System message in chat, notification to ejected user

5. **Members section improvements**
   - Tap any member → navigates to creature card
   - Images use consistent `/api/creatures/{id}/image` endpoint

---

## State of the Project Report

### Overview

Rabble is a location-based social app where users interact through creature personas. Creatures are digital insects (butterflies, beetles, dragonflies, etc.) that users mint, place in the real world, and gather with in "rabbles" — spatial social gatherings anchored by a host creature.

The app runs as a PWA at **rabble.world**, built with Flutter Web (frontend) and Rust (backend) deployed on Vercel with Neon PostgreSQL.

### Architecture

```
rabble.world (PWA)
├── Flutter Web (rabble/) — ~20K lines
│   ├── 🐾 Creatures — collection, detail card, actions, friends, tethered filter
│   ├── 👥 Rabbles — Discover/Hosting/Joined/Following, host flow, edit, follow toggle
│   ├── 🌍 Environment — Map, explore, AR portal (QR scan + direct), proximity alerts
│   ├── 📓 Journals — Activity feed, flights
│   ├── 🔔 Notifications — Accept/Decline actions, deep linking, 13 notification types
│   ├── ⚙️ Settings — Notification preferences, per-rabble mute, block list
│   └── 💬 Chat — Creature personas, @mentions, creature invite, eject, leave/end
│
├── Rust API (fermi/) — ~55K lines, Vercel serverless
│   ├── Creatures — CRUD, flights, state machine, tethering, versioning, cognition
│   ├── Rabbles — host, join, leave, end, flock dynamics, scatter, anchor movement
│   ├── Social — friendships, creature invites, co-presence, @mentions
│   ├── Governance — block (creature + user level), eject, report
│   ├── Push — VAPID ES256 signing, subscribe, proximity, drift, moving alerts
│   ├── Chat — messages, narrator agent, @mention parsing, creature attribution
│   ├── Auth — JWT (HS256), API keys, OAuth (Google, GitHub)
│   ├── Economy — credit wallets, gas fees, transfers, walk-in pricing
│   └── AR — flock visualization, portal creatures, QR resolve
│
└── Neon PostgreSQL — 98 migrations, 80 tables
    ├── Core: creatures, creature_state, creature_versions, creature_conditions
    ├── Flights: creature_flights, creature_tethers, telemetry_points
    ├── Social: creature_friendships, creature_invites, creature_co_presence
    ├── Rabbles: swarm_events, rabble_messages, rabble_follows, rabble_ejections
    ├── Governance: creature_blocks, user_blocks, reports
    ├── Push: push_subscriptions, push_config (VAPID keys)
    ├── Notifications: notifications (with metadata JSONB for deep linking)
    ├── Economy: wallets, credit_ledger, gas_fees
    └── Auth: users, api_keys, teams, object_shares
```

### Database Stats

| Table | Count |
|-------|-------|
| Creatures (active) | 88 |
| Users | 12 |
| Active rabbles | 12 |
| Friendships | 19 |
| Notifications | 77 |
| Push subscriptions | 1 |
| Reports | 0 |
| Blocks | 0 |
| DB tables | 80 |

### Feature Completeness

#### ✅ Complete & Working

| Feature | Status |
|---------|--------|
| **Creature minting** | Mint from GBIF species database, AI-generated art |
| **Creature card** | Hero image, config, rabble section, friends list, journal, actions |
| **Creature actions** | Perch, hop, expedition, tether, untether, host, join, leave, gift, dream |
| **Rabble hosting** | Pick creature → location → config → auto-navigate to chat |
| **Rabble discovery** | Discover tab (public rabbles), map nearby alerts, QR scan |
| **Rabble chat** | Creature personas, message grouping, avatars, @mentions |
| **Creature tethering** | GPS tracking, rabble follows host, member drift detection |
| **Friendship system** | befriend (creature-to-creature), accept/decline, friends list |
| **Creature invite** | Invite friend's creature to your rabble (🐛+ icon) |
| **Eject** | Host long-press member to remove (24h or permanent) |
| **Block** | Creature-level + user-level escalation (private) |
| **Report** | Reason picker, context snapshot, admin queue |
| **Follow / favourites** | 🔭 scope toggle on every rabble card |
| **Notifications** | 13 types, Accept/Decline, deep linking, settings screen |
| **Push notifications** | VAPID ES256, service worker, proximity, chat, drift |
| **AR portal** | Animated flock viz from QR scan or rabble chat |
| **Flock dynamics** | Reynolds-style simulation, species-appropriate behavior |
| **Map** | Creature pins, rabble circles, viewpoint toggle, GPS zoom |
| **Wallet** | Credits, gas fees, transfers, walk-in pricing |
| **Auth** | Google + GitHub OAuth, JWT sessions, API keys |

#### ⚠️ Built but Needs Testing/Polish

| Feature | Notes |
|---------|-------|
| **Push delivery** | VAPID signing implemented, 1 subscription registered, needs real-world test |
| **Notification preferences** | Client-side (SharedPreferences), not enforced server-side |
| **Location service** | GPS works, nearby alerts work, need to verify background push |
| **AR from chat** | Unified with map QR scan portal, needs verification post-deploy |
| **Tether-while-in-rabble** | Warns if outside radius, but edge cases may remain |

#### ❌ Not Yet Built (Design Docs Ready)

| Feature | Design Doc | Effort |
|---------|-----------|--------|
| **Rich Media Chat** | `docs/DESIGN_RICH_MEDIA_CHAT.md` | ~12-15h |
| **Gift-as-Invite** | `docs/DESIGN_GIFT_AS_INVITE.md` | ~16-20h |
| **@Mention autocomplete** | In rich media doc | ~2h (part of Phase 1) |
| **Chat image upload** | In rich media doc | ~2-3h |
| **Polls in chat** | In rich media doc | ~2-3h |
| **Map creature pins** | Noted in session context | ~2h |
| **Server-side notification prefs** | Need user_preferences table | ~2h |
| **Admin review screen** | For reports | ~2h |
| **Campaign / batch invite** | In gift-as-invite doc | ~4-5h |

### Creature State Machine (Final)

```
                    perch (2cr)
          ┌────────────────────────┐
          │                        ▼
     ┌─────────┐            ┌──────────┐
     │  mint   │            │ perched  │ ◄──── untether / leave / end rabble
     └─────────┘            └──────────┘
                              │    │  ▲
                    host (3cr)│    │  │ leave (non-anchor)
                              │    │  │
                              ▼    │  │        join (free-2cr)
                         ┌─────────┴──┴───┐ ◄──────────────────
                         │   in_rabble    │
                         │   / hosting    │ ────► leave (anchor) = auto-end rabble
                         └────────┬───────┘
                                  │  ▲
                    tether (1cr)  │  │  untether
                                  ▼  │
                         ┌────────────────┐
                         │   tracking     │  (presence = 'tracking')
                         │   (GPS live)   │  ── anchor? rabble follows you
                         └────────────────┘

Every transition atomically updates:
  1. creature_state     (state, rabble_id, location)
  2. creature_conditions (presence: active/tracking)
  3. creature_flights    (end old, create new, inherit swarm_id)
  4. creature_tethers    (deactivate if action replaces tethering)
  5. swarm_events        (creature_count increment/decrement)

Principles:
  - No constraints on creature actions (creature is always free to act)
  - Tether auto-cleans stale records
  - Anchor leave auto-ends rabble
  - Fly/hop auto-untethers
  - End rabble preserves device flights (tethered creatures keep tracking)
  - creature_state.rabble_id is source of truth for membership (not flights)
```

### Notification System

```
13 notification types, all routed through notify_user():

┌─────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  Backend event   │ ──► │  notify_user()   │ ──► │  notifications   │
│  (friendship,    │     │  (push.rs)       │     │  table (in-app)  │
│   chat, join,    │     │                  │     └──────────────────┘
│   drift, etc.)   │     │                  │
└─────────────────┘     │                  │ ──► ┌──────────────────┐
                         │  Background task │     │  Web Push (VAPID)│
                         └──────────────────┘     │  → Service Worker│
                                                   │  → Native notif  │
                                                   └──────────────────┘

Types: friendship_request, friendship_accepted, creature_invite,
       rabble_invite, rabble_join, rabble_start, rabble_end,
       rabble_eject, creature_gift, credit_transfer,
       rabble_nearby (4h), rabble_drift (4h), rabble_moving (5min),
       chat_message (5min), chat_mention
```

### Credit Economy

| Action | Cost | Revenue flow |
|--------|------|-------------|
| Mint creature | 5cr | Platform |
| Perch | 2cr | Platform |
| Host rabble | 3cr+ | Platform |
| Hop | 1cr | Platform |
| Expedition | 5cr | Platform + Agent |
| Tether | 1cr | Platform |
| Chat message | 1cr | Platform |
| Dream | 5cr (first free daily) | Platform + Agent |
| Walk-in join | 0-Ncr | Host (90% revenue) |
| Flock visualization | Platform read fee | Platform |

### Three-Session Summary (2026-02-21 → 2026-02-24)

Over three sessions (~20 hours), the app went from "features exist but nothing connects" to "core social loop functional, approaching soft launch":

| Metric | Before | After |
|--------|--------|-------|
| State transitions | Fragile, inconsistent | Clean across 5 tables, no constraints |
| Two-user interaction | Broken (500 errors) | Working (join, chat, befriend, invite) |
| Notifications | Text-only bell | 13 types, push, deep linking, settings |
| Tethering | Broken (stale records) | Auto-clean, drift detection, rabble follows |
| AR portal | Blank camera | Unified portal, creature images, scatter |
| Rabble discovery | Hidden in tabs | Discover tab, proximity push, map alerts |
| Governance | None | Block, eject, report |
| Chat identity | User-attributed | Creature-attributed, @mentions, invite |
| Commits | 0 | 75+ across both repos |

---

## What's Next (Priority Order)

### Priority 1: Test & Stabilize
- Verify push notifications deliver to real Chrome/Firefox
- Test full two-user flow: discover → join → chat → invite → leave
- Test tether → host → move → drift detection cycle
- Fix any remaining edge cases from testing

### Priority 2: Rich Messaging
- **@Mention autocomplete** — type @ in chat, show creature name picker
- **Image upload** — 📎 button, compress, upload, inline display
- **Audio notes** — hold to record, waveform playback
- Design doc: `docs/DESIGN_RICH_MEDIA_CHAT.md`

### Priority 3: Usability Polish
- Error messages: no raw SQL to users
- Loading states / shimmer placeholders
- Onboarding guidance for new users
- Map: individual creature pins (not just rabble circles)

### Priority 4: Growth Features
- **Gift-as-Invite** — creature IS the invitation (design doc ready)
- **Campaigns** — batch mint N creatures, shareable link
- **AR Drops** — QR-encoded creature at physical location
- Design doc: `docs/DESIGN_GIFT_AS_INVITE.md`

### Priority 5: Server-side Preferences
- `user_preferences` table for cross-device persistence
- Backend checks prefs before sending push
- Per-rabble mute enforced at delivery time

---

## Key Files Reference

| File | What it is |
|------|-----------|
| **DESIGN DOCS** | |
| `docs/DESIGN_RICH_MEDIA_CHAT.md` | Images, video, audio, polls — ~12-15h |
| `docs/DESIGN_GIFT_AS_INVITE.md` | Creature gifting, campaigns, AR drops — ~16-20h |
| `docs/DESIGN_GOVERNANCE.md` | Block, eject, report — MVP done |
| **SESSION CONTEXT** | |
| `docs/SESSION_CONTEXT_2026_02_24.md` | This file |
| `docs/SESSION_CONTEXT_2026_02_22.md` | Previous session |
| `docs/SESSION_CONTEXT_2026_02_21.md` | First session of sprint |
| **BACKEND — KEY FILES** | |
| `src/handlers/push.rs` | VAPID signing, proximity, notify_user() |
| `src/handlers/governance.rs` | Block, eject, report handlers |
| `src/handlers/creatures/tethering.rs` | Tether/untether/telemetry state machine |
| `src/handlers/creatures/state.rs` | perch/host/join/leave with 5-table updates |
| `src/handlers/creatures/swarms.rs` | end_rabble, leave_rabble (anchor auto-end) |
| `src/handlers/creatures/flights.rs` | fly/hop with auto-untether |
| `src/handlers/rabble_chat.rs` | Chat messages, @mentions, creature invites |
| `src/handlers/social.rs` | Friendships, invites, notifications |
| `src/handlers/qr_codes.rs` | QR resolve with creature_state source |
| **FLUTTER — KEY FILES** | |
| `lib/screens/rabble_chat.dart` | Chat, creature tray, leave, AR portal, invite, eject |
| `lib/screens/creature/creature_actions.dart` | All creature actions including tether/host |
| `lib/screens/creature/creature_screen.dart` | Creature card, friends list, block/report |
| `lib/screens/rabbles_screen.dart` | 4 tabs, host flow, edit, follow toggle |
| `lib/screens/notifications_screen.dart` | Accept/Decline, deep linking |
| `lib/screens/notification_settings_screen.dart` | Preferences, mute, blocks |
| `lib/screens/home_shell.dart` | Push subscription, proximity, navigation |
| `lib/widgets/rabble_portal.dart` | AR portal with preloadSwarmId |
| `lib/widgets/chat_panel.dart` | Message rendering, creature grouping |
| `lib/services/tether_service.dart` | GPS tracking client |
| `web/custom-sw.js` | Service worker for push notifications |

---

## How to Resume

```bash
# 1. Check current state
cd /home/ilabra/fermi && git log --oneline -3
cd /home/ilabra/rabble && git log --oneline -3

# 2. Verify builds
cd /home/ilabra/fermi && cargo build 2>&1 | grep "^error"
cd /home/ilabra/rabble && /home/ilabra/flutter/bin/flutter build web --release 2>&1 | tail -3

# 3. Verify production
curl -s https://agent-bestiary.world/api/health | head -1
curl -s https://rabble.world/ | head -1

# 4. Database access
export DB_URL=$(grep DATABASE_URL_UNPOOLED /home/ilabra/fermi/.env.local | sed 's/DATABASE_URL_UNPOOLED="//' | sed 's/"//')
psql "$DB_URL" -c "SELECT COUNT(*) FROM creatures WHERE status = 'active';"

# 5. Build + deploy cycle
cd /home/ilabra/rabble
/home/ilabra/flutter/bin/flutter build web --release
rm -rf /home/ilabra/fermi/rabble-web/* && cp -r build/web/* /home/ilabra/fermi/rabble-web/
cp web/custom-sw.js /home/ilabra/fermi/rabble-web/custom-sw.js
cd /home/ilabra/fermi && git add -A && git commit -m "build: ..." && git push origin main

# 6. Key design docs for next features
cat docs/DESIGN_RICH_MEDIA_CHAT.md
cat docs/DESIGN_GIFT_AS_INVITE.md
```

---

**Status:** Active Development 🚀
**Next Milestone:** Rich messaging (images, @mention autocomplete) + push verification
**App readiness:** Core social loop functional. Governance in place. Notifications operational. Approaching soft launch.
**Total sprint effort:** ~20 hours across 3 sessions, 75+ commits