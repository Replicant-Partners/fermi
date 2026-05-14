# SimOps v2 — Spec Pack

Three specs that together describe the SimOps v2 build and the platform primitive it depends on. Read in order.

| # | File | Audience | Read first if you are… |
|---|---|---|---|
| 1 | [`01_APP_PRIMITIVE.md`](./01_APP_PRIMITIVE.md) | ABW codebase (`/home/ilabra/fermi`) — the engineer who ships the platform PR | working on ABW backend |
| 2 | [`02_KASK_SIMOPS_APP.md`](./02_KASK_SIMOPS_APP.md) | kask codebase (`/home/ilabra/kask`) — the engineer who ships SimOps v2 | working on the kask UI |
| 3 | [`03_BUILDING_NEW_APPS.md`](./03_BUILDING_NEW_APPS.md) | future app builders (kask or external) | designing a new app on ABW |

## Dependency order

```
Doc 1 (ABW platform PR)
   ↓ unlocks
Doc 2 (kask SimOps build)
   ↓ documents the pattern as
Doc 3 (recipe for future apps)
```

Doc 1 ships first. Doc 2 cannot start until Doc 1 is live. Doc 3 is reference material, can be written/read anytime.

## Source material this pack is built on

- [`../ABW_CAPABILITY_BRIEF.md`](../ABW_CAPABILITY_BRIEF.md) — the questions we asked the ABW codebase
- [`../ABW_CAPABILITY_BRIEF_ANSWERS.md`](../ABW_CAPABILITY_BRIEF_ANSWERS.md) — the source-grounded answers
- Long design conversation between user and assistant that resulted in the three-layer mental model: **Composition** (recipe) / **Compound agent** (actor) / **App** (product wrapper)

## Glossary (locked)

| Term | Meaning |
|---|---|
| **Agent** | An entity callable via `/api/agents/:id/execute`. Atomic. |
| **Compound agent** | An agent that internally orchestrates sub-agents. |
| **Composition** | A named recipe: list of agent IDs that work well together. |
| **Fleet** | Informal synonym for "the agents an App composes." Prose only, not code. |
| **App** | A registered platform artifact: schema + composition + workspace template + UI pointer + economics. Spawns workspaces. *(new — introduced by Doc 1)* |
| **Workspace** | A team row. Runtime container: budget, files, chat, members. |
| **Origin** | A workspace tag identifying which App created it. Already exists. |
| **Project** | kask homepage usage. User-facing listing. Often 1:1 with an App. |

`Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` are NOT in the platform's lexicon.
