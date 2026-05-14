# SimOps v2 — Spec Pack

> **Status: Phase 1 shipped.** Both sides are live.
> - Doc 1 (ABW App primitive): committed `3832d65` + `d98b82a` on fermi/main
> - Doc 2 (kask SimOps v2): committed `ce6daa9` on kask/main
> - App auto-seeded at startup — no manual registration needed
>
> Remaining work: Phase 2 (Scenarios), Phase 3 (Experiments), Phase 4 (Sharing).

Three specs that together describe the SimOps v2 build and the platform primitive it depends on.

| # | File | Status |
|---|---|---|
| 1 | [`01_APP_PRIMITIVE.md`](./01_APP_PRIMITIVE.md) | ✅ **Shipped** — `/api/apps`, `/api/simops/cascade`, `apps/kask_simops.json` auto-seeded |
| 2 | [`02_KASK_SIMOPS_APP.md`](./02_KASK_SIMOPS_APP.md) | ✅ **Phase 1 shipped** — `kask-sim-client.js`, wizard + composer migrated, shell live |
| 3 | [`03_BUILDING_NEW_APPS.md`](./03_BUILDING_NEW_APPS.md) | 📖 Reference — pattern documented from SimOps |

## What's live

```
POST /api/apps/kask_simops/workspaces  →  provisions workspace (250cr, 3 agents, 4 files)
GET  /api/apps/kask_simops/workspaces  →  list caller's SimOps workspaces
POST /api/simops/cascade               →  deterministic cascade, no LLM, <1ms
kask.bio/projects/simops?workspace=X  →  four-mode shell (Intake / Compose / — / —)
```

## What's still to build (Phase 2–4)

- Scenarios mode — side-by-side parameter variants, git diff view
- Experiments mode — FPL engine, distribution comparison, Decision recorder
- Sharing — `POST /api/shares` with `object_type: "workspace"`
- `sidestream_miner`, `comparator`, `sensor_advisor` agents

## Source material

- [`../ABW_CAPABILITY_BRIEF.md`](../ABW_CAPABILITY_BRIEF.md) — questions asked of the ABW codebase
- [`../ABW_CAPABILITY_BRIEF_ANSWERS.md`](../ABW_CAPABILITY_BRIEF_ANSWERS.md) — source-grounded answers

## Glossary (locked)

| Term | Meaning |
|---|---|
| **Agent** | An entity callable via `/api/agents/:id/execute`. Atomic. |
| **Compound agent** | An agent that internally orchestrates sub-agents. |
| **Composition** | A named recipe: list of agent IDs that work well together. |
| **Fleet** | Informal synonym for "the agents an App composes." Prose only, not code. |
| **App** | A registered platform artifact: schema + composition + workspace template + UI pointer + economics. Spawns workspaces. |
| **Workspace** | A team row. Runtime container: budget, files, chat, members. |
| **Origin** | A workspace tag identifying which App created it. |
| **Project** | kask homepage usage. User-facing listing. Often 1:1 with an App. |

`Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` are NOT in the platform's lexicon.
