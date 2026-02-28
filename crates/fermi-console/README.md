# Fermi Console

MMOG-style forecasting command center built on [GPUI](https://gpui.rs/) (Zed's GPU-accelerated UI framework).

## Platform Support

| Platform | Status | Renderer |
|----------|--------|----------|
| Linux (Ubuntu/Debian) | ✅ Primary | Vulkan via Blade + X11/Wayland |
| macOS | 🔲 Planned | Metal |
| Windows | 🔲 Planned | DirectX via Blade |

## Build Dependencies

### Linux (Ubuntu / Debian)

GPUI requires several system libraries for X11/Wayland windowing and font rendering:

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

These are the same dependencies required to build [Zed](https://github.com/zed-industries/zed/blob/main/docs/src/development/linux.md) from source.

### macOS

Install Xcode and command line tools:

```bash
xcode-select --install
```

GPUI uses Metal for rendering on macOS — no additional dependencies needed.

### Windows

Windows support requires the MSVC toolchain. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload.

## Building

```bash
# From the fermi workspace root:
cargo build -p fermi-console

# Run:
cargo run -p fermi-console

# Release build (recommended for daily use):
cargo build -p fermi-console --release
cargo run -p fermi-console --release
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `⌘1` / `Ctrl+1` | Dashboard |
| `⌘2` / `Ctrl+2` | Portfolio |
| `⌘3` / `Ctrl+3` | Agent Fleet |
| `⌘4` / `Ctrl+4` | Composer |
| `⌘5` / `Ctrl+5` | Leaderboard |
| `⌘N` / `Ctrl+N` | New Forecast |
| `⌘Q` / `Ctrl+Q` | Quit |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Fermi Console (GPUI native app)                    │
│                                                     │
│  ┌───────────┐ ┌────────────┐ ┌──────────────────┐  │
│  │ Dashboard │ │ Portfolio  │ │ Agent Fleet      │  │
│  │ - Brier   │ │ - Active   │ │ - Status         │  │
│  │ - Streak  │ │ - Resolved │ │ - Performance    │  │
│  │ - Rank    │ │ - Drafts   │ │ - Execution log  │  │
│  └───────────┘ └────────────┘ └──────────────────┘  │
│                                                     │
│  ┌─────────────────────────────────────────────────┐ │
│  │ Forecast Composer                               │ │
│  │ - Question builder, driver editor, simulation   │ │
│  │ - Local Monte Carlo (instant, no server needed) │ │
│  │ - Results: histogram, tornado, sensitivity      │ │
│  └─────────────────────────────────────────────────┘ │
│                                                     │
│  ┌──────────────────┐ ┌────────────────────────────┐ │
│  │ Leaderboard      │ │ Tournaments (future)       │ │
│  └──────────────────┘ └────────────────────────────┘ │
└────────────────────────┬────────────────────────────┘
                         │
          ┌──────────────┴──────────────┐
          │ Local (instant, offline)    │
          │ - FPL parsing & validation  │
          │ - Monte Carlo simulation    │
          │ - Chart rendering           │
          │ - Portfolio cache (SQLite)   │
          └──────────────┬──────────────┘
                         │ HTTPS (when needed)
          ┌──────────────┴──────────────┐
          │ ABW API Server              │
          │ - Agent execution (LLM)     │
          │ - Brier scoring             │
          │ - Leaderboard               │
          │ - Forecast publishing        │
          │ - Team collaboration        │
          └─────────────────────────────┘
```

## Theme

Ayu Mirage color palette — the same theme used across the Fermi ecosystem.

| Color | Hex | Usage |
|-------|-----|-------|
| Background | `#1F2430` | Primary background |
| Deep BG | `#171B24` | Sidebar |
| Elevated | `#272D38` | Cards, panels |
| Foreground | `#CBCCC6` | Primary text |
| Muted | `#5C6773` | Secondary text |
| Cyan | `#5CCFE6` | Primary accent |
| Green | `#BAE67E` | Success, good Brier |
| Gold | `#FFCC66` | Highlights, warnings |
| Orange | `#FFAE57` | Secondary accent |
| Red | `#FF6666` | Errors, bad Brier |
| Purple | `#D4BFFF` | Premium, tournaments |
| Blue | `#73D0FF` | Info, agent fleet |

## Development Status

**Phase 0: Spike** ✅ — GPUI shell, dashboard with mock data, panel navigation

**Phase 1: Core** 🔲 — Portfolio with real data, forecast composer, agent fleet

**Phase 2: Server Integration** 🔲 — Auth, API client, Brier scoring, publishing

**Phase 3: Social** 🔲 — Leaderboard, shared forecasts, teams

See [FERMI_NATIVE_CONSOLE.md](../../docs/fermi/discussions/FERMI_NATIVE_CONSOLE.md) for the full design exploration.