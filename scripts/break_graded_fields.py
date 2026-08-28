#!/usr/bin/env python3
"""Break the graded-field decisions and require the build to notice.

Targets the **lib** tier deliberately (`cargo test -p fermi --lib`). The
falsification registry is an integration test, so cargo builds the `api-server`
bin to run it, and a parallel session working in `src/handlers/` can leave that
bin broken for reasons unrelated to the change under test. The module's own suite
is the primary tier anyway; the registry is the build gate on top of it.

    python3 scripts/break_graded_fields.py
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GT = REPO / "src" / "grounding_trust.rs"
AS = REPO / "src" / "assertions.rs"


def expect_red(name, selector, must_mention):
    r = subprocess.run(
        ["cargo", "test", "-p", "fermi", "--lib", selector],
        cwd=REPO, capture_output=True, text=True, timeout=1800,
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
    def __init__(self, path, old, new, expect):
        self.path, self.old, self.new, self.expect = path, old, new, expect

    def __enter__(self):
        self.original = self.path.read_text()
        n = self.original.count(self.old)
        assert n == 1, (
            f"anchor occurs {n} times in {self.path.name}, expected 1\n{self.old!r}"
        )
        self.path.write_text(self.original.replace(self.old, self.new, 1))
        assert self.expect in self.path.read_text(), f"state absent: {self.expect!r}"
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


def main():
    results = []

    # 1. An ungraded block inherits a pass. `enforce` only grades blocks it saw,
    #    so this is the common case, and it would make an ungraded field
    #    indistinguishable from a verified one on the trace.
    print("break 1: an ungraded block defaults to tool_verified")
    with Break(
        GT,
        "                    .unwrap_or(PROV_UNAVAILABLE),",
        "                    .unwrap_or(PROV_TOOL),",
        ".unwrap_or(PROV_TOOL),",
    ):
        results.append(
            expect_red(
                "an_ungraded_block_floors_at_the_bottom_and_keeps_the_claim",
                "grounding_trust",
                "an_ungraded_block_floors_at_the_bottom_and_keeps_the_claim",
            )
        )

    # 2. The claimed value is dropped. `Violation.removed` exists precisely
    #    because this is the only evidence that could answer which model
    #    fabricates what, and a null cannot be labelled.
    print("break 2: the claimed value is not retained")
    with Break(
        GT,
        "                value: get_path(doc, c.path).cloned().unwrap_or(Value::Null),",
        "                value: Value::Null,",
        "                value: Value::Null,",
    ):
        results.append(
            expect_red(
                "an_ungraded_block_floors_at_the_bottom_and_keeps_the_claim",
                "grounding_trust",
                "an_ungraded_block_floors_at_the_bottom_and_keeps_the_claim",
            )
        )

    # 3. The settling tool stops being read from the contract, so every field
    #    routes to a person and the tool queue is permanently empty.
    print("break 3: settleable_by ignores the contract")
    with Break(
        GT,
        "                settleable_by: match c.grounding {\n                    Grounding::Sourced { tool, .. } => Some(tool),\n                    _ => None,\n                },",
        "                settleable_by: None,",
        "                settleable_by: None,",
    ):
        results.append(
            expect_red(
                "the_settling_tool_is_read_from_the_contract",
                "grounding_trust",
                "the_settling_tool_is_read_from_the_contract",
            )
        )

    # 4. The basis is dropped when minting the assertion. This is the whole
    #    argument for `from_graded_field`: a tool-verified field with an empty
    #    basis floors at `pending_human_check`, so it would enqueue a person to
    #    re-check something a tool already answered.
    print("break 4: the block's grade is not carried into the basis")
    with Break(
        AS,
        "        basis: vec![f.provenance.to_string()],",
        "        basis: vec![],",
        "        basis: vec![],",
    ):
        results.append(
            expect_red(
                "carrying_the_blocks_grade_is_the_difference_between_verified_and_pending",
                "assertions",
                "carrying_the_blocks_grade_is_the_difference",
            )
        )

    # 5. A contracted field recorded as prose. `ExtractionPath::Prose` is capped
    #    at `model_inference`, so the cap would silently defeat the contract
    #    however well sourced the field was.
    print("break 5: a contracted field is recorded as prose")
    with Break(
        AS,
        "        extraction: ExtractionPath::TypedField {\n            schema: format!(\"contract:{agent_id}\"),\n            field_path: f.path.to_string(),\n        },",
        "        extraction: ExtractionPath::Prose {\n            pattern: format!(\"contract:{agent_id}\"),\n        },",
        "extraction: ExtractionPath::Prose {",
    ):
        results.append(
            expect_red(
                "a_contracted_field_is_a_typed_field_and_not_prose",
                "assertions",
                "a_contracted_field_is_a_typed_field_and_not_prose",
            )
        )

    # 6. An absent field is enqueued. Every contracted agent declares more
    #    fields than any one response carries, so the queue would be mostly the
    #    agents' silence on arrival and abandoned.
    print("break 6: an absent field becomes a queue item")
    with Break(
        AS,
        "    if f.value.is_null() {\n        return Err(NotEnqueued {",
        "    if false {\n        return Err(NotEnqueued {",
        "    if false {\n        return Err(NotEnqueued {",
    ):
        results.append(
            expect_red(
                "an_absent_field_is_not_a_queue_item",
                "assertions",
                "an_absent_field_is_not_a_queue_item",
            )
        )

    # 7. A non-numeric claim is dropped silently instead of reported. An empty
    #    queue that is empty because nothing could be enqueued reads identically
    #    to one that is empty because nothing is wrong.
    print("break 7: a non-representable claim is dropped without a reason")
    with Break(
        AS,
        '            why: "the claim is not numeric and `Assertion::value` is a `Spread`. \\',
        '            why: "skipped. \\',
        'why: "skipped. \\',
    ):
        results.append(
            expect_red(
                "a_non_numeric_claim_is_refused_with_its_reason_rather_than_dropped",
                "assertions",
                "a_non_numeric_claim_is_refused_with_its_reason",
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
