#!/usr/bin/env python3
"""Break the document recovery in `response_floor` and require a red.

The break is the *original code*: a bare `serde_json::from_str` that gives up on
a document wrapped in prose. It shipped that way and graded 28 of 28 semantic
rules as resting on nothing, so this harness is the record that the fix is
covered.

    python3 scripts/break_response_floor.py
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GT = REPO / "src" / "grounding_trust.rs"
UNIT = ["cargo", "test", "-p", "fermi", "--lib", "grounding_trust"]


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
        self.original = GT.read_text()
        n = self.original.count(self.old)
        assert n == 1, f"anchor occurs {n} times, expected 1\n{self.old!r}"
        GT.write_text(self.original.replace(self.old, self.new, 1))
        assert self.expect in GT.read_text(), f"state absent: {self.expect!r}"
        return self

    def __exit__(self, *exc):
        GT.write_text(self.original)
        assert GT.read_text() == self.original, "failed to revert!"
        return False


RECOVER = """    let mut doc: Value = match crate::agent_backend::envelope::extract_json(response_text) {
        Some(v) => v,
        // No document anywhere in the response. Genuinely ungrounded: there are
        // no typed fields to have been sourced, and now that is a finding about
        // the response rather than about the parser.
        None => return Some(PROV_UNAVAILABLE),
    };"""


def main():
    results = []

    # 1. The original defect, restored: a bare parse that gives up on a document
    #    returned inside prose. This is what shipped, and 64 of 94 retained
    #    responses from contracted agents are packaged that way.
    print("break 1: document recovery reverts to a bare serde_json::from_str")
    with Break(
        RECOVER,
        """    let mut doc: Value = match serde_json::from_str::<Value>(response_text) {
        Ok(v) if v.is_object() => v,
        _ => return Some(PROV_UNAVAILABLE),
    };""",
        "match serde_json::from_str::<Value>(response_text) {",
    ):
        results.append(
            expect_red(
                "a_document_wrapped_in_prose_is_graded_rather_than_dismissed",
                "a_document_wrapped_in_prose_is_graded_rather_than_dismissed",
            )
        )

    # 2. The symmetric error the fix could have introduced: buying a floor by
    #    accepting anything that parses. Modelled as the specific temptation —
    #    a sourced block with no match is ABSENT, so why should it drag the floor
    #    down? Because that is the empty-set inversion: filter them out and a
    #    document containing none of its contracted fields stops being graded on
    #    the fields it is missing.
    print("break 2: blocks with no match are excluded from the floor")
    with Break(
        """    Some(floor(report.provenance.iter().map(|(_, v)| *v)))""",
        """    Some(floor(
        report
            .provenance
            .iter()
            .map(|(_, v)| *v)
            .filter(|v| *v != PROV_NO_MATCH),
    ))""",
        ".filter(|v| *v != PROV_NO_MATCH)",
    ):
        results.append(
            expect_red(
                "recovering_a_document_is_not_the_same_as_finding_content",
                "recovering_a_document_is_not_the_same_as_finding_content",
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
