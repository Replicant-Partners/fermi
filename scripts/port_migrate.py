#!/usr/bin/env python3
"""Propose a typed `output_contract` from the evidence a card actually carries.

WHY THIS IS A PROPOSER AND NOT A GENERATOR
==========================================

The obvious idea is to turn `produces: ["anomaly-triage-plan", ...]` into a
JSON Schema automatically. Measured across the 100 curated cards, the
evidence for doing that does not exist:

    accepts  labels  330 | match a declared tool input:        18  (5%)
    produces labels  339 | match a key in a prompt JSON shape: 17  (5%)
    cards with any parseable output shape at all:              25 / 100

For ~95% of labels there is nothing in the card corroborating them. A
generator would therefore be inventing the type system for almost the whole
corpus — and the result would look completely convincing, because a
plausible schema is exactly as well-formed as a true one. That is the defect
`src/grounding_trust.rs` exists to catch, industrialised.

So this tool emits a **draft**, annotates every value with the evidence that
produced it, and marks the rest `NEEDS_AUTHOR`.

THE MARKERS DELIBERATELY FAIL VALIDATION
========================================

`NEEDS_AUTHOR` is not a valid `grounding.status`, so a draft cannot be
pasted into a card and published: `card_contract::validate` rejects it on
`grounding_status_valid`. That is intentional. A migration tool whose output
passes the gate it is migrating toward is a fabrication engine with good
manners.

EVIDENCE, IN DESCENDING ORDER OF TRUST
======================================

  response_text   what the agent actually returned. Gold standard, and the
                  reason migration 199 exists. Not yet available — the
                  column began accruing on deploy. See issue #34.
  prompt_json     a strictly-parseable JSON object in the system prompt.
                  Real shape, written by the author. 25 cards have one.
  tool_input      a declared tool's `input_schema` property. Real JSON
                  Schema, useful for `accepts`.
  enumeration     a closed set stated in the prompt ("L0|L1|L2|L3",
                  "approve/relabel/intervene"). A real constraint, but it
                  says nothing about which FIELD carries it.
  label           the port name. A name. Not evidence of anything.

USAGE
=====

    scripts/port_migrate.py --triage                 # the whole queue, by track
    scripts/port_migrate.py --propose anomaly_triager
    scripts/port_migrate.py --propose anomaly_triager --write   # into the card
"""

from __future__ import annotations

import argparse
import collections
import glob
import json
import os
import re
import sys

# Not a valid `grounding.status`, so a draft cannot pass the publish gate.
NEEDS_AUTHOR = "NEEDS_AUTHOR"

# Document-shaped nouns, used only to propose a type NAME.
DOC_NOUNS = ("plan", "profile", "report", "summary", "assessment", "analysis",
             "recommendation", "forecast", "result", "response", "output")


# ─── loading ───────────────────────────────────────────────────────────

def load(root: str) -> dict[str, dict]:
    out = {}
    for path in sorted(glob.glob(os.path.join(root, "agents/*/*/agent_card.json"))):
        with open(path, encoding="utf-8") as fh:
            card = json.load(fh)
        card["_path"] = path
        out[card["agent_id"]] = card
    return out


def caps(card: dict) -> dict:
    return card.get("capabilities") or {}


def prompt_of(card: dict) -> str:
    return caps(card).get("system_prompt") or card.get("system_prompt") or ""


def tools_of(card: dict) -> list[dict]:
    return [t for t in (caps(card).get("mcp_tools") or []) if isinstance(t, dict)]


# ─── evidence gathering ────────────────────────────────────────────────

def brace_spans(text: str) -> list[str]:
    spans, depth, start = [], 0, None
    for i, ch in enumerate(text):
        if ch == "{":
            if depth == 0:
                start = i
            depth += 1
        elif ch == "}" and depth > 0:
            depth -= 1
            if depth == 0 and start is not None:
                spans.append(text[start:i + 1])
    return spans


def prompt_shape(card: dict) -> dict | None:
    """Largest strictly-parseable JSON object in the prompt.

    Strict only. A tolerant parser would raise coverage while making the
    result unfalsifiable, and coverage is itself the finding.
    """
    best = None
    for span in brace_spans(prompt_of(card)):
        try:
            v = json.loads(span)
        except (ValueError, TypeError):
            continue
        if isinstance(v, dict) and (best is None or len(span) > best[1]):
            best = (v, len(span))
    return best[0] if best else None


def tool_inputs(card: dict) -> dict[str, tuple[str, dict]]:
    """`property name -> (tool name, its JSON Schema fragment)`."""
    out = {}
    for t in tools_of(card):
        for name, spec in ((t.get("input_schema") or {}).get("properties") or {}).items():
            out.setdefault(name, (t.get("name", "?"), spec))
    return out


ENUM_RE = re.compile(r"`?\b([a-z][a-z0-9_]{2,})(?:\s*[|/]\s*([a-z][a-z0-9_]{2,})){1,6}\b`?")


def enumerations(card: dict) -> list[list[str]]:
    """Closed sets stated in the prompt, e.g. `approve|relabel|intervene`.

    A real constraint that belongs in a schema — but it says nothing about
    WHICH field carries it, so it is offered as a hint and never wired in
    automatically.
    """
    found, seen = [], set()
    for m in ENUM_RE.finditer(prompt_of(card)):
        parts = [p.strip(" `") for p in re.split(r"[|/]", m.group(0).strip(" `"))]
        parts = [p for p in parts if re.fullmatch(r"[a-z][a-z0-9_]{2,}", p)]
        if len(parts) >= 2:
            key = tuple(sorted(parts))
            if key not in seen:
                seen.add(key)
                found.append(parts)
    return found[:8]


# ─── triage ────────────────────────────────────────────────────────────

def track(card: dict) -> tuple[str, str]:
    """Which migration track is this agent on, and why."""
    oc = caps(card).get("output_contract") or {}
    if isinstance(oc.get("schema"), dict) and (oc["schema"].get("properties")):
        return "TYPED", "already declares a schema"
    if prompt_shape(card) is not None:
        return "RATIFY", "prompt carries a JSON shape — extract, confirm, adopt"
    if oc.get("produces_schema"):
        return "AUTHOR", "names a type but declares none; the name is not a contract"
    return "AUTHOR_OR_PROSE", "no shape anywhere — decide whether it should emit one"


# ─── proposal ──────────────────────────────────────────────────────────

def propose(card: dict) -> dict:
    agent_id = card["agent_id"]
    shape = prompt_shape(card)
    tnames = [t.get("name", "?") for t in tools_of(card)]

    # Type NAME. Derived from the agent id plus a document noun found in the
    # produces labels — a naming convenience, not a claim about content.
    noun = next(
        (n for p in (card.get("produces") or []) for n in DOC_NOUNS if n in p.lower()),
        "output",
    )
    type_name = f"{agent_id}/{noun}"

    properties: dict = {}
    grounding: dict = {}

    if shape:
        for key in sorted(shape):
            properties[key] = {"_evidence": "prompt_json", "type": ["object", "array", "string", "number", "boolean", "null"]}
            grounding[key] = {
                "status": NEEDS_AUTHOR,
                "why": (
                    f"Field `{key}` was found in the prompt's example document, so its "
                    f"NAME is evidence-backed. Where its VALUE comes from is not: choose "
                    f"sourced (name one of {tnames or ['(no tools declared)']} and the "
                    f"response field), inferred (say what from), narrative, or unavailable."
                ),
            }
    else:
        grounding["_no_shape"] = {
            "status": NEEDS_AUTHOR,
            "why": (
                "This card declares no output shape anywhere — not in an "
                "output_contract and not as a JSON example in the prompt. Nothing "
                "can be proposed without inventing it. Either write the document "
                "shape into the prompt and re-run this tool, or declare the agent "
                "conversational and leave it untyped and non-composable."
            ),
        }

    draft = {
        "_draft": True,
        "_generated_by": "scripts/port_migrate.py",
        "_evidence_note": (
            "Every NEEDS_AUTHOR must be replaced before this card can publish; "
            "NEEDS_AUTHOR is not a valid grounding.status, so the gate rejects "
            "this draft by construction."
        ),
        "domain": caps(card).get("output_contract", {}).get("domain") or f"{agent_id}_domain",
        "produces_schema": type_name,
        "schema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": type_name,
            "type": "object",
            "properties": properties,
        },
        "grounding": grounding,
    }
    return draft


def report_proposal(card: dict) -> None:
    agent_id = card["agent_id"]
    shape = prompt_shape(card)
    tin = tool_inputs(card)
    enums = enumerations(card)
    tr, why = track(card)

    print(f"── {agent_id}   [{tr}]  {why}\n")

    print("  Declared today (labels, unverified)")
    print(f"    accepts   {card.get('accepts') or '—'}")
    print(f"    produces  {card.get('produces') or '—'}\n")

    print("  Evidence found")
    print(f"    prompt_json   {sorted(shape) if shape else '(none — no parseable shape)'}")
    matched = [a for a in (card.get("accepts") or []) if a.replace("-", "_") in tin]
    print(f"    tool_input    {matched or '(no accepts label matches a tool input)'}")
    print(f"    enumeration   {enums[:3] if enums else '(none detected)'}")
    print("    response_text (not yet available — mig-199 began accruing on deploy; #34)\n")

    draft = propose(card)
    holes = sum(
        1 for g in draft["grounding"].values() if g.get("status") == NEEDS_AUTHOR
    )
    print(f"  Draft: {len(draft['schema']['properties'])} field(s) proposed, "
          f"{holes} decision(s) left to you\n")
    print(json.dumps(draft, indent=2))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=".")
    ap.add_argument("--propose", metavar="AGENT_ID")
    ap.add_argument("--triage", action="store_true", help="the whole queue, by track")
    ap.add_argument("--write", action="store_true",
                    help="write the draft into the card (still unpublishable)")
    args = ap.parse_args()

    cards = load(args.root)
    if not cards:
        sys.exit("no cards found")

    if args.triage:
        by = collections.defaultdict(list)
        for cid, card in cards.items():
            by[track(card)[0]].append(cid)
        print("── Migration queue ─────────────────────────────────────────\n")
        for name, blurb in [
            ("TYPED", "done — schema declared"),
            ("RATIFY", "prompt has a shape: extract, confirm, adopt (semi-mechanical)"),
            ("AUTHOR", "names a type but declares none"),
            ("AUTHOR_OR_PROSE", "no shape at all: author one, or declare it conversational"),
        ]:
            ids = sorted(by.get(name, []))
            print(f"  {name:<16} {len(ids):>3}   {blurb}")
            if name in ("TYPED", "RATIFY"):
                for i in ids:
                    print(f"                       · {i}")
        print("\n  Nothing here is automatic. RATIFY is the cheap track; the rest is")
        print("  authoring, and no amount of label-reading substitutes for it.")
        return 0

    if args.propose:
        card = cards.get(args.propose)
        if not card:
            sys.exit(f"no such agent: {args.propose}")
        report_proposal(card)
        if args.write:
            card.setdefault("capabilities", {})["output_contract"] = propose(card)
            path = card.pop("_path")
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(card, fh, indent=2, ensure_ascii=False)
                fh.write("\n")
            print(f"\nwrote draft into {path} — still unpublishable until the "
                  f"NEEDS_AUTHOR entries are resolved")
        return 0

    ap.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
