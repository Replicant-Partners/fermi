#!/usr/bin/env python3
"""Where the three tool declarations disagree.

Three places in this repo record which tool an agent uses, and they were free to
drift:

  1. agents/curated/<id>/agent_card.json -> capabilities.mcp_tools
     Read by the contract builder, the publish gate, invalid_tool_declarations.
  2. src/grounding_trust.rs -> FIELD_CONTRACTS, per output field.
     Read by the trace view, field_probe, hop enforcement.
  3. The system prompt, in English.
     Read by the model. Which is to say: it is the one that actually runs.

`capabilities.mcp_tools` does not restrict anything today -- the registry offers
every LLM-visible builtin in its class and the card only ADDS tools the registry
lacks -- so an agent can call a tool for 218 runs while declaring something else
entirely. That is how this was found. See docs/ISSUES_tool_declaration_gap.md.

SECTION 1 (contract vs card) is exact and is enforced by
tests/tool_declaration_reconciliation.rs. It should print nothing.

SECTION 2 (prompt vs card) is a REGEX OVER PROSE and is deliberately NOT
enforced anywhere. A prompt can name a tool in passing, describe one it does not
use, or discuss a concept that happens to share a tool's name. Every row is a
question, not a defect. This exits 0 whatever it finds.

Usage:  python3 scripts/tool_declaration_report.py [--quiet]
"""

import json
import os
import re
import sys
import glob

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QUIET = "--quiet" in sys.argv


def cards():
    out = {}
    for p in sorted(glob.glob(os.path.join(ROOT, "agents/curated/*/agent_card.json"))):
        agent = os.path.basename(os.path.dirname(p))
        try:
            out[agent] = json.load(open(p))
        except Exception as e:
            print("  ! unreadable card %s: %s" % (agent, e))
    return out


def declared(card):
    caps = card.get("capabilities") or {}
    return {
        t.get("name")
        for t in (caps.get("mcp_tools") or [])
        if isinstance(t, dict) and t.get("name")
    }


def contracted():
    """agent -> tools named by Grounding::Sourced in FIELD_CONTRACTS.

    A regex over Rust, which is why the exact version of this check lives in
    tests/tool_declaration_reconciliation.rs and reads the const through the
    compiler. This copy exists so the report is one command rather than two.
    """
    src = open(os.path.join(ROOT, "src/grounding_trust.rs")).read()
    pat = re.compile(
        r'agent_id:\s*"([a-z0-9_]+)"\s*,\s*\n\s*path:\s*"([^"]+)"\s*,\s*\n'
        r'\s*grounding:(?:[^\n]*\n){0,3}?\s*tool:\s*"([a-z0-9_]+)"'
    )
    out = {}
    for agent, _path, tool in pat.findall(src):
        out.setdefault(agent, set()).add(tool)
    return out


def registry_names():
    """Builtin tool names, by scanning the definition sites.

    `agent_backend::tools::platform_tool_names()` is authoritative; this cannot
    call it. Names are taken from `name: "..."` literals in the tool files and
    may over- or under-count. Stated rather than hidden, because a report whose
    inputs are approximate should say which ones.
    """
    names = set()
    for rel in (
        "src/agent_backend/tools_legacy.rs",
        "src/agent_backend/weather_tools.rs",
    ):
        p = os.path.join(ROOT, rel)
        if os.path.exists(p):
            names |= set(re.findall(r'name:\s*"([a-z][a-z0-9_]{5,})"', open(p).read()))
    return names


def main():
    C = cards()
    CON = contracted()
    TOOLS = registry_names()

    print("Tool declaration report")
    print("=" * 72)
    print("%d curated cards - %d agents with tool-sourced field contracts - "
          "%d builtin names found by scan" % (len(C), len(CON), len(TOOLS)))
    print()

    # ── 1. exact ────────────────────────────────────────────────────────
    print("1. FIELD_CONTRACTS names a tool the card does not declare  [EXACT, ENFORCED]")
    print("-" * 72)
    rows = 0
    for agent in sorted(CON):
        if agent not in C:
            if not QUIET:
                print("   (%s has contracts but no curated card on disk)" % agent)
            continue
        gap = sorted(CON[agent] - declared(C[agent]))
        if gap:
            rows += 1
            print("   %-24s contract: %s" % (agent, ", ".join(gap)))
            print("   %-24s card:     %s"
                  % ("", ", ".join(sorted(declared(C[agent]))) or "(nothing)"))
    print("   %d divergence(s)." % rows,
          "Enforced by tests/tool_declaration_reconciliation.rs." if not rows
          else "FAILS tests/tool_declaration_reconciliation.rs.")
    print()

    # ── 2. heuristic ────────────────────────────────────────────────────
    print("2. Prompt names a tool the card does not declare  [HEURISTIC, NOT ENFORCED]")
    print("-" * 72)
    print("   A regex over prose. Every row is a question, not a defect:")
    print("   a prompt may name a tool in passing, or a word may collide with one.")
    print()
    hits = []
    for agent in sorted(C):
        card = C[agent]
        prompt = card.get("system_prompt") or ""
        if not prompt:
            continue
        d = declared(card)
        named = {
            t for t in TOOLS
            if re.search(r"\b%s\b" % re.escape(t), prompt)
        }
        gap = sorted(named - d)
        if gap:
            hits.append((agent, gap, sorted(d)))
    for agent, gap, d in hits:
        print("   %-26s prompt: %s" % (agent, ", ".join(gap[:5])
                                       + (" (+%d)" % (len(gap) - 5) if len(gap) > 5 else "")))
        if not QUIET:
            print("   %-26s card:   %s" % ("", ", ".join(d[:5]) or "(nothing)"))
    print()
    print("   %d card(s) of %d." % (len(hits), len(C)))
    print()
    print("Deferred to the tool registry refactor: whether mcp_tools should GRANT")
    print("rather than describe, and whether the card's list should be GENERATED")
    print("from FIELD_CONTRACTS rather than maintained beside it.")

    # Always 0. This is a report.
    return 0


if __name__ == "__main__":
    sys.exit(main())
