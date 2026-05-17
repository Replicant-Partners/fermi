# `smoke-test-apps.sh`

End-to-end smoke test for the three drop-dead-easy Apps creation paths.

## What it covers

- **Path 1 — CLI:** `abw app new` / `validate` / `deploy` / `spawn`
- **Path 2 — Conversational:** `POST /api/xaman/sessions` (type=`app_design`) → `POST /api/xaman/sessions/:id/create-app`
- **Path 3 — Fork:** create a workspace → `POST /api/workspaces/:id/fork-to-app` → publish the returned draft via `POST /api/apps`
- **Negatives:** reserved slug → 409, invalid slug → 400, duplicate slug → 409 on 2nd POST, unauthenticated → 401/403
- **Cleanup:** archives every App created (best effort) so reruns are idempotent

## Prerequisites

- `bash`, `curl`, `jq` on `$PATH`
- `cargo build -p abw-cli` has run (only if you want Path 1 — otherwise pass `--skip-cli`)
- An API key minted at `$ABW_BASE_URL/settings/api-keys`

## Usage

```bash
ABW_BASE_URL=https://agent-bestiary.world \
ABW_API_TOKEN=<key> \
    ./scripts/smoke-test-apps.sh
```

Against local dev:

```bash
ABW_BASE_URL=http://localhost:3000 \
ABW_API_TOKEN=<key> \
    ./scripts/smoke-test-apps.sh
```

## Flags

| Flag | Default | What it does |
|---|---|---|
| `--cli-binary <path>` | `./target/debug/abw` | Override path to the `abw` binary |
| `--keep` | (archive at end) | Leave the Apps + workspaces in place for inspection |
| `--skip-cli` | (run) | Skip the CLI section (use when the binary isn't built) |
| `--skip-session` | (run) | Skip the conversational path |
| `--skip-fork` | (run) | Skip the fork-from-workspace path |
| `--skip-negatives` | (run) | Skip the negative-case section |
| `-h` / `--help` | | Print usage |

## Exit codes

- `0` — all assertions passed
- `1` — at least one assertion failed (summary lists which)
- `2` — preflight failed (missing env, missing dep, server unreachable)

## What it doesn't do

- It does **not** exercise `abw login` (the localhost-callback OAuth flow). Token-based auth is scriptable; OAuth login requires manual browser interaction, so we skip it. Test `abw login` by hand before running the script.
- It does **not** clean up workspaces created during Path 1 / Path 3 — there's no public workspace-delete endpoint. Workspaces accumulate slowly across runs; clean them up via the dashboard when they pile up.
- It does **not** test the `Save as App` modal UI directly — only the underlying endpoint. UI test via the browser.

## CI usage

Suitable for CI as a post-deploy smoke test. Add to a GitHub Actions job that runs after a deploy lands:

```yaml
- name: Smoke test Apps paths
  env:
    ABW_BASE_URL: ${{ secrets.ABW_STAGING_URL }}
    ABW_API_TOKEN: ${{ secrets.ABW_STAGING_TOKEN }}
  run: |
    cargo build -p abw-cli
    ./scripts/smoke-test-apps.sh
```

Use a dedicated CI API key with a `cli` scope so log scrapes don't accidentally exfiltrate a personal key.
