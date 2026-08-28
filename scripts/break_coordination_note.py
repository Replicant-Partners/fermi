#!/usr/bin/env python3
"""Break each new coordination-note decision and require the build to notice.

Why this exists, in the words of the incidents:

  * a `str.replace` matched nothing, the tests stayed green, and the green was
    nearly believed. Twice. So every break here asserts `count(OLD) == 1`
    *before* substituting, and asserts the resulting file contains the broken
    state *after*.
  * a probe left a field at its default so both SQL expressions agreed, and the
    break came back green against production with a tautology live. So a break
    that comes back green is a failure of this script, reported as one.

Run from the repository root:  python3 scripts/break_coordination_note.py
"""

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

NOTE = REPO / "src" / "coordination_note.rs"
COHERENCE = REPO / "src" / "handlers" / "workspace" / "coherence.rs"


def run(args):
    return subprocess.run(
        args, cwd=REPO, capture_output=True, text=True, timeout=1800
    )


def expect_red(name, args, must_mention):
    """Run a test selector and require it to fail, naming the right thing."""
    r = run(args)
    ok = r.returncode != 0
    blob = r.stdout + r.stderr
    if not ok:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(
            f"  !! {name}: went red, but not in `{must_mention}` — it may have "
            f"failed for an unrelated reason, which is not evidence."
        )
        return False
    print(f"  ok {name}: red, in {must_mention}")
    return True


class Break:
    def __init__(self, path, old, new, expect_present):
        self.path = path
        self.old = old
        self.new = new
        self.expect_present = expect_present
        self.original = None

    def __enter__(self):
        self.original = self.path.read_text()
        n = self.original.count(self.old)
        assert n == 1, (
            f"anchor occurs {n} times in {self.path.name}, expected exactly 1. "
            f"Take the anchor from the current file text — `cargo fmt` moves it.\n"
            f"anchor: {self.old!r}"
        )
        broken = self.original.replace(self.old, self.new)
        assert broken != self.original, "substitution was a no-op"
        self.path.write_text(broken)
        # The state, not the substitution.
        now = self.path.read_text()
        assert self.expect_present in now, (
            f"the edit applied but the state it was named for is not in the file: "
            f"{self.expect_present!r}"
        )
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


FLOOR_BLOCK_ANCHOR = "                let d = fermi::coordination_note::deliver("
FLOOR_BLOCK_BROKEN = "                let d = deliver_it("


def main():
    results = []

    # 1. `is_problem` must distinguish the outcome to hope for from a refusal.
    #    Modelled on: a caller that warns on `AlreadyTargeted` trains its
    #    readers to ignore the warning that matters.
    print("break 1: is_problem treats AlreadyTargeted as a problem")
    with Break(
        NOTE,
        "matches!(d, Delivery::NotAMember | Delivery::Failed { .. })",
        "matches!(\n        d,\n        Delivery::NotAMember | Delivery::Failed { .. } | Delivery::AlreadyTargeted\n    )",
        "| Delivery::AlreadyTargeted",
    ):
        results.append(
            expect_red(
                "falsification_registry",
                ["cargo", "test", "--test", "falsification_registry"],
                "every_falsification_distinguishes_its_two_worlds",
            )
        )
        results.append(
            expect_red(
                "coordination_note unit tests",
                ["cargo", "test", "-p", "fermi", "--lib", "coordination_note"],
                "a_note_the_model_already_wrote_is_not_a_problem",
            )
        )

    # 2. The platform floor is deleted from the coherence shelf — the exact
    #    state Loop 3 was in for the life of the feature: the tool exists, has
    #    one caller, the build is clean, the endpoint returns the same 200, and
    #    `coordinator_observation` stays at 0.
    print("break 2: the coherence shelf stops delivering the brief")
    src = COHERENCE.read_text()
    start = src.index("    if let Some(brief) = consultant_output.clone() {")
    end = src.index("    // Post coherence update to workspace chat")
    with Break(
        COHERENCE,
        src[start:end],
        "    let _ = &consultant_output;\n\n",
        "let _ = &consultant_output;",
    ):
        results.append(
            expect_red(
                "the platform floor is wired",
                [
                    "cargo",
                    "test",
                    "-p",
                    "fermi",
                    "--tests",
                    "the_platform_delivers_the_coordination_brief",
                ],
                "the_platform_delivers_the_coordination_brief_and_does_not_only_ask_for_it",
            )
        )

    # 3. The floor's duplicate check loses its per-run cutoff. With `None` it
    #    re-delivers the brief on every evaluation; the test must say so.
    print("break 3: the floor's duplicate check is unbounded")
    with Break(
        COHERENCE,
        "                    Some(run_started_at),",
        "                    None,",
        "\n                    None,\n",
    ):
        results.append(
            expect_red(
                "the floor is bounded to this run",
                [
                    "cargo",
                    "test",
                    "-p",
                    "fermi",
                    "--tests",
                    "the_floor_suppresses_duplicates_within_the_run",
                ],
                "the_floor_suppresses_duplicates_within_the_run_and_not_across_runs",
            )
        )

    # 4. The strategist lookup goes back to being unresolved. This check was
    #    written before `handler_source` existed, scanned the whole file, and
    #    was satisfied by the string literal inside its own assertion — so it
    #    had never been capable of failing. Registered here so that is on the
    #    record rather than in a memory.
    print("break 4: the shelf stops resolving the registered strategist")
    with Break(
        COHERENCE,
        "JOIN agents a ON a.agent_id = t.coordination_strategist_id",
        "JOIN agents a ON a.agent_id = t.owner_id",
        "a.agent_id = t.owner_id",
    ):
        results.append(
            expect_red(
                "the shelf reads the registered strategist",
                [
                    "cargo",
                    "test",
                    "-p",
                    "fermi",
                    "--tests",
                    "coherence_shelf_reads_the_registered_strategist",
                ],
                "coherence_shelf_reads_the_registered_strategist",
            )
        )

    print()
    if all(results):
        print(f"all {len(results)} break(s) were seen. Tree reverted.")
        return 0
    print(f"{results.count(False)} of {len(results)} break(s) went unnoticed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
