#!/usr/bin/env bash
# Install the `abw` CLI — downloads the pre-built binary from GitHub Releases.
#
# Supported platforms (auto-detected):
#   - Linux x86_64
#   - Linux aarch64 (ARM64)
#   - macOS x86_64 (Intel)
#   - macOS aarch64 (Apple Silicon)
#   - Windows x86_64  (use Git Bash, WSL, or a POSIX shell)
#
# Usage:
#
#   curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/fermi/main/scripts/install-abw.sh | bash
#
# Environment overrides:
#
#   ABW_VERSION       — release tag to install (default: latest stable abw-v* tag)
#   ABW_INSTALL_DIR   — where to drop the binary (default: $HOME/.local/bin)
#   ABW_FORCE         — set to 1 to overwrite an existing install without prompting
#
# Examples:
#
#   # Install latest stable
#   curl -fsSL .../install-abw.sh | bash
#
#   # Install a specific release
#   ABW_VERSION=abw-v0.1.0 bash <(curl -fsSL .../install-abw.sh)
#
#   # Install system-wide (requires sudo)
#   ABW_INSTALL_DIR=/usr/local/bin sudo bash <(curl -fsSL .../install-abw.sh)

set -euo pipefail

REPO="Replicant-Partners/fermi"
BIN_NAME="abw"
INSTALL_DIR="${ABW_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${ABW_VERSION:-}"
FORCE="${ABW_FORCE:-0}"

# ── Pretty output ────────────────────────────────────────────────────

red()    { printf '\033[31m%s\033[0m\n' "$*"; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
bold()   { printf '\033[1m%s\033[0m\n' "$*"; }
dim()    { printf '\033[2m%s\033[0m\n' "$*"; }

die() { red "error: $*"; exit 1; }

# ── Preflight ────────────────────────────────────────────────────────

for cmd in curl uname mkdir chmod tar; do
    command -v "$cmd" >/dev/null 2>&1 || die "required command '$cmd' not found"
done

# ── Detect target triple ─────────────────────────────────────────────

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS:$ARCH" in
    Linux:x86_64)     TARGET="x86_64-unknown-linux-gnu"; EXT="tar.gz"; EXE="" ;;
    Linux:aarch64|Linux:arm64)
                      TARGET="aarch64-unknown-linux-gnu"; EXT="tar.gz"; EXE="" ;;
    Darwin:x86_64)    TARGET="x86_64-apple-darwin"; EXT="tar.gz"; EXE="" ;;
    Darwin:arm64)     TARGET="aarch64-apple-darwin"; EXT="tar.gz"; EXE="" ;;
    MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64)
                      TARGET="x86_64-pc-windows-msvc"; EXT="zip"; EXE=".exe" ;;
    *)
        die "unsupported platform: $OS $ARCH
        Try building from source instead:
            cargo install --git https://github.com/${REPO} --bin abw abw-cli" ;;
esac

bold "Installing abw CLI"
dim  "  platform: $OS $ARCH ($TARGET)"
dim  "  install:  $INSTALL_DIR"

# ── Resolve version ──────────────────────────────────────────────────

if [[ -z "$VERSION" ]]; then
    dim "  resolving latest abw-v* tag from GitHub…"
    # GitHub Releases API doesn't give a great way to filter by tag prefix.
    # We list recent releases and pick the first one whose tag starts with abw-v.
    RELEASES_JSON=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=30" \
        || die "failed to query GitHub releases — is the repo public and your network reachable?")

    # Pull the first release whose tag_name starts with abw-v and isn't a prerelease
    VERSION=$(printf '%s' "$RELEASES_JSON" \
        | grep -E '"tag_name"[[:space:]]*:[[:space:]]*"abw-v[0-9]' \
        | head -1 \
        | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')

    if [[ -z "$VERSION" ]]; then
        die "could not find any abw-v* release. Try setting ABW_VERSION=abw-latest to grab the prerelease channel."
    fi
fi
dim  "  version:  $VERSION"

# ── Download ─────────────────────────────────────────────────────────

ARCHIVE="${BIN_NAME}-${VERSION}-${TARGET}.${EXT}"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

bold "Downloading…"
dim "  $URL"
if ! curl -fsSL "$URL" -o "$TMP/$ARCHIVE"; then
    die "download failed. Check that the release '${VERSION}' published artifacts for ${TARGET}."
fi

# ── Extract ──────────────────────────────────────────────────────────

bold "Extracting…"
if [[ "$EXT" == "tar.gz" ]]; then
    tar -xzf "$TMP/$ARCHIVE" -C "$TMP" || die "extract failed"
elif [[ "$EXT" == "zip" ]]; then
    command -v unzip >/dev/null 2>&1 || die "'unzip' required for Windows install"
    unzip -q "$TMP/$ARCHIVE" -d "$TMP" || die "extract failed"
fi

if [[ ! -f "$TMP/${BIN_NAME}${EXE}" ]]; then
    die "extracted archive does not contain ${BIN_NAME}${EXE} — release may be malformed"
fi

# ── Install ──────────────────────────────────────────────────────────

mkdir -p "$INSTALL_DIR"
DEST="${INSTALL_DIR}/${BIN_NAME}${EXE}"

if [[ -e "$DEST" && "$FORCE" != "1" ]]; then
    if [[ -t 0 ]]; then
        yellow "  $DEST already exists. Overwrite? [y/N]"
        read -r reply
        case "$reply" in
            y|Y|yes) ;;
            *) die "aborted — set ABW_FORCE=1 to skip this prompt" ;;
        esac
    else
        # No tty (e.g. piped from curl) — refuse to clobber silently.
        die "$DEST already exists. Re-run with ABW_FORCE=1 to overwrite."
    fi
fi

mv "$TMP/${BIN_NAME}${EXE}" "$DEST"
chmod +x "$DEST"

# ── Verify ───────────────────────────────────────────────────────────

bold "Verifying…"
if ! "$DEST" --version >/dev/null 2>&1; then
    yellow "  warning: '$DEST --version' failed. Binary is installed but may not be runnable on this system."
fi

green "✓ abw installed at $DEST"

# ── Path advice ──────────────────────────────────────────────────────

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        # Already on PATH — nothing to say
        ;;
    *)
        echo
        yellow "⚠  $INSTALL_DIR is not on your \$PATH."
        case "$SHELL" in
            *zsh)
                dim "   Add to ~/.zshrc:"
                dim "     export PATH=\"$INSTALL_DIR:\$PATH\""
                ;;
            *bash)
                dim "   Add to ~/.bashrc (or ~/.bash_profile on macOS):"
                dim "     export PATH=\"$INSTALL_DIR:\$PATH\""
                ;;
            *fish)
                dim "   Add to ~/.config/fish/config.fish:"
                dim "     set -gx PATH $INSTALL_DIR \$PATH"
                ;;
            *)
                dim "   Add $INSTALL_DIR to your shell's PATH."
                ;;
        esac
        ;;
esac

echo
bold "Next steps:"
echo "  $BIN_NAME login                          # opens browser to /auth/cli"
echo "  $BIN_NAME app new my_first_app           # scaffold an App"
echo "  cd my_first_app && \$EDITOR manifest.json"
echo "  $BIN_NAME app deploy                     # register on the platform"
echo "  $BIN_NAME app spawn my_first_app --open  # create + open a workspace"
echo
dim "Docs:"
dim "  Creating an App:   https://github.com/${REPO}/blob/main/docs/specs/03_CREATING_APPS.md"
dim "  Runtime UX guide:  https://github.com/${REPO}/blob/main/docs/specs/07_BUILDING_WITH_ABW.md"
