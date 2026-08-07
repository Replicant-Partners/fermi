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
#         Linux → a bare `fermi-console` binary in the tarball.
#         macOS → an ad-hoc signed `Fermi Console.app` bundle instead.
#
# This is the LOCAL dev packaging script. CI publishes the real release
# assets (plain binaries + a macOS .app zip) separately.
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

# macOS gets a real .app bundle so testers can double-click it; Linux
# stays a bare binary next to its data folders. Everything downstream
# branches on this one flag. (macOS ships bash 3.2 — keep it simple.)
if [ "$OS" = "darwin" ]; then
    IS_MACOS=1
else
    IS_MACOS=0
fi
APP_NAME="Fermi Console.app"

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
if [ "$IS_MACOS" = "1" ]; then
    # ── macOS: wrap the binary in a minimal .app bundle ────────
    APP_DIR="$STAGE_DIR/$APP_NAME"
    mkdir -p "$APP_DIR/Contents/MacOS"
    mkdir -p "$APP_DIR/Contents/Resources"

    # Unquoted heredoc so $VERSION interpolates. Nothing else in this
    # XML contains `$` or backticks, so there's nothing to escape.
    cat > "$APP_DIR/Contents/Info.plist" << PLISTEOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Fermi Console</string>
    <key>CFBundleDisplayName</key>
    <string>Fermi Console</string>
    <key>CFBundleExecutable</key>
    <string>fermi-console</string>
    <key>CFBundleIdentifier</key>
    <string>world.agent-bestiary.fermi-console</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLISTEOF

    cp "$BINARY" "$APP_DIR/Contents/MacOS/fermi-console"
    chmod +x "$APP_DIR/Contents/MacOS/fermi-console"
    echo "  ✓ Bundle: $APP_NAME"

    # Re-sign, always. The linker applies an ad-hoc signature at build
    # time, but ANY post-link modification invalidates it — `strip`,
    # `install_name_tool`, even just dropping the executable into a new
    # bundle whose Info.plist/_CodeSignature the old signature never
    # covered. On Apple Silicon that is fatal, not cosmetic: the kernel
    # refuses to exec an arm64 binary with a broken signature, so the
    # app dies with SIGKILL and no useful message. Ad-hoc (`--sign -`)
    # is enough for local testing; CI does the same thing.
    if codesign --force --deep --sign - "$APP_DIR" 2>/dev/null; then
        echo "  ✓ Ad-hoc signed"
    else
        echo "  ⚠ codesign failed — the app may refuse to launch on Apple Silicon."
        echo "    Re-sign manually: codesign --force --deep --sign - \"$APP_DIR\""
    fi
else
    cp "$BINARY" "$STAGE_DIR/fermi-console"
    chmod +x "$STAGE_DIR/fermi-console"
fi

# Agent cards (required for local execution).
#
# These stay in the *bundle* directory, beside `Fermi Console.app` on
# macOS — not inside Contents/Resources. The app resolves
# `agents/curated` relative to its working directory (or $AGENTS_DIR),
# and a copy inside the bundle would be invisible to that lookup anyway.
# run.sh cds into the bundle dir and exports AGENTS_DIR, which is why
# it's the recommended way to launch on both platforms.
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
#
# Assembled from three chunks so the install/run/troubleshooting bits
# can differ per platform while the ABW/workflow prose stays shared.
# Every heredoc delimiter is quoted — the body is full of backticks and
# `$200`, which an unquoted heredoc would happily mangle.
{
cat << 'READMEEOF'
# Fermi Console — Tester Build

**Version:** 0.10.0-dev
**What is this?** A native desktop app for probabilistic forecasting using AI agents.

## Quick Start
READMEEOF

if [ "$IS_MACOS" = "1" ]; then
cat << 'READMEMACEOF'

### 1. Install system dependencies

None. Metal and the frameworks the app links against ship with macOS.

### 2. Run

```bash
cd fermi-console-*        # this directory
./run.sh
```

`run.sh` is the recommended launcher: it cds into this directory and points
`AGENTS_DIR` at the bundled agent cards. Double-clicking or `open "Fermi
Console.app"` also works, but LaunchServices starts the app with `/` as its
working directory, so it won't find `agents/curated` unless you set
`AGENTS_DIR` yourself.

**First launch:** the app is ad-hoc code-signed but not Apple-notarized, so
Gatekeeper will block a plain double-click. Right-click `Fermi Console.app`
→ **Open** → **Open**, or allow it under
**System Settings → Privacy & Security → Open Anyway**. Only needed once.
READMEMACEOF
else
cat << 'READMELINUXEOF'

### 1. Install system dependencies

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
READMELINUXEOF
fi

cat << 'READMEEOF'

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
READMEEOF

if [ "$IS_MACOS" = "1" ]; then
cat << 'READMEMACEOF'

- **"Fermi Console" is damaged / can't be opened:** Gatekeeper quarantine. Run
  `xattr -dr com.apple.quarantine "Fermi Console.app"`, then right-click → Open.
- **App dies instantly with no window (Apple Silicon):** the code signature is
  broken. Re-sign it: `codesign --force --deep --sign - "Fermi Console.app"`.
- **Agents fail with "Sign in to ABW":** You need to sign in first. Go to Dashboard → Sign In.
- **"Failed to load agents":** Launch via `./run.sh` — see step 2. Double-clicking
  the .app starts it in `/`, where `agents/curated/` doesn't exist.
- **Window doesn't appear:** Try `RUST_LOG=info ./run.sh` for debug output.
- **Agent research returns empty/mock data:** Confirm you're signed in (Dashboard shows your name).

## Feedback

Please report issues with:
- What you were doing
- What you expected
- What happened instead
- Console output (`RUST_LOG=info ./run.sh 2>&1 | tee fermi.log`)
READMEMACEOF
else
cat << 'READMELINUXEOF'

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
READMELINUXEOF
fi
} > "$STAGE_DIR/README.md"

# GPUI binds these to the `secondary-` modifier, which is Cmd on macOS
# and Ctrl everywhere else — so the shortcut table and the workflow prose
# are wrong out of the box on a Mac. Apple's own convention omits the
# "+" (⌘S, not ⌘+S), hence the trailing plus is consumed too.
if [ "$IS_MACOS" = "1" ]; then
    sed 's/Ctrl+/⌘/g' "$STAGE_DIR/README.md" > "$STAGE_DIR/README.md.tmp"
    mv "$STAGE_DIR/README.md.tmp" "$STAGE_DIR/README.md"
fi

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

# On macOS the binary lives inside the .app bundle. We exec it directly
# rather than using `open`, so it inherits this CWD (and therefore finds
# agents/curated) and keeps its stdout/stderr attached to this terminal.
if [ -x "./Fermi Console.app/Contents/MacOS/fermi-console" ]; then
    exec "./Fermi Console.app/Contents/MacOS/fermi-console" "$@"
fi

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
if [ "$IS_MACOS" = "1" ]; then
    # bsdtar otherwise emits an AppleDouble `._foo` sidecar for every
    # file carrying extended attributes, which litters the archive and
    # confuses people extracting it on Linux. The bundle's signature is
    # embedded in the Mach-O and in Contents/_CodeSignature, not in
    # xattrs, so dropping them is safe.
    COPYFILE_DISABLE=1 tar czf "${BUNDLE_NAME}.tar.gz" "$BUNDLE_NAME"
else
    tar czf "${BUNDLE_NAME}.tar.gz" "$BUNDLE_NAME"
fi
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
if [ "$IS_MACOS" = "1" ]; then
    echo "  (or double-click '$APP_NAME' — first launch needs right-click → Open,"
    echo "   since the bundle is ad-hoc signed but not notarized. Note that"
    echo "   double-clicking starts the app in / , so it won't see agents/curated;"
    echo "   ./run.sh is the reliable path for testers.)"
    echo ""
fi
echo "  Then sign in via Dashboard → Google/GitHub to enable agents."
echo "═══════════════════════════════════════════════════════════"
