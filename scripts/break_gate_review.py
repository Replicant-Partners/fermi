#!/usr/bin/env python3
"""Break each gate-review decision and require the build to notice.

Same contract as `break_coordination_note.py`: assert the anchor occurs exactly
once, assert the *resulting state* is in the file (not merely that a replace
happened), run the selector, require red, and revert.

The live tier needs a database. Point `PROBE_URL` at a throwaway Postgres with
migrations 214 and 216 applied:

    docker run -d --name p -e POSTGRES_PASSWORD=probe -e POSTGRES_DB=probe \\
        -p 55499:5432 postgres:16
    psql ... -c 'CREATE EXTENSION IF NOT EXISTS pgcrypto' \\
        -f migrations/214_gate_decisions.sql \\
        -f migrations/216_gate_decision_reviews.sql

    PROBE_URL=postgres://postgres:probe@127.0.0.1:55499/probe \\
        python3 scripts/break_gate_review.py

Never run this against production: break 5 drops a constraint and puts it back.
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
REVIEW = REPO / "src" / "gate_review.rs"
GATE_API = REPO / "src" / "gate_api.rs"

PROBE_URL = os.environ.get("PROBE_URL")


def run(args, env=None):
    e = dict(os.environ)
    if env:
        e.update(env)
    return subprocess.run(
        args, cwd=REPO, capture_output=True, text=True, timeout=1800, env=e
    )


def expect_red(name, args, must_mention, env=None):
    r = run(args, env)
    blob = r.stdout + r.stderr
    if r.returncode == 0:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(
            f"  !! {name}: red, but not in `{must_mention}` — it may have failed "
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
            f"from the current file text — `cargo fmt` moves it.\n{self.old!r}"
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
LIVE = [
    "cargo",
    "test",
    "--test",
    "gate_review_contract",
    "--",
    "--ignored",
    "--test-threads=1",
]


def main():
    results = []

    # 1. An unread ledger reads as a pass. The state the platform was in for its
    #    whole life: 214 gave two gates a ledger and nothing let anyone judge a
    #    row in it.
    print("break 1: an unreviewed ledger is reported as upheld")
    with Break(
        REVIEW,
        "    if reviewed == 0 {\n        return Standing::Unreviewed { decisions };\n    }",
        "    if reviewed == 0 {\n        return Standing::Upheld { tally };\n    }",
        "return Standing::Upheld { tally };\n    }",
    ):
        results.append(
            expect_red(
                "gate_review::reading",
                REGISTRY,
                "every_falsification_distinguishes_its_two_worlds",
            )
        )
        results.append(
            expect_red(
                "an_unread_ledger_and_a_clean_one_are_different_states",
                ["cargo", "test", "-p", "fermi", "--lib", "gate_review"],
                "an_unread_ledger_and_a_clean_one_are_different_states",
            )
        )

    # 2. A verdict token nothing declares is folded into the nearest bucket —
    #    the obvious implementation, and it makes a widened CHECK invisible in
    #    the one place it appears as data.
    print("break 2: an undeclared verdict is bucketed instead of refused")
    with Break(
        REVIEW,
        "        match token.parse::<GateReviewVerdict>()? {",
        "        match token\n            .parse::<GateReviewVerdict>()\n            .unwrap_or(GateReviewVerdict::Unclear)\n        {",
        ".unwrap_or(GateReviewVerdict::Unclear)",
    ):
        results.append(
            expect_red(
                "gate_review::tally_from_counts",
                REGISTRY,
                "every_falsification_distinguishes_its_two_worlds",
            )
        )

    # 3. The constraint-name guess is wrong. This is the break the unit tests
    #    structurally cannot catch — they feed the function the same literal the
    #    implementation contains, which is a closed loop. Only the database
    #    settles it.
    print("break 3: classify_write_error guesses the constraint name wrong")
    with Break(
        REVIEW,
        'Some("gate_decision_reviews_rationale_check") => Refusal::RationaleRequired,',
        'Some("gate_decision_reviews_rationale_chk") => Refusal::RationaleRequired,',
        "gate_decision_reviews_rationale_chk",
    ):
        if PROBE_URL:
            results.append(
                expect_red(
                    "the live tier sees the wrong constraint name",
                    LIVE,
                    "an_uncited_overturn_is_refused_and_the_refusal_reaches_the_reviewer",
                    env={"DATABASE_URL": PROBE_URL},
                )
            )
        else:
            print("  -- skipped: set PROBE_URL. This is the break that matters most.")

    # 4. A review door on a gate whose decisions never leave the process. A
    #    permanently empty queue, and the reviewer's conclusion is that the gate
    #    has never refused anything.
    print("break 4: a review door on a Retention::Counted gate")
    with Break(
        GATE_API,
        '        subject: "admission",\n        method: "POST",',
        '        subject: "rate_limit",\n        method: "POST",',
        'subject: "rate_limit",\n        method: "POST",',
    ):
        results.append(
            expect_red(
                "a_review_door_only_exists_where_the_decisions_do",
                ["cargo", "test", "-p", "fermi", "--lib", "gate_api"],
                "a_review_door_only_exists_where_the_decisions_do",
            )
        )

    # 5. The load-bearing constraint is gone from the database — the "migration
    #    ran and did nothing" case, which no source check can see. Dropped and
    #    restored on the probe.
    print("break 5: the rationale constraint is missing from the database")
    if PROBE_URL:
        drop = [
            "psql",
            PROBE_URL,
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            "ALTER TABLE public.gate_decision_reviews "
            "DROP CONSTRAINT gate_decision_reviews_rationale_check",
        ]
        add = [
            "psql",
            PROBE_URL,
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            "ALTER TABLE public.gate_decision_reviews "
            "ADD CONSTRAINT gate_decision_reviews_rationale_check "
            "CHECK (verdict <> 'overturned' OR (rationale IS NOT NULL "
            "AND length(trim(rationale)) > 0))",
        ]
        assert run(drop).returncode == 0, "could not drop the constraint on the probe"
        try:
            results.append(
                expect_red(
                    "the live tier sees an unenforced rationale rule",
                    LIVE,
                    "an_uncited_overturn_is_refused_and_the_refusal_reaches_the_reviewer",
                    env={"DATABASE_URL": PROBE_URL},
                )
            )
        finally:
            assert run(add).returncode == 0, "FAILED TO RESTORE the constraint!"
            print("  (constraint restored)")
    else:
        print("  -- skipped: set PROBE_URL.")

    print()
    if all(results) and results:
        print(f"all {len(results)} break(s) were seen. Tree reverted.")
        return 0
    print(f"{results.count(False)} of {len(results)} break(s) went unnoticed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
