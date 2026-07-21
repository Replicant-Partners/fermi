#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
#  Fermi Console installer  —  Linux x86_64
#
#  What it does, in order:
#    1. Installs the GPUI runtime libraries via apt (asks for sudo once).
#    2. Downloads the latest fermi-console binary from GitHub Releases.
#    3. Verifies it's a real ELF64 executable (not an HTML error page).
#    4. Installs to ~/.local/bin and makes it executable.
#    5. Adds ~/.local/bin to PATH in the user's shell rc if missing.
#    6. Prints a "you're done" message with the launch command.
#
#  Usage:
#    curl -fsSL https://YOUR-HOST/install.sh | bash
#
#  Env knobs (all optional):
#    FERMI_VERSION=v0.8.0      pin a specific release; default: latest
#    FERMI_INSTALL_DIR=/path   override install dir; default: ~/.local/bin
#    FERMI_SKIP_APT=1          skip the apt step (BYO runtime libs)
#    FERMI_NO_PATH_EDIT=1      don't touch ~/.bashrc etc.
# ─────────────────────────────────────────────────────────────────────

set -euo pipefail

REPO="Replicant-Partners/fermi"
BINARY="fermi-console-linux-x86_64"
INSTALL_DIR="${FERMI_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${FERMI_VERSION:-latest}"

# ─── Tiny terminal helpers ───────────────────────────────────────────
# Only emit escape sequences when stdout is a TTY so piping through
# `less` or logging to a file stays clean.
if [ -t 1 ]; then
  BOLD=$'\033[1m'
  DIM=$'\033[2m'
  RED=$'\033[31m'
  GREEN=$'\033[32m'
  YELLOW=$'\033[33m'
  RESET=$'\033[0m'
else
  BOLD=""; DIM=""; RED=""; GREEN=""; YELLOW=""; RESET=""
fi

say()   { printf '%s\n' "$*"; }
step()  { printf '\n%s▸ %s%s\n' "$BOLD" "$*" "$RESET"; }
ok()    { printf '  %s✓%s %s\n' "$GREEN" "$RESET" "$*"; }
warn()  { printf '  %s⚠%s %s\n' "$YELLOW" "$RESET" "$*"; }
fail()  { printf '\n%s✗ %s%s\n' "$RED" "$*" "$RESET" >&2; exit 1; }

# ─── Banner ──────────────────────────────────────────────────────────
printf '%s\n' "$BOLD"
cat <<'BANNER'
  ┌─────────────────────────────────────────┐
  │  Fermi Console — installer              │
  │  Native forecasting workspace for Linux │
  └─────────────────────────────────────────┘
BANNER
printf '%s\n' "$RESET"

# ─── Preflight ───────────────────────────────────────────────────────
[ "$(uname -s)" = "Linux" ] || \
  fail "This installer is Linux-only. On macOS/Windows, build from source: cargo build --release -p fermi-console"

[ "$(uname -m)" = "x86_64" ] || \
  fail "Pre-built binaries are x86_64 only (detected: $(uname -m)). Build from source for other arches."

command -v curl >/dev/null 2>&1 || fail "This script needs 'curl'. Please install it and re-run."

# ─── Step 1: runtime libraries ──────────────────────────────────────
if [ "${FERMI_SKIP_APT:-}" = "1" ]; then
  step "Runtime libraries (skipped — FERMI_SKIP_APT=1)"
elif command -v apt-get >/dev/null 2>&1; then
  step "Installing runtime libraries (may ask for your password)…"
  # The GPUI stack needs Vulkan + Wayland + XCB + fontconfig. Missing
  # any one of these causes the app to launch but never draw a window,
  # which is the #1 support issue.
  PKGS="libxcb1 libxkbcommon0 libxkbcommon-x11-0 libfontconfig1 libfreetype6 libwayland-client0 libvulkan1 mesa-vulkan-drivers"
  if sudo -n true 2>/dev/null; then
    sudo apt-get update -qq && sudo apt-get install -y -qq $PKGS
  else
    say "  ${DIM}(sudo will prompt for your login password)${RESET}"
    sudo apt-get update -qq && sudo apt-get install -y -qq $PKGS
  fi
  ok "Runtime libraries ready"
else
  step "Runtime libraries"
  warn "apt-get not found — this looks like a non-Debian distro."
  warn "Please make sure these libraries are installed via your package manager:"
  warn "  libxcb, libxkbcommon, libxkbcommon-x11, libfontconfig, libfreetype, libwayland, libvulkan"
fi

# ─── Step 2: download binary ────────────────────────────────────────
step "Downloading Fermi Console (${VERSION})…"

if [ "$VERSION" = "latest" ]; then
  DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY}"
else
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}"
fi

mkdir -p "$INSTALL_DIR"
DEST="$INSTALL_DIR/fermi-console"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

# --progress-bar renders a single-line bar over curl's default noise,
# so the tester sees actual progress instead of nothing until it's done.
# The 2>&1 lets the bar update over the same line in a piped context.
if ! curl -fL --progress-bar "$DOWNLOAD_URL" -o "$TMP"; then
  fail "Download failed. Check that a release exists at https://github.com/${REPO}/releases"
fi

# ─── Step 3: sanity-check the download ──────────────────────────────
# The single most common failure mode of "curl into bash" installers
# is silently downloading a 4KB HTML error page from a CDN and calling
# it a day. Guard against that.
SIZE=$(wc -c < "$TMP")
if [ "$SIZE" -lt 1048576 ]; then
  fail "Downloaded file is only ${SIZE} bytes — expected multi-MB binary. Aborting."
fi

# ELF magic bytes: 0x7F 'E' 'L' 'F'. `file` isn't always installed;
# a raw byte check is portable and enough.
if [ "$(head -c 4 "$TMP" | od -An -c | tr -d ' \n')" != "177ELF" ]; then
  fail "Downloaded file is not an ELF binary. Aborting so we don't install junk."
fi

mv "$TMP" "$DEST"
chmod +x "$DEST"
ok "Installed to $DEST"

# ─── Step 4: PATH ────────────────────────────────────────────────────
NEEDS_PATH_HINT=0
case ":$PATH:" in
  *":$INSTALL_DIR:"*) : ;;
  *) NEEDS_PATH_HINT=1 ;;
esac

if [ "$NEEDS_PATH_HINT" = "1" ] && [ "${FERMI_NO_PATH_EDIT:-}" != "1" ]; then
  step "Adding $INSTALL_DIR to your PATH…"
  # Only edit the shell rc that matches the invoking shell. Best-effort;
  # idempotent (we grep first so re-running the installer is a no-op).
  RC=""
  case "$(basename "${SHELL:-/bin/bash}")" in
    zsh)  RC="$HOME/.zshrc" ;;
    fish) RC="$HOME/.config/fish/config.fish" ;;
    *)    RC="$HOME/.bashrc" ;;
  esac

  if [ -n "$RC" ]; then
    mkdir -p "$(dirname "$RC")"
    touch "$RC"
    if ! grep -Fq "$INSTALL_DIR" "$RC"; then
      # Fish uses a different syntax — respect that.
      if [ "$(basename "$RC")" = "config.fish" ]; then
        printf '\n# Added by Fermi Console installer\nfish_add_path %s\n' "$INSTALL_DIR" >> "$RC"
      else
        printf '\n# Added by Fermi Console installer\nexport PATH="%s:$PATH"\n' "$INSTALL_DIR" >> "$RC"
      fi
      ok "Updated $RC"
    else
      ok "$RC already has $INSTALL_DIR"
    fi
  fi
fi

# ─── Done ────────────────────────────────────────────────────────────
printf '\n%s────────────────────────────────────────────────────────────%s\n' "$GREEN" "$RESET"
printf '  %s✓ Fermi Console installed%s\n' "$BOLD" "$RESET"
printf '%s────────────────────────────────────────────────────────────%s\n\n' "$GREEN" "$RESET"

if [ "$NEEDS_PATH_HINT" = "1" ]; then
  say "To launch it now, run:"
  printf '  %sfermi-console%s\n' "$BOLD" "$RESET"
  say ""
  say "(if 'fermi-console' isn't found, open a new terminal window first,"
  say " or run: ${BOLD}source ${RC:-~/.bashrc}${RESET})"
else
  say "Launch it with:"
  printf '  %sfermi-console%s\n' "$BOLD" "$RESET"
fi

say ""
say "${DIM}Updates are handled inside the app — Help → Check for Updates.${RESET}"
