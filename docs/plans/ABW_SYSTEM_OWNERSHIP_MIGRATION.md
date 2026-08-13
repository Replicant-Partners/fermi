# Migration Plan — `abw-system` agent ownership (P5)

Status: **planned, not scheduled** · Executes
`docs/specs/AGENT_CREDENTIAL_MODEL.md` §8 P5 · Owner: Ivan

> Run this when the platform is otherwise quiet. Nothing here is urgent;
> everything here is reversible.

## 0. The one-paragraph version

20 agents that are ABW *substrate* are currently owned by
`ivan@axolotl.partners` because the seeder assigns every card to "the
earliest admin". This moves them to the `abw-system` principal, which
already exists and already holds the platform's provider keys. It is a
**one-column change** (`agents.user_id`) plus a card field so it never
drifts back. Rabble agents are explicitly out of scope — they are a
tenant app, not substrate.

---

## 1. Why this is a low-risk change

The property that makes it safe, and that must be re-verified before
starting:

> **Credential resolution does not consult ownership.**

`funding_principal_for` (`src/api_server.rs:5196`) routes on
`is_platform_funded(agent.tier)`
(`src/agent_backend/credentials.rs:179`), which is `tier == "system" ||
tier == "curated"`. It returns the literal `"abw-system"`. Ownership is
not read. So moving `owner_id` cannot unfund an agent.

This is exactly the failure that took down the Fermi orchestra when
SPEC_28 shipped (78 curated agents hard-failing `Unfunded`), so it is
worth re-confirming rather than trusting this document.

Second safety property:

> **The re-seeder will not undo the change.**

`agent-bestiary/memory/src/store.rs:479` upserts with
`user_id = COALESCE(agents.user_id, EXCLUDED.user_id)` — an existing
non-null owner survives. Note the comment at `src/api_server.rs:4591-4596`
claims the opposite ("they will be reassigned to the admin on re-seed").
**That comment is wrong** and should be corrected in Phase 1, because it
would otherwise talk the next person out of this migration.

---

## 2. Scope: three buckets

The test: **does the platform invoke it, or does a user hire it?**

### Bucket A → `abw-system` (20 agents)

| Group | Agents | tier today |
|---|---|---|
| Navigation / meta | `xaman_ek`, `fermi` | system |
| Loop 1 — dreaming | `ontologist`, `dream_coordinator` | system |
| | `dream_narrator` | curated |
| Loop 3 — coherence | `coherence_evaluator`, `intention_coordinator` | system |
| | `cohere_and_coordinate` | curated |
| Observability | `observability_coordinator`, `eval_runner`, `anomaly_triager`, `dyad_observer` | curated |
| Platform-invoked | `observation_analyst`, `swarm_coordinator` | curated |
| Coordination strategies | `debate_strategist`, `moe_router_strategist`, `pipeline_strategist`, `vote_strategist` | curated |
| Billing | `stripe_billing` | system |
| Superseded | `coherence_consultant` | curated |

`observation_analyst` and `swarm_coordinator` already carry
`// SPEC_28 — platform-service agent` in the code that invokes them
(`src/handlers/observations.rs:659`, `src/handlers/swarm_telemetry.rs:438`).

### Bucket B → **stays put, this migration does not touch it**

Rabble is a tenant app of ABW. Its agents are app content, not
substrate. Eight of them carry `tier: system` today, which is a
*funding* statement, not an ownership one — leaving them alone keeps
their behaviour identical:

`keeper`, `naturalist`, `navigator`, `swarm_host`, `flight_coordinator`,
`reynolds_flock`, `rabble_anchor_manager`, `rabble_lifecycle_coordinator`
— plus the wider Rabble surface (`rabble_curator`, `ar_*`, `wild_*`,
`specimen_minter`, `species_resolver`, `wing_segmenter`, …).

Giving Rabble its own `app-rabble` principal is a **separate exercise**
needing its own inventory (the surface is ~25 agents, not 8). Do not
bundle it. Noted as Phase 5.

### Bucket C → stays with `ivan@axolotl.partners`

Everything else. Hireable curated products: `performance_coach`,
`publish_coach`, `stripe_connect_advisor`, the football/simops/biotech
research agents, etc. This is the "layer above the system".

---

## 3. What actually changes

Verified by reading each call site, not assumed.

| Behaviour | Site | Effect |
|---|---|---|
| Credential resolution | `credentials.rs:179` | **none** — tier-based |
| Re-seed | `store.rs:479` | **none** — `COALESCE` preserves |
| Eval access | `eval.rs:24`, `eval.rs:345` | **none** — keyed on `tier == "curated"` |
| Workspace hire gate | `messages.rs:1205` | **none** — keyed on tier |
| Observatory gate | `observatory.rs:70` | **none** — the `owner_id.is_none()` branch is already dead (owners are non-null today) |
| Admin access | `rbac.rs:73` | **none** — `can_admin()` bypass unaffected |
| **Royalties** | `gas.rs:274-285` | ⚠️ **changes for 13 agents** — see below |
| `/hire` on own agent | `messages.rs:1195` | Ivan can now `/hire` these instead of being told "use /add" — an improvement |
| Ownership audit buckets | `admin.rs:1175` | 20 agents move `mine` → `others` (cosmetic) |

### The one real consequence: royalties

`gas.rs:280` skips royalty when `tier == "system"`. The 7 Bucket A
agents already at `tier: system` are unaffected. The **13 at
`tier: curated`** currently pay a royalty into Ivan's wallet on every
third-party execution; after the move that flows to `abw-system`'s
wallet instead.

That is arguably the correct end state, but it is real money movement
and must be a deliberate decision, not a surprise. Two options:

- **(a) Accept it** — platform substrate earns to the platform. Simplest.
- **(b) Align tier in Phase 3** — set those 13 to `tier: system`, which
  stops the royalty entirely and also hides them from the marketplace
  (`crates/fermi-console/src/main.rs:21179` filters `tier == "system"`).
  Correct for infrastructure that is never hired, but it is a *second*
  variable — keep it in its own phase so rollback stays clean.

Recommendation: (a) now, (b) later once (a) has settled.

---

## 4. Phases

### Phase 0 — Pre-flight (read-only, ~10 min)

```sql
-- 0.1 The principal exists and is intact (mig 171 + 181).
SELECT user_id, email, role, auth_provider FROM users WHERE user_id = 'abw-system';

-- 0.2 It holds the platform keys.
SELECT provider, scope, label FROM agent_credentials
 WHERE principal_id = 'abw-system' ORDER BY provider, scope;

-- 0.3 Current owner of the roster + royalty exposure.
SELECT tier, user_id, COUNT(*)
  FROM agents
 WHERE agent_name IN ( /* §2 Bucket A list */ )
 GROUP BY tier, user_id ORDER BY tier;

-- 0.4 Baseline: nothing already orphaned.
SELECT * FROM rbac_orphans;   -- mig 163; expect zero rows
```

Prefer `GET /api/admin/rbac/orphans` (`admin_rbac.rs:270`) over the raw
view — a past integrity audit flagged the view as possibly absent, and
the endpoint fails loudly if it is.

Also run `GET /api/admin/agent-ownership-audit` and keep the JSON — it
is the before-picture.

**Abort if:** `abw-system` is missing (mig 181 hasn't run), or
`rbac_orphans` is non-empty (fix that first; don't add to it).

### Phase 1 — Make it stick (code, no data change)

Ship these together, *before* touching data, so the migration can't be
undone by a deploy:

1. **Card field.** Add optional `owning_principal: Option<String>` to
   `AgentCard` (`src/agent_backend/agent_card.rs`). Set
   `"owning_principal": "abw-system"` in the 20 Bucket A cards.
2. **Seeder honours it** — `src/api_server.rs:4597`:
   ```rust
   owner_id: card.owning_principal.clone().or_else(|| admin_user_id.clone()),
   ```
   This is the durable fix: new substrate agents get the right owner at
   birth, so this migration never has to be repeated.
3. **Fix the stale comment** at `src/api_server.rs:4591-4596`.
4. **Invariant test**, in the style of `test_system_agents_have_system_tier`
   (`agent_card.rs:1066`): every card declaring `owning_principal` must
   name a known principal, and every card tagged `platform-service` must
   declare one. Note the naive invariant `tier == 'system' ⟺ owner ==
   'abw-system'` is **false** by design — Rabble holds `tier: system`
   with a different owner. Assert against the roster, not the tier.

Deploy. Confirm the seeder runs clean and no ownership changed yet
(`COALESCE` means existing non-null owners are untouched).

### Phase 2 — Move the data

Two options; prefer the first.

**Option 1 — existing endpoint** (no migration file, already audited):

```bash
curl -X POST "$API/api/admin/agent-ownership-reassign" \
  -H "Authorization: Bearer $IVAN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"agent_names":["xaman_ek","fermi", ...],"new_owner_user_id":"abw-system"}'
```

It validates that the target exists in `users` before updating
(`admin.rs:1287`) and returns per-agent `updated | not_found` — check
for any `not_found`, which means a name typo.

**Option 2 — migration file** `190_abw_system_ownership.sql`, if you'd
rather it be replayable. Must back up first:

```sql
CREATE TABLE IF NOT EXISTS agent_ownership_backup_p5 AS
SELECT agent_name, user_id, tier, NOW() AS captured_at
  FROM agents WHERE agent_name IN ( /* roster */ );

UPDATE agents SET user_id = 'abw-system'
 WHERE agent_name IN ( /* roster */ );
```

Either way, capture the backup table — it makes rollback exact rather
than reconstructed.

### Phase 3 — Verify

```sql
-- 3.1 Roster moved, nothing else did.
SELECT user_id, COUNT(*) FROM agents GROUP BY user_id ORDER BY 2 DESC;

-- 3.2 No orphans introduced (FK from mig 162 should have prevented it).
SELECT * FROM rbac_orphans;   -- or GET /api/admin/rbac/orphans
```

Functional smoke — these exercise the paths that actually matter:

- `xaman_ek` answers (`POST /api/xaman/ask`) → proves system-agent
  execution still resolves credentials.
- A workspace message routing to `cohere_and_coordinate` → proves the
  default strategist still runs.
- A dreaming/consolidation cycle → proves `ontologist` +
  `dream_coordinator` still run.
- `GET /api/admin/agent-ownership-audit` → 20 agents now under `others`.
- Execute one Bucket A curated agent as a **non-Ivan** account and check
  `credit_transactions` — confirms the royalty now lands in
  `abw-system`'s wallet (the expected change from §3).

### Phase 4 — Administration (do not skip)

Two *different* problems here, and they need different mechanisms.
Conflating them is the mistake an earlier draft of this plan made.

#### 4a. Agent administration — object-scoped, works today

1. Create a `platform-operators` team.
2. Grant it `Permission::Admin` on the Bucket A agents via
   `object_shares` (`POST /api/shares`). `rbac::require` already honours
   team grants for `ObjectType::Agent` (`fermi-auth/src/rbac.rs:16`).

Attributable per person and revocable. **Do not** use the "view as
user" feature — `abw-system` is on its deny-list
(`src/handlers/impersonation.rs`) precisely because impersonating a
principal that is `role='admin'` and holds the platform's keys would be
a privilege-escalation path, not an administration path.

#### 4b. Principal administration — keys and wallet: **NOT POSSIBLE TODAY**

Keys live in `agent_credentials` keyed by `principal_id`; revenue lands
in `wallets` keyed by `owner_id`. Neither is an ACL'd *object*, so
`object_shares` does not reach them. Current reality:

| Need | Today | Severity |
|---|---|---|
| List `abw-system` keys | no endpoint; SQL only | blocks ops |
| **Rotate** a key | **impossible without SQL** — `bootstrap_agent_credential_if_absent` (`secrets.rs:361`) returns early when a row exists, so changing the env var does *nothing*. Rotation = `DELETE FROM agent_credentials …` then redeploy | **pre-existing, serious** |
| See `abw-system` wallet | `/api/wallet` is self-scoped (`wallet.rs:22`) | becomes urgent at Phase 2 |
| Move credits out | `transfer_credits_handler` sender is always the caller (`wallet.rs:117`) | becomes urgent at Phase 2 |
| Cash out | no payout path exists for **any** principal; Stripe is inbound-only (`/api/billing/checkout`) | not a blocker |

Note the key-rotation defect is **independent of this migration** —
`abw-system` already holds the platform's provider keys. It is broken
today.

**Design — `ObjectType::Principal`.** Make the principal itself an
ACL'd object, following the substrate's own documented extension
recipe (`fermi-auth/src/rbac.rs:22-33`):

1. Add `ObjectType::Principal` (`as_str` = `"principal"`, owner table
   `users`).
2. Its `owner_id` **is itself** (`abw-system`). Since nothing can
   authenticate as it, access comes only from platform-admin bypass or
   an explicit team grant — exactly the desired semantics.
3. Grant `platform-operators` `Permission::Admin` on it.
4. Gate the new endpoints on
   `rbac::require(Principal, "abw-system", Admin)`.

Endpoints to build:

```
GET    /api/admin/principals/:id/credentials          # metadata only, never values
PUT    /api/admin/principals/:id/credentials/:provider/:scope   # set / rotate
DELETE /api/admin/principals/:id/credentials/:provider/:scope
POST   /api/admin/principals/:id/credentials/:provider/:scope/verify
GET    /api/admin/principals/:id/wallet               # balance + ledger
POST   /api/admin/principals/:id/wallet/transfer      # move credits out, logged
```

Mostly wiring: `store_agent_credential` (`secrets.rs:283`) already
exists, already upserts, and is **never called from HTTP**. The audit
pattern exists too (`log_secret_access` / `get_secret_audit_log`).

The `verify` endpoint matters more than it looks — rotating the
platform's shared OpenAI/Anthropic key with no pre-flight check risks
unfunding every platform agent at once.

This generalises: `app-rabble` gets the same treatment in Phase 5 for
free.

#### 4c. Hygiene

**Demote `abw-system` to `role = 'viewer'`.** It is a non-login
principal (blank `password_hash`), its role is never consulted today,
and leaving it `admin` keeps it in the
`WHERE role='admin' ORDER BY created_at ASC LIMIT 1` lottery in the
seeder (`api_server.rs:4531`) should Ivan's row ever change.

> **Ordering constraint:** ship 4b's wallet endpoints **before Phase 2**.
> The moment ownership moves, royalties from 13 agents start accruing
> into a wallet nobody can see or empty.

### Phase 5 — Later, separately

- `app-rabble` principal + Rabble ownership (needs its own inventory).
- Tier alignment for the 13 curated Bucket A agents (§3 option b).
- Spec P1: make `is_platform_funded` **owner-based** instead of
  tier-based. This is the real end state — it collapses the two axes
  into one — but it is only safe *after* ownership is correct, which is
  what this plan establishes.

---

## 5. Rollback

One column, one statement:

```sql
UPDATE agents a
   SET user_id = b.user_id
  FROM agent_ownership_backup_p5 b
 WHERE a.agent_name = b.agent_name;
```

Or via the same reassign endpoint with Ivan's `user_id`
(`2e644008-f5c7-47c5-854c-3801df9879cc`, per
`docs/HANDOVER_2026-08-01.md`).

Also revert the Phase 1 card fields, otherwise the next boot re-applies
`owning_principal` to any row whose owner is null. (It will *not*
re-apply to rows you rolled back to Ivan — `COALESCE` protects them —
but leaving the cards inconsistent with the DB is how drift starts.)

No data is destroyed at any point: `agents.user_id` is the only mutated
column, and episodes, wallets, credentials and ACLs are untouched.

---

## 6. Effort

| Phase | Work | Risk |
|---|---|---|
| 0 Pre-flight | 10 min, read-only | none |
| 1 Code + cards | ~2 h | low — no data change |
| **4b** Principal admin API | **~1 day** | medium — new RBAC object type + key custody |
| 2 Move | 5 min | low — one column, backed up |
| 3 Verify | ~30 min | — |
| 4a/4c Team + demote | ~1 h | low |

**4b comes before 2** (see the ordering constraint above), so this is
not the half-day it first looked like — call it a day and a half, with
the principal-administration surface as the bulk of it. Every step
remains independently revertible.

The alternative is to run Phase 2 first and administer the wallet by
SQL until 4b lands. Defensible if you want the ownership change soon,
but it means the platform's revenue sits somewhere only a psql session
can reach — which is the same class of problem this whole plan exists
to remove.
