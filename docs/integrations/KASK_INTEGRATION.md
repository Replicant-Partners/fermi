# Kask ↔ ABW integration spec

Reference for the kask.bio backend session — what to call, what
shape the payloads are, and how to verify the integration is working
correctly end-to-end.

**ABW base URL**: `https://agent-bestiary.world`
**Kask base URL**: `https://kask.bio`
**App registered**: `kask_simops` (auto-seeded from
`apps/kask_simops.json` in the ABW repo)
**Cross-domain auth**: token-in-fragment redirect (Lax cookie blocks
the cookie path; commit `4d15a64`)

---

## 1. The contract — App → Workspace spawn

Kask's job is to call **one endpoint** when a user clicks "Start a
new SimOps process" on kask.bio:

```
POST https://agent-bestiary.world/api/apps/kask_simops/workspaces
Authorization: Bearer <user_session_token>     # OAuth-issued ABW JWT
Content-Type: application/json

{
  "name":        "SimOps — pilot brewery run",   // optional; falls back to default_name_pattern
  "description": "Three-vessel pilot run...",    // optional
  "extra_budget": 0,                              // optional; added to template initial_budget (250)
  "auto_hire_override": null                      // optional; overrides template auto_hire list
}
```

Response (`201 Created`):

```json
{
  "workspace_id":   "458bcd72-926a-46d2-91b4-5910c8c1e305",
  "workspace_slug": "kask_simops-9f3e2a1c",
  "name":           "SimOps — pilot brewery run",
  "origin":         "kask_simops",
  "budget":         250,
  "provisioned": {
    "files_written": 4,
    "agents_hired":  3
  }
}
```

The ABW workspace is fully provisioned by this single call:

- `teams` row created with `origin = 'kask_simops'`, owner = the
  authed user, budget = template `initial_budget + extra_budget`
- Workspace wallet seeded with the budget credits (via
  `credit_deposit`)
- `workspace_agents` rows for each name in `auto_hire`
  (`simops_advisor`, `simops_cascade`, `simops_narrator`), all with
  `relationship = 'system'`
- Workspace git repo initialized and seeded with `initial_files`
  from the app manifest:
  - `simops/process.yaml` — process YAML stub
  - `simops/budget.yaml` — discovery budget config
  - `.app/manifest.yaml` — provenance (kask_simops, schema, ui URL)
  - `context/readme.md` — onboarding text

Where the user goes next:

- **Continue inside kask.bio** — kask owns the rendering layer for
  SimOps. Pass `workspace_id` back to your UI and call into the
  ABW API for state.
- **Hand off to ABW UI** — `https://agent-bestiary.world/workspace/<workspace_id>`
  opens the substrate-side workspace surface (chat, files, shelf,
  member list).

---

## 2. Auth — getting the user_session_token

Kask redirects users to ABW for OAuth (Google), then receives the
session token back as a URL fragment:

```
https://kask.bio/auth/abw-callback#token=<jwt>&user_id=<uuid>
```

Cookie-based session won't work cross-domain (Lax SameSite blocks
the cookie on the fetch path). The token-in-fragment redirect is
the workaround — kask reads the fragment client-side, stashes the
token, then uses it as a Bearer token on subsequent ABW API calls.

Kask.bio is already on the OAuth redirect allowlist
(`ada0a74 fix: add kask.bio to OAuth redirect allowlist`).

**Flow from kask.bio:**

```
1. User clicks "Sign in" on kask.bio
2. Redirect: https://agent-bestiary.world/auth/google?redirect_to=https://kask.bio/auth/abw-callback
3. Google OAuth → ABW callback → ABW issues JWT
4. ABW redirects: https://kask.bio/auth/abw-callback#token=<jwt>&user_id=<uuid>
5. Kask reads fragment, stashes token in localStorage/sessionStorage
6. All subsequent ABW API calls: Authorization: Bearer <jwt>
```

JWT lifetime: standard HS256 self-issued (env: `JWT_SECRET`).

---

## 3. State queries — reading from kask's side

Once a workspace is spawned, kask reads workspace state via:

```
GET  /api/workspaces/:workspace_id                    -- header info, budget, members, agents
GET  /api/workspaces/:workspace_id/messages           -- chat history
GET  /api/workspaces/:workspace_id/files              -- git-backed artifacts
GET  /api/workspaces/:workspace_id/files-raw/<path>   -- file bytes
GET  /api/apps/kask_simops/workspaces                 -- all workspaces this user spawned from this app
```

Kask renders SimOps' own UX on top of these reads — process.yaml
editor, cascade visualisations, narrator transcript. The ABW
substrate provides state; kask provides the domain interface.

---

## 4. Agent invocation — running SimOps work

Kask invokes ABW agents inside the workspace via:

```
POST /api/workspaces/:workspace_id/messages
Authorization: Bearer <jwt>
Content-Type: application/json

{
  "content":      "@simops_advisor What should I optimise for in this batch?",
  "message_type": "agent_invocation"
}
```

The `@<agent>` parsing is the standard chat-driven invocation flow.
For background invocations (no human message), kask can POST
directly to:

```
POST /api/agents/simops_cascade/execute
Authorization: Bearer <jwt>
{
  "query": "...",
  "workspace_id": "<workspace_id>"   // so the agent runs in workspace context with workspace tools
}
```

Streaming (SSE) variant: `/api/agents/:id/execute/stream` returns
`text/event-stream`. Useful for the long-running cascade /
optimisation runs where you want progressive updates.

---

## 5. Gas / credits — who pays for what

| Action | Charged from |
|---|---|
| Workspace spawn | The 250 initial budget is *transferred from your wallet to the workspace wallet* — i.e. the user pays at spawn time |
| Agent execution inside the workspace | Workspace wallet (1 cr / 1k tokens + 10% gas) |
| Chat message | Workspace wallet (1 cr/msg) |
| Hire/add agents (beyond auto-hire) | Workspace wallet (5 cr / 2 cr) |
| Coherence/eval/dreaming triggered from kask | Workspace wallet |

So the user funds the workspace once, and kask burns from that pool
until topped up. To top up, kask can POST:

```
POST /api/workspaces/:workspace_id/budget
{ "amount": 100 }
```

That transfers credits from user wallet → workspace wallet, with
the usual purchased-balance check (admin bypass applies).

---

## 6. Verifying the integration works

A four-step smoke test for the kask session to run. If all four
pass, the integration is healthy.

### 6.1 App is registered

```
curl https://agent-bestiary.world/api/apps/kask_simops
```

Should return a JSON object with the app manifest content. If 404
or missing, the seeder didn't pick up `apps/kask_simops.json` —
re-deploy or check Railway boot logs for `seed_apps_to_database`.

### 6.2 Auth round-trip works

From kask.bio, run the OAuth flow. Confirm:

- The token in the fragment is present
- A subsequent `GET /api/auth/me` with `Authorization: Bearer <token>`
  returns the user's profile (200 with `user_id`, `display_name`)

### 6.3 A spawn from kask actually provisions

```
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Smoke test"}' \
  https://agent-bestiary.world/api/apps/kask_simops/workspaces
```

Expect 201 with `workspace_id`. Then:

```
curl -H "Authorization: Bearer $TOKEN" \
  https://agent-bestiary.world/api/workspaces/<workspace_id>
```

Verify the response includes:

- `origin: "kask_simops"`
- `workspace_budget: 250`
- `agents` array contains the three auto-hired agents
- `description` matches what you sent (or null if you didn't)

### 6.4 The workspace shows up correctly in the harness UI

Open `https://agent-bestiary.world/workspace/<workspace_id>` directly.
Confirm:

- Workspace header shows the name and "kask_simops" origin badge
- Three agents visible in the left sidebar (simops_advisor /
  cascade / narrator)
- File tree under Files tab shows the four initial files
- Budget reads 250 cr remaining

If any of these are off, the failure is in spawn provisioning —
file an issue with the response body from 6.3 attached.

---

## 7. Origin tagging — for downstream rollups

Every workspace spawned from kask gets `teams.origin = 'kask_simops'`.
The ABW dashboard's "Compositions" block has an origin filter
dropdown that lets the user see only kask-spawned workspaces (`Mine
(ABW)` is default; switching to `kask_simops` shows all kask
spawns). This is how operators audit a vertical's footprint without
mixing it with the user's own ABW workspaces.

When the Apps dashboard block ships (separate commit), each app row
will show a `<count> workspaces spawned` next to its name, calling
`GET /api/apps/:slug/workspaces` for per-user counts.

---

## 8. Known gotchas

- **Don't share API keys across users.** Each user's JWT is theirs.
  Kask should hold the JWT in user-scoped storage and never use a
  service-account token to spawn workspaces on behalf of users —
  the spawn handler intentionally uses the calling user as the
  workspace owner so wallet charges land in the right place.
- **Initial budget is real spend.** Spawning costs the user 250 cr
  up front. If the user has insufficient transferable balance,
  the spawn fails with 402 `Payment Required`. Kask should surface
  this clearly in its UI before redirecting to the spawn call.
- **The 'sys' owner on kask_simops in apps/kask_simops.json** is a
  seed-time placeholder. On boot, the seeder upserts but leaves
  `owner_user_id = 'sys'`. The visibility is `public` so spawning
  works for any authed user regardless of owner.
- **`teams.origin` is set at spawn time only.** Existing workspaces
  created before the app primitive landed have `origin =
  'bestiary_workspace'` — you can't retroactively re-tag those as
  kask without an explicit migration. Going forward, every kask
  spawn lands with the correct origin.

---

## 9. What's pending on the ABW side

- The **Apps dashboard block** on `agent-bestiary.world/dashboard`
  is being built as a peer to "My Agents" and "Compositions". Once
  shipped, users will see kask_simops as a first-class entity
  with a "Spawn workspace" button — duplicates what kask.bio does
  but useful for power users testing without leaving ABW.
- A **server-side rollup endpoint** `GET /api/me/apps-health` (analogous
  to `/api/me/loop-health`) is on the roadmap — would let
  the Apps block show per-app cost, executions, and health badges
  in one fetch. Not blocking for kask's integration.
