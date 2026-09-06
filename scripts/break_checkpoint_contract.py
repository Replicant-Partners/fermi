#!/usr/bin/env python3
"""Break the checkpoint-outcome contract and require the build to notice.

Six breaks over the shape the UX team specified in
`docs/UX_CONTRACT_belt_outcomes.md` (written when the checkpoint row was still
called a belt). Every one of them is a mistake that would render as a plausible
row of checkpoints -- which is the point. A trace that is wrong in a way that
LOOKS wrong gets caught by the person reading it; these are the ones that look
finished.

    python3 scripts/break_checkpoint_contract.py

NOTE on this environment: a parallel session writes files with inconsistent
mtimes, which defeats cargo's fingerprint cache and produces stale test binaries.
Every break below touches the file it edited, so a green result cannot be cached.
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
AT = REPO / "src" / "artifact_trace.rs"
GT = REPO / "src" / "gate_trust.rs"


def run(args):
    return subprocess.run(args, cwd=REPO, capture_output=True, text=True, timeout=1800)


def expect_red(name, args, must_mention):
    r = run(args)
    blob = r.stdout + r.stderr
    if r.returncode == 0:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(f"  !! {name}: red, but `{must_mention}` is not named.")
        print("     ---- first 40 lines ----")
        print("\n".join(blob.splitlines()[:40]))
        return False
    print(f"  ok {name}: red, naming {must_mention}")
    return True


class Break:
    """Apply an edit, ASSERT it applied, and assert the resulting state exists.

    The assertion is the whole harness. Twice in this project a `str.replace`
    matched nothing, the tests stayed green, and the green was nearly believed.
    """

    def __init__(self, path, old, new, expect):
        self.path, self.old, self.new, self.expect = path, old, new, expect

    def __enter__(self):
        self.original = self.path.read_text()
        n = self.original.count(self.old)
        assert n == 1, f"anchor occurs {n} times in {self.path.name}, expected 1"
        self.path.write_text(self.original.replace(self.old, self.new, 1))
        assert self.expect in self.path.read_text(), f"state absent: {self.expect!r}"
        os.utime(self.path, None)
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        os.utime(self.path, None)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


LIB = ["cargo", "test", "-p", "fermi", "--lib", "artifact_trace"]
REG = ["cargo", "test", "-p", "fermi", "--test", "falsification_registry"]


def main():
    results = []

    # 1. `credit` renders as a debt. The harm the token set exists to prevent:
    #    a permanent, correct NULL shown as an unpaid obligation on every
    #    artifact forever -- and a debt that can never be paid is one a reader learns to
    #    ignore, including on the rungs where it is real.
    print("break 1: a gate that fires before the artifact reported as a finding")
    with Break(
        AT,
        "    if spec.decides_before_the_artifact {\n        return Absent {\n            token: NotRecordedReason::FiresBeforeArtifact,",
        "    if false {\n        return Absent {\n            token: NotRecordedReason::FiresBeforeArtifact,",
        "    if false {\n        return Absent {",
    ):
        results.append(
            expect_red(
                "the_absence_token_comes_from_the_gate_registry",
                LIB,
                "the_absence_token_comes_from_the_gate_registry",
            )
        )

    # 2. The mirror: a gate that records, labelled as firing before the artifact.
    #    Quieter and worse -- it takes the one token that IS a finding and hides
    #    it behind the one token that is nobody's work.
    print("break 2: a recording gate labelled permanent-and-correct")
    with Break(
        GT,
        "    pub decides_before_the_artifact: bool,",
        "    pub decides_before_the_artifact: bool,\n    #[allow(dead_code)]\n    pub _brk: (),",
        "pub _brk: ()",
    ):
        # A struct-literal break is a compile error rather than a test failure,
        # so this one asserts the compiler owns the property -- which is the
        # cheaper guarantee and the one to prefer where it exists.
        r = run(["cargo", "check", "-p", "fermi", "--lib"])
        ok = r.returncode != 0 and "missing field" in (r.stdout + r.stderr)
        print(
            f"  {'ok' if ok else '!!'} GateSpec is exhaustive: "
            f"{'a new field must be filled at every site' if ok else 'NOT ENFORCED'}"
        )
        results.append(ok)

    # 3. `narrow_by_age` claims permanence with nothing to prove it. The calm
    #    version ships, because a row of grey rings that all explain themselves
    #    looks finished.
    print("break 3: an absence called permanent with no timestamp")
    with Break(
        AT,
        "        // No timestamp on the artifact: cannot tell, so do not claim to.\n        (None, Some(_)) => false,",
        "        // No timestamp on the artifact: cannot tell, so do not claim to.\n        (None, Some(_)) => true,",
        "(None, Some(_)) => true,",
    ):
        results.append(
            expect_red(
                "an_absence_is_only_permanent_with_a_timestamp_to_prove_it",
                LIB,
                "an_absence_is_only_permanent_with_a_timestamp_to_prove_it",
            )
        )
        results.append(
            expect_red(
                "registry: artifact_trace::narrow_by_age",
                REG,
                "narrow_by_age",
            )
        )

    # 4. A rung claiming both a verdict and a reason there isn't one. The client
    #    is a two-way branch, so it renders the verdict and drops the
    #    contradiction in silence.
    print("break 4: a rung reporting both ways at once")
    with Break(
        AT,
        "        self.decided.is_some() != self.decided_absent.is_some()",
        "        self.decided.is_some() || self.decided_absent.is_some()",
        "self.decided.is_some() || self.decided_absent.is_some()",
    ):
        results.append(
            expect_red(
                "registry: artifact_trace::reports_exactly_one_way",
                REG,
                "reports_exactly_one_way",
            )
        )

    # 5. `checkpoints()` pre-fills a recomputation it cannot have computed. A
    #    zero here puts "0 violations" on every rung of every route -- a clean
    #    bill of health for a document this function has never seen.
    print("break 5: the declared checkpoints asserting something about an episode")
    with Break(
        AT,
        "                decided: None,\n                decided_absent: Some(not_recorded(spec)),\n                recomputed: None,",
        "                decided: None,\n                decided_absent: Some(not_recorded(spec)),\n                recomputed: Some(Recomputed { fields: 0, violations: 0 }),",
        "recomputed: Some(Recomputed { fields: 0, violations: 0 })",
    ):
        results.append(
            expect_red(
                "the_declared_checkpoints_assert_nothing_about_an_episode",
                LIB,
                "the_declared_checkpoints_assert_nothing_about_an_episode",
            )
        )

    # 6. A verdict vocabulary that quietly loses `undetermined`. It is the
    #    awkward third reading and the expected verdict for most of the corpus,
    #    and folding it into either neighbour is how an absent check becomes
    #    indistinguishable from a passing one.
    print("break 6: the third verdict dropped")
    with Break(
        GT,
        'pub const DECISIONS: &[&str] = &["approved", "refused", "undetermined"];',
        'pub const DECISIONS: &[&str] = &["approved", "refused"];',
        'pub const DECISIONS: &[&str] = &["approved", "refused"];',
    ):
        results.append(
            expect_red(
                "the_declared_checkpoints_assert_nothing_about_an_episode",
                LIB,
                "the_declared_checkpoints_assert_nothing_about_an_episode",
            )
        )

    print()
    if all(results):
        print(f"all {len(results)} break(s) were caught.")
        return 0
    print(f"!! {results.count(False)} of {len(results)} break(s) went unnoticed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
