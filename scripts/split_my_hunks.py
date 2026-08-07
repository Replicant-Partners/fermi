#!/usr/bin/env python3
"""Stage only *my* hunks from a file shared with a concurrently-running agent.

Another agent edits this repo at the same time. Committing `git add <file>`
on a shared file would sweep its half-finished work into my release commit,
so every shared file is split hunk-by-hunk.

Classification is by content marker, and the rule is **refuse on ambiguity**:
a hunk that matches both sides, or neither, stops the run and gets looked at
by a human. A previous attempt at this false-matched on the word
"capabilities" (agent cards have capabilities too) and nearly committed
someone else's work, hence the deliberate absence of a fallback.

Usage:  split_my_hunks.py <file> [<file> ...]
"""
import re
import subprocess
import sys
import tempfile

# Unambiguous: these words do not appear in the other agent's work.
MINE = [
    "annotation",
    "Annotation",
    "Assumptions",
    "FORECAST_LEVEL_KEY",
    "Spec 32",
    "share_permission_in_flight",
    "set_share_permission",
    "promote-",
    "contested_assumption",
    "driver_annotations",
    "forecast_git",
    "commit_files_as",
    "history_reconcile",
    "183_driver_annotations",
    "mark_orphaned_annotations",
    "driver_names_in",
    "grant edit access",
    "ungrounded",
    "rollup",
    "ROLLUP",
    "roll-up",
    "rolled-up",
    "contested_assumption",
    "SurfaceItem",
    "surface_items",
    "Events vs conditions",
    "EVENT",
    "CONDITION",
]

# The other agent's active specs (28 credentials, 29 orchestra) plus the
# incidental UI work it has in flight.
THEIRS = [
    "SPEC_28",
    "SPEC_29",
    "credential",
    "Credential",
    "schema_trust",
    "orchestra",
    "Orchestra",
    "mcp_tools",
    "server_agent_cards",
    "save_disabled",
    "agent_picker",
    "stripe",
    "Stripe",
    "integrity_reconciliation",
    "180_orchestra_members",
    "mod keys",
    "keys::",
    "menu_row",
    "secondary-",
    "simops",
    "dynamics",
    "keys::",
    "mod keys",
    "menu_row",
    "secondary-",
    "target_os = \"macos\"",
    "SCHEMA_AND_RULE_INTEGRITY",
    "126_agent_version_full_config",
    "agent_versions",
    "embedding_provenance",
    "2026-08-06 audit",
]


def hunks(path):
    """Split `git diff -U3 -- path` into (header_lines, [hunk_text])."""
    diff = subprocess.run(
        ["git", "--no-pager", "diff", "-U3", "--", path],
        capture_output=True, text=True, check=True,
    ).stdout
    if not diff.strip():
        return None, []
    lines = diff.split("\n")
    first = next(i for i, l in enumerate(lines) if l.startswith("@@"))
    header = lines[:first]
    starts = [i for i, l in enumerate(lines) if l.startswith("@@")]
    out = []
    for n, i in enumerate(starts):
        end = starts[n + 1] if n + 1 < len(starts) else len(lines)
        out.append("\n".join(lines[i:end]).rstrip("\n"))
    return header, out


def classify(hunk):
    """'mine' | 'theirs', or raise on an ambiguous hunk."""
    # Only changed lines vote. Context lines belong to whoever was there
    # first and would otherwise misattribute a hunk that merely sits near
    # someone else's code.
    changed = "\n".join(
        l for l in hunk.split("\n") if l[:1] in "+-" and not l.startswith(("+++", "---"))
    )
    mine = [m for m in MINE if m in changed]
    theirs = [t for t in THEIRS if t in changed]
    if mine and not theirs:
        return "mine"
    if theirs and not mine:
        return "theirs"
    raise SystemExit(
        f"AMBIGUOUS HUNK — refusing.\n"
        f"  matched mine:   {mine}\n"
        f"  matched theirs: {theirs}\n"
        f"{hunk[:1500]}\n"
    )


def main(paths):
    for path in paths:
        header, hs = hunks(path)
        if not hs:
            print(f"  {path}: no changes")
            continue
        keep = [h for h in hs if classify(h) == "mine"]
        print(f"  {path}: {len(keep)}/{len(hs)} hunks are mine")
        if not keep:
            continue
        patch = "\n".join(header + keep) + "\n"
        with tempfile.NamedTemporaryFile("w", suffix=".patch", delete=False) as f:
            f.write(patch)
            name = f.name
        # --recount: dropping hunks invalidates the new-side line counts in
        # the remaining headers. git locates hunks by context anyway.
        subprocess.run(
            ["git", "apply", "--cached", "--recount", name], check=True
        )


if __name__ == "__main__":
    main(sys.argv[1:])
