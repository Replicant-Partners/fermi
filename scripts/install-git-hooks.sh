#!/usr/bin/env bash
# Installs the project's git hooks into .git/hooks.
#
# Run once after cloning. Idempotent — re-running just refreshes the
# symlinks. We use symlinks (not copies) so the hooks stay in sync with
# the repo automatically as the canonical scripts under
# scripts/git-hooks/ evolve.
#
# Usage: ./scripts/install-git-hooks.sh

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
src_dir="$repo_root/scripts/git-hooks"
dst_dir="$repo_root/.git/hooks"

if [ ! -d "$src_dir" ]; then
    echo "Expected hooks at $src_dir but the directory is missing." >&2
    exit 1
fi

mkdir -p "$dst_dir"

for src in "$src_dir"/*; do
    [ -f "$src" ] || continue
    name=$(basename "$src")
    dst="$dst_dir/$name"
    # If a non-symlink hook already exists, back it up so we don't trample
    # local customisations.
    if [ -f "$dst" ] && [ ! -L "$dst" ]; then
        backup="$dst.before-install-$(date +%s)"
        echo "Backing up existing $name → $(basename "$backup")"
        mv "$dst" "$backup"
    fi
    ln -sf "../../scripts/git-hooks/$name" "$dst"
    chmod +x "$src"
    echo "Installed $name"
done

echo
echo "Done. Hooks now live under .git/hooks/ as symlinks to scripts/git-hooks/."
echo "Edit the tracked versions; the hooks update automatically."
