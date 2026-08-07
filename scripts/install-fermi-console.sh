#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
#  Fermi Console installer  —  Linux x86_64 · macOS (Apple Silicon + Intel)
#
#  What it does, in order:
#    1. Works out which pre-built binary this machine needs.
#    2. On Linux: installs the GPUI runtime libraries via apt (asks for
#       sudo once). On macOS: nothing to install — Metal and the system
#       frameworks GPUI links against ship with the OS.
#    3. Downloads the matching fermi-console binary from GitHub Releases.
#    4. Verifies it's a real executable (ELF64 on Linux, Mach-O on
#       macOS) and not an HTML error page.
#    5. Installs to ~/.local/bin and makes it executable.
#    6. On macOS: clears the quarantine flag and checks the ad-hoc
#       signature survived the trip.
#    7. Adds ~/.local/bin to PATH in the user's shell rc if missing.
#    8. Prints a "you're done" message with the launch command.
#
#  Usage:
#    curl -fsSL https://YOUR-HOST/install.sh | bash
#
#  Env knobs (all optional):
#    FERMI_VERSION=v0.8.0      pin a specific release; default: latest
#    FERMI_INSTALL_DIR=/path   override install dir; default: ~/.local/bin
#    FERMI_SKIP_APT=1          skip the apt step (BYO runtime libs).
#                              No effect on macOS — that step never runs.
#    FERMI_NO_PATH_EDIT=1      don't touch ~/.bashrc / ~/.zshrc etc.
# ─────────────────────────────────────────────────────────────────────

# NOTE: macOS still ships bash 3.2 (GPL2 licensing), so everything below
# has to stay 3.2-clean — no associative arrays, no ${var^^}, no `mapfile`.
set -euo pipefail

# The default download host is baked in but overridable so testers on
# a staging domain, or an operator running the installer locally, can
# point at a different server without editing this script.
DOWNLOAD_HOST="${FERMI_DOWNLOAD_HOST:-https://agent-bestiary.world}"
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
  ┌──────────────────────────────────────┐
  │  Fermi Console — installer           │
  │  Native forecasting workspace        │
  │  Linux x86_64 · macOS arm64 / x86_64 │
  └──────────────────────────────────────┘
BANNER
printf '%s\n' "$RESET"

# ─── Preflight: which build do we need? ──────────────────────────────
# The release workflow publishes one asset per platform slug, and the
# server's download redirect takes that slug as `?platform=`. Keep these
# three strings in lockstep with the workflow's matrix — a typo here
# silently downgrades to the Linux default on the server side.
UNAME_S="$(uname -s)"
UNAME_M="$(uname -m)"

PLATFORM=""
IS_MACOS=0
case "${UNAME_S}/${UNAME_M}" in
  Linux/x86_64)  PLATFORM="linux-x86_64" ;;
  Darwin/arm64)  PLATFORM="macos-aarch64"; IS_MACOS=1 ;;
  Darwin/x86_64) PLATFORM="macos-x86_64";  IS_MACOS=1 ;;
esac

if [ -z "$PLATFORM" ]; then
  fail "No pre-built binary for ${UNAME_S} ${UNAME_M}. Supported: Linux x86_64, macOS arm64, macOS x86_64. Elsewhere, build from source: cargo build --release -p fermi-console"
fi

# Rosetta wrinkle: when the *shell* is running translated (an x86_64
# Homebrew setup, or Terminal.app with "Open using Rosetta" ticked),
# `uname -m` reports x86_64 even on Apple Silicon. That would hand the
# user an emulated build with no Metal fast path. `sysctl.proc_translated`
# is Apple's own answer to "am I being translated right now": 1 under
# Rosetta, 0 natively, and the key doesn't exist at all on a real Intel
# Mac (hence the `|| echo 0`).
if [ "$PLATFORM" = "macos-x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
  PLATFORM="macos-aarch64"
  warn "This shell is running under Rosetta — installing the native Apple Silicon build instead."
fi

command -v curl >/dev/null 2>&1 || fail "This script needs 'curl'. Please install it and re-run."

# ─── Step 1: runtime libraries ──────────────────────────────────────
# macOS has no equivalent step: GPUI renders through Metal, and
# CoreText/CoreFoundation/AppKit are part of the OS. There is genuinely
# nothing to install, so we don't pretend otherwise (and never ask for
# sudo on a Mac).
if [ "$IS_MACOS" = "1" ]; then
  step "Runtime libraries (not needed on macOS — Metal and system frameworks ship with the OS)"
elif [ "${FERMI_SKIP_APT:-}" = "1" ]; then
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
step "Downloading Fermi Console (${VERSION}, ${PLATFORM})…"

# We hit the server's `/fermi-console/download` redirect, not GitHub
# directly. That keeps the release backend swappable and works while
# the source repo is private (GitHub returns 404 anonymously for
# private-repo release assets — this indirection lets ops point the
# redirect at an R2 bucket or self-hosted URL instead).
#
# `platform=` is what picks the right asset. The server defaults to
# linux-x86_64 when it's absent so older installed clients keep working,
# which also means a missing/misspelled slug fails as a *Linux* download
# on a Mac — hence the Mach-O magic check below.
if [ "$VERSION" = "latest" ]; then
  DOWNLOAD_URL="${DOWNLOAD_HOST}/fermi-console/download?platform=${PLATFORM}"
else
  DOWNLOAD_URL="${DOWNLOAD_HOST}/fermi-console/download?v=${VERSION}&platform=${PLATFORM}"
fi

mkdir -p "$INSTALL_DIR"
DEST="$INSTALL_DIR/fermi-console"
# GNU mktemp defaults to a template when given none; BSD/macOS mktemp
# exits with a usage error instead. Always pass an explicit template.
TMP="$(mktemp "${TMPDIR:-/tmp}/fermi-console.XXXXXX")"
trap 'rm -f "$TMP"' EXIT

# --progress-bar renders a single-line bar over curl's default noise,
# so the tester sees actual progress instead of nothing until it's done.
# The 2>&1 lets the bar update over the same line in a piped context.
# `-L` follows the redirect from /fermi-console/download to the actual
# binary host. `-f` makes curl fail on 4xx/5xx so we surface real errors.
if ! curl -fL --progress-bar "$DOWNLOAD_URL" -o "$TMP"; then
  fail "Download failed. If this persists, ask the maintainer whether a ${PLATFORM} build has been published yet."
fi

# ─── Step 3: sanity-check the download ──────────────────────────────
# The single most common failure mode of "curl into bash" installers
# is silently downloading a 4KB HTML error page from a CDN and calling
# it a day. Guard against that.
SIZE=$(wc -c < "$TMP")
if [ "$SIZE" -lt 1048576 ]; then
  fail "Downloaded file is only ${SIZE} bytes — expected multi-MB binary. Aborting."
fi

# `file` isn't always installed; a raw magic-byte check is portable and
# enough. This also catches "wrong platform asset", not just error pages.
if [ "$IS_MACOS" = "1" ]; then
  # Mach-O magic, first four bytes, as hex:
  #   cf fa ed fe → MH_MAGIC_64 little-endian. Every modern macOS
  #                 binary is 64-bit LE, arm64 and x86_64 alike.
  #   ca fe ba be → FAT_MAGIC, a universal ("fat") archive holding
  #                 several thin slices. We don't ship universal
  #                 binaries today, but accepting it means a future
  #                 switch to `lipo`-merged assets doesn't break every
  #                 already-installed copy of this script.
  MAGIC="$(head -c 4 "$TMP" | od -An -t x1 | tr -d ' \n')"
  case "$MAGIC" in
    cffaedfe|cafebabe) : ;;
    *) fail "Downloaded file is not a Mach-O binary (magic: ${MAGIC}). Aborting so we don't install junk." ;;
  esac
else
  # ELF magic bytes: 0x7F 'E' 'L' 'F'.
  if [ "$(head -c 4 "$TMP" | od -An -c | tr -d ' \n')" != "177ELF" ]; then
    fail "Downloaded file is not an ELF binary. Aborting so we don't install junk."
  fi
fi

mv "$TMP" "$DEST"
chmod +x "$DEST"
ok "Installed to $DEST"

# ─── Step 3b: macOS Gatekeeper housekeeping ─────────────────────────
if [ "$IS_MACOS" = "1" ]; then
  # curl doesn't attach com.apple.quarantine — that's a LaunchServices
  # thing applied by browsers, Mail, AirDrop and some corporate proxies.
  # But testers do sometimes re-download by hand, and a quarantined
  # binary gets killed with an unhelpful "cannot be opened" dialog. It
  # costs nothing to clear it, and `|| true` keeps the common case
  # (attribute not present → xattr exits non-zero) from tripping `set -e`.
  xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true

  # Our macOS builds are ad-hoc signed (`codesign -s -`), not notarized.
  # On Apple Silicon an ad-hoc signature is not optional: the kernel
  # refuses to exec an arm64 binary whose signature is missing or stale,
  # so a byte-mangling proxy or a truncated download shows up as "killed"
  # rather than a nice error. Warn loudly, but don't fail — an Intel Mac
  # runs fine either way, and the user may still want the file.
  if command -v codesign >/dev/null 2>&1; then
    if codesign -v "$DEST" 2>/dev/null; then
      ok "Code signature verified"
    else
      warn "Code signature is missing or invalid — the app may refuse to launch."
      warn "Try re-running this installer; if it persists, re-sign locally with:"
      warn "  codesign --force --sign - \"$DEST\""
    fi
  fi
fi

# ─── Step 4: PATH ────────────────────────────────────────────────────
NEEDS_PATH_HINT=0
case ":$PATH:" in
  *":$INSTALL_DIR:"*) : ;;
  *) NEEDS_PATH_HINT=1 ;;
esac

# macOS has defaulted to zsh since Catalina, so when $SHELL is unset
# (cron, some CI containers, `env -i`) guessing bash would write a
# .bashrc that nothing ever sources.
if [ "$IS_MACOS" = "1" ]; then
  DEFAULT_SHELL="/bin/zsh"
  FALLBACK_RC="$HOME/.zshrc"
  FALLBACK_RC_LABEL="~/.zshrc"
else
  DEFAULT_SHELL="/bin/bash"
  FALLBACK_RC="$HOME/.bashrc"
  FALLBACK_RC_LABEL="~/.bashrc"
fi

if [ "$NEEDS_PATH_HINT" = "1" ] && [ "${FERMI_NO_PATH_EDIT:-}" != "1" ]; then
  step "Adding $INSTALL_DIR to your PATH…"
  # Only edit the shell rc that matches the invoking shell. Best-effort;
  # idempotent (we grep first so re-running the installer is a no-op).
  RC=""
  case "$(basename "${SHELL:-$DEFAULT_SHELL}")" in
    zsh)  RC="$HOME/.zshrc" ;;
    fish) RC="$HOME/.config/fish/config.fish" ;;
    bash) RC="$HOME/.bashrc" ;;
    *)    RC="$FALLBACK_RC" ;;
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
  say " or run: ${BOLD}source ${RC:-$FALLBACK_RC_LABEL}${RESET})"
else
  say "Launch it with:"
  printf '  %sfermi-console%s\n' "$BOLD" "$RESET"
fi

if [ "$IS_MACOS" = "1" ]; then
  say ""
  say "${DIM}Want a Dock icon instead of a terminal command? Grab the${RESET}"
  say "${DIM}'Fermi Console.app' zip for ${PLATFORM} from the GitHub release:${RESET}"
  say "${DIM}  https://github.com/Replicant-Partners/fermi/releases${RESET}"
  say "${DIM}It's ad-hoc signed but not notarized, so the first launch needs${RESET}"
  say "${DIM}right-click → Open (or System Settings → Privacy & Security).${RESET}"
fi

say ""
say "${DIM}Updates are handled inside the app — Help → Check for Updates.${RESET}"
