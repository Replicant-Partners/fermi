#!/usr/bin/env python3
"""Break the create page four ways, and watch `tests/create_page_verdict.rs` go red.

The guard holds one property: *the create page reads the publish gate, it does
not imitate it.* That property has two halves and either alone is trivially
satisfiable -- a page showing nothing imitates nothing -- so the guard has four
tests and each mutation here must turn exactly one of them red and no others.

Two of the mutations reinstate the defect that was actually reported:
`buildReview` held five hand-written checks, one claiming "would pass the
publish gate", on a page that never called `run_publish_checks`.

    python3 scripts/break_create_page_verdict.py                    # this tree
    python3 scripts/break_create_page_verdict.py /tmp/fv-guard      # a worktree

## Why the second form exists

`cargo` in the main working tree tells you nothing when another session shares
the clone -- its tree is frequently mid-flight and uncompilable, and a failure
you did not cause is indistinguishable from a guard that fired. The reliable
form is a detached worktree with its own target dir, which must NOT be shared
with another worktree (two worktrees sharing one `CARGO_TARGET_DIR` serve each
other's test binaries, and that failure produces plausible results):

    git worktree add /tmp/fv-guard --detach HEAD
    cp .env /tmp/fv-guard/.env
    touch /tmp/fv-guard/src/lib.rs /tmp/fv-guard/agent-bestiary/memory/src/*.rs
    CARGO_TARGET_DIR=$PWD/target/vguard \\
      python3 scripts/break_create_page_verdict.py /tmp/fv-guard
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO
PAGE = ROOT / "templates" / "agent_create.html"
MANIFEST = ROOT / "Cargo.toml"

CARGO = [
    "cargo", "test", "--test", "create_page_verdict",
    "--manifest-path", str(MANIFEST),
]


def drop_fetch(s):
    """The page stops asking the platform at all."""
    pat = "/publish-checks"
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "/nothing-at-all")


def drop_can_publish(s):
    """The page stops reading the platform's own reduction of its checks."""
    pat = "can_publish"
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "ok_to_go")


def collapse_standards(s):
    """Legality and ambition roll back into one list, one colour ramp."""
    pat = "As designed"
    assert pat in s, f"pattern absent: {pat}"
    return s.replace(pat, "Other stuff")


def rehardcode_a_check(s):
    """The reported defect: the page writing a gate check's name itself."""
    anchor = '<div class="verdict" id="verdict"></div>'
    assert anchor in s, f"pattern absent: {anchor}"
    return s.replace(
        anchor,
        anchor + "\n<!-- has_valence: the affective signature is required -->",
    )


# (label, the test that must go red, mutation)
BREAKS = [
    ("drop the publish-checks fetch",
     "the_page_asks_the_platform", drop_fetch),
    ("stop reading can_publish",
     "the_verdict_is_read_and_not_derived", drop_can_publish),
    ("collapse the two standards",
     "both_standards_are_rendered", collapse_standards),
    ("re-hardcode a gate check name",
     "no_publish_check_is_hardcoded_in_the_page", rehardcode_a_check),
]


def run():
    p = subprocess.run(CARGO, capture_output=True, text=True, env=dict(os.environ))
    return p.returncode, p.stdout + p.stderr


def failed_tests(out):
    # `test result: FAILED. ...` also starts with "test " and contains FAILED.
    # Counting it as a case made every correctly-isolated guard look like a
    # collateral failure: the summary line masquerading as a test.
    return [
        line for line in out.splitlines()
        if line.startswith("test ")
        and not line.startswith("test result:")
        and line.rstrip().endswith("FAILED")
    ]


def main():
    if not PAGE.exists():
        print(f"no such page: {PAGE}")
        return 1

    original = PAGE.read_text()

    rc, out = run()
    if rc != 0:
        print("BASELINE IS NOT GREEN -- nothing below means anything.")
        print("If this is the shared working tree, use a detached worktree;")
        print("see this file's docstring.")
        print(out[-3000:])
        return 1
    print(f"baseline green: {PAGE}\n")

    problems = 0
    try:
        for label, expect, mutate in BREAKS:
            PAGE.write_text(mutate(original))
            rc, out = run()
            PAGE.write_text(original)

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
        PAGE.write_text(original)
        assert PAGE.read_text() == original, f"FAILED TO RESTORE {PAGE}"

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
