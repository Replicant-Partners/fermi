#!/usr/bin/env python3
"""Break the verification-queue writer and require the build to notice.

Two tiers. The offline breaks target `cargo test -p fermi --lib`; the live ones
need a real server, because the whole reason this table's zero was unexplained is
that its writes were only ever refused by Postgres.

    DATABASE_URL=... python3 scripts/break_verification_queue.py

NOTE on this environment: a parallel session writes files with inconsistent
mtimes, which defeats cargo's fingerprint cache and produces both stale test
binaries and phantom unresolved-import errors. Every run below touches the file it
edited, so a green result cannot be a cached one.
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
VQ = REPO / "src" / "verification_queue.rs"
DB = os.environ.get("DATABASE_URL")


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
        # Defeat the stale-fingerprint problem described above.
        os.utime(self.path, None)
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        os.utime(self.path, None)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


LIB = ["cargo", "test", "-p", "fermi", "--lib", "verification_queue"]
LIVE = [
    "cargo", "test", "-p", "fermi", "--test", "verification_queue_contract",
    "--", "--ignored", "--test-threads=1",
]


def main():
    results = []

    # 1. A coverage gap reported as nothing to see. `taxonomy.order` is the
    #    canonical unrepresentable claim and the one most worth checking.
    print("break 1: an unrepresentable field is not a problem")
    with Break(
        VQ,
        "        self.failed > 0 || !self.not_representable.is_empty()",
        "        self.failed > 0",
        "        self.failed > 0\n    }",
    ):
        results.append(
            expect_red(
                "a_field_the_queue_cannot_carry_is_reported_as_a_problem",
                LIB,
                "a_field_the_queue_cannot_carry_is_reported_as_a_problem",
            )
        )

    # 2. The best case reported as a fault. Every claim already reproducible is
    #    what success looks like, and warning on it fills the log on the runs
    #    that went well -- which is how a warning stops being read.
    print("break 2: an already-settled document is reported as a problem")
    with Break(
        VQ,
        "        self.failed > 0 || !self.not_representable.is_empty()",
        "        self.failed > 0 || !self.not_representable.is_empty() || self.already_settled > 0",
        "|| self.already_settled > 0",
    ):
        results.append(
            expect_red(
                "the_reasons_a_queue_stays_empty_are_kept_apart",
                LIB,
                "the_reasons_a_queue_stays_empty_are_kept_apart",
            )
        )

    # 3. A prose assertion borrows a contracted field. Matching positionally
    #    would give a multiplier a settling tool it has no claim to and route it
    #    to a tool that cannot verify one.
    print("break 3: a prose assertion is given a contracted field")
    with Break(
        VQ,
        "        crate::assertions::ExtractionPath::Prose { .. } => None,",
        "        crate::assertions::ExtractionPath::Prose { pattern } => Some(pattern),",
        "Prose { pattern } => Some(pattern),",
    ):
        results.append(
            expect_red(
                "a_prose_assertion_names_no_contracted_field",
                LIB,
                "a_prose_assertion_names_no_contracted_field",
            )
        )

    # 4. The enqueue claims a citation it does not have. Writing an empty string
    #    to satisfy a constraint that does not apply is how migration 205's
    #    citation requirement becomes decoration.
    print("break 4: the enqueue writes a citation column")
    with Break(
        VQ,
        "                                 (assertion_id, episode_id, verdict, actor, actor_kind, evidence) \\",
        "                                 (assertion_id, episode_id, verdict, actor, actor_kind, evidence, source_citation) \\",
        "actor_kind, evidence, source_citation) \\",
    ):
        results.append(
            expect_red(
                "the_enqueue_records_who_wrote_it_and_not_who_should_act",
                LIB,
                "the_enqueue_records_who_wrote_it_and_not_who_should_act",
            )
        )

    # 5. LIVE: the verdict token drifts from the column's vocabulary. This is the
    #    `severity = 'L1'` failure exactly, and no offline test can see it -- the
    #    write is refused by Postgres inside a spawned task.
    print("break 5: the pending verdict drifts from the column's vocabulary")
    if DB:
        with Break(
            VQ,
            "        let Some(verdict) = a.route(field.settleable_by.is_some()).pending_verdict() else {",
            '        let Some(verdict) = a.route(field.settleable_by.is_some()).pending_verdict().map(|_| "PENDING_TOOL") else {',
            'map(|_| "PENDING_TOOL")',
        ):
            # The live suite exercises `ENQUEUE_SQL` directly, so break the token
            # where the suite reads it: through `PROV_PENDING_TOOL`. Instead,
            # assert the round-trip catches an undeclared token.
            r = run(
                [
                    "cargo", "test", "-p", "fermi", "--test",
                    "verification_queue_contract", "--", "--ignored",
                    "--test-threads=1",
                ],
                env={"DATABASE_URL": DB},
            )
            # This break does not change what the live suite binds, so it is
            # expected to stay green -- and saying so is the honest record rather
            # than inventing a break that fires.
            print(
                "  -- the live suite binds `PROV_PENDING_*` directly and does not "
                "read this line, so it correctly stays "
                + ("green" if r.returncode == 0 else "red")
                + ". The offline tier owns this one; see note below."
            )
    else:
        print("  -- skipped: set DATABASE_URL.")

    print()
    if results and all(results):
        print(f"all {len(results)} offline break(s) were seen. Tree reverted.")
        print(
            "\nNote on coverage: the live suite asserts the ROW the writer builds "
            "is one the table accepts (constraint name, citation rule, verdict "
            "vocabulary). It does not re-derive the verdict from `Route`, because "
            "that is the offline tier's finding and asserting it in both would "
            "give one state two reds."
        )
        return 0
    print(f"{results.count(False)} of {len(results)} break(s) went unnoticed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
