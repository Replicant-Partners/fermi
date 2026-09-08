#!/usr/bin/env python3
"""Break the coordination-notes seam five ways, and watch the guard go red.

`tests/coordination_notes_surface.rs` asserts both ends of one dependency: the
coordination-notes endpoint is keyed by an agent's **uuid**, the specimen page
is reached by **name**, and the panel therefore depends on `profile.agent_id`
being in the specimen payload.

That dependency fails invisibly. With no id the panel renders "the coordination
notes could not be read" — and the endpoint is honestly empty today, so the
panel is expected to render an empty state anyway. Neither a reader nor a
screenshot can tell those apart, which is the exact confusion these surfaces
exist to end. So the guard has to be shown to fire.

    python3 scripts/break_coordination_notes_seam.py [tree]

`tree` defaults to the repo root. Pass a detached worktree instead when the
working tree is mid-flight: `cargo` in a shared clone tells you nothing about
what you changed, and this harness compiles.

    git worktree add target/wt --detach HEAD
    # copy only your files in
    python3 scripts/break_coordination_notes_seam.py target/wt
"""

import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
TREE = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO

HANDLER = TREE / "src/handlers/specimen.rs"
PAGE = TREE / "templates/specimen.html"

# Two target dirs must never be shared between two trees: they will serve each
# other's test binaries, and a pristine worktree once reported 8 tests in a file
# containing 7. Silently. Under `target/` because /tmp is a small tmpfs.
ENV = {
    **os.environ,
    "CARGO_TARGET_DIR": str(REPO / "target" / f"break-notes-{TREE.name}"),
}

# (file, name, find, replace). A mutation that matched nothing is
# indistinguishable from a guard that did not fire, so every one asserts first.
BREAKS = [
    (
        HANDLER,
        "the handler stops serving agent_id",
        '            "agent_id": agent_id,\n',
        "",
    ),
    (
        HANDLER,
        "agent_id moved out of `profile` to the top level",
        '    Ok(Json(json!({\n        "profile": {',
        '    Ok(Json(json!({\n        "agent_id": agent_id,\n        "profile": {',
    ),
    (
        HANDLER,
        "  \u2014 and removed from `profile`",
        '            "agent_id": agent_id,\n            "agent_name"',
        '            "agent_name"',
    ),
    (
        PAGE,
        "the page stops reading profile.agent_id",
        "const id = (D.profile || {}).agent_id;",
        "const id = (D.profile || {}).uuid;",
    ),
    (
        PAGE,
        "the panel stops branching on consolidated",
        'n.consolidated ? "dreamt" : "not"',
        '"dreamt"',
    ),
    (
        PAGE,
        "the empty case composes its own sentence",
        'esc(NOTES.detail || "No coordination note on file.")',
        '"No coordination note on file."',
    ),
]

# The move is two edits on one file and has to be applied together. Applied
# singly it is a *duplication*, which leaves the key where the page reads it —
# and the guard is right to stay green for that. Getting this wrong is how a
# mutation comes back green and gets mistaken for a hole in the check.
PAIRED = {1: 2}


def run():
    r = subprocess.run(
        ["cargo", "test", "--test", "coordination_notes_surface"],
        capture_output=True,
        text=True,
        cwd=str(TREE),
        env=ENV,
    )
    return r.returncode, r.stdout + r.stderr


def reason(out):
    for line in out.splitlines():
        s = line.strip()
        if any(
            n in s
            for n in ("no longer", "not inside", "has no words", "does not distinguish")
        ):
            return s
    return "(no reason line \u2014 did it fail to compile?)"


def main():
    code, out = run()
    if code != 0:
        print("the guard is red BEFORE any mutation, so nothing below means anything:")
        print(out)
        return 2
    print(f"baseline green in {TREE}.\n")

    missed = []
    for i, (path, name, find, repl) in enumerate(BREAKS):
        if i in PAIRED.values():
            continue
        orig = path.read_text()
        if find not in orig:
            print(f"  SKIPPED  {name}\n           the pattern is not in the file, so the "
                  f"mutation would be a no-op.\n")
            missed.append(name + " (pattern absent)")
            continue
        mutated = orig.replace(find, repl, 1)
        label = name
        if i in PAIRED:
            _, name2, find2, repl2 = BREAKS[PAIRED[i]]
            assert find2 in mutated, "pattern absent: " + name2
            mutated = mutated.replace(find2, repl2, 1)
            label = name + name2
        path.write_text(mutated)
        try:
            code, out = run()
        finally:
            path.write_text(orig)
        if code == 0:
            print(f"  STILL GREEN  {label}\n               the guard cannot see this.\n")
            missed.append(label)
        else:
            print(f"  red          {label}\n               {reason(out)[:150]}\n")

    code, out = run()
    if code != 0:
        print("RESTORE FAILED \u2014 the tree is not back to green:")
        print(out)
        return 3

    if missed:
        print(f"\n{len(missed)} mutation(s) the guard could not catch:")
        for m in missed:
            print("  \u2716 " + m)
        print("\nFix the check, not the break \u2014 unless the break was the no-op.")
        return 1
    print("all breaks were caught, and the tree is green again.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
