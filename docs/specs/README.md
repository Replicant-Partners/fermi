# Apps on ABW — Spec Pack

> **Status: shipped.** App primitive is live; three creation paths and the
> runtime-UX surface are wired end-to-end.

## TL;DR — three ways to make an App, then build its runtime

App creation and App operation are two layers. The creation step gets you a
registered App (5 minutes via any of three paths). The operation step is
where you wire the UI, iterate the agent prompt, and ship the actual product.

### Layer 1 — Create an App (registration)

> All three paths produce the same artifact (a registered App with a spawn URL)
> by feeding the same `abw-apps-core` substrate. Pick the path that fits the person.

**1. In conversation** (best for non-coders, designers, domain experts)
```
@xaman_ek help me build an App for <your idea>
```
The session walks you through canonical document → fleet → workspace template → UI surface, then surfaces **Create App →** in the sidebar.

**2. In code** (best for developers, CI, git-tracked manifests)
```bash
abw login              # one-time, opens browser
abw app new <slug>     # scaffolds the directory
abw app deploy         # validates + registers
abw app spawn <slug>   # spawn a workspace from it
```
See `crates/abw-cli/` for the source.

**3. From a working workspace** (best when you've built something that works)
In any workspace, click **Save as App** in the header. The platform drafts a manifest from the workspace state, lists what looks intentional vs incidental, lets you edit, and publishes.

→ **Full recipe and patterns to copy:** [`03_CREATING_APPS.md`](./03_CREATING_APPS.md)

### Layer 2 — Build the runtime UX (after registration)

Once your App is registered, the real work begins: wiring a UI, iterating
the agent's prompt, parsing the action grammar your agent emits, debugging
errors. That's a different skill set from registering an App.

→ **Practical runtime guide:** [`07_BUILDING_WITH_ABW.md`](./07_BUILDING_WITH_ABW.md) — covers the schema endpoint, a 100-line working UI, the 2-minute prompt iteration loop, the error catalog, known limitations, and a worked example (Mario's `efrain_ai`).

---

## The docs

| # | File | What it covers | Read when |
|---|---|---|---|
| 1 | [`01_APP_PRIMITIVE.md`](./01_APP_PRIMITIVE.md) | The App data model + REST API. Source of truth for the schema and endpoints. | You're implementing platform internals or need to look up exact endpoint shapes. **Don't read this first** unless you have to — humans use the paths above. |
| 2 | [`../shared/02_KASK_SIMOPS_APP.md`](../shared/02_KASK_SIMOPS_APP.md) | SimOps v2 as the worked example. Lives under `docs/shared/` because it's the canonical reference shared by both kask and ABW maintainers. | You want a full reference App with rich action grammar, four UI modes, and live agents. |
| 3 | [`03_CREATING_APPS.md`](./03_CREATING_APPS.md) | **Start here for creation.** Three paths, when to make an App vs not, patterns to copy. | You have an idea and want to register an App on the platform. |
| 4 | [`04_APP_CLI_EXTENSION.md`](./04_APP_CLI_EXTENSION.md) | Roadmap — extending the App primitive with a generated CLI. Design doc, no code yet. | You're thinking about per-App auto-generated CLI tooling. |
| 5 | [`05_ACTION_PROTOCOL_KASK_MIGRATION.md`](./05_ACTION_PROTOCOL_KASK_MIGRATION.md) | The App Action Protocol: how kask migrates its existing clients onto the new `__ACTION__` block grammar. | You maintain a client that consumes ABW agents and want to support the action grammar. |
| 6 | [`06_ABW_HANDOFF.md`](./06_ABW_HANDOFF.md) | SimOps v3 ABW handoff brief — what the ABW maintainer needs to ship before the v3 alpha can be smoke-tested. | You're Ivan, or you're picking up the v3 hand-off. |
| 7 | [`07_BUILDING_WITH_ABW.md`](./07_BUILDING_WITH_ABW.md) | **Start here for runtime UX.** The practical guide for vibe coders. Schema endpoint, dispatcher UI, prompt iteration loop, error catalog, known limitations. | Your App is registered; now you need to build its UI and iterate the agent. |

**Suggested reading order for a new App developer:**
1. This README — orient
2. Doc 03 — register an App (5 minutes)
3. Doc 07 — build the runtime around it

Skip Doc 1, 4, 5, 6 unless they intersect what you're doing.

---

## What's live in the platform

```
GET  /api/apps                              list Apps
POST /api/apps                              register a new App (CLI + UI use this)
GET  /api/apps/:slug                        get one App
PUT  /api/apps/:slug                        update an App (owner only)
GET  /api/apps/:slug/schema                 the action grammar for an App's agents
POST /api/apps/:slug/workspaces             spawn a workspace from an App
GET  /api/apps/:slug/workspaces             list caller's workspaces from this App
POST /api/apps/:slug/publish                promote visibility to public
POST /api/apps/:slug/archive                archive

POST /api/xaman/sessions/:id/create-app     create an App from a ready app_design session
POST /api/workspaces/:id/fork-to-app        draft a manifest from a working workspace

POST /api/workspaces/:id/actions/:type      dispatch a typed action
GET  /api/workspaces/:id/actions            list action log
POST /api/workspaces/:id/actions/:id/accept accept a pending mutate_document
POST /api/workspaces/:id/actions/:id/reject reject a pending mutate_document
GET  /api/workspaces/:id/annotations        list annotations

GET  /auth/cli?callback=...&state=...       CLI localhost-callback OAuth entry point
GET  /auth/cli/finish?cb=...&state=...      mints CLI API key and redirects to localhost
```

The three creation paths all go through the same `abw_apps_core::build_manifest` substrate, so validation, defaults, and structured suggestions are consistent regardless of how the App was created.

## Source map

| Surface | Path |
|---|---|
| Substrate (pure validation + defaults) | `crates/abw-apps-core/` |
| CLI | `crates/abw-cli/` (binary: `target/debug/abw` or `target/release/abw`) |
| HTTP handlers (Apps) | `src/handlers/apps.rs` |
| HTTP handlers (xamanEK session create-app) | `src/handlers/xaman.rs` |
| HTTP handlers (workspace fork-to-app) | `src/handlers/workspace/core.rs` |
| HTTP handlers (workspace actions / annotations) | `src/handlers/workspace_actions.rs` (and adjacent) |
| Auth (CLI login) | `src/handlers/auth.rs::auth_cli_start` + `auth_cli_finish` |
| Xaman Ek `app_design` session | `agents/curated/xaman_ek/agent_card.json` (system prompt §"Session types") |
| Workspace fork introspection | `src/apps/workspace_fork.rs` |
| UI: workspace "Save as App" modal | `templates/workspace.html` (#save-as-app-modal) |
| UI: xaman-ek "Create App" sidebar button | `static/js/widgets/xaman-ek.js::createAppFromSession` |
| Auto-seed-from-filesystem | `src/api_server.rs::seed_apps_to_database` (loads `apps/*.json` on startup) |

## Glossary (locked)

| Term | Meaning |
|---|---|
| **Agent** | An entity callable via `/api/agents/:id/execute`. Atomic. |
| **Compound agent** | An agent that internally orchestrates sub-agents. |
| **Composition** | A named recipe: list of agent IDs that work well together (peer to App; can exist without an App attached). |
| **Fleet** | Informal synonym for "the agents an App composes." Prose only, not code. |
| **App** | A registered platform artifact: schema + composition + workspace template + UI pointer + economics. Spawns workspaces. |
| **Workspace** | A team row. Runtime container: budget, files, chat, members. |
| **Origin** | A workspace tag identifying which App created it. |
| **Project** | kask homepage usage. User-facing listing. Often 1:1 with an App. |
| **Action block** | The `__ACTION__ { ... } __END_ACTION__` JSON envelope an agent emits in its response. The UI parses these out, dispatches to `/api/workspaces/:id/actions/:type`, and surfaces the prose separately. |
| **Action grammar** | The set of action types and shapes an App's agents may emit. Exposed at `GET /api/apps/:slug/schema`. |

`Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` are NOT in the platform's lexicon.

## Source material (background reading)

- [`../ABW_CAPABILITY_BRIEF.md`](../ABW_CAPABILITY_BRIEF.md) — questions asked of the ABW codebase before the App primitive landed
- [`../ABW_CAPABILITY_BRIEF_ANSWERS.md`](../ABW_CAPABILITY_BRIEF_ANSWERS.md) — source-grounded answers
