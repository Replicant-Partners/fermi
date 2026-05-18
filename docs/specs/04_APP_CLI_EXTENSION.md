# Doc 4 — Extending the App Primitive with a Generated CLI

**Audience:** ABW codebase (`/home/ilabra/fermi`), external App developers (Mario and successors).
**Status:** 🗺 **roadmap** — design doc only; no code ships with this document.
**Depends on:** Doc 1 (App primitive), Doc 3 (Building new Apps), `abw-cli` binary (shipped).

---

## 1. The idea in one paragraph

The ABW App manifest already contains everything needed to drive a command-line interface: which agents are hired, what the strategist accepts and produces, what the canonical document looks like, and sample queries that demonstrate the interaction grammar. This document proposes treating the **manifest as a CLI grammar** — a first-class derivation from the App primitive that lets developers and operators interact with their App's workspace from a terminal without writing any glue code. The generated CLI is not a replacement for the workspace UI; it is the scripting and automation surface that the UI currently lacks, targeted at developers building on ABW apps and operators who want to integrate ABW into existing pipelines.

---

## 2. The design pattern: manifest as CLI grammar

An App manifest today looks like this (simplified):

```json
{
  "slug": "kask_simops",
  "workspace_template": {
    "auto_hire": ["simops_companion", "simops_cascade", "..."],
    "initial_files": [{ "path": "simops/process.yaml", "content": "..." }]
  }
}
```

The strategist agent card adds:

```json
{
  "agent_id": "simops_companion",
  "accepts": ["kask_simops/process_question", "kask_simops/edit_request"],
  "produces": ["kask_simops/action_block", "kask_simops/prose_response"],
  "capabilities": {
    "output_contract": {
      "domain": "process_optimisation",
      "produces_schema": "kask-simops/action_block",
      "calibration_signal": "sosa_observation"
    }
  },
  "metadata": {
    "sample_queries": [
      "Help me model a kombucha brewing process.",
      "Fork a variation that captures CO2 from fermentation."
    ]
  }
}
```

And the action grammar in the system prompt enumerates six typed actions:
`edit_process`, `invoke_agent`, `fork_variation`, `compare_variations`,
`declare_sosa_contract`, `annotate`.

**These three sources — the manifest, the agent card, the action grammar — are a complete CLI specification.** The derivation is mechanical:

| Manifest / card element | CLI element |
|---|---|
| `app.slug` | Binary name (e.g. `simops`) |
| `workspace_template.initial_files[*].path` | File path arguments (`simops/process.yaml`) |
| `strategist.accepts[*]` | Input types / accepted flags |
| `strategist.produces_schema` | Output format (JSON schema for `--json` flag) |
| `output_contract.action_types[*]` | Subcommands (`simops fork-variation`, `simops compare`) |
| `metadata.sample_queries[*]` | `--help` examples |
| `auto_hire[*]` + member `accepts`/`produces` | `simops invoke <agent>` subcommand |

---

## 3. What the generated CLI looks like

For `kask_simops`, the generated surface would be:

```
simops <subcommand> [flags]

Subcommands derived from action grammar:
  chat             Conversational turn with simops_companion
  edit             Propose a process edit (wraps edit_process action)
  fork             Fork a process variation (wraps fork_variation action)
  compare          Compare variations (wraps compare_variations action)
  annotate         Record an observation (wraps annotate action)
  invoke           Call a member agent directly

Subcommands derived from workspace template:
  workspace list   List workspaces for this App
  workspace spawn  Spawn a new workspace
  workspace use    Set the active workspace for this session

File operations:
  process show     Print the current simops/process.yaml
  process diff     Diff current vs a named variation
  files list       List workspace files
  files get <path> Print a workspace file
  files put <path> Write a workspace file from stdin
```

The conversational path:

```bash
$ simops chat "fork a variation that captures 75% of the CO2 from fermentation"

Forking variation "co2-capture-75"...

  __ACTION__
  { "type": "fork_variation", "name": "co2-capture-75", ... }
  __END_ACTION__

The variation patches the fermentation stage to add a CO2 capture
sidestream at 75% capture fraction. Run `simops compare base co2-capture-75`
to see the NER and carbon delta.
```

The structured path (action grammar → typed subcommand):

```bash
$ simops fork --name "co2-capture-75" \
              --from base \
              --hypothesis "Capturing 75% of fermentation CO2 improves NER and reduces carbon footprint"

✓ Variation created: simops/variations/co2-capture-75.yaml
```

```bash
$ simops compare base co2-capture-75 --metrics ner,carbon,npv

Winner by NER:     co2-capture-75 (0.61 vs 0.58)
Winner by carbon:  co2-capture-75 (net sequestration vs net emission)
Winner by NPV:     co2-capture-75 (+21% median)

Narrative: The CO2-capture variant wins on all three primary metrics...
```

For an efrain-style notes app, the generated surface would be much simpler — no action grammar, just `efrain chat`, `efrain notes list`, `efrain notes add`.

---

## 4. Three levels of generation

The design pattern admits three implementation levels, each delivering value on its own:

### Level 1 — Shell wrapper (1–2 days)

`abw app generate-cli <slug>` fetches the manifest and emits a parameterized shell script:

```bash
#!/usr/bin/env bash
# Generated CLI for kask_simops (SimOps)
# Generated by: abw app generate-cli kask_simops
# Regenerate: abw app generate-cli kask_simops > ~/bin/simops

ABW_APP_SLUG="kask_simops"
ABW_STRATEGIST="simops_companion"

chat() { abw workspace message --app "$ABW_APP_SLUG" --agent "$ABW_STRATEGIST" "$@"; }
workspace() { abw workspace "$@" --app "$ABW_APP_SLUG"; }
process() {
  case "$1" in
    show) abw files get --app "$ABW_APP_SLUG" simops/process.yaml ;;
    *) echo "usage: simops process show" ;;
  esac
}

"${@:-chat}"
```

No new binary. No compilation. The script calls the existing `abw` binary with app-specific defaults pre-filled. `abw app generate-cli kask_simops > ~/bin/simops && chmod +x ~/bin/simops` is the entire install.

**Delivery:** one new `abw` subcommand + one Handlebars/mustache template per language target (bash, zsh, fish).

### Level 2 — Generated Rust CLI (1–2 weeks)

`abw app build-cli <slug>` fetches the manifest and generates a Rust `clap`-based binary:

- Subcommands from `output_contract.action_types` — typed flags, proper `--help`
- `--json` flag on every subcommand using `produces_schema` for validation
- Tab-completion generated by clap
- Binary name = app slug

The code-generation target is a `clap` `Command` struct in a `build.rs`-style generated file, compiled into a standalone binary. The generation is deterministic: the same manifest always produces the same binary interface. App version bumps regenerate the CLI.

**Delivery:** a `codegen` module in `abw-cli` that emits Rust source from a manifest, plus a one-command build (`abw app build-cli kask_simops`).

### Level 3 — Self-describing CLIs from `output_contract` (weeks, not days)

At this level the App's `output_contract.produces_schema` is a full JSON Schema (or TypeScript interface) for the action grammar. The CLI generator reads the schema and produces typed subcommands with argument validation, schema-validated output, and machine-readable `--json` responses.

This is the version where:

```bash
$ simops fork --help
Usage: simops fork [OPTIONS] --name <NAME> --hypothesis <TEXT>

Arguments:
  --name <NAME>          Human-readable variation name [required]
  --from <SLUG>          Source variation [default: base]
  --hypothesis <TEXT>    What you expect this variation to achieve [required]

Options:
  --patch <JSON>         Additional patch fields as JSON
  --json                 Output as JSON (kask-simops/action_block schema)
```

...is generated entirely from the `output_contract.produces_schema` definition and validated against it at runtime.

**Delivery:** a JSON Schema → clap argument parser code-generator; schema registration in the App primitive (`POST /api/apps/:slug/schema`); CLI binary update/upgrade via `abw app update-cli kask_simops`.

---

## 5. What this unlocks

### For App developers (Mario, kask, future external developers)

- Scripts that manipulate their app's workspace from CI pipelines
- REPL-style interaction during development without opening a browser
- Integration with existing terminal toolchains (pipe output to `jq`, drive from `make`, compose with `xargs`)

### For operators in regulated or air-gapped environments

The CLI + local Ollama (Phase 0 topology) = a fully local, fully scriptable agent pipeline:

```bash
OLLAMA_BASE_URL=http://localhost:11434/v1 simops chat "run the cascade on my process"
```

No browser. No cloud inference. The CLI is the only interface needed.

### For the ABW substrate

A generated CLI is evidence that the App primitive is **complete** — that it contains enough information to drive arbitrary interfaces, not just the workspace UI. This is the same argument as "if you can generate a CLI from it, you can generate a REST client, a webhook handler, a Slack bot." The manifest is the contract; the CLI is one projection of it.

---

## 6. The key design constraint: the CLI must track the manifest

Every manifest change that adds an action type, changes the strategist, or updates `auto_hire` must trigger a CLI regeneration. The mechanism:

- **Shell wrapper (Level 1):** regenerate by running `abw app generate-cli` again. Idempotent, safe to run in CI.
- **Compiled CLI (Level 2/3):** the binary embeds the manifest version (`git_commit_sha` from the App record). On execution, it checks `GET /api/apps/:slug` and warns if the server's version is ahead of the compiled version. `abw app update-cli <slug>` fetches the latest manifest and recompiles.

This is the same discipline as the App manifest's relationship to workspace spawning: the manifest is the source of truth; all derived artifacts track it.

---

## 7. Relationship to the existing `abw` CLI

The existing `abw` CLI (`abw-cli/`) is the **generic substrate**. A generated App CLI is a **thin derived layer** on top of it. The generated CLI calls the `abw` library (or API) for all actual execution; it only adds app-specific defaults and action-grammar-derived subcommands.

This means:

- Auth, token management, API communication — all in `abw`, reused unchanged
- Workspace lifecycle (`spawn`, `list`, `use`) — in `abw`, wrapped with `--app kask_simops` default
- Message sending — in `abw`, wrapped with `--agent simops_companion` default
- File read/write — in `abw`, wrapped with app-canonical paths
- Action grammar subcommands — generated layer only; call `abw workspace message` and parse the response

The generated CLI does not replace `abw`. It makes `abw` ergonomic for a specific App's user.

---

## 8. Phasing

| Phase | What ships | Effort | When |
|---|---|---|---|
| **0 — already done** | `abw` generic CLI; `abw app new/validate/deploy/spawn` | shipped | — |
| **1 — shell wrapper** | `abw app generate-cli <slug>` → bash script | 1–2 days | after kask action dispatcher is stable |
| **2 — typed subcommands** | Generated Rust CLI from `output_contract.action_types` | 1–2 weeks | after at least one App has a stable output_contract in production |
| **3 — schema-validated** | Full JSON Schema → clap code generation | weeks | after Level 2 has real usage and the schema registry exists |

Level 1 is the right next step. It requires `simops_companion` v3 to be stable in production (it is, as of `9e0ee60`) and the kask action dispatcher to be live (kask-side work). Once those two conditions are met, the shell wrapper is a one-day addition to `abw-cli`.

---

## 9. What this is NOT

- **Not a replacement for the workspace UI.** The CLI is the scripting surface; the browser is the exploratory and visual surface. They are complementary.
- **Not a new execution engine.** The CLI sends messages to the workspace; the execution happens on ABW's existing stack.
- **Not multi-agent orchestration.** The CLI talks to the strategist; the strategist routes to members. The CLI doesn't bypass the MoE design.
- **Not agent authoring.** That's `abw agent new` / the agent card template / xaman_ek. The generated CLI assumes the agents already exist.

---

## 10. The one-sentence version

**The App manifest is a grammar; the CLI is the terminal projection of that grammar; generating it is a mechanical derivation that makes Apps scriptable without any additional developer effort.**
