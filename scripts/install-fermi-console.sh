#!/usr/bin/env bash
# Install fermi-console — downloads the pre-built binary from GitHub Releases.
# Requires: curl, apt (Ubuntu/Debian) or compatible runtime libs on other distros.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-fermi-console.sh | bash
#
# Or with a specific version:
#   FERMI_VERSION=v0.1.0 bash <(curl -fsSL ...)

set -euo pipefail

REPO="Replicant-Partners/fermi"
BINARY="fermi-console-linux-x86_64"
INSTALL_DIR="${FERMI_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${FERMI_VERSION:-latest}"

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
bold()  { printf '\033[1m%s\033[0m\n' "$*"; }

# ── Detect OS ────────────────────────────────────────────────────────
if [[ "$(uname -s)" != "Linux" ]]; then
  red "fermi-console pre-built binaries are currently Linux-only."
  red "On macOS/Windows, build from source: cargo build --release -p fermi-console"
  exit 1
fi

bold "Installing fermi-console..."

# ── Install runtime system libraries ─────────────────────────────────
if command -v apt-get &>/dev/null; then
  echo "Installing runtime dependencies via apt..."
  sudo apt-get install -y -qq \
    libxcb1 \
    libxkbcommon0 \
    libxkbcommon-x11-0 \
    libfontconfig1 \
    libfreetype6 \
    libwayland-client0 \
    libvulkan1 \
    mesa-vulkan-drivers 2>/dev/null || true
else
  echo "Note: apt not found. Ensure these libraries are installed:"
  echo "  libxcb, libxkbcommon, libfontconfig, libfreetype, libwayland, libvulkan"
fi

# ── Resolve download URL ──────────────────────────────────────────────
if [[ "$VERSION" == "latest" ]]; then
  DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY}"
else
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}"
fi

# ── Download ──────────────────────────────────────────────────────────
mkdir -p "$INSTALL_DIR"
DEST="$INSTALL_DIR/fermi-console"

echo "Downloading from: $DOWNLOAD_URL"
if ! curl -fL --progress-bar "$DOWNLOAD_URL" -o "$DEST"; then
  red "Download failed. Check that a release exists at:"
  red "  https://github.com/${REPO}/releases"
  exit 1
fi
chmod +x "$DEST"

# ── Verify it runs ────────────────────────────────────────────────────
if ! "$DEST" --version &>/dev/null 2>&1; then
  # GPUI apps don't have --version, just check the binary is executable
  if [[ -x "$DEST" ]]; then
    green "Binary installed at $DEST"
  else
    red "Binary is not executable — something went wrong."
    exit 1
  fi
else
  green "Binary installed at $DEST"
fi

# ── PATH hint ─────────────────────────────────────────────────────────
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "Add $INSTALL_DIR to your PATH to run fermi-console from anywhere:"
  echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
fi

echo ""
green "Done! Run: fermi-console"
echo "  (or: $DEST)"
