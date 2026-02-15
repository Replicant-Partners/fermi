# Debug Checklists

Post-wipe testing checklists for Rabble. DB state: 8 users, 299 agents, 0 creatures/wallets/teams.

**Note**: First login after wipe will auto-create wallet (100cr signup grant) and personal workspace (menagerie). Test with a user that already exists in the users table.

---

## Checklist 0: Auth & Wallet Bootstrap

- [ ] `GET /api/health` — 200
- [ ] `GET /api/auth/me` — returns user JSON (cookie auth)
- [ ] Wallet auto-created on first authenticated request — `GET /api/wallet` returns `balance: 100`
- [ ] Credit ledger has 1 entry: `grant`, 100cr
- [ ] `GET /api/billing/tiers` — returns 4 tiers (Starter 250/$5, Explorer 750/$12, Keeper 2000/$25, Breeder 5000/$50)
- [ ] `POST /api/billing/dev-topup` — adds credits (dev only)

---

## Checklist 1: Creature Lifecycle

### Mint
- [ ] `POST /api/creatures/mint` with `{ species, lat, lng }` — 201, charges 3cr
- [ ] Creature appears in `GET /api/creatures?owner=me`
- [ ] GBIF species lookup returns field notes + scientific name
- [ ] Wallet balance: 97cr (100 - 3)
- [ ] `creature_state` row created with `state: 'idle'`

### Perch
- [ ] `POST /api/creatures/:id/perch` with `{ lat, lng }` — 200, charges 2cr
- [ ] Creates `swarm_events` row (rabble)
- [ ] `creature_state.rabble_id` set
- [ ] `creature_state.h3_cell` computed
- [ ] Wallet balance: 95cr

### Art Generation
- [ ] `POST /api/creatures/:id/art` — 200, charges 5cr
- [ ] `creature_images` row created
- [ ] `GET /api/creatures/:id/image` returns image data

---

## Checklist 2: Movement

### Hop (1cr, no agent)
- [ ] `POST /api/creatures/:id/fly` with `{ lat, lng }` (no `prompt`) — 200, charges 1cr
- [ ] `creature_flights` row created with pattern `fly`
- [ ] `creature_state` location updated
- [ ] Gas tx_type: `fly`

### Expedition (5cr, with agent)
- [ ] `POST /api/creatures/:id/fly` with `{ lat, lng, prompt: "Fly to the park" }` — 200, charges 5cr
- [ ] `creature_flights` row created with pattern `expedition`
- [ ] Agent dispatch fires (flight_coordinator)
- [ ] Gas tx_type: `expedition`

### Tether
- [ ] `POST /api/creatures/:id/tether` — 200, charges 1cr
- [ ] `creature_tethers` row created
- [ ] `DELETE /api/creatures/:id/tether` — 200, ends tether

### Flight Record (legacy)
- [ ] `POST /api/creatures/:id/flights/record` with waypoints — 200
- [ ] `POST /api/creatures/:id/flights/:fid/end` — 200

---

## Checklist 3: Social

### Create Swarm (Rabble)
- [ ] Perch auto-creates swarm (tested in Checklist 1)
- [ ] `GET /api/swarms` — lists all swarms
- [ ] `GET /api/swarms/:id` — swarm detail with creatures

### Join Rabble
- [ ] Mint + perch a second creature at different location
- [ ] `POST /api/creatures/:id/join` with `{ swarm_id }` — 200, charges 0-2cr
- [ ] Creature's `creature_state.rabble_id` updated to target swarm
- [ ] Creature appears in swarm member list

### Rabble Chat
- [ ] `POST /api/rabble/:swarm_id/messages` with `{ creature_id, content }` — 200, charges 1cr
- [ ] Message appears in `GET /api/rabble/:swarm_id/messages`
- [ ] Agent-mediated response generated (workspace message)

### Gift (free)
- [ ] `POST /api/creatures/:id/transfer` with `{ to_user_id }` — 200, free
- [ ] Creature ownership changes
- [ ] Transfer logged in `creature_versions`

---

## Checklist 4: Sensors

### Enemy Sensor
- [ ] **Enable**: `POST /api/creatures/:id/enemy-sensor` `{ "action": "enable" }` — 200, charges 5cr
- [ ] `creature_conditions.active_modules` includes `enemy_sensor`
- [ ] **Scan**: `POST /api/creatures/:id/enemy-sensor` `{ "action": "check" }` — 200, charges 1cr
- [ ] Returns `{ threat_level, assessment, nearby_count }`
- [ ] `creature_versions` has `enemy_scan` transition
- [ ] **Strategy**: `POST /api/creatures/:id/enemy-sensor` `{ "action": "strategy", "prompt": "How to defend?" }` — 200, charges 1cr
- [ ] Returns interactive suggestions
- [ ] **Disable**: `POST /api/creatures/:id/enemy-sensor` `{ "action": "disable" }` — 200, free
- [ ] **Edge**: Check without enabling — 400

### Prey Locator
- [ ] **Enable**: `POST /api/creatures/:id/prey-locator` `{ "action": "enable" }` — 200, charges 5cr
- [ ] **Scan**: `POST /api/creatures/:id/prey-locator` `{ "action": "scan" }` — 200, charges 2cr
- [ ] Returns nearby prey list
- [ ] **Stalk**: `POST /api/creatures/:id/prey-locator` `{ "action": "stalk", "target_creature_id": "..." }` — 200, charges 5cr
- [ ] Returns flight plan to target
- [ ] **Strategy**: `POST /api/creatures/:id/prey-locator` `{ "action": "strategy", "prompt": "..." }` — 200, charges 2cr
- [ ] **Edge**: Scan with no nearby creatures — returns empty list

### Genome Profiler
- [ ] **Enable**: `POST /api/creatures/:id/genome-profiler` `{ "action": "enable" }` — 200, charges 5cr
- [ ] **Profile**: `POST /api/creatures/:id/genome-profiler` `{ "action": "profile" }` — 200, charges 2cr
- [ ] Returns phylogenetic data
- [ ] `creature_versions` has `genome_profile` transition
- [ ] **Cached**: Second profile call returns cached result (still charges? check)
- [ ] **Edge**: Profile without enabling — 400

---

## Checklist 5: Dreaming

- [ ] Creature must have at least one workspace message (chat first)
- [ ] `POST /api/creatures/:id/dream` — 200, charges 5cr
- [ ] Dream narrative generated by dream_narrator agent
- [ ] `creature_versions` has `dream` transition
- [ ] 1hr cooldown enforced — second dream within 1hr returns error
- [ ] **Daily bonus**: First dream of the day per user — 0cr charged
- [ ] Response includes `"dream_bonus": true` and `"cost": 0`
- [ ] Second dream same day charges 5cr, `"dream_bonus": false`

---

## Checklist 6: Leveling & Specialization

- [ ] `GET /api/creatures/:id/level` — returns level, score, specialization
- [ ] Fresh creature (1 mint) — Level 0 or 1
- [ ] After several actions (fly, chat, scan) — level increases
- [ ] Specialization reflects dominant activity type
- [ ] Growth bars show per-dimension breakdown

---

## Checklist 7: Workspaces

- [ ] Personal workspace (menagerie) created on first login
- [ ] `GET /api/workspaces/personal` — returns workspace
- [ ] Swarm auto-creates workspace with system agents
- [ ] `GET /api/teams` — lists user's workspaces
- [ ] Workspace messages appear after agent interactions (chat, sensor, dream)

---

## Checklist 8: Economy

### Wallet
- [ ] `GET /api/wallet` — balance, granted_balance, purchased_balance
- [ ] `GET /api/wallet/transactions` — full ledger history
- [ ] Each action creates a `credit_ledger` entry with correct tx_type

### Gas Fees (verify amounts)
| Action | Expected Cost |
|--------|--------------|
| Mint | 3cr |
| Perch | 2cr |
| Hop | 1cr |
| Expedition | 5cr |
| Tether | 1cr |
| Chat | 1cr |
| Art | 5cr |
| Enemy enable | 5cr |
| Enemy scan | 1cr |
| Prey enable | 5cr |
| Prey scan | 2cr |
| Prey stalk | 5cr |
| Genome enable | 5cr |
| Genome profile | 2cr |
| Dream | 5cr (first daily free) |
| Transfer | free |

### Insufficient Funds
- [ ] Action with 0 balance — returns 402 or appropriate error
- [ ] Error message mentions insufficient credits

---

## Checklist 9: Creature State Machine

### Presence
- [ ] Default presence: `active`
- [ ] `PUT /api/creatures/:id/presence` `{ "presence": "sleeping" }` — 200
- [ ] Sleeping creature cannot: fly, join, chat — returns 409
- [ ] `PUT /api/creatures/:id/presence` `{ "presence": "active" }` — restores actions

### Visibility
- [ ] `PUT /api/creatures/:id/visibility` `{ "visibility": "private" }` — 200
- [ ] Private creatures excluded from enemy/prey scans by other users
- [ ] Public by default

### Status
- [ ] `PUT /api/creatures/:id/status` — update creature status text

---

## Checklist 10: History / Log

Every action should create a `creature_versions` entry. Verify these event types appear:

- [ ] `mint` — on creature creation
- [ ] `fly` / `expedition` — on hop/expedition
- [ ] `join` — on swarm join
- [ ] `dream` — on dream
- [ ] `enemy_scan` — on enemy sensor check
- [ ] `enemy_strategy` — on enemy follow-up
- [ ] `prey_scan` — on prey locator scan
- [ ] `prey_stalk` — on prey stalk
- [ ] `prey_strategy` — on prey follow-up
- [ ] `genome_profile` — on genome profile
- [ ] `transfer` — on creature gift

Check: `GET /api/creatures/:id/versions` — returns all transitions in order

---

## Checklist 11: Flutter Client (rabble.world)

### Creature Screen
- [ ] Creature hero renders with brain icon (CognitivePill)
- [ ] Tapping brain shows level breakdown sheet
- [ ] Action chips show cost labels: Hop (1cr), Expedition (5cr), etc.
- [ ] Sensor pills: Hunt (2cr), Scan (1cr), Genome (2cr)
- [ ] "List" chip shows marketplace coming soon dialog
- [ ] Log tab shows all events with correct icons and colors

### Chat Panel
- [ ] Messages render newest at bottom
- [ ] Sending a message charges 1cr and shows agent response

### Spatial View
- [ ] Constellation/radar scene renders (no camera)
- [ ] Nearby creatures show with distance labels
- [ ] Compass rose and heading parallax work

### Dream
- [ ] Dream dialog shows "Cost: 5cr (first dream each day is free)"
- [ ] Result sheet shows DAILY DREAM badge on first dream
- [ ] Wellness hint appears

---

## Quick Smoke Test (5 minutes)

1. [ ] Login → wallet shows 100cr
2. [ ] Mint a creature → 97cr
3. [ ] Perch it → 95cr
4. [ ] Hop to new location → 94cr
5. [ ] Enable enemy sensor → 89cr
6. [ ] Scan for enemies → 88cr
7. [ ] Dream → 88cr (daily bonus, free)
8. [ ] Check level → Level 1+
9. [ ] Check creature log → shows all events
10. [ ] Check wallet transactions → shows all charges
