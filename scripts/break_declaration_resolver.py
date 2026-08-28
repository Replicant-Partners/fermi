#!/usr/bin/env python3
"""Break the declaration resolver in `panel_absence` and require a red.

Same contract as the sibling harnesses: assert the anchor matched exactly once,
assert the resulting state is present rather than trusting the replace, run the
selector, require red for the right reason, revert.

    python3 scripts/break_declaration_resolver.py
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PA = REPO / "src" / "panel_absence.rs"
UNIT = ["cargo", "test", "-p", "fermi", "--lib", "panel_absence"]


def expect_red(name, must_mention):
    r = subprocess.run(
        UNIT, cwd=REPO, capture_output=True, text=True, timeout=1800,
        env=dict(os.environ),
    )
    blob = r.stdout + r.stderr
    if r.returncode == 0:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(f"  !! {name}: red, but not in `{must_mention}`.")
        return False
    print(f"  ok {name}: red, in {must_mention}")
    return True


class Break:
    def __init__(self, old, new, expect):
        self.old, self.new, self.expect = old, new, expect

    def __enter__(self):
        self.original = PA.read_text()
        n = self.original.count(self.old)
        assert n == 1, f"anchor occurs {n} times, expected 1\n{self.old!r}"
        PA.write_text(self.original.replace(self.old, self.new, 1))
        assert self.expect in PA.read_text(), f"state absent: {self.expect!r}"
        return self

    def __exit__(self, *exc):
        PA.write_text(self.original)
        assert PA.read_text() == self.original, "failed to revert!"
        return False


def main():
    results = []

    # 1. A census that could not be gathered treated as an empty one. Zero
    #    coverage everywhere is the most alarming reading available and a failed
    #    query has no standing to make it.
    print("break 1: a missing census is treated as zero coverage")
    with Break(
        "    let Some(census) = o.declarations.as_ref() else {\n        return base;\n    };",
        "    let fallback = crate::declaration_ladder::Census::default();\n"
        "    let census = o.declarations.as_ref().unwrap_or(&fallback);",
        "unwrap_or(&fallback)",
    ):
        results.append(
            expect_red(
                "a_missing_census_is_not_reported_as_zero_coverage",
                "a_missing_census_is_not_reported_as_zero_coverage",
            )
        )

    # 2. A rung nobody declares reported as `idle`. The panel cannot fill
    #    because its input does not exist, which is not the panel being
    #    correctly empty -- and `idle` is the reading that stops anyone looking.
    print("break 2: an undeclared rung reads idle")
    with Break(
        "    if declared == 0 {\n        return Absence {\n            token: \"undeclared\",",
        "    if false {\n        return Absence {\n            token: \"undeclared\",",
        "    if false {\n        return Absence {\n            token: \"undeclared\",",
    ):
        results.append(
            expect_red(
                "a_declared_rung_makes_an_empty_panel_idle_rather_than_unknowable",
                "a_declared_rung_makes_an_empty_panel_idle_rather_than_unknowable",
            )
        )

    # 3. A fleet of nothing but fixtures reported as a fleet that declared
    #    nothing -- technically true and useless, because it sends an author to
    #    declare a rung on rows that are about to be deleted.
    print("break 3: a fixture-only fleet reports as undeclared")
    with Break(
        "    if census.real == 0 {\n        return Absence {\n            token: \"no_subjects\",",
        "    if false {\n        return Absence {\n            token: \"no_subjects\",",
        "    if false {\n        return Absence {\n            token: \"no_subjects\",",
    ):
        results.append(
            expect_red(
                "a_fleet_of_only_fixtures_reports_no_subjects_rather_than_undeclared",
                "a_fleet_of_only_fixtures_reports_no_subjects_rather_than_undeclared",
            )
        )

    # 4. The coverage the reading rests on is dropped from the detail, so a
    #    reader cannot tell 2 of 2 from 2 of 400.
    print("break 4: the reading drops the coverage it rests on")
    with Break(
        '            "{} {} of {} real agents declare `{rung}`, so the input exists. An \\\n             empty panel here is a statement about what those declarations add \\\n             up to, not about whether anyone made them.",\n            p.if_empty, declared, census.real',
        '            "{} Agents declare `{rung}`, so the input exists.",\n            p.if_empty',
        "Agents declare `{rung}`, so the input exists.",
    ):
        results.append(
            expect_red(
                "a_declared_rung_makes_an_empty_panel_idle_rather_than_unknowable",
                "a_declared_rung_makes_an_empty_panel_idle_rather_than_unknowable",
            )
        )

    # 5. The shrink-only ratchet on the unresolved list still bites: putting a
    #    panel back into `Unresolved` must be argued for, not slipped in.
    print("break 5: a resolved panel is quietly returned to Unresolved")
    with Break(
        '        resolved_by: Resolver::Declaration { rung: "ports" },',
        '        resolved_by: Resolver::Unresolved {\n            why: "A sentence long enough to clear the eighty character floor that \\\n                  the ratchet imposes on every unresolved panel\'s reason field.",\n        },',
        "the ratchet imposes on every unresolved panel",
    ):
        results.append(
            expect_red(
                "the_unresolved_list_may_only_shrink",
                "the_unresolved_list_may_only_shrink",
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
