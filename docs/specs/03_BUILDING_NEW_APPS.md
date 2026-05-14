# Doc 3 — Building New Apps on ABW

**Audience:** future you, future kask developers, future external app builders.
**Status:** recipe — written once SimOps v2 (Doc 2) has shipped and the App primitive (Doc 1) is live.
**Depends on:** Doc 1 (App primitive on ABW).
**Length:** short — this is a recipe, not a manual.

---

## What is an App on ABW

An **App** is a packaged product on the ABW platform: a registered manifest that ties together
- a canonical **schema** for the documents the app produces,
- a **composition** of agents (the fleet),
- a **workspace template** that ABW uses to provision a runtime container,
- a **UI surface** (typically hosted on kask.bio or a partner domain),
- and (eventually) an **economic policy** for revenue accounting.

Users enter an App, work inside a workspace it spawns, and leave with persistent state, agent collaborators, and forecasts they can compare and share.

Apps coexist with the platform's other primitives — they don't replace anything. Compositions stay recipes; compound agents stay actor-agents; workspaces stay runtime containers. Apps are the *product wrapper* layer that ties them together.

See Doc 1 for the formal data model and API. This doc is the recipe.

---

## When you should make an App

Make an App when you have:

1. **A user-facing surface** people enter and do something in (not just call once)
2. **A persistent state** worth keeping across sessions (files, conversations, decisions)
3. **A coherent agent team** that collaborates on the user's task
4. **A canonical document** the work revolves around (a process, a forecast, a creature, a track, etc.)

If you only have a single agent that answers questions, don't make an App — just publish the agent. If you have multiple agents but no persistent state, register a **composition** instead. If you have everything except a UI, you have an App, you just haven't pointed at the UI yet.

---

## The 30-minute App

You can stand up a minimal App in about half an hour. Here's the path.

### 1. Decide the four things

| Question | Example answer (Tonic Lab) |
|---|---|
| What's the canonical document? | An order: `{member_id, goal, adaptogens[], base, boosters, format}` |
| What's the agent fleet? | `tonic_advisor`, `adaptogen_curator`, `wellness_correlator` |
| What's the workspace template? | Budget 50, auto-hire `tonic_advisor`, initial file `tonic/profile.yaml` |
| Where's the UI? | `kask.bio/projects/tonic-lab` |

If you can write these four answers down, you're ready.

### 2. Register the JSON Schema (optional but recommended)

Author a JSON Schema for the canonical document. Put it inline in the manifest's `schema_json` field, or host it at a stable URL and reference via `schema_slug`. The schema is what makes the App **introspectable** — Xaman Ek can describe it, kask UI can validate against it, downstream agents know what shape to produce.

### 3. Author the fleet

For each agent in the fleet:
- If it exists in the bestiary, you're done.
- If it's a variant of an existing one, **fork it** via `POST /api/agents/:id/fork` and tweak its system prompt.
- If it's new, author it. The `companion_builder_coach` agent in the bestiary walks you through agent design — persona, system prompt, tool list, evaluation criteria.

Authoring tip: the **compound-agent pattern** works well when one agent acts as the front door and delegates to specialists. Look at `social_media_studio` or `cohere_and_coordinate` for examples.

### 4. Register the composition (optional but good practice)

Add your fleet to the bestiary's `composition_patterns`. This makes it discoverable — other Apps can reuse your team. The naming convention is `<vertical_or_app>_<role>` (e.g. `simops_fleet`, `tonic_intake`, `rabble_lifecycle`).

### 5. Write the workspace template

The workspace template tells ABW how to provision a runtime container when a user creates one from your App. Minimum useful template:

```jsonc
{
  "initial_budget": 100,
  "auto_hire": ["your_primary_agent"],
  "initial_files": [
    { "path": "<app_slug>/state.yaml", "content": "<empty initial state>" },
    { "path": ".app/manifest.yaml", "content": "app_slug: <app_slug>\nschema: <schema_slug>\n" }
  ]
}
```

Choose `initial_budget` such that a typical first session can be completed without topping up. Look at what your fleet costs per call (visible in agent metrics or by experimentation), multiply by expected first-session calls, add ~30% headroom.

### 6. POST the manifest

```bash
curl -X POST https://agent-bestiary.world/api/apps \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d @your-app-manifest.json
```

Your App is live. Anyone you share the slug with can now spawn workspaces from it.

### 7. Point your UI at it

Your kask page (or wherever the App's UI lives) makes three kinds of calls:
- `ABW.app.spawnWorkspace('<your_slug>', { name })` — create a workspace
- `ABW.readWorkspaceFile / writeWorkspaceFile` — read/write the canonical document
- Either `POST /api/workspaces/:id/messages` with `@agent_name` (charges workspace budget) or `POST /api/agents/:id/execute/stream` (charges user wallet) to invoke agents

That's the whole interface.

---

## Patterns to copy

### The **stateful interview** pattern (SimOps)
- An advisor agent runs a multi-turn conversation
- Each turn is logged as a workspace message
- The agent builds the canonical document incrementally
- The user can branch, skip, or save partial state

Good for: discovery work, intake processes, configuration tasks.

### The **side-by-side variants** pattern (SimOps Scenarios, future apps)
- Canonical document is forked into named variants
- Variants compared via `git/diff`
- Variants run through forecasts or simulations for A/B

Good for: anything where users want to compare configurations or strategies.

### The **content-pipeline** pattern (social_media_studio)
- One compound agent orchestrates: brief → image → caption → publish
- User provides a brief; downstream is automatic
- Workspace holds drafts and a publish log

Good for: production workflows where the user's job is mostly "review and approve".

### The **AR/spatial** pattern (Rabble)
- Workspace state includes spatial primitives (H3 cells)
- Multiple users coexist in a shared workspace
- Agents observe and react to state changes

Good for: location-aware experiences, multi-user real-time.

### The **forecasting** pattern (Fermi Console)
- Canonical document is a forecast (question, drivers, FPL source)
- Persistent across sessions via `/api/forecasts`
- Schedules re-run the forecast over time
- Portfolios group related forecasts

Good for: any quantitative prediction app. Reusable directly via the existing Fermi APIs.

### The **transactional kiosk** pattern (Tonic Lab — when built)
- Each interaction is short and complete (an order, a measurement)
- State persists per member, not per session
- Hardware integration via dedicated agents
- Membership data feeds wellness recommendations

Good for: physical-world kiosks, retail, point-of-care.

---

## Working with Xaman Ek

Xaman Ek is the platform navigator. When designing an App, ask Xaman Ek:

- **"What agents exist for <domain>?"** — finds existing fleet members so you don't re-author
- **"Are there composition patterns like <description>?"** — finds reusable templates
- **"What's a good fleet for <use case>?"** — gets a starting composition
- **"What's missing from my fleet to do X?"** — identifies gaps and suggests new agents to author

Once your App is registered with `visibility: public` or `unlisted`, Xaman Ek can describe it to other users:

- **"What Apps are available for <domain>?"** — surfaces your App
- **"How do I use <your App>?"** — pulls from your manifest's `description` and homepage

To improve discoverability, write a clear `description` and `tagline` on your manifest. These are what Xaman Ek reads.

---

## Economics (when they exist)

The `revenue_share` and `pricing_policy` fields on the App manifest are reserved but inert in v1 of the App primitive. When ABW ships revenue accounting:

- Set `pricing_policy = "metered"` for usage-based billing
- Set `pricing_policy = "subscription"` for flat-rate access to your App
- Set `revenue_share = { "app_owner": 0.7, "platform": 0.3 }` to declare your cut
- Use `POST /api/workspaces/:id/budget` to add credits to a workspace (already exists)
- Workspace credit consumption already flows to agent owners proportionally — App-level routing comes next

Don't design around these features until they ship. They're listed here so you know what's coming.

---

## Publication & sharing

Once your App is registered:

- `visibility: private` — only you can spawn workspaces from it. Useful for development.
- `visibility: unlisted` — anyone with the slug can spawn, but it's not in `GET /api/apps` without auth. Useful for beta.
- `visibility: public` — listed in the catalogue, anyone can spawn.

For **collaborative workspaces** (you and a colleague work on the same Process / Tonic profile / forecast), use the workspace-sharing primitive: `POST /api/shares` with `object_type: "workspace"` (available once Doc 1 §6.2 ships).

For **public artifacts** (a published version of a workspace with a stable URL): not yet supported. Coming in a follow-up to Doc 1.

---

## Checklist — before you ship

- [ ] Manifest validates against the App schema (slug format, no reserved tags, workspace_template structure)
- [ ] Initial budget is enough for a typical first session × 1.3
- [ ] All agents in `auto_hire` exist and are accessible
- [ ] Canonical document schema (JSON Schema or YAML example) documented
- [ ] First-run flow tested end-to-end: spawn workspace → do work → reload → resume
- [ ] At least one example workspace exists for screenshots and docs
- [ ] Description and tagline are clear enough for Xaman Ek to surface
- [ ] UI handles the auth-required state cleanly (Fermi-console-style sign-in flow, see kask's `hooks.js`)
- [ ] At least one experiment / forecast / output is producible end-to-end

---

## When NOT to make an App

- **One-off task.** Just call an agent.
- **No persistent state.** Use the agent + composition layer directly.
- **No coherent domain.** If you can't name the canonical document in one sentence, you don't have a domain.
- **Not your platform.** If you're consuming the ABW catalogue from your own backend, you might not need a registered App at all — just call the APIs.

Apps are a commitment to maintaining a product surface. They earn that commitment by giving users continuity, sharing, and discoverability that ad-hoc agent calls cannot.

---

## Examples to read

- **SimOps** — Doc 2 in this folder. Stateful interview + side-by-side variants + forecasting.
- **Fermi Console** — `fermi/INTRODUCTION.md` + `fermi/FERMI_CONSOLE.md`. Forecast-centric, the original.
- **Rabble** — repo at `/home/ilabra/fermi/rabble-web`. AR/spatial pattern.
- **Adaptogen Lab** (future) — TBD; will use the knowledge-curation pattern around `adaptogen_curator`.

---

## Glossary

Same as Doc 1 §9. The terms `App`, `Composition`, `Compound agent`, `Workspace`, `Origin` are the platform vocabulary. Avoid `Vertical`, `Studio`, `Lab`, `Workbench`, `Surface` — they're not in the platform's lexicon.
