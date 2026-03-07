#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────
# package-console.sh — Package Fermi Console for tester distribution
#
# Usage:
#   ./scripts/package-console.sh              # debug build (fast)
#   ./scripts/package-console.sh --release    # release build (slow, optimized)
#   ./scripts/package-console.sh --help
#
# Output: dist/fermi-console-<os>-<arch>-<date>.tar.gz
# ─────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

VERSION="0.10.0-dev"
DATE="$(date +%Y%m%d)"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
BUILD_MODE="debug"
CARGO_FLAGS=""

# ── Parse args ─────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --release)
            BUILD_MODE="release"
            CARGO_FLAGS="--release"
            ;;
        --help|-h)
            echo "Usage: $0 [--release]"
            echo ""
            echo "Packages Fermi Console into a distributable tarball."
            echo ""
            echo "Options:"
            echo "  --release    Build with optimizations (slower build, faster app)"
            echo "  --help       Show this help"
            echo ""
            echo "Output: dist/fermi-console-<os>-<arch>-<date>.tar.gz"
            echo ""
            echo "The tester unpacks and runs:"
            echo "  ANTHROPIC_API_KEY=sk-ant-... ./fermi-console"
            exit 0
            ;;
    esac
done

BUNDLE_NAME="fermi-console-${OS}-${ARCH}-${DATE}"
DIST_DIR="$PROJECT_ROOT/dist"
STAGE_DIR="$DIST_DIR/$BUNDLE_NAME"

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║  Fermi Console Packager                                  ║"
echo "╠═══════════════════════════════════════════════════════════╣"
echo "║  Version:  $VERSION"
echo "║  OS:       $OS / $ARCH"
echo "║  Build:    $BUILD_MODE"
echo "║  Output:   dist/$BUNDLE_NAME.tar.gz"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# ── Step 1: Build ──────────────────────────────────────────────
echo "▸ Building fermi-console ($BUILD_MODE)…"
cargo build -p fermi-console $CARGO_FLAGS 2>&1 | grep -E "Compiling fermi-console|Finished|error" || true

BINARY="$PROJECT_ROOT/target/$BUILD_MODE/fermi-console"
if [ ! -f "$BINARY" ]; then
    echo "✗ Build failed — binary not found at $BINARY"
    exit 1
fi
echo "  ✓ Binary: $(du -h "$BINARY" | cut -f1)"

# ── Step 2: Stage files ───────────────────────────────────────
echo "▸ Staging distribution…"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
mkdir -p "$STAGE_DIR/agents/curated"
mkdir -p "$STAGE_DIR/forecasts"

# Binary
cp "$BINARY" "$STAGE_DIR/fermi-console"
chmod +x "$STAGE_DIR/fermi-console"

# Agent cards (required for local execution)
if [ -d "$PROJECT_ROOT/agents/curated" ]; then
    AGENT_COUNT=$(find "$PROJECT_ROOT/agents/curated" -name "*.json" -o -name "*.yaml" -o -name "*.yml" | wc -l)
    cp -r "$PROJECT_ROOT/agents/curated/"* "$STAGE_DIR/agents/curated/" 2>/dev/null || true
    echo "  ✓ Agents: $AGENT_COUNT cards"
else
    echo "  ⚠ No agents/curated directory found"
fi

# Sample forecasts (if any exist, include a few as examples)
if [ -d "$PROJECT_ROOT/forecasts" ]; then
    FORECAST_COUNT=$(find "$PROJECT_ROOT/forecasts" -name "*.fpl" | wc -l)
    if [ "$FORECAST_COUNT" -gt 0 ]; then
        # Copy up to 3 example forecasts (fpl + evidence + state)
        find "$PROJECT_ROOT/forecasts" -name "*.fpl" | head -3 | while read fpl; do
            base="${fpl%.fpl}"
            cp "$fpl" "$STAGE_DIR/forecasts/" 2>/dev/null || true
            cp "${base}.evidence.md" "$STAGE_DIR/forecasts/" 2>/dev/null || true
            cp "${base}.state.json" "$STAGE_DIR/forecasts/" 2>/dev/null || true
        done
        EXAMPLE_COUNT=$(find "$STAGE_DIR/forecasts" -name "*.fpl" | wc -l)
        echo "  ✓ Example forecasts: $EXAMPLE_COUNT"
    fi
fi

# ── Step 3: Create README for testers ─────────────────────────
cat > "$STAGE_DIR/README.md" << 'READMEEOF'
# Fermi Console — Tester Build

**Version:** 0.10.0-dev
**What is this?** A native desktop app for probabilistic forecasting using AI agents.

## Quick Start

### 1. Install system dependencies (Linux only)

```bash
sudo apt-get install -y \
    libxcb1 libxkbcommon0 libxkbcommon-x11-0 \
    libfontconfig1 libfreetype6 libvulkan1
```

### 2. Run

```bash
cd fermi-console-*        # this directory
./fermi-console
```

### 3. Sign in to ABW (required for agents)

When the app opens, go to the **Dashboard** panel and click **Sign In with Google** or **Sign In with GitHub**.

This authenticates you with the Agent Bestiary World (ABW) platform. All agent execution (AI research) runs through ABW — you do **not** need your own API keys. ABW handles LLM costs.

Without signing in, the app still works for editing forecasts, running simulations, and viewing saved forecasts — but agents won't be able to research.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+Enter | Research question (fires Fermi agent) |
| Ctrl+R | Run Monte Carlo simulation |
| Ctrl+S | Save forecast (FPL + evidence + state) |
| Ctrl+P | Publish to ABW platform |
| Ctrl+N | New forecast |
| Ctrl+O | Import FPL file |
| Ctrl+E | Cycle tabs (Edit → FPL → Wiki) |
| Ctrl+1-5 | Switch panels (Dashboard, Portfolio, Fleet, Composer, Leaderboard) |

## Workflow

1. **Sign in** via Dashboard → Google or GitHub (required for agent research)
2. **Type a question** in the Question Hub (e.g., "Will AMD reach $200 by 2026-12-31?")
3. **Press Ctrl+Enter** — Fermi decomposes into drivers with a base rate
4. **Review drivers** — click each to edit parameters (p5/p50/p95)
5. **Assign agents** — click "+ Assign Agent" on drivers for evidence
6. **Set confidence** — type 0-100 in the Confidence % field per driver
7. **Simulate** — Ctrl+R runs 10K Monte Carlo iterations
8. **Save** — Ctrl+S writes `.fpl`, `.evidence.md`, `.state.json`

## Troubleshooting

- **Black screen / crash on launch:** Check GPU drivers. GPUI requires Vulkan support.
- **Agents fail with "Sign in to ABW":** You need to sign in first. Go to Dashboard → Sign In.
- **"Failed to load agents":** Make sure `agents/curated/` is next to the binary.
- **Window doesn't appear:** Try `RUST_LOG=info ./fermi-console` for debug output.
- **Agent research returns empty/mock data:** Confirm you're signed in (Dashboard shows your name).

## Feedback

Please report issues with:
- What you were doing
- What you expected
- What happened instead
- Console output (`RUST_LOG=info ./fermi-console 2>&1 | tee fermi.log`)
READMEEOF

echo "  ✓ README.md"

# ── Step 4: Create launcher script ────────────────────────────
cat > "$STAGE_DIR/run.sh" << 'RUNEOF'
#!/usr/bin/env bash
# Convenience launcher for Fermi Console
cd "$(dirname "${BASH_SOURCE[0]}")"

export AGENTS_DIR="${AGENTS_DIR:-./agents/curated}"
export RUST_LOG="${RUST_LOG:-warn,fermi_console=info}"

echo "Fermi Console starting…"
echo "  Sign in via Dashboard → Google/GitHub to enable agent research."
echo ""

exec ./fermi-console "$@"
RUNEOF
chmod +x "$STAGE_DIR/run.sh"
echo "  ✓ run.sh launcher"

# ── Step 5: Create .env.example ───────────────────────────────
cat > "$STAGE_DIR/.env.example" << 'ENVEOF'
# Agent research runs through ABW — sign in via Google/GitHub in the app.
# No API keys needed for normal usage.

# Optional: Override agent card directory
# AGENTS_DIR=./agents/curated

# Optional: Logging level
# RUST_LOG=warn,fermi_console=info

# Dev only: Local agent execution (bypass ABW, use your own Anthropic key)
# ANTHROPIC_API_KEY=sk-ant-your-key-here
ENVEOF
echo "  ✓ .env.example"

# ── Step 6: Package ───────────────────────────────────────────
echo "▸ Creating tarball…"
cd "$DIST_DIR"
tar czf "${BUNDLE_NAME}.tar.gz" "$BUNDLE_NAME"
TARBALL_SIZE=$(du -h "${BUNDLE_NAME}.tar.gz" | cut -f1)

# Cleanup staging
rm -rf "$STAGE_DIR"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  ✓ Package ready: dist/${BUNDLE_NAME}.tar.gz ($TARBALL_SIZE)"
echo ""
echo "  Send to tester. They unpack and run:"
echo ""
echo "    tar xzf ${BUNDLE_NAME}.tar.gz"
echo "    cd ${BUNDLE_NAME}"
echo "    ./run.sh"
echo ""
echo "  Then sign in via Dashboard → Google/GitHub to enable agents."
echo "═══════════════════════════════════════════════════════════"
