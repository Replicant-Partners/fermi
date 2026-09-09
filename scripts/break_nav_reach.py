#!/usr/bin/env python3
"""Break the nav three ways, and watch `tests/nav_reaches_every_page.rs` go red.

The first mutation is the bug itself: remove `window.Nav = Nav` from the widget
and nine templates go back to rendering with no header and nothing thrown. That
state shipped, was invisible in every log, and was found by screenshot -- so the
guard's whole value is that this mutation is loud.

    python3 scripts/break_nav_reach.py                 # this tree
    python3 scripts/break_nav_reach.py /tmp/fv-nav     # a worktree

`cargo` in a shared clone tells you nothing when another session's tree is
mid-flight, so the second form exists; see
`scripts/break_create_page_verdict.py` for the worktree recipe, and note that
two worktrees must not share a CARGO_TARGET_DIR.
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO

NAV = ROOT / "static" / "js" / "widgets" / "nav.js"
BESTIARY = ROOT / "templates" / "bestiary.html"

CARGO = [
    "cargo", "test", "--test", "nav_reaches_every_page",
    "--manifest-path", str(ROOT / "Cargo.toml"),
]


def unexpose(s):
    """The bug: `const Nav` is lexical, so `window.Nav` stays undefined."""
    pat = "window.Nav = Nav;"
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "// window.Nav = Nav;")


def load_without_init(s):
    """A page that pays for the script and renders no header."""
    pat = "Nav.init({ current: \"bestiary\" })"
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "void 0")


def init_without_loading(s):
    """A page that initialises a widget it never loaded."""
    pat = '<script src="/static/js/widgets/nav.js"></script>'
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "")


# (label, file, mutation, test that must go red)
BREAKS = [
    ("stop exposing window.Nav (the original bug)", NAV, unexpose,
     "the_nav_is_reachable_the_way_templates_reach_for_it"),
    ("load nav.js and never init it", BESTIARY, load_without_init,
     "loading_the_nav_and_initialising_it_go_together"),
    ("init the nav without loading it", BESTIARY, init_without_loading,
     "loading_the_nav_and_initialising_it_go_together"),
]


def run():
    p = subprocess.run(CARGO, capture_output=True, text=True, env=dict(os.environ))
    return p.returncode, p.stdout + p.stderr


def failed_tests(out):
    # `test result: FAILED. ...` also starts with "test " and contains FAILED.
    return [
        line for line in out.splitlines()
        if line.startswith("test ")
        and not line.startswith("test result:")
        and line.rstrip().endswith("FAILED")
    ]


def main():
    for f in (NAV, BESTIARY):
        if not f.exists():
            print(f"no such file: {f}")
            return 1

    rc, out = run()
    if rc != 0:
        print("BASELINE IS NOT GREEN -- nothing below means anything.")
        print(out[-3000:])
        return 1
    print(f"baseline green under {ROOT}\n")

    problems = 0
    originals = {f: f.read_text() for f in (NAV, BESTIARY)}
    try:
        for label, target, mutate, expect in BREAKS:
            target.write_text(mutate(originals[target]))
            rc, out = run()
            target.write_text(originals[target])

            red = failed_tests(out)
            fired = any(expect in line for line in red)
            collateral = [line for line in red if expect not in line]

            print(f"{'RED  ' if fired else 'GREEN'}  {label}")
            print(f"         expected red: {expect}")
            if not fired:
                print("         *** THE GUARD DID NOT FIRE ***")
                problems += 1
            if collateral:
                print(f"         also red, should be none: {collateral}")
                problems += 1
    finally:
        for f, body in originals.items():
            f.write_text(body)
            assert f.read_text() == body, f"FAILED TO RESTORE {f}"

    rc, _ = run()
    if rc != 0:
        print("\nbaseline did NOT return to green after restore")
        problems += 1
    else:
        print("\nbaseline restored green")

    print("\nRESULT:", "every guard fires on its own mutation and no other"
          if not problems else f"{problems} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
