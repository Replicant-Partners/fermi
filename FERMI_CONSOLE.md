# Fermi Console

A native desktop application for probabilistic forecasting, built with GPUI (Zed's GPU-accelerated UI framework).

## Quick Start

```bash
# 1. Clone and navigate to the repo
cd /path/to/fermi

# 2. Build the console
cargo build -p fermi-console

# 3. Run it
cargo run -p fermi-console
```

That's it! The app will launch with a window.

## System Requirements

### Linux (Ubuntu/Debian)

Install these dependencies first:

```bash
sudo apt-get install -y \
  libxcb1-dev \
  libxkbcommon-dev \
  libxkbcommon-x11-dev \
  libfontconfig1-dev \
  libfreetype-dev \
  libwayland-dev \
  libvulkan-dev
```

### macOS

```bash
xcode-select --install
```

### Windows

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with "Desktop development with C++" workload.

## Environment Variables (Optional)

| Variable | Description |
|----------|-------------|
| `FERMI_API_KEY` | API token for Agent Bestiary World |
| `ANTHROPIC_API_KEY` | Enable real agent execution (otherwise uses mock) |
| `FMP_API_KEY` | Financial data provider |
| `AGENTS_DIR` | Custom path to agent directory |

Example:

```bash
FERMI_API_KEY="your-token" cargo run -p fermi-console
```

## Building

```bash
# Debug build (faster compilation)
cargo build -p fermi-console

# Release build (faster runtime, ~10-30 min)
cargo build -p fermi-console --release

# Run directly (after building)
./target/debug/fermi-console
./target/release/fermi-console
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+1` | Dashboard |
| `Ctrl+2` | Portfolio |
| `Ctrl+3` | Agent Fleet |
| `Ctrl+4` | Composer |
| `Ctrl+5` | Leaderboard |
| `Ctrl+N` | New Forecast |
| `Ctrl+Q` | Quit |

## Troubleshooting

**No window appears?**

```bash
RUST_LOG=info ./target/debug/fermi-console 2>&1 | head -50
```

Check for:
- GPU/Vulkan errors → ensure drivers are installed
- Missing system libraries → see System Requirements above

**Agents not loading?**

The app looks for `agents/curated` in the project root. Make sure that directory exists.

**"No ANTHROPIC_API_KEY found" warning?**

This is normal. Agents will use a mock executor. Set `ANTHROPIC_API_KEY` for real LLM calls.

## Project Structure

```
fermi/
├── crates/fermi-console/    # This application
│   ├── src/
│   │   ├── main.rs          # Entry point
│   │   └── cockpit.rs      # Main UI layout
│   └── Cargo.toml
├── agents/curated/          # Agent definitions
└── target/                  # Build output
```

## More Info

- See `crates/fermi-console/README.md` for detailed architecture
- See `docs/fermi/discussions/FERMI_NATIVE_CONSOLE.md` for design docs
