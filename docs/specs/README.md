# Apps on ABW — Spec Pack

> **Status: shipped.** App primitive is live; three paths to create one are wired end-to-end.

## TL;DR — three ways to make an App

> All three produce the same artifact (a registered App with a spawn URL) by feeding the same `abw-apps-core` substrate. Pick the path that fits the person.

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

→ **Full recipe and patterns to copy:** [`03_BUILDING_NEW_APPS.md`](./03_BUILDING_NEW_APPS.md)

---

## The three docs

| # | File | What it covers |
|---|---|---|
| 1 | [`01_APP_PRIMITIVE.md`](./01_APP_PRIMITIVE.md) | The App data model + REST API. Source of truth for the schema and endpoints. **Don't read this first** unless you're implementing platform internals — humans use the three paths above. |
| 2 | [`02_KASK_SIMOPS_APP.md`](./02_KASK_SIMOPS_APP.md) | SimOps v2 as the worked example. Shows the four-mode app shape end-to-end against the live SimOps deployment. |
| 3 | [`03_BUILDING_NEW_APPS.md`](./03_BUILDING_NEW_APPS.md) | **Start here.** The recipe. Three paths, patterns to copy, when to make an App vs not. |

## What's live in the platform

```
GET  /api/apps                              list Apps
POST /api/apps                              register a new App (CLI + UI use this)
GET  /api/apps/:slug                        get one App
PUT  /api/apps/:slug                        update an App (owner only)
POST /api/apps/:slug/workspaces             spawn a workspace from an App
GET  /api/apps/:slug/workspaces             list caller's workspaces from this App
POST /api/apps/:slug/publish                promote visibility to public
POST /api/apps/:slug/archive                archive

POST /api/xaman/sessions/:id/create-app     create an App from a ready app_design session
POST /api/workspaces/:id/fork-to-app        draft a manifest from a working workspace

GET  /auth/cli?callback=...&state=...       CLI localhost-callback OAuth entry point
GET  /auth/cli/finish?cb=...&state=...      mints CLI API key and redirects to localhost
```

The three creation paths all go through the same `abw_apps_core::build_manifest` substrate, so validation, defaults, and structured suggestions are consistent regardless of how the App was created.

## Source map

| Surface | Path |
|---|---|
| Substrate (pure validation + defaults) | `crates/abw-apps-core/` |
| CLI | `crates/abw-cli/` (binary: `target/debug/abw` or `target/release/abw`) |
| HTTP handlers | `src/handlers/apps.rs`, `src/handlers/xaman.rs`, `src/handlers/workspace/core.rs` |
| Auth (CLI login) | `src/handlers/auth.rs::auto_cli_start` + `auth_cli_finish` |
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

`Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` are NOT in the platform's lexicon.

## Source material (background reading)

- [`../ABW_CAPABILITY_BRIEF.md`](../ABW_CAPABILITY_BRIEF.md) — questions asked of the ABW codebase before the App primitive landed
- [`../ABW_CAPABILITY_BRIEF_ANSWERS.md`](../ABW_CAPABILITY_BRIEF_ANSWERS.md) — source-grounded answers
