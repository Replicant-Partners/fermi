# Installing the `abw` CLI

The `abw` command-line is the developer-facing path for building
Apps on the Agent Bestiary Workspace platform.

## One-liner (Linux / macOS / WSL / Git Bash)

```bash
curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-abw.sh | bash
```

This drops the `abw` binary into `~/.local/bin`. If that directory
isn't on your `$PATH` the installer prints the exact line to add to
your shell's rc file (`~/.bashrc`, `~/.zshrc`, or `~/.config/fish/config.fish`).

## Supported platforms

The installer auto-detects:

- Linux x86_64
- Linux aarch64 (ARM64)
- macOS x86_64 (Intel)
- macOS aarch64 (Apple Silicon)
- Windows x86_64 — run under Git Bash, WSL, or another POSIX shell

For anything else, build from source (see below).

## Verify

```bash
abw --version          # abw 0.2.0
abw login              # opens browser, authenticates this machine
abw whoami             # shows base URL + your user
```

Credentials land at `~/.abw/credentials`.

## First App, end-to-end

```bash
abw app new my_first_app           # scaffolds manifest.json + agents/
cd my_first_app
$EDITOR manifest.json              # edit composition, budget, etc.
abw app deploy                     # validate + register on the platform
abw app spawn my_first_app --open  # spawn a workspace and open it in your browser
```

## Install options

System-wide install (one binary for all users):

```bash
ABW_INSTALL_DIR=/usr/local/bin sudo bash <(curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-abw.sh)
```

Pin a specific release:

```bash
ABW_VERSION=abw-v0.2.0 bash <(curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-abw.sh)
```

Overwrite an existing install without prompting:

```bash
ABW_FORCE=1 curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-abw.sh | bash
```

## Build from source

For unsupported platforms, or to track `main`:

```bash
cargo install --git https://github.com/Replicant-Partners/fermi --bin abw abw-cli
```

Requires a recent Rust toolchain (`rustup` from <https://rustup.rs>).

## Troubleshooting

| Symptom | Fix |
|---|---|
| `command not found: abw` after install | Add `~/.local/bin` to `$PATH` and restart your shell. The installer prints the exact line. |
| `abw login` doesn't open a browser | It also prints the URL — copy-paste it. |
| `error: 401 Unauthorized` from any command | Run `abw login` again; tokens expire. |
| Stuck on an old version | Re-run with `ABW_FORCE=1` to overwrite. |
| `error: server returned 400 Bad Request: Invalid workspace ID` | The previous command in the pipeline probably emitted `null` (e.g. `jq` couldn't find the field, or a `curl` call failed silently). Run that command on its own to see the real response. |

## Docs

- [Doc 03 — Creating Apps](specs/03_CREATING_APPS.md) — manifest schema, three creation paths
- [Doc 07 — Building with ABW](specs/07_BUILDING_WITH_ABW.md) — runtime UX inside a workspace
