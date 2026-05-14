# Doc 1 — App Primitive on ABW

**Audience:** the ABW-side codebase agent (`/home/ilabra/fermi`) and the engineer who ships this PR.
**Status:** spec — ready to implement.
**Scope:** add a new first-class concept called **App** to the ABW platform. Backwards-compatible — existing workspaces, compositions, and compound agents continue to work unchanged.

---

## 1. What this adds — in one paragraph

ABW today knows about **agents** (atomic), **compound agents** (one agent orchestrating sub-agents), **compositions** (recipes — named agent lists), and **workspaces** (= teams — runtime containers with budget, files, chat, members). It does not formally know about **products built on top of all of that**. Today, "SimOps" or "Rabble" or "Fermi Console" exists only as a client-side convention plus an `origin` tag on the workspaces they create. This document adds **App** as a first-class platform concept: a registered, ownable, introspectable artifact that ties together a composition, a canonical schema, a workspace template, and a UI pointer. Workspaces continue to work without an App attached. Apps are opt-in.

---

## 2. The mental model — locked

```
Composition  — a recipe        ("use these agents together for X")
Compound     — an actor agent  ("an agent that runs other agents internally")
App          — a product       ("schema + composition + workspace template + UI + economics")
Workspace    — a runtime       ("a team's container: budget, files, chat, members")
```

Relationships:

- An **App** *references* one or more compositions and *includes* compound and atomic agents in them.
- An **App** *defines* a workspace template — when a user creates a workspace from the App, ABW provisions it with the right initial budget, files, and hired agents.
- A **Workspace** *may be linked to* an App via the existing `origin` field (which becomes the App's `slug` for App-spawned workspaces). Workspaces without an App still work — they remain `origin = "bestiary_workspace"` or whatever they are today.

What this is **not**:
- Not a replacement for compositions. Compositions remain the agent-orchestration recipe layer.
- Not a replacement for compound agents. Apps reference them; they keep their own lifecycle.
- Not a forced migration. Rabble, Silat, Fermi Console, the menagerie, all existing flows: untouched.
- Not (yet) an economics layer. The `revenue_share` field is reserved but inert in v1.
- Not (yet) a publication / public-URL system. That's a follow-up.
- Not (yet) a Xaman-Ek-introspection layer. Xaman Ek can read `/api/apps` in v1 but no special hooks are added.

---

## 3. Data model

### 3.1 New table: `apps`

```sql
-- Migration: NNN_apps.sql

CREATE TABLE IF NOT EXISTS public.apps (
    -- Identity
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug             TEXT NOT NULL UNIQUE,           -- e.g. "kask_simops"; doubles as the workspace origin tag
    name             TEXT NOT NULL,                  -- human display: "SimOps"
    tagline          TEXT,                           -- one-liner

    -- Ownership
    owner_user_id    TEXT NOT NULL,                  -- references users.user_id
    owner_team_id    UUID REFERENCES teams(id) ON DELETE SET NULL,  -- optional team-as-owner

    -- Surface
    homepage_url     TEXT,                           -- where the user actually clicks (e.g. https://kask.bio/projects/simops)
    icon_url         TEXT,

    -- The composition this app uses (by name, references composition_patterns or workspace compositions).
    -- We don't FK because compositions today are described in the catalogue as named patterns.
    composition_slug TEXT,

    -- The canonical document schema for this app.
    -- A JSON Schema document registered separately; this field holds its slug/version.
    schema_slug      TEXT,                           -- e.g. "kask-simops/2"
    schema_json      JSONB,                          -- inline JSON Schema (denormalised for ergonomics)

    -- Workspace template — JSONB blob describing the workspace ABW should provision.
    -- See §3.3 for shape.
    workspace_template JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Economics (reserved, inert in v1)
    revenue_share    JSONB DEFAULT NULL,             -- e.g. { "app_owner": 0.7, "platform": 0.3 }
    pricing_policy   TEXT DEFAULT 'platform_default', -- "platform_default" | "subscription" | "metered" | "free"

    -- Lifecycle
    visibility       TEXT NOT NULL DEFAULT 'private'
                         CHECK (visibility IN ('private', 'unlisted', 'public')),
    published_at     TIMESTAMPTZ,
    archived_at      TIMESTAMPTZ,

    -- Bookkeeping
    description      TEXT,
    metadata         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS apps_owner_idx       ON public.apps(owner_user_id);
CREATE INDEX IF NOT EXISTS apps_visibility_idx  ON public.apps(visibility);
CREATE INDEX IF NOT EXISTS apps_slug_idx        ON public.apps(slug);
```

### 3.2 Link from workspaces to apps

We don't add a new column. **Reuse `teams.origin`** — when a workspace is spawned from an App, `origin` is set to the App's `slug`. The link is:

```sql
-- Already exists from migration 112; no change needed.
-- teams.origin TEXT DEFAULT 'bestiary_workspace'
```

A workspace can be queried back to its App with:

```sql
SELECT a.* FROM apps a JOIN teams t ON t.origin = a.slug WHERE t.id = $1;
```

For App-spawned workspaces, the origin **must** equal an existing App slug. Workspaces with origins that don't match any App (`bestiary_workspace`, `rabble_workspace`, etc.) work normally — they're just not App-attached.

### 3.3 Workspace template shape

The `workspace_template` JSONB holds enough information for ABW to provision a complete workspace on App-create.

```jsonc
{
  // Initial budget (credits seeded on workspace creation)
  "initial_budget": 200,

  // Agents to auto-hire into the workspace on creation
  "auto_hire": ["simops_advisor", "simops_cascade"],

  // Initial files to write (one git commit on creation)
  "initial_files": [
    {
      "path": "simops/process.yaml",
      "content": "name: New Process\nstages: []\n"
    },
    {
      "path": ".app/manifest.yaml",
      "content": "app_slug: kask_simops\nschema: kask-simops/2\n"
    }
  ],

  // Optional: default workspace name pattern when user doesn't supply one
  "default_name_pattern": "SimOps — {user}'s process",

  // Optional: composition versions to pre-create inside the workspace
  "compositions": []
}
```

**Note:** the workspace template is interpreted by the App-spawn handler (§4.5). It is not executed lazily — provisioning happens atomically on create.

### 3.4 What an `App` looks like over the wire

```jsonc
{
  "id": "uuid",
  "slug": "kask_simops",
  "name": "SimOps",
  "tagline": "Design, simulate, and compare process pipelines.",
  "owner": {
    "user_id": "ivan@kask.bio",
    "team_id": null
  },
  "homepage_url": "https://kask.bio/projects/simops",
  "icon_url": null,
  "composition_slug": "simops_fleet",
  "schema_slug": "kask-simops/2",
  "schema_json": { /* JSON Schema document */ },
  "workspace_template": { /* see §3.3 */ },
  "revenue_share": null,
  "pricing_policy": "platform_default",
  "visibility": "private",
  "published_at": null,
  "archived_at": null,
  "description": "kask's process simulation app...",
  "metadata": {},
  "created_at": "2026-05-14T...",
  "updated_at": "2026-05-14T..."
}
```

---

## 4. API surface

All endpoints return `Content-Type: application/json`. All write endpoints require an authenticated principal (`AuthPrincipal`). Authorization rules per-endpoint in §5.

### 4.1 `POST /api/apps` — register a new App

Handler: `handlers::apps::create_app_handler` (new file `src/handlers/apps.rs`).

Request body (`CreateAppRequest`):
```rust
pub struct CreateAppRequest {
    pub slug: String,                          // required, must match /^[a-z][a-z0-9_]{2,63}$/
    pub name: String,                          // required
    pub tagline: Option<String>,
    pub homepage_url: Option<String>,
    pub icon_url: Option<String>,
    pub composition_slug: Option<String>,
    pub schema_slug: Option<String>,
    pub schema_json: Option<serde_json::Value>,
    pub workspace_template: Option<serde_json::Value>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub visibility: Option<String>,            // defaults to "private"
}
```

Response: `201 Created` with the full `App` JSON.

Errors:
- `409 Conflict` — slug already in use
- `400 Bad Request` — invalid slug format, malformed workspace_template, etc.
- `401 Unauthorized` — no auth

The authenticated user becomes the `owner_user_id` automatically. The slug **may not** collide with reserved origin tags: `bestiary_workspace`, `rabble_workspace`, `personal_workspace`, `silat_workspace` (configurable in code).

### 4.2 `GET /api/apps` — list Apps

Query params:
- `visibility` — filter (`private` | `unlisted` | `public`)
- `owner` — filter by owner user_id (returns only your private apps by default if omitted)
- `slug_prefix` — filter (e.g. `kask_` to list all kask apps)

Behaviour:
- Without auth → returns only `visibility = "public"`
- With auth → returns public + your own private + your own unlisted
- Owner can also list their team-owned apps if `owner_team_id` is set and they're a team member

Response:
```jsonc
{
  "apps": [ /* array of App JSON */ ],
  "total": 12
}
```

### 4.3 `GET /api/apps/:slug` — get one App

Returns 404 if the slug doesn't exist or the caller can't see it (private and not owned).

### 4.4 `PUT /api/apps/:slug` — update an App

Body: any subset of the fields in `CreateAppRequest`. Slug cannot be changed (rename = archive + create new). Updates `updated_at`. Only the owner can update.

### 4.5 `POST /api/apps/:slug/workspaces` — spawn a workspace from an App

This is **the killer endpoint**. It's the one-call provisioning flow.

Request:
```jsonc
{
  "name": "Kombucha 200L exploration",         // optional, defaults to workspace_template.default_name_pattern
  "description": "string",                     // optional
  "extra_budget": 50,                          // optional — added to initial_budget from template
  "auto_hire_override": ["..."]                // optional — overrides workspace_template.auto_hire
}
```

Behaviour (single transaction):
1. Read the App by slug. 403 if `visibility = "private"` and caller != owner.
2. Generate a unique workspace slug: `{app_slug}-{user_id_hash}-{ulid}`.
3. Insert a row into `teams` with:
   - `name` = request.name (or filled from template)
   - `slug` = generated above
   - `description` = request.description
   - `owner_id` = caller's user_id
   - `origin` = `app.slug`
   - `workspace_budget` = `template.initial_budget + (request.extra_budget || 0)`
4. For each `template.initial_files`, write the file via the same path the existing `write_workspace_file_handler` uses. Each becomes a git commit.
5. For each agent in `template.auto_hire` (or override), call the same logic as `hire_agent_handler` to attach it to the workspace.
6. Return the **created workspace** JSON (the same shape `get_workspace_handler` returns), plus a `provisioned: { files_written, agents_hired }` block for transparency.

Response: `201 Created`.

Errors:
- `404 Not Found` — app slug doesn't exist or not visible
- `403 Forbidden` — visibility doesn't permit caller
- `409 Conflict` — slug collision (rare; auto-retry with different ulid)
- `402 Payment Required` — caller can't afford the initial budget (the budget is allocated from the caller's wallet)

### 4.6 `GET /api/apps/:slug/workspaces` — list workspaces this App spawned

For the caller (filtered to workspaces they have membership in).

```sql
SELECT t.* FROM teams t
JOIN team_members m ON m.team_id = t.id
WHERE t.origin = $1 AND m.member_id = $caller_user_id;
```

Response: standard workspaces list shape.

### 4.7 `POST /api/apps/:slug/publish` — promote visibility

Sets `visibility = "public"`, `published_at = NOW()`. Only owner. Idempotent.

### 4.8 `POST /api/apps/:slug/archive` — archive an App

Sets `archived_at = NOW()`. Archived apps:
- Don't appear in `GET /api/apps` (unless `?include_archived=true`)
- Existing workspaces spawned from them continue to work
- `POST /api/apps/:slug/workspaces` returns 410 Gone

### 4.9 Route registration

In `src/api_server.rs`, add (after the existing `/api/agents` block, before the workspace block):

```rust
        // App registry
        .route("/api/apps", get(handlers::apps::list_apps_handler))
        .route("/api/apps", post(handlers::apps::create_app_handler))
        .route("/api/apps/:slug", get(handlers::apps::get_app_handler))
        .route("/api/apps/:slug", put(handlers::apps::update_app_handler))
        .route("/api/apps/:slug/workspaces", post(handlers::apps::spawn_workspace_handler))
        .route("/api/apps/:slug/workspaces", get(handlers::apps::list_app_workspaces_handler))
        .route("/api/apps/:slug/publish", post(handlers::apps::publish_app_handler))
        .route("/api/apps/:slug/archive",  post(handlers::apps::archive_app_handler))
```

---

## 5. Authorization rules

| Endpoint | Who can call |
|---|---|
| `POST /api/apps` | Any authenticated user |
| `GET /api/apps` | Anyone (filtered by visibility) |
| `GET /api/apps/:slug` | Anyone if public; otherwise only owner |
| `PUT /api/apps/:slug` | Owner only |
| `POST /api/apps/:slug/workspaces` | Anyone if app is public/unlisted; only owner if private |
| `GET /api/apps/:slug/workspaces` | Caller can see workspaces they're a member of |
| `POST /api/apps/:slug/publish` | Owner only |
| `POST /api/apps/:slug/archive` | Owner only |

All callers must have a valid `AuthPrincipal`. Anonymous (`/api/apps` GET only) returns `visibility = "public"` only.

---

## 6. The two unrelated patches we need for SimOps Phase 1

These are NOT part of the App primitive itself, but **must land in the same PR** to unblock SimOps v2. They're tiny:

### 6.1 Accept `origin` in `CreateTeamRequest`

In `src/handlers/teams.rs` around line 26 (`create_team_handler`):

```rust
#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub origin: Option<String>,        // <-- ADD THIS
}
```

And pass it through to the DB insert (defaulting to `"bestiary_workspace"` if `None`). This unblocks any kask-side workspace creation that wants to set `origin = "kask_simops"` directly (e.g. for migrating existing workspaces, or for testing without going through the App-spawn flow).

### 6.2 Add `"workspace"` to `ObjectType` for sharing

In `fermi-auth/src/types.rs:134` (`ObjectType::from_str`):

```rust
"workspace" => Some(ObjectType::Workspace),
```

Add the `Workspace` variant to the enum. Update the DB CHECK constraint with a new migration:

```sql
-- Migration: NNN_object_type_workspace.sql
ALTER TABLE object_shares DROP CONSTRAINT IF EXISTS object_shares_object_type_check;
ALTER TABLE object_shares ADD CONSTRAINT object_shares_object_type_check
    CHECK (object_type IN ('agent', 'capability', 'forecast', 'index', 'repo', 'file', 'workspace'));
```

This unblocks workspace-level sharing for SimOps collaborative use.

### 6.3 Add a direct (non-LLM) simops cascade endpoint

Per the capability brief: `simops_cascade` going through an LLM kills latency for live Compose-mode editing.

Add to `src/api_server.rs`:

```rust
        .route("/api/simops/cascade", post(handlers::simops::cascade_handler))
```

Handler (new file `src/handlers/simops.rs`):

```rust
use simops::process::ProcessConfig;
use simops::cascade::{cascade_forward, cascade_backward};

#[derive(Deserialize)]
pub struct CascadeRequest {
    pub process: ProcessConfig,
    pub direction: String,         // "forward" | "backward"
    pub quantity: f64,             // input_quantity for forward, target_output for backward
}

pub async fn cascade_handler(
    _principal: AuthPrincipal,     // require auth; cheap, no credit charge
    Json(req): Json<CascadeRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    req.process.validate().map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let result = match req.direction.as_str() {
        "forward"  => cascade_forward(&req.process, req.quantity),
        "backward" => cascade_backward(&req.process, req.quantity),
        _ => return Err((StatusCode::BAD_REQUEST, "direction must be 'forward' or 'backward'".to_string())),
    }.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(result)))
}
```

No LLM, no credit charge, sub-millisecond latency. Used by kask Compose mode for live stage-edit feedback.

### 6.4 Extend `ProcessConfig` schema

Add to `Stage` in `crates/simops/src/process.rs`:

```rust
pub struct Stage {
    // ... existing fields ...
    pub sidestreams: Option<Vec<Sidestream>>,
    pub sensors:     Option<Vec<Sensor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sidestream {
    pub id: String,
    pub name: String,                       // e.g. "CO2", "Pellicle", "Hotel Liquid"
    pub resource: Resource,
    pub capture_fraction: f64,              // 0..1 — what fraction we're capturing
    pub value_per_unit_usd: Option<f64>,
    pub current_disposition: Option<String>, // "vented" | "captured" | "sold" | "discarded"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sensor {
    pub id: String,
    pub name: String,
    pub measures: String,                   // e.g. "pH", "temperature", "DO"
    pub unit: String,
    pub sosa_property_uri: Option<String>,
}
```

Both fields are `Option<Vec<...>>` so existing test data and YAML files continue to parse. No version bump needed in cascade math — sidestreams and sensors are metadata for downstream agents (sidestream_miner, sensor_advisor), they don't affect mass-balance.

---

## 7. Implementation plan

### 7.1 File layout

New files:
- `migrations/<NNN>_apps.sql` — the apps table
- `migrations/<NNN+1>_object_type_workspace.sql` — sharing primitive extension
- `src/handlers/apps.rs` — all the App handlers
- `src/handlers/simops.rs` — the direct cascade handler

Modified files:
- `src/api_server.rs` — register the new routes
- `src/handlers/mod.rs` — add `pub mod apps; pub mod simops;`
- `src/handlers/teams.rs` — accept `origin` in `CreateTeamRequest`
- `fermi-auth/src/types.rs` — add `ObjectType::Workspace`
- `crates/simops/src/process.rs` — add `Sidestream`, `Sensor`, and `Option<Vec<...>>` fields to `Stage`
- `crates/simops/src/lib.rs` — re-export the new types

### 7.2 Tests

For Doc 1 the test bar:

- Unit: App CRUD round-trip (create → get → update → list filters)
- Unit: slug validation (reject reserved tags, invalid chars)
- Unit: workspace-template parsing
- Integration: full `POST /api/apps/:slug/workspaces` flow with a 2-agent template + 2-file template + budget seeding
- Integration: `GET /api/apps/:slug/workspaces` only returns workspaces the caller is a member of
- Integration: visibility-based access on `GET /api/apps`
- Migration: `apps` table created cleanly, idempotent

For 6.x patches:
- Unit: `CreateTeamRequest` round-trips with and without `origin`
- Integration: `POST /api/shares` accepts `object_type: "workspace"`
- Integration: `POST /api/simops/cascade` returns correct numbers for a 3-stage test process; validates direction; rejects malformed config

### 7.3 Rollout

1. Land all migrations (idempotent — they use `IF NOT EXISTS` patterns).
2. Deploy. `seed_apps_to_database()` runs at startup and upserts all `apps/*.json`
   manifests automatically — no manual step required.
   Verify with `GET /api/apps/kask_simops` returning the seeded manifest.
3. kask side proceeds with Doc 2.

No data migration is needed for existing workspaces. They retain their current `origin` values and don't get an App attached.

> **Implementation note (updated):** Step 3 of the original rollout ("Manually register
> via `POST /api/apps`") was superseded by `seed_apps_to_database()` in commit `d98b82a`.
> Drop a `*.json` file in `apps/` and it's live on the next deploy.

---

## 8. What's intentionally deferred

These are out of scope for Doc 1 but the App primitive is designed not to block them:

| Feature | When | Why deferred |
|---|---|---|
| App publication with public URL (`kask.bio/apps/simops`) | Phase 5 of SimOps build | Needs more thought on routing and rendering |
| App forking ("clone this App as a starting point") | Future | Architecturally clean but unneeded for kask's first apps |
| Revenue-share enforcement | Future | The `revenue_share` JSONB is reserved but the credit-flow code that would honour it doesn't exist; lock the schema, defer the behaviour |
| Xaman Ek introspection hooks (`/api/apps/onboard`, etc.) | When Xaman Ek needs them | Xaman Ek can read `/api/apps` already in v1; structured onboarding hooks come when we need them |
| Composition versioning per App | When an App is changed without breaking existing workspaces | The current `workspace_template` is interpreted at spawn time; later we may snapshot per workspace |
| Migration of Rabble / Silat / Fermi Console to register as Apps | Owner-driven, never forced | They keep working as today; they opt in if/when their owners want App-catalogue benefits |
| Server-side schedulers (cron for forecast schedules etc.) | Future ABW work | Documented in capability brief §9; not blocking |

---

## 9. Glossary lock-in

For this and all future ABW/kask documents:

| Term | Meaning |
|---|---|
| **Agent** | An entity with an `agent_id`, callable via `/api/agents/:id/execute`. Atomic. |
| **Compound agent** | An agent that internally orchestrates sub-agents. Indistinguishable from outside. |
| **Composition** | A named recipe: list of agent IDs that work well together. Already exists in catalogue. |
| **Fleet** | Informal synonym for "the agents an App composes" — use in prose, not in code. |
| **App** | A registered platform artifact: schema + composition + workspace template + UI pointer + economics. Spawns workspaces. (NEW — this document.) |
| **Workspace** | A team row. Runtime container: budget, files, chat, members. May be App-spawned (`origin = app_slug`) or unattached (`origin = "bestiary_workspace"` etc.). |
| **Origin** | A workspace tag identifying which App (or pre-App system) created it. Already exists. |
| **Project** | kask homepage usage. User-facing listing. Often 1:1 with an App; doesn't have to be. |

`Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` are NOT terms in this codebase. If they appear, fix them.

---

## 10. What success looks like

After this PR lands and the first App is registered, the following one-liner works end-to-end:

```bash
curl -X POST https://agent-bestiary.world/api/apps/kask_simops/workspaces \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Kombucha 200L exploration"}'
```

…and returns a fully-provisioned workspace with: `origin = "kask_simops"`, budget seeded, `simops_advisor` and `simops_cascade` hired, `simops/process.yaml` written, `.app/manifest.yaml` pointing back to the App.

That's the contract. The rest of SimOps v2 is built against this contract on the kask side (see Doc 2).
