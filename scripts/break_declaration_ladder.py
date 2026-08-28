#!/usr/bin/env python3
"""Break each declaration-ladder decision and require the build to notice.

Same contract as the other two harnesses: assert the anchor occurs exactly once,
assert the *resulting state* is in the file rather than trusting the replace,
run the selector, require red for the right reason, revert.

    python3 scripts/break_declaration_ladder.py
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LADDER = REPO / "src" / "declaration_ladder.rs"


def run(args):
    return subprocess.run(
        args, cwd=REPO, capture_output=True, text=True, timeout=1800,
        env=dict(os.environ),
    )


def expect_red(name, args, must_mention):
    r = run(args)
    blob = r.stdout + r.stderr
    if r.returncode == 0:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(
            f"  !! {name}: red, but not in `{must_mention}` -- it may have failed "
            f"for an unrelated reason, which is not evidence."
        )
        return False
    print(f"  ok {name}: red, in {must_mention}")
    return True


class Break:
    def __init__(self, path, old, new, expect_present):
        self.path, self.old, self.new = path, old, new
        self.expect_present = expect_present

    def __enter__(self):
        self.original = self.path.read_text()
        n = self.original.count(self.old)
        assert n == 1, (
            f"anchor occurs {n} times in {self.path.name}, expected 1. Take it "
            f"from the current file text -- `cargo fmt` moves it.\n{self.old!r}"
        )
        self.path.write_text(self.original.replace(self.old, self.new))
        now = self.path.read_text()
        assert self.expect_present in now, (
            f"the edit applied but the state it was named for is not in the "
            f"file: {self.expect_present!r}"
        )
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


REGISTRY = ["cargo", "test", "--test", "falsification_registry"]
UNIT = ["cargo", "test", "-p", "fermi", "--lib", "declaration_ladder"]
DISTINGUISH = "every_falsification_distinguishes_its_two_worlds"


def main():
    results = []

    # 1. The whole point: an undeclared agent's silence billed to the platform.
    print("break 1: an undeclared agent's silence is attributed to the platform")
    with Break(
        LADDER,
        "        Silence::Undeclared { .. } => Owner::AgentAuthor,",
        "        Silence::Undeclared { .. } => Owner::Platform,",
        "Silence::Undeclared { .. } => Owner::Platform,",
    ):
        results.append(expect_red("declaration_ladder::whose_work", REGISTRY, DISTINGUISH))
        results.append(
            expect_red(
                "an_undeclared_agent_is_the_authors_work_and_not_the_platforms",
                UNIT,
                "an_undeclared_agent_is_the_authors_work_and_not_the_platforms",
            )
        )

    # 2. The ordering inside `attribute`: a cold counter must outrank everything,
    #    or a freshly booted server sends authors to write contracts for readings
    #    that fix themselves on the next request.
    print("break 2: the cold-counter check moves below the declaration check")
    with Break(
        LADDER,
        "    if cold {\n        return Silence::ColdCounter;\n    }\n    match l {",
        "    match l {",
        "pub fn attribute(cold: bool, l: &Legibility, traversed: i64) -> Silence {\n    match l {",
    ):
        results.append(expect_red("declaration_ladder::attribute", REGISTRY, DISTINGUISH))
        results.append(
            expect_red(
                "a_cold_counter_outranks_every_other_explanation",
                UNIT,
                "a_cold_counter_outranks_every_other_explanation",
            )
        )

    # 3. Cruft checked after legibility, so a fully-declared fixture reports as
    #    Legible and inflates the coverage numerator with rows about to be
    #    deleted. The fleet has no such row today, which is why the ordering has
    #    to be asserted rather than observed.
    print("break 3: cruft is checked after legibility")
    with Break(
        LADDER,
        "    if is_test_cruft(agent_name) {\n        return Disposition::Prune;\n    }\n    match l {\n        Legibility::Declared => Disposition::Legible,",
        "    match l {\n        Legibility::Declared => Disposition::Legible,",
        "pub fn disposition(agent_name: &str, l: &Legibility) -> Disposition {\n    match l {",
    ):
        results.append(expect_red("declaration_ladder::disposition", REGISTRY, DISTINGUISH))
        results.append(
            expect_red(
                "a_fully_declared_fixture_is_still_a_prune_target",
                UNIT,
                "a_fully_declared_fixture_is_still_a_prune_target",
            )
        )

    # 4. An unrecognised rung name counts as progress, so coverage can rise by
    #    inventing a token.
    print("break 4: an invented rung name counts as a declared rung")
    with Break(
        LADDER,
        "    if have.is_empty() {\n        Legibility::Opaque",
        "    if have.is_empty() && present.is_empty() {\n        Legibility::Opaque",
        "if have.is_empty() && present.is_empty() {",
    ):
        results.append(expect_red("declaration_ladder::legibility", REGISTRY, DISTINGUISH))
        results.append(
            expect_red(
                "a_rung_this_module_does_not_declare_is_not_progress",
                UNIT,
                "a_rung_this_module_does_not_declare_is_not_progress",
            )
        )

    # 5. The cruft predicate stops matching, which sends 110 fixtures onto the
    #    retrofit worklist and halves every rung's reported coverage.
    print("break 5: the cruft predicate stops matching")
    with Break(
        LADDER,
        '    agent_name.starts_with("test_agent_")\n}',
        '    agent_name.starts_with("test_agent_fixture_")\n}',
        'starts_with("test_agent_fixture_")',
    ):
        results.append(expect_red("declaration_ladder::is_test_cruft", REGISTRY, DISTINGUISH))
        results.append(
            expect_red(
                "the_census_keeps_the_two_worklists_separate",
                UNIT,
                "the_census_keeps_the_two_worklists_separate",
            )
        )

    # 6. The census counts cruft in the denominator, so the ports rung reads 2 of
    #    5 instead of 2 of 2 -- and at fleet scale 93 of 206 instead of 93 of 96,
    #    which makes an almost-complete rung look like a third done.
    print("break 6: cruft lands in the coverage denominator")
    with Break(
        LADDER,
        "        if is_test_cruft(name) {\n            c.cruft += 1;\n            continue;\n        }\n        c.real += 1;",
        "        if is_test_cruft(name) {\n            c.cruft += 1;\n        }\n        c.real += 1;",
        "            c.cruft += 1;\n        }\n        c.real += 1;",
    ):
        results.append(
            expect_red(
                "the_census_keeps_the_two_worklists_separate",
                UNIT,
                "the_census_keeps_the_two_worklists_separate",
            )
        )

    # 7. The remedy names an arbitrary missing rung rather than the cheapest, so
    #    an author is told to write a field contract for an agent that has not
    #    declared its ports.
    print("break 7: the remedy names the most expensive missing rung")
    with Break(
        LADDER,
        "            rung: missing.first().copied().unwrap_or(LADDER[0].rung),",
        "            rung: missing.last().copied().unwrap_or(LADDER[0].rung),",
        "missing.last().copied()",
    ):
        results.append(
            expect_red(
                "the_remedy_offered_is_the_cheapest_missing_declaration",
                UNIT,
                "the_remedy_offered_is_the_cheapest_missing_declaration",
            )
        )

    print()
    if results and all(results):
        print(f"all {len(results)} break(s) were seen. Tree reverted.")
        return 0
    print(f"{results.count(False)} of {len(results)} break(s) went unnoticed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
