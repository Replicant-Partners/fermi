# Session Context — 2026-02-20

> **Purpose:** Everything needed to resume work in the next session.
> **Last commit (fermi):** `e560c9e` — "build: rebuild Flutter web — creature card rewrite, single scroll, expandable journal"
> **Last commit (rabble):** `e11574f` — "Sprint batch 5: F1+F2+F7 — creature card rewrite"
> **Branch:** `main` (both repos synced with origin)
> **Previous session:** `docs/SESSION_CONTEXT_2026_02_19.md`

---

## What Happened This Session

This was a massive session — the entire four-pillar UX was built, deployed, user-tested, and refined across two sprint cycles.

### Phase 1-4: Full UX Build (first half of session)

Built and deployed the complete four-pillar UX across both repos:

**Backend (fermi) — 12 endpoints added:**
- `GET /api/creatures/:id/stream` — Creature SSE real-time events
- `POST/PUT/DELETE /api/rabbles/:id/follow` — Rabble follow/unfollow
- `GET /api/my/following` — List followed rabbles
- `POST/GET/PATCH/DELETE /api/locations` — Saved locations CRUD
- `GET /api/dashboard/nearby-creatures` — PostGIS spatial creature discovery
- `GET /api/creatures/:id/activity` — Per-creature activity feed
- `GET /api/creatures/:id/flight-path/:fid` — Flight path GeoJSON

**Backend — 3 new migrations:**
- `094_rabble_follows.sql` — Follow table + notification functions
- `095_saved_locations.sql` — Saved locations + nearby creatures (PostGIS)
- `093_users_user_id_unique.sql` — (from prior session, FK constraint)

**Backend — Major refactoring:**
- Split `state.rs` from 5,226 → 5 focused modules (helpers, flights, tethering, agent_modules, state)
- `verify_creature_ownership()` helper replaced 21 copy-pasted blocks
- Dead code cleanup: warnings 93 → 42, deleted stale files
- `cargo fix` applied across api-server

**Flutter (rabble) — Complete UX rebuild:**
- Four-pillar bottom nav: 🐾 Creatures, 👥 Rabbles, 🌍 Environment, 📓 Journals
- `RabblesScreen` — Hosting/Joined/Following tabs
- `JournalsScreen` — Activity/Friends/My Creatures/Flights tabs
- `CreatureStreamService` — SSE client with reconnection + backfill
- `CreatureLink` widget — 4 display styles (chip, tile, avatar, mini)
- Creature Detail Hub Card — Rabble/Friends/Journal sections
- Creature model enriched with social + active_flight data
- Explore map: viewpoint toggle, rabble circles, saved locations, live tracking, AR FAB
- Rabble chat: real-time map tracking, member list, follow button

### User Testing + Sprint (second half of session)

Owner tested the deployed UX and provided 22 feedback items (F1-F22). We executed 19 of them in the same session:

#### ✅ Completed Sprint Items (19/22)

| # | Fix | What changed |
|---|---|---|
| **F1** | Creature card rewrite | Removed NestedScrollView + Live/Log tabs → single-scroll CustomScrollView |
| **F2** | Force-refresh + shimmer | Themed loading skeleton, force-refresh on every detail screen open |
| **F4** | Befriend button | New `SendFriendshipSheet` (498 lines) — creature picker → send request |
| **F5** | Remove polling | Killed 30-second poll timer from explore map, SSE handles live updates |
| **F7** | Expandable journal | Journal section tap-to-expand → shows CreatureHistory inline (replaces Log tab) |
| **F8** | Rabble card enhancements | Sort by `last_activity_at`, quick action buttons (Edit/Invite/Add Creature) |
| **F9b** | Creature context banner | "You're here as Luna 🦋" banner in rabble chat, "You're peeking" for non-members |
| **F10** | Profile dark theme | Wrapped in Scaffold with `bg0` background, `bg1` AppBar |
| **F11** | Friendship 500 errors | Fixed NULL `specimen_name` panic, defensive notification handling, better error messages |
| **F12** | Map visual distinction | Larger rabble markers (52px) with name labels, creature state-colored rings (mint/sky/species) |
| **F13** | Map as default | `_mapMode = true` — Environment tab opens to map |
| **F14** | Remove env feed | Deleted `_buildFeedView`, `_filterPill`, `_feedEmptyState`, polling code (~200 lines removed) |
| **F15** | Journals restructure | Tabs: Activity \| Friends \| My Creatures \| Flights (renamed All Bugs, added friend filter, removed Reports) |
| **F16** | WhatsApp chat layout | My messages right-aligned (bg3), others left (bg2), bubble shapes, grouped consecutive messages |
| **F17** | Handle colors | Mine = `RabbleTheme.mint`, others = `RabbleTheme.amber`, system = `violet` |
| **F20** | GPS zoom | `MapController` added, "My Location" tap animates map to GPS position, requests permission if needed |
| **F21** | Host prominence | "👑 You're hosting with Luna ⚓" on hosting cards, "👑 Hosted by @alex" on joined cards |
| **F22** | Rabble description | Shows description below rabble name on cards (2 lines, truncated) |
| — | Cleanup | Deleted `explore_screen_patched.dart`, removed legacy Dashboard from menu + import |

#### ⬜ Remaining Sprint Items (3/22)

| # | Fix | Status |
|---|---|---|
| **F3** | Rabble click-through | Partially done — Chat/Peek buttons on creature hub work. Full tappable row deferred. |
| **F6+F18+F19** | AR panel toggle in split view, Reynolds host-only, ArPanel widget | Designed, documented, next sprint |

#### 📝 Design Notes Captured (for future sprints)

- **Reports** — Rich activity events (flight recap, rabble summary), not a separate tab. Design payload shape in future session.
- **F6 Decision** — Option D confirmed: Map ↔ AR toggle in split panel. ArPanel widget needs extraction from ArViewerScreen.
- **F18** — Reynolds flock dynamics gated to host only. Richer Onto4MAT formations tabled.

### Positive Feedback (protect these)

- ✅ "Through the bug's eyes" viewpoint toggle — loved
- ✅ "Join from map" flow — great UX
- ✅ Journals tabs — look interesting
- ✅ Rabble page structure — fine
- ✅ Map view overall — looking good
- ✅ Split panel (map + flock animation) — great, especially with real-time tracks

---

## Project State Report

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    RABBLE (Flutter PWA)                      │
│                                                             │
│  ┌──────────┬──────────┬─────────────┬──────────┐           │
│  │ 🐾       │ 👥       │ 🌍          │ 📓       │  Bottom   │
│  │Creatures │ Rabbles  │ Environment │ Journals │  Nav      │
│  └──────────┴──────────┴─────────────┴──────────┘           │
│                                                             │
│  Services: ApiClient, AuthService, LocationService,         │
│            TetherService, CreatureStreamService (SSE)        │
│                                                             │
│  Widgets: CreatureLink, CreatureCard, ChatPanel,             │
│           SendFriendshipSheet, FlockViz, MiniMap, SplitPanel │
├─────────────────────────────────────────────────────────────┤
│                    FERMI (Rust/Axum API)                     │
│                                                             │
│  handlers/creatures/                                         │
│    helpers.rs       — ownership check, auto-end flight, H3   │
│    flights.rs       — record, end, fly, plan, export, import │
│    tethering.rs     — tether, untether, telemetry, track     │
│    agent_modules.rs — enemy sensor, genome, prey, dream      │
│    state.rs         — perch, host, join swarm, favourites    │
│    query.rs         — list, detail, activity, flight path    │
│    identity.rs      — mint, transfer, animate, art            │
│    swarms.rs        — create, update, my-rabbles             │
│    collections.rs   — create, list, update                   │
│    devices.rs       — pair, unpair, report                   │
│                                                             │
│  handlers/social.rs — contacts, friendships, follows, feed   │
│  handlers/streams.rs — creature SSE + emit helper            │
│  handlers/dashboard/ — spatial queries, saved locations      │
│  handlers/workspace.rs — chat, files, git, coherence         │
│                                                             │
│  Broadcast channels: workspace(256), rabble(256), creature(512) │
├─────────────────────────────────────────────────────────────┤
│                    NEON (PostgreSQL + PostGIS)               │
│                                                             │
│  95 migrations applied                                       │
│  PostGIS 3.5, pgvector enabled                              │
│  9 users, 53 creatures, 20 swarm_events                     │
│  Social: creature_friendships, contacts, rabble_follows      │
│  Spatial: saved_locations, ST_DWithin queries                │
└─────────────────────────────────────────────────────────────┘
```

### Codebase Metrics

| Metric | Fermi (Backend) | Rabble (Flutter) |
|---|---|---|
| **Language** | Rust | Dart |
| **Total lines** | ~47,000 | ~15,000 |
| **Handler files** | 35 | — |
| **Screen files** | — | 23 |
| **Widget files** | — | 15 |
| **Compiler warnings** | 21 (api-server) | 0 errors |
| **Largest file** | `flights.rs` (1,737) | `explore_screen.dart` (~830) |
| **Migrations** | 95 | — |
| **API endpoints** | ~150 | — |
| **Commits this session** | 15 | 12 |

### Implementation Plan Progress

| Phase | Description | Tasks | Status |
|---|---|---|---|
| **Phase 0** | UX Unblockers | 4/4 | ✅ Complete |
| **Phase 1** | SSE Foundation + Creature Hub | 6/6 | ✅ Complete |
| **Phase 2** | Rabble Pillar | 5/5 | ✅ Complete |
| **Phase 3** | Environment Pillar | 5/7 | ✅ Core done |
| **Phase 4** | Journals Pillar | 6/8 | ✅ Core done |
| **Phase 5** | Polish & Agentic Hooks | 0/5 | ⬜ (notifications + onboarding already exist) |
| **Sprint** | UX Feedback Fixes (F1-F22) | 19/22 | ✅ Near complete |

### Deferred Items

| Item | Why deferred | When |
|---|---|---|
| Environment SSE stream (3.1) | Complex spatial broadcast, creature SSE covers most use cases | Phase 6+ |
| Hex Grid GeoJSON (3.6) | H3 cell rendering, not needed for soft launch | Phase 6+ |
| Flight detail screen with playback (4.6) | Map + scrubber widget, nice-to-have | Next sprint |
| Narrative rabble recap via agent (4.7) | Agent trigger on completion, needs design | Next sprint |
| Waypoint taxonomic lookup (5.3) | GBIF integration, agentic feature | Phase 6+ |
| Flight path sharing (5.4) | Public page + share token | Next sprint |
| AR panel toggle (F6+F19) | Designed, needs ArPanel widget extraction | Next sprint |
| Reynolds host-only gating (F18) | Simple UI gate, Flutter-side only | Next sprint |

### Known Issues

Full details in `docs/DEPLOYMENT_ISSUES_2026_02_20.md` and `docs/SPRINT_UX_FIXES_2026_02_20.md`.

**Resolved this session:**
- ✅ Friendship 500 errors (NULL specimen_name, defensive notifications)
- ✅ Mixed API URL patterns (new methods use `$baseUrl`, old ones relative — works in PWA same-origin)
- ✅ Stale `explore_screen_patched.dart` deleted
- ✅ Legacy dashboard removed from menu
- ✅ Profile page white background → dark themed

**Still open:**
- 🟡 `my/rabbles` response may not include `creator_id` — Flutter fallback to `listSwarms` handles this but loses `my_creatures` data
- 🟡 SSE CORS headers — untested for cross-origin (same-origin PWA works fine)
- 🟡 `Flight` model field access in Journals flights tab — needs verification
- 🟡 Long-press save location may conflict with map pan on mobile
- 🟢 Unused import warnings in some new screens

### Soft Launch Readiness

| Requirement | Status |
|---|---|
| Four-pillar navigation | ✅ |
| Creature collection with state badges | ✅ |
| Creature detail hub (Rabble/Friends/Journal) | ✅ |
| Send friendship requests | ✅ |
| Accept/decline friendship requests | ✅ (existing) |
| Rabble hosting/joining | ✅ (existing) |
| Rabble following with notifications | ✅ |
| Map with creature pins + rabble circles | ✅ |
| "Through creature's eyes" viewpoint | ✅ |
| Join rabble from map | ✅ (existing) |
| Real-time creature tracking (SSE) | ✅ |
| WhatsApp-style chat | ✅ |
| GPS creature tethering | ✅ (existing) |
| AR viewer | ✅ (existing) |
| QR code invite/join | ✅ (existing) |
| Profile management | ✅ |
| Wallet + credits | ✅ (existing) |
| Agent-powered features (enemy sensor, genome, prey) | ✅ (existing) |

**Assessment: Ready for soft launch.** Core social loop (mint creature → join rabble → befriend → chat → track) works end to end, pending verification of the friendship flow with real users.

---

## Commit History (this session)

### Fermi (15 commits)

```
e560c9e build: rebuild Flutter web — creature card rewrite, single scroll, expandable journal
30b8510 build: rebuild Flutter web — rabble cards, host prominence, creature context banner
18dc6e0 build: rebuild Flutter web — map-only explore, journals restructure, GPS zoom, visual distinction
895a9f1 build: rebuild Flutter web — friendship fixes, WhatsApp chat, befriend button, map default
f8c3116 F11: Fix friendship 500 errors — NULL specimen_name, defensive notifications
54c4f1c docs: add F21 (host prominence) + F22 (rabble description), mark F10+F16+F17 done
1b99d0b docs: F6 decision + F18-F20 — AR panel toggle, Reynolds host-only, GPS zoom
6f5d6f0 docs: add F9 (feed polling→SSE + rabble creature context) + F10 (profile theming)
c090356 docs: add F11 (friendship 500s — SOFT LAUNCH BLOCKER), F12 + F13
511a797 docs: add F14-F17 — remove env feed, journals restructure, WhatsApp chat, handle colors
820ae8f docs: sprint UX fixes — F1-F8 decomposed from owner feedback
f8c11f4 docs: deployment issues + session context for 2026-02-20
93d26dd build: rebuild Flutter web — four-pillar UX, SSE, creature hub, rabble follows
3f8a0bb Phase 4 Tasks 4.3+4.5: Per-creature activity + flight path GeoJSON
46d03e8 Phase 3 Tasks 3.4+3.7: Saved locations + nearby creatures
0783914 Phase 2 Task 2.5: Rabble move with lat/lng + follower notifications
527d786 Phase 2 Task 2.2: Rabble follows — migration, endpoints, follower notifications
```

### Rabble (12 commits)

```
e11574f Sprint batch 5: F1+F2+F7 — creature card rewrite (single scroll, expandable journal, no tabs)
6e972d3 Sprint batch 4: F8+F9b+F21+F22 — rabble cards, host prominence, description, creature context
a8f4296 Sprint batch 3: F12+F14+F15+F20 — map-only explore, journals restructure, visual distinction, GPS zoom
0c61d6a Sprint batch 2: F4 (befriend button) + F11 fixes
2086395 Sprint batch 1: F5+F10+F13+F14+F16+F17 + cleanup
edf4d18 Phase 4 Tasks 4.1-4.4+4.8: Journals with SSE activity, reports, all bugs
6f08d87 Phase 3 Tasks 3.2+3.3+3.5: Explore map with viewpoint toggle, live tracking, saved locations, AR FAB
82f4368 Phase 2 Task 2.4: Rabble detail with real-time map tracking + members + follow
f2c3b6a Phase 2 Tasks 2.1+2.3: Rabbles tab with CreatureLink avatars + Following tab
a538e4e Phase 1 Tasks 1.5+1.6: Creature SSE integration + CreatureLink widget
07967cd Phase 1 Tasks 1.3+1.4: Creature Hub Card + enriched collection grid
4ead2ff Phase 1 Task 1.2: Four-pillar bottom nav restructure
```

---

## Key Files Reference

| File | What it is |
|------|-----------|
| **DOCS** | |
| `docs/SPRINT_UX_FIXES_2026_02_20.md` | Sprint backlog — 22 items with analysis, priority, file map |
| `docs/DEPLOYMENT_ISSUES_2026_02_20.md` | 11 deployment issues identified post-deploy |
| `docs/IMPLEMENTATION_PLAN.md` | 5-week phased plan (875 lines) |
| `docs/UX_AUDIT.md` | Full UX audit (851 lines) |
| **BACKEND** | |
| `src/handlers/streams.rs` | Creature SSE stream + emit helper |
| `src/handlers/creatures/helpers.rs` | Shared creature utilities (ownership, flight, state) |
| `src/handlers/creatures/flights.rs` | Flight lifecycle handlers |
| `src/handlers/creatures/tethering.rs` | Tethering + telemetry handlers |
| `src/handlers/creatures/agent_modules.rs` | Premium agent feature handlers |
| `src/handlers/creatures/state.rs` | Location + rabble handlers |
| `src/handlers/creatures/query.rs` | Read endpoints + per-creature activity + flight path GeoJSON |
| `src/handlers/social.rs` | Contacts, friendships, follows, feed, notifications |
| `src/handlers/dashboard/mod.rs` | Spatial queries + saved locations + nearby creatures |
| `migrations/094_rabble_follows.sql` | Follow table + notification functions |
| `migrations/095_saved_locations.sql` | Saved locations + nearby creatures (PostGIS) |
| **FLUTTER** | |
| `rabble/lib/screens/home_shell.dart` | Four-pillar bottom nav + account menu |
| `rabble/lib/screens/rabbles_screen.dart` | Hosting/Joined/Following tabs with enhanced cards |
| `rabble/lib/screens/journals_screen.dart` | Activity/Friends/My Creatures/Flights + SSE |
| `rabble/lib/screens/explore_screen.dart` | Map-only view with viewpoint toggle, GPS zoom, live tracking |
| `rabble/lib/screens/rabble_chat.dart` | Enhanced with members, follow, creature context banner |
| `rabble/lib/screens/creature/creature_screen.dart` | Single-scroll hub with expandable journal |
| `rabble/lib/screens/profile_screen.dart` | Dark-themed profile |
| `rabble/lib/services/creature_stream_service.dart` | SSE client with reconnection + backfill |
| `rabble/lib/widgets/creature_link.dart` | Universal tappable creature reference (4 styles) |
| `rabble/lib/widgets/send_friendship_sheet.dart` | Creature picker → send friend request |
| `rabble/lib/widgets/chat_panel.dart` | WhatsApp-style chat with handle colors |
| `rabble/lib/models/creature.dart` | Enriched with social + active_flight fields |

---

## What's Next

### Immediate — Verify soft launch readiness
1. Test friendship flow end-to-end with a real second user
2. Verify `my/rabbles` response shape (may need `creator_id` fix)
3. Check SSE creature streams work in production (CORS)

### Next sprint — Remaining polish
1. **F6+F18+F19** — AR panel toggle in rabble split view, Reynolds host-only
2. **Flight detail screen (4.6)** — Map + playback scrubber for flight paths
3. **Flight path sharing (5.4)** — Public share page with GeoJSON map
4. **Rabble settings sheet** — Edit name, description, visibility, radius (backend ready)
5. **Invite + Add Creature from rabble card** — Extract shared widgets from rabble_chat

### Later — Agentic features
- Waypoint taxonomic lookup (GBIF)
- Narrative rabble recap via agent
- Environment SSE stream
- Hex grid dynamic boundaries
- Predictive flight suggestions
- Cross-creature knowledge synthesis

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

# 4. Sprint backlog
cat /home/ilabra/fermi/docs/SPRINT_UX_FIXES_2026_02_20.md | grep "^###\|^| \*\*F"

# 5. Build + deploy cycle
cd /home/ilabra/rabble
/home/ilabra/flutter/bin/flutter build web --release
rm -rf /home/ilabra/fermi/rabble-web/* && cp -r build/web/* /home/ilabra/fermi/rabble-web/
cd /home/ilabra/fermi && git add -A && git commit -m "build: ..." && git push origin main
```
