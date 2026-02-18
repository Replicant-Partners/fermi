# Session Context — 2026-02-18

> **Purpose:** Everything needed to resume work in the next session.
> **Last commit:** `f59b442` — "Phase 0: UX unblockers — all 17 checks passing"
> **Branch:** `main` (1 commit ahead of `origin/main`)

---

## What Happened This Session

### 1. UX Audit Completed

We performed a comprehensive UX audit of the Rabble mobile app (Flutter PWA) and the Agent Bestiary web platform. The audit is driven by a four-pillar UX decomposition from `docs/ux-notes.md`:

- 🐾 **Creatures** — Collection of creature cards (the anchor object)
- 👥 **Rabbles** — Hosted / Joined / Following
- 🌍 **Environment** — Explore (my locations, favourites, hex grid) + Discover (nearby, location source)
- 📓 **Journals** — Logs, reports, flight path reconstructions, activity

**Core insight:** The Creature Detail Card must be the **universal hub** — always showing the most recent state from all four pillars. Every creature reference in the entire app should be a tappable link to this hub. This resolves the cognitive fragmentation where users have to mentally assemble creature state from 4-5 disconnected screens.

**Output:** `docs/UX_AUDIT.md` (851 lines) — full audit with backend readiness matrix, gap analysis, design system debt assessment.

### 2. Ten Design Decisions Locked In

The owner answered all 10 clarifying questions inline in the audit doc. Key decisions:

| # | Decision |
|---|----------|
| D1 | Four pillars = four bottom nav tabs in Flutter |
| D2 | Creature Detail reachable from everywhere; creature carries its affordances |
| D3 | Following rabbles = **active notifications** (not passive bookmarks) |
| D4 | Peek does NOT auto-follow |
| D5 | Dual viewpoint: "My Location" (GPS) + "Through [Creature]'s Eyes" |
| D6 | Hex grid for both rabble placement AND spatial awareness; dynamic boundary moves with anchor creature |
| D7 | Rich flight path reports with species-you-might-have-met + plan for agentic waypoint hooks |
| D8 | Flight paths shareable/exportable |
| D9 | **SSE push everywhere** — reuse chat SSE pattern, no polling |
| D10 | **Flutter first** — Flutter PWA IS the web experience |

**Output:** `docs/IMPLEMENTATION_PLAN.md` (875 lines) — 5-week phased plan with SSE architecture, per-task effort estimates, risk register.

### 3. Phase 0 Executed — All Unblockers Resolved

Ran the verification script against production (Neon PostgreSQL). Found and fixed:

| Issue | Resolution |
|-------|-----------|
| **PostGIS not installed** | `CREATE EXTENSION postgis` — now v3.5 |
| **Migration 089 not applied** (dashboard spatial functions) | Applied after PostGIS was enabled |
| **Migration 091 failing** (swarm_participants) | Root cause: `users.user_id` lacked a UNIQUE constraint needed for FK. Created migration 093 to add it, then 091 applied cleanly. |
| **Migration 090 (social layer)** | Already applied — all 4 tables, 1 column, 5 SQL functions present |
| **Notification column mismatch** | Already fixed in code. All 8 INSERT statements use correct columns (`type`, `message`). Migration 092 self-heals any DB-level stale columns. |

**Final result: 17/17 verification checks passing.**

**Output:**
- `scripts/phase0-run.sh` — reusable verification script with `--fix` mode
- `scripts/phase0-verify.sql` — pure SQL alternative
- `migrations/093_users_user_id_unique.sql` — UNIQUE constraint on `users.user_id`

---

## Current State of the System

### Database (Production — Neon)

```
PostgreSQL with PostGIS 3.5, pgvector
9 users, 53 creatures, 20 swarm_events, 4 notifications
All social tables present (creature_friendships, creature_invites, activity_events, rabble_co_presence, swarm_participants)
All social SQL functions present (5 functions)
Dashboard spatial function present (get_my_rabbles_with_status)
```

### Backend (Rust/Axum)

- **Compiles clean** (warnings only, no errors)
- Migrations run on server startup (`api_server.rs:run_migrations`)
- Migration order fixed: 093 runs before 091 to ensure FK constraint exists
- Notification handlers all use correct column names
- Rich creature detail endpoint returns: `social`, `active_flight`, `creature_state`, `rabble_id`, `rabble_name`, `cognition_level`
- `GET /api/my/rabbles` returns hosting/participating split with `my_creatures` array per rabble
- SSE exists for chat (`workspace stream`) and activity feed (`feed/stream`) — needs extension for creature + environment streams

### Flutter (Separate Repo)

- Source is **NOT** in the `fermi` repo — only compiled output in `rabble-web/`
- Previous commit `94f4447` mentions "rebuild Flutter web with creature picker ownerId fixes" — the Flutter fix for task 0.4 may already be done in the Flutter repo
- The Flutter app does NOT yet render the enriched creature detail data
- The Flutter app does NOT yet call `GET /api/my/rabbles`

### Git Status

```
Branch: main
Ahead of origin/main by 1 commit (f59b442)
Needs: git push origin main
```

---

## What's Next: Phase 1

**Phase 1 — SSE Foundation + Creature Hub (5-7 days)**

This is the next block of work. It has two tracks:

### Track A: Backend (Rust) — Creature SSE Stream

**Task 1.1** — Create `GET /api/creatures/:id/stream` endpoint

File to create: `src/handlers/streams.rs`

Architecture:
- Add `creature_broadcast: broadcast::Sender<CreatureEvent>` to `AppState` in `api_server.rs`
- `CreatureEvent { creature_id: Uuid, event_type: String, payload: Value }`
- SSE handler subscribes to broadcast, filters by creature_id
- Auth required: must own creature or be in same rabble

Wire broadcast sends into existing mutation handlers:
- `src/handlers/creatures/state.rs` — fly, perch, tether, untether, end_flight, push_telemetry, join_swarm
- `src/handlers/social.rs` — creature friendship accept
- `src/handlers/creatures/identity.rs` — transfer

Event types: `state_changed`, `location_update`, `flight_started`, `flight_ended`, `friend_request`, `friend_accepted`, `entered_rabble`, `left_rabble`

Reference the existing SSE patterns:
- `src/handlers/workspace.rs` — workspace chat SSE
- `src/handlers/social.rs` — activity feed SSE (`activity_feed_stream_handler`)

### Track B: Flutter — Bottom Nav + Creature Hub

**Task 1.2** — Restructure bottom nav to four pillar tabs
**Task 1.3** — Creature collection grid (🐾 tab root screen)
**Task 1.4** — Creature Detail Hub Card (the centrepiece)
**Task 1.5** — SSE integration on creature detail
**Task 1.6** — Universal `CreatureLink` widget

The creature detail card layout is fully specified in `docs/UX_AUDIT.md` section 7 and `docs/IMPLEMENTATION_PLAN.md` task 1.4.

---

## Key Files Reference

| File | What it is |
|------|-----------|
| `docs/UX_AUDIT.md` | Full UX audit with backend readiness matrix (851 lines) |
| `docs/IMPLEMENTATION_PLAN.md` | 5-week phased plan with SSE architecture (875 lines) |
| `docs/ux-notes.md` | Original four-pillar decomposition (Mermaid flowchart) |
| `docs/ux-social-improvements.md` | Social features gap analysis (386 lines) |
| `scripts/phase0-run.sh` | Production verification script (rerun anytime) |
| `src/api_server.rs` | Route registration, AppState, migration runner |
| `src/handlers/creatures/mod.rs` | Creature handler re-exports |
| `src/handlers/creatures/query.rs` | Creature list + detail endpoints |
| `src/handlers/creatures/state.rs` | Fly, perch, tether, track, join-swarm handlers |
| `src/handlers/creatures/swarms.rs` | My-rabbles, create/update swarm |
| `src/handlers/social.rs` | Contacts, friendships, invites, activity feed + SSE |
| `src/handlers/dashboard/mod.rs` | Nearby rabbles, spatial dashboard queries |
| `static/js/widgets/` | Well-factored web widget system (nav, toast, modal, etc.) |
| `templates/workspace.html` | 3,378-line monolith (future cleanup, not priority) |

---

## Open Questions / Decisions for Next Session

1. **Push the commit** — `git push origin main` hasn't been done yet
2. **Verify Flutter repo status** — commit `94f4447` suggests the creature picker ownerId fix may already be in the Flutter source. Confirm before duplicating work.
3. **Start Phase 1 Track A or Track B first?** — Backend SSE (Track A) can proceed independently in this repo. Flutter work (Track B) requires the Flutter repo. Recommendation: start Track A here.
4. **SSE broadcast channel sizing** — The existing workspace broadcast uses `broadcast::channel(100)`. For creature streams with frequent telemetry, may need larger buffer or consider using `watch` channels for latest-state-only scenarios.

---

## How to Resume

```bash
cd /home/ilabra/fermi

# Verify you're on the right commit
git log --oneline -3

# Verify prod is still healthy
export $(grep -v '^#' .env | xargs) && bash scripts/phase0-run.sh

# Start Phase 1 Track A: create creature SSE stream
# See docs/IMPLEMENTATION_PLAN.md "Phase 1" section for full spec
```
