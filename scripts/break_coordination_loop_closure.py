#!/usr/bin/env python3
"""Break each fix in `tests/coordination_loop_closure.rs` and require a red.

Two defects were fixed together, and both are the kind that leave the build
clean and every existing test green:

  1. the coordination strategist ran with no KG retrieval and persisted no
     episode, so the agent told to notice recurring patterns had no record that
     any previous session existed;
  2. Stage 0 declared every member's intention on the member's behalf, so the
     conflict checker compared one agent's guesses to each other.

A guard against a silent defect is worth exactly as much as its ability to go
red, and that is not observable from a passing suite. So each break below puts
the code back into the broken state and requires the named test to fail.

The specific trap this script was written after: the first version of
`every_agent_execution_path_persists_an_episode` scanned for the bare string
`agent_output_to_episode`, and `coherence.rs` already contained it — in an
import it never called, with a comment saying so. The test passed against the
defect. Break 2 below is that exact state, and it must be red.

A break that comes back GREEN is a failure of the guard, reported as one.

Run from the repository root:
    python3 scripts/break_coordination_loop_closure.py
"""

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

COHERENCE = REPO / "src" / "handlers" / "workspace" / "coherence.rs"
FLOOR = REPO / "src" / "plan_solicitation.rs"
INTENTIONS = REPO / "src" / "intentions.rs"
TOOLS = REPO / "src" / "agent_backend" / "tools_legacy.rs"
LOOP_MODEL = REPO / "src" / "loop_model.rs"
CARD = REPO / "agents" / "curated" / "cohere_and_coordinate" / "agent_card.json"


def run(args):
    return subprocess.run(args, cwd=REPO, capture_output=True, text=True, timeout=2400)


def expect_red(name, args, must_mention):
    """Run a selector and require it to fail, naming the right thing."""
    r = run(args)
    blob = r.stdout + r.stderr
    if r.returncode == 0:
        print(f"  !! {name}: STAYED GREEN. The break applied and nothing saw it.")
        return False
    if must_mention not in blob:
        print(
            f"  !! {name}: went red, but `{must_mention}` is not in the output — "
            f"it may have failed for an unrelated reason, which is not evidence."
        )
        return False
    print(f"  ok {name}: red, in {must_mention}")
    return True


class Break:
    """Apply one substitution, assert it actually landed, always revert."""

    def __init__(self, path, old, new, expect_present, expect_absent=None):
        self.path = path
        self.old = old
        self.new = new
        self.expect_present = expect_present
        self.expect_absent = expect_absent
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
        # Assert the resulting STATE, not merely that a substitution happened.
        now = self.path.read_text()
        assert self.expect_present in now, (
            f"the edit applied but the state it was named for is not in the "
            f"file: {self.expect_present!r}"
        )
        if self.expect_absent is not None:
            assert self.expect_absent not in now, (
                f"the edit applied but {self.expect_absent!r} is still present, "
                f"so the break is not the state it claims to be"
            )
        return self

    def __exit__(self, *exc):
        self.path.write_text(self.original)
        assert self.path.read_text() == self.original, "failed to revert!"
        return False


SUITE = ["cargo", "test", "--test", "coordination_loop_closure"]


def main():
    results = []

    # ── 1. the strategist stops retrieving what it learned ────────────────
    #
    # The state Loop 1 was in for this agent: the import is present, the call
    # is gone, the build is clean, and every session opens as its first.
    print("break 1: the strategist runs without KG retrieval")
    src = COHERENCE.read_text()
    start = src.index("                let card = match strategist.as_ref() {")
    end = src.index("                let agent_stmt = ast::AgentStmt {")
    with Break(
        COHERENCE,
        src[start:end],
        "",
        "let agent_stmt = ast::AgentStmt {",
        expect_absent="enrich_with_kg_context(",
    ):
        results.append(
            expect_red(
                "retrieval is wired on every execution path",
                SUITE,
                "every_agent_execution_path_retrieves_learned_knowledge",
            )
        )

    # ── 2. the strategist stops recording its run ─────────────────────────
    #
    # THE VACUITY TEST. `agent_output_to_episode` stays in the import line and
    # in the prose. Only the call goes. A scan for the bare name passes here;
    # the guard must not.
    print("break 2: the strategist's run is dropped (import left behind)")
    src = COHERENCE.read_text()
    start = src.index("                        if let Ok(db_agent) = strategist.as_ref() {")
    end = src.index("                        let _ = state.registry.record_execution(")
    with Break(
        COHERENCE,
        src[start:end],
        "",
        "use crate::{agent_output_to_episode",
        expect_absent="agent_output_to_episode(",
    ):
        results.append(
            expect_red(
                "an episode is written, not merely importable",
                SUITE,
                "every_agent_execution_path_persists_an_episode",
            )
        )

    # ── 3. delegated work is orphaned again ───────────────────────────────
    print("break 3: solicited plans are recorded as roots")
    with Break(
        COHERENCE,
        "parent_episode_id: Some(strategist_episode_id),",
        "parent_episode_id: None, // the strategist's run is not a parent",
        "parent_episode_id: None,",
    ):
        results.append(
            expect_red(
                "delegations hang off the strategist's episode",
                SUITE,
                "delegations_from_the_strategist_hang_off_its_own_episode",
            )
        )

    # ── 4. the coordinator's guesses collide with each other again ────────
    #
    # The substantive half of defect 2. Two rows the strategist wrote from one
    # transcript score 0.99 against each other, and `suggest_differentiation`
    # sends two agents off to split work neither claimed.
    print("break 4: two inferred intentions are compared to each other")
    with Break(
        INTENTIONS,
        "            if !a.source.is_first_hand() && !b.source.is_first_hand() {\n                continue;\n            }\n",
        "",
        "let sim = match (&a.embedding, &b.embedding) {",
        expect_absent="if !a.source.is_first_hand() && !b.source.is_first_hand()",
    ):
        results.append(
            expect_red(
                "inferred-vs-inferred overlap is suppressed",
                ["cargo", "test", "-p", "fermi", "--lib", "intentions::"],
                "two_inferred_intentions_do_not_overlap_with_each_other",
            )
        )

    # ── 5. provenance becomes a claim the caller makes ────────────────────
    #
    # The model told that first-hand rows are trusted more, handed a field with
    # which to assert its guess was a report.
    print("break 5: `source` is accepted from the tool input")
    with Break(
        TOOLS,
        '        Some(caller) if caller == agent_id => crate::intentions::IntentionSource::SelfDeclared,',
        '        Some(_) if input.get("source").is_some() => crate::intentions::IntentionSource::SelfDeclared,',
        'input.get("source").is_some()',
        expect_absent="Some(caller) if caller == agent_id",
    ):
        results.append(
            expect_red(
                "provenance is derived, not asserted",
                SUITE,
                "intention_provenance_is_derived_from_the_caller",
            )
        )

    # ── 6. the loop model stops distinguishing asked from assumed ─────────
    #
    # One combined count is what let the stage read as healthy while one agent
    # had written every row about all the others.
    print("break 6: `plans` counts inferred rows as though they were answers")
    with Break(
        LOOP_MODEL,
        'sink_sql: "SELECT count(*)::bigint AS n FROM workspace_intentions \\\n                            WHERE source = \'solicited\'",',
        'sink_sql: "SELECT count(*)::bigint AS n FROM workspace_intentions",',
        'id: "plans"',
        expect_absent="WHERE source = 'solicited'",
    ):
        results.append(
            expect_red(
                "the plans sink counts only solicited rows",
                SUITE,
                "the_loop_model_distinguishes_asked_for_plans_from_inferred_ones",
            )
        )

    # ── 7. Stage 0 goes back to assuming first ────────────────────────────
    #
    # The tool still exists and is still dispatchable. Only the order changes,
    # and order is the instruction: the first tool named is the one called.
    print("break 7: Stage 0 reaches for declare_intention first")
    card = CARD.read_text()
    anchor = "- Call `get_intention_map`. Read `grounding_reading` first"
    assert card.count(anchor) == 1, "Stage 0 bullet list has moved"
    with Break(
        CARD,
        anchor,
        "- Call `declare_intention` for each member's next action, then "
        "`get_intention_map`. Read `grounding_reading` first",
        "- Call `declare_intention` for each member's next action",
    ):
        results.append(
            expect_red(
                "Stage 0 asks before it assumes",
                SUITE,
                "stage_0_leads_with_soliciting_rather_than_inferring",
            )
        )

    # ── 8. the floor stops asking ──────────────────────────────────
    #
    # The exact state Stage 0 shipped in: `solicit_agent_plan` exists, is
    # dispatchable, is named in the card and in the prompt — and whether any
    # member is ever asked comes down to whether a model feels like calling it.
    # Build clean, endpoint returns 200, `source='solicited'` stays at zero.
    print("break 8: the platform stops soliciting and only asks a model to")
    src = COHERENCE.read_text()
    start = src.index("    let plan_floor = if depth == \"recommendations\" {")
    end = src.index("    // For premium tiers, invoke the workspace's strategist directly.")
    with Break(
        COHERENCE,
        src[start:end],
        "    let plan_floor = fermi::plan_solicitation::Floor::default();\n\n",
        "let plan_floor = fermi::plan_solicitation::Floor::default();",
        expect_absent="run_plan_floor(&state",
    ):
        results.append(
            expect_red(
                "the platform asks, not just the model",
                SUITE,
                "the_platform_solicits_plans_rather_than_only_asking_a_model_to",
            )
        )

    # ── 9. the floor runs after the strategist ─────────────────────────
    #
    # The subtle one, and the reason it gets its own break: every count is
    # identical either way. Plans are solicited, rows land, `loop3.plans`
    # climbs — and the run that paid for it diagnosed against a map the floor
    # had not filled yet. A retrospective floor for a prospective stage.
    print("break 9: the floor runs after the strategist instead of before")
    src = COHERENCE.read_text()
    start = src.index("    let plan_floor = if depth == \"recommendations\" {")
    end = src.index("    // For premium tiers, invoke the workspace's strategist directly.")
    block = src[start:end]
    # Move it below the strategist by parking a default before and the real
    # call after. Compiles, runs, grounds nothing this cycle.
    moved = (
        "    let plan_floor = fermi::plan_solicitation::Floor::default();\n\n"
        + "    // For premium tiers, invoke the workspace's strategist directly."
    )
    with Break(
        COHERENCE,
        block + "    // For premium tiers, invoke the workspace's strategist directly.",
        moved,
        "let plan_floor = fermi::plan_solicitation::Floor::default();",
        expect_absent="run_plan_floor(&state",
    ):
        results.append(
            expect_red(
                "the floor precedes the strategist",
                SUITE,
                "the_plan_floor_runs_before_the_strategist",
            )
        )

    # ── 10. the cap stops being reported ─────────────────────────────
    #
    # Truncate silently and the strategist reads a partially grounded map as a
    # fully grounded one — treating the members nobody asked as members with
    # nothing to say.
    print("break 10: the per-run cap truncates silently")
    with Break(
        COHERENCE,
        "        floor.capped = needing.len() - ps::MAX_PER_RUN;\n",
        "",
        "needing.truncate(ps::MAX_PER_RUN);",
        expect_absent="floor.capped = needing.len()",
    ):
        results.append(
            expect_red(
                "a truncated floor says so",
                SUITE,
                "the_floor_is_bounded_by_a_cap_and_a_freshness_window",
            )
        )

    # ── 11. the floor grows its own INSERT ───────────────────────────
    #
    # Two writers agree today and drift on `source` the first time either
    # changes — on the one field whose entire purpose is that a caller cannot
    # forge it. §3.4, on the value that carries the most weight.
    print("break 11: the floor writes its own intention row")
    with Break(
        FLOOR,
        "    let written = crate::agent_backend::tools::write_intention(",
        '    let _second_answer = "INSERT INTO workspace_intentions (source) VALUES (\'self\')";\n'
        "    let written = crate::agent_backend::tools::write_intention(",
        "INSERT INTO workspace_intentions",
    ):
        results.append(
            expect_red(
                "one intention writer, not two",
                SUITE,
                "both_solicitation_paths_share_one_intention_writer",
            )
        )

    # ── 12. the loop model claims a prompt still drives the stage ──────────
    #
    # `Prompted` licenses reading a zero as "the model declined". Once the
    # platform does the asking that reading is false, and leaving it would let
    # a genuinely broken floor hide behind a disposition it no longer has.
    print("break 12: `plans` is declared Prompted again")
    with Break(
        LOOP_MODEL,
        "                trigger: Trigger::Request,\n                accounted: Some(Sink::WorkspaceIntentions),",
        '                trigger: Trigger::Prompted { asked_by: "a prompt, maybe" },\n'
        "                accounted: Some(Sink::WorkspaceIntentions),",
        'asked_by: "a prompt, maybe"',
    ):
        results.append(
            expect_red(
                "the stage is Request-driven",
                SUITE,
                "the_loop_model_distinguishes_asked_for_plans_from_inferred_ones",
            )
        )

    print()
    if all(results):
        print(f"all {len(results)} break(s) were caught. The guards are load bearing.")
        return 0
    print(
        f"{results.count(False)} of {len(results)} break(s) went unnoticed. "
        f"Those guards assert nothing."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
