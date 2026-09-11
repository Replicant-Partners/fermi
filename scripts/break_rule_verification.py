#!/usr/bin/env python3
"""Break the rule verifier six ways, and watch each guard go red.

`verification_status` is about to start changing rows: a `Verified` verdict
grants a rule ordering preference in every future extraction prompt, and a
`Rejected` one deactivates it and removes it from retrieval permanently. A
verifier that promotes on no evidence, or that treats a missing baseline as a
zero one, is worse than the nothing it replaces.

So every threshold is asserted by a pure test, and every one of those tests is
broken here.

    python3 scripts/break_rule_verification.py                 # this tree
    python3 scripts/break_rule_verification.py /tmp/fv-rv      # a worktree

Use the second form in a shared clone; see
`scripts/break_create_page_verdict.py` for the worktree recipe and the warning
about sharing a CARGO_TARGET_DIR between worktrees.
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else REPO
SRC = ROOT / "agent-bestiary" / "memory" / "src" / "rule_verification.rs"

CARGO = [
    "cargo", "test", "-p", "agent-bestiary-memory",
    "--manifest-path", str(ROOT / "Cargo.toml"),
    "rule_verification",
]


def promote_on_no_evidence(s):
    pat = "pub const MIN_RUNS: i64 = 10;"
    assert pat in s, pat
    return s.replace(pat, "pub const MIN_RUNS: i64 = 0;")


def missing_baseline_becomes_zero(s):
    pat = """    let (Some(rate), Some(baseline)) = (o.rate(), baseline) else {
        return Adjudication::Pending(Why::NoBaseline);
    };"""
    assert pat in s, "baseline guard not found"
    return s.replace(
        pat,
        """    let rate = o.rate().unwrap_or(0.0);
    let baseline = baseline.unwrap_or(0.0);""",
    )


def fixed_threshold_instead_of_baseline(s):
    pat = "    if rate >= baseline {"
    assert pat in s, pat
    return s.replace(pat, "    if rate >= 0.65 {")


def reject_without_a_margin(s):
    pat = "    if rate <= baseline - REJECT_MARGIN {"
    assert pat in s, pat
    return s.replace(pat, "    if rate < baseline {")


def drop_the_caveat(s):
    pat = '"caveat": "Correlation over runs that had this rule in the prompt. \\'
    assert pat in s, "caveat literal not found"
    i = s.index(pat)
    j = s.index('",\n', s.index("caused the outcome.", i)) + len('",\n')
    return s[:i] + '"note": "removed",\n' + s[j:]


def zero_rate_for_no_runs(s):
    pat = "        (self.runs > 0).then(|| self.successes as f64 / self.runs as f64)"
    assert pat in s, pat
    return s.replace(
        pat,
        "        Some(if self.runs > 0 { self.successes as f64 / self.runs as f64 } else { 0.0 })",
    )


BREAKS = [
    ("promote on no evidence (MIN_RUNS = 0)",
     "no_evidence_promotes_nothing", promote_on_no_evidence),
    ("treat a missing baseline as 0.0",
     "a_missing_baseline_is_not_a_zero_baseline", missing_baseline_becomes_zero),
    ("judge against a fixed 0.65 bar, not the agent",
     "the_same_rate_is_judged_against_the_agent_it_belongs_to",
     fixed_threshold_instead_of_baseline),
    ("reject anything below baseline, no margin",
     "rejection_requires_a_margin_and_verification_does_not", reject_without_a_margin),
    ("drop the causation caveat from the evidence",
     "every_verdict_names_its_method_and_its_limit", drop_the_caveat),
    ("report 0.0 for a rule with no resolved runs",
     "an_unresolved_rule_has_no_rate_rather_than_a_zero_one", zero_rate_for_no_runs),
]


def run():
    p = subprocess.run(CARGO, capture_output=True, text=True, env=dict(os.environ))
    return p.returncode, p.stdout + p.stderr


def failed_tests(out):
    # `test result: FAILED. ...` also starts with "test " and contains FAILED.
    return [
        l for l in out.splitlines()
        if l.startswith("test ") and not l.startswith("test result:")
        and l.rstrip().endswith("FAILED")
    ]


def main():
    if not SRC.exists():
        print(f"no such file: {SRC}")
        return 1
    original = SRC.read_text()

    rc, out = run()
    if rc != 0:
        print("BASELINE IS NOT GREEN -- nothing below means anything.")
        print(out[-3000:])
        return 1
    print(f"baseline green under {ROOT}\n")

    problems = 0
    try:
        for label, expect, mutate in BREAKS:
            SRC.write_text(mutate(original))
            rc, out = run()
            SRC.write_text(original)

            red = failed_tests(out)
            # A mutation that stops the crate compiling is not a guard firing.
            #
            # Detected on rustc's own markers, NOT on a bare "error:" — cargo
            # prints `error: test failed, to rerun pass ...` for every genuine
            # test failure, so the loose check reported all six mutations as
            # build breaks and hid the fact that all six guards were firing
            # correctly. A detector that cannot tell its two cases apart is the
            # defect this whole harness exists to catch, committed in the
            # harness.
            if "error[E" in out or "could not compile" in out:
                print(f"BUILD  {label}\n         *** mutation did not compile ***")
                problems += 1
                continue
            fired = any(expect in l for l in red)
            collateral = [l for l in red if expect not in l]

            print(f"{'RED  ' if fired else 'GREEN'}  {label}")
            print(f"         expected red: {expect}")
            if not fired:
                print("         *** THE GUARD DID NOT FIRE ***")
                problems += 1
            if collateral:
                print(f"         also red: {[l.split()[1] for l in collateral]}")
    finally:
        SRC.write_text(original)
        assert SRC.read_text() == original, f"FAILED TO RESTORE {SRC}"

    rc, _ = run()
    print("\nbaseline restored green" if rc == 0
          else "\nbaseline did NOT return to green")
    if rc != 0:
        problems += 1

    print("\nRESULT:", "every threshold is asserted by a test that fires"
          if not problems else f"{problems} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
