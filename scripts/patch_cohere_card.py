#!/usr/bin/env python3
"""One-shot patch: teach cohere_and_coordinate to ask, not assume.

Stage 0 declared every member's intention on their behalf, inferred from the
transcript, and the platform had no way to tell that from a member declaring
its own. `solicit_agent_plan` (mig-218) is the round trip; this rewrites the
stage to lead with it and adds the tool declaration.

Run from the repo root. Idempotent.
"""
import json
import pathlib
import sys

CARD = pathlib.Path("agents/curated/cohere_and_coordinate/agent_card.json")

OLD_MARKER = "## Stage 0 — Pre-flight (intention coordination)"
NEXT_MARKER = "\n\n---\n\n## Stage 1 — Assess"

NEW_STAGE_0 = """## Stage 0 — Pre-flight (intention coordination)

The intention map is platform state, not a file. The table behind it is `workspace_intentions`. Earlier versions of this protocol told you to keep it in `_coordination/intention_map.json`; that file was never wired to anything, so anything written there is invisible to the platform and to every other agent.

**Ask the agents. Do not infer their plans.** This is the part of Stage 0 that was missing for the life of the feature, and it is the part that makes the stage coordination rather than narration.

An intention is the agent's own account of what it is about to do. What you can form by reading a transcript is a *belief* about that account, and the two are different objects — the platform records the difference as `source`:

- `self` / `solicited` — first-hand. The agent said it.
- `inferred` — you wrote it from observation.
- `unattributed` — an old row whose author was never recorded.

Overlap detection between two `inferred` rows is suppressed, and it should be: both were written by you, in one turn, from one transcript, and a high similarity between two of your own paraphrases is a fact about your prose rather than about the team. Telling two agents to differentiate on that basis sends them off to split work neither of them ever claimed.

Before any significant agent action:

- Call `get_intention_map`. Read `grounding_reading` first — it tells you whether the map is the team's account of itself or your own. `UNGROUNDED` means nobody has been asked.
- Call `solicit_agent_plan` for each member whose next action bears on the workspace goal, passing `context` so the agent plans against the real objective and not only against the last twenty messages. It returns that agent's own plan, the conflict signal, and `teammate_assignment` — the agent's view of who should own what.
- **Compare the `teammate_assignment` answers across members.** Where two agents disagree about the division of labour, you have found a coordination failure that no TEC score surfaces and no transcript shows, because it lives in what each agent assumed rather than in what anyone said.
- Use `declare_intention` only for a member you could not reach. It records as `inferred`; say so in the brief rather than presenting it as the agent's plan.
- Every write is conflict-checked and returns one of: CLEAR | OVERLAP_WARNING | CONFLICT_ALERT | DEPENDENCY_WAIT | BUDGET_GATE. `check_conflicts` re-runs the check across the whole map without writing.
- On OVERLAP_WARNING, call `suggest_differentiation` and read its `grounding_caveat` before acting. An overlap between two solicited plans is a real collision; one involving an inferred row is a hypothesis you should put to the agents.
- Call `clear_intention` (`completed` | `cancelled` | `superseded`) when an action finishes. A stale active row generates phantom conflicts for everyone after it.
- Call `emit_coherence_signal` to feed IntentionAligns / IntentionConflicts into the coherence graph: aligned intentions are positive constraints (+), conflicting intentions are incoherence relations (−).

A CLEAR signal over an ungrounded map is not evidence of alignment. It is evidence that nobody was asked.

Skip Stage 0 on read-only or analytical invocations."""

SOLICIT_TOOL = {
    "name": "solicit_agent_plan",
    "description": (
        "Ask a workspace member what it intends to do next and record the answer as that "
        "agent's own plan (source=solicited). Returns the plan, the conflict signal against "
        "the rest of the map, and the agent's view of who should own what. Prefer this to "
        "declare_intention for any agent you can reach: a plan you inferred from the "
        "transcript is your belief about that agent, and two such beliefs cannot be checked "
        "against each other."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "agent_id": {
                "type": "string",
                "description": "The member to ask, by agent name or id. Must be a member of this workspace.",
            },
            "context": {
                "type": "string",
                "description": "What the workspace is trying to do right now, so the agent plans against the goal rather than only the transcript.",
            },
        },
        "required": ["agent_id"],
    },
}

# The six Stage 0 tools were named in the prompt and never declared on the card,
# so the runtime exposed them and the card did not admit to them.
INTENTION_TOOLS = [
    ("get_intention_map", "Read the workspace intention map, including each row's `source` and the map's overall `grounding_reading`."),
    ("declare_intention", "Register a planned next action. Records as `self` when an agent declares its own and `inferred` when declared on another agent's behalf."),
    ("check_conflicts", "Re-run conflict detection across the whole map without writing. Returns the signal plus grounding."),
    ("clear_intention", "Mark an intention completed, cancelled or superseded so it stops participating in conflict checks."),
    ("suggest_differentiation", "Report the axes on which two intentions overlap, with a grounding caveat when either side is second-hand. Names the pattern; does not prescribe the split."),
    ("emit_coherence_signal", "Push an IntentionAligns or IntentionConflicts relation into the TEC coherence graph."),
]


def main() -> int:
    if not CARD.exists():
        print(f"not found: {CARD} (run from the repo root)", file=sys.stderr)
        return 1

    card = json.loads(CARD.read_text())
    prompt = card["system_prompt"]

    start = prompt.find(OLD_MARKER)
    end = prompt.find(NEXT_MARKER)
    if start == -1 or end == -1 or end < start:
        print("could not locate Stage 0; card structure changed", file=sys.stderr)
        return 1
    card["system_prompt"] = prompt[:start] + NEW_STAGE_0 + prompt[end:]

    tools = card["capabilities"]["mcp_tools"]
    have = {t["name"] for t in tools}

    if "solicit_agent_plan" not in have:
        # Immediately before the tools it supersedes, so a reader meets the
        # asking tool before the assuming one.
        tools.insert(0, SOLICIT_TOOL)

    for name, desc in INTENTION_TOOLS:
        if name not in have:
            tools.append({"name": name, "description": desc})

    skills = card["capabilities"]["skills"]
    if "plan-solicitation" not in skills:
        skills.insert(skills.index("intention-coordination") + 1, "plan-solicitation")

    CARD.write_text(json.dumps(card, indent=2, ensure_ascii=False) + "\n")
    print(f"patched {CARD}: {len(tools)} tools, {len(skills)} skills")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
