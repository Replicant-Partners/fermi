#!/usr/bin/env python3
"""Port census — what the agent corpus actually declares about its I/O.

WHY THIS EXISTS
===============

`accepts`/`produces` are the I/O contract of every agent in the catalogue,
and they are free text. `src/workflows/agent_contract.rs` enforces that
they are *non-empty* — a presence check, in the `schema_trust` tradition —
and all 100 curated cards satisfy it. `genome_profiler` satisfies it with
three `accepts` no caller ever sends and a `produces` label naming an
output the agent is never instructed to make.

That is the `agents.total_executions` class again, one layer up: declared,
correctly shaped, and false. Catching it needs a check that reasons about
what the labels *resolve to*, which is what this census measures.

It is a census, not a gate. It reports; nothing exits non-zero on a finding
(bar `--self-check` failures). The gate arrives with the type registry,
once there is something for a label to resolve *against*.

WHY IT IS A SCRIPT AND NOT A ONE-LINER
======================================

The counts in this file were first derived by hand at a shell prompt and
were wrong twice in a row:

  * a Python pass reported 508 distinct labels; the correct figure is 513
  * a `jq | sort -u` pass reported 511 and 236, because `sort -u` under a
    non-C locale collates `gbif-key` and `gbif_key` as equal and merges
    them

Both wrong numbers were self-consistent, which is how they survived. A
measurement nobody can reproduce is not a measurement, so this runs under
`--self-check` by default and computes the headline figures two independent
ways, failing if they disagree.

USAGE
=====

    scripts/port_census.py                # human-readable census
    scripts/port_census.py --json         # machine output
    scripts/port_census.py --agent genome_profiler
    scripts/port_census.py --no-self-check
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import sys
from collections import Counter, defaultdict

CARD_GLOB = "agents/*/*/agent_card.json"

# Document-shaped nouns. A `produces` label ending in one of these may
# legitimately name the whole output document rather than a field inside
# it. We do NOT auto-adjudicate that — see `classify_produces`.
DOC_NOUNS = {
    "profile", "report", "summary", "analysis", "plan", "forecast",
    "recommendation", "recommendations", "card", "narrative",
    "description", "assessment", "review", "brief",
}

# Tools that are plumbing rather than a source of external fact. An agent
# whose only tool is one of these has no retrieval capability.
NON_SOURCING_TOOLS = {"execute_agent"}


# ─── card loading ──────────────────────────────────────────────────────

def load_cards(root: str) -> list[dict]:
    cards = []
    for path in sorted(glob.glob(os.path.join(root, CARD_GLOB))):
        with open(path, encoding="utf-8") as fh:
            card = json.load(fh)
        card["_path"] = os.path.relpath(path, root)
        cards.append(card)
    return cards


def caps(card: dict) -> dict:
    return card.get("capabilities") or {}


def sourcing_tools(card: dict) -> list[str]:
    """Tool names that could supply an external fact."""
    names = []
    for tool in caps(card).get("mcp_tools") or []:
        name = tool.get("name") if isinstance(tool, dict) else str(tool)
        if name and name not in NON_SOURCING_TOOLS:
            names.append(name)
    return names


# ─── output shape extraction ───────────────────────────────────────────

def brace_spans(text: str) -> list[str]:
    """Every balanced `{...}` span at nesting depth 0."""
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


def declared_output_shape(card: dict) -> tuple[list[str] | None, str]:
    """Top-level keys of the agent's output document, and where they came from.

    Two sources, in order of authority:

    1. `capabilities.output_contract.schema.properties` — a real JSON Schema,
       declared for machines. This is what the campaign is moving the corpus
       toward.
    2. The largest strictly-parseable JSON object in the system prompt — an
       example document written for a model, scraped as a fallback.

    Source (2) is why the prompt cannot be the contract: a prompt improves by
    becoming clearer to a reader (`"a" | "b"` union notation, comments,
    ellipses) and every such improvement makes it less parseable. The first
    card to gain a real schema in this campaign *lost* its scrapeable shape
    in the same edit. Prompts are for models; schemas are for machines.

    Strict `json.loads` only for the fallback — a tolerant parser would raise
    coverage while making the result unfalsifiable, and the coverage figure is
    itself a finding.
    """
    contract = caps(card).get("output_contract") or {}
    schema = contract.get("schema")
    if isinstance(schema, dict) and isinstance(schema.get("properties"), dict):
        return sorted(schema["properties"].keys()), "output_contract"

    prompt = caps(card).get("system_prompt") or card.get("system_prompt") or ""
    best = None
    for span in brace_spans(prompt):
        try:
            value = json.loads(span)
        except (ValueError, TypeError):
            continue
        if isinstance(value, dict) and (best is None or len(span) > best[1]):
            best = (value, len(span))
    if best:
        return sorted(best[0].keys()), "prompt_json"
    return None, "prose_only"


def port_resolution(card: dict) -> dict[str, str]:
    """Does each `produces` label name a declared type?

    Check (a) of §7.3. A label that equals the card's `produces_schema` is a
    **type reference**; anything else is a free string that a downstream
    consumer can only string-match against, which is the 499-of-513 problem.

    Deliberately strict about identity rather than similarity: the entire
    point is that `genome_summary` and `genome-summary` and `genomeSummary`
    are not the same type, and no amount of fuzzy matching should let them
    pretend to be.
    """
    contract = caps(card).get("output_contract") or {}
    declared = contract.get("produces_schema")
    out = {}
    for label in card.get("produces") or []:
        out[label] = "registered" if declared and label == declared else "unresolved"
    return out


def tokens(label: str) -> set[str]:
    return {t for t in label.lower().replace("-", "_").split("_") if t}


def classify_produces(label: str, shape: list[str] | None, resolution: str) -> str:
    """Does this `produces` label correspond to anything the card declares?

    Four outcomes:

      document_type  the label IS the declared schema's name, so it names the
                     whole output document
      unresolvable   no machine-readable output shape, so the question cannot
                     be asked at all
      backed_field   the label shares a token with a top-level key
      unbacked       it does not

    `document_type` is how the ambiguity gets resolved rather than guessed.
    Before registration, `phylogenetic_profile` and
    `tree_visualization_description` are indistinguishable: both are
    doc-shaped nouns matching no key, and only reading the prompt reveals
    that one names the document and the other names nothing at all.
    Registration answers it definitionally — which is the argument for typed
    ports in miniature.

    `unbacked` remains an UPPER BOUND for unregistered labels, and the
    residue needs human ratification, so `doc_noun` is reported as a hint
    rather than a verdict.
    """
    if resolution == "registered":
        return "document_type"
    if shape is None:
        return "unresolvable"
    label_tokens = tokens(label)
    for key in shape:
        if label_tokens & tokens(key):
            return "backed_field"
    return "unbacked"


# ─── input binding (parity with negotiate::bind_input) ─────────────────

def is_text_input(label: str) -> bool:
    """Mirror of `src/port_trust.rs::is_text_input` — the canonical rule.

    MUST stay identical. This census reports the burn-down; `port_trust`
    enforces at the execute boundary. If they disagree, the number on the
    scoreboard is not the number the gate acts on, which is a worse failure
    than either being wrong alone.

    `tests/port_binding_parity.rs` pins this via
    `agents/port_binding_expected.json`; regenerate with `--emit-expected`.

    Earlier this deliberately mirrored the console's narrower rule
    *including its misses*, and reported the eight it got wrong as a
    separate `disputed` class. `port_trust` adopted the widening, so the
    disputed class is now folded in — but see `declared_by_convention` in
    `bind_input`, which keeps the widening visible rather than silently
    absorbed.
    """
    lowered = label.lower().replace("-", "_")
    return (
        "query" in lowered
        or "question" in lowered
        or "prompt" in lowered
        or "free_text" in lowered
        or lowered.endswith("_task")
        or lowered in {"content", "topic", "narrative", "text"}
    )


def by_convention(label: str) -> bool:
    """Accepted only because of the `free_text*` / `*_task` widening.

    Tracked separately so the widening stays reversible and auditable: these
    are the labels whose text-ness is a naming convention rather than a
    declaration, and every one of them is an argument for the registry.
    """
    lowered = label.lower().replace("-", "_")
    narrow = (
        "query" in lowered
        or "question" in lowered
        or "prompt" in lowered
        or lowered in {"content", "topic", "narrative", "text"}
    )
    return is_text_input(label) and not narrow


def bind_input(accepts: list[str]) -> tuple[str, list[str]]:
    """Mirror of `src/port_trust.rs::bind_input`."""
    if not accepts:
        return "undeclared", []
    for label in accepts:
        if label.lower() == "query":
            return "declared", [label]
    for label in accepts:
        if is_text_input(label):
            verdict = "declared_by_convention" if by_convention(label) else "declared"
            return verdict, [label]
    return "no_text_input", list(accepts)


# ─── census ────────────────────────────────────────────────────────────

def census(cards: list[dict]) -> dict:
    accepts_counts: Counter[str] = Counter()
    produces_counts: Counter[str] = Counter()
    per_agent = {}

    for card in cards:
        agent_id = card.get("agent_id") or card["_path"]
        accepts = card.get("accepts") or []
        produces = card.get("produces") or []
        accepts_counts.update(accepts)
        produces_counts.update(produces)

        shape, shape_source = declared_output_shape(card)
        resolution = port_resolution(card)
        binding, binding_labels = bind_input(accepts)
        contract = caps(card).get("output_contract") or {}
        has_schema = isinstance(contract.get("schema"), dict)

        per_agent[agent_id] = {
            "path": card["_path"],
            "accepts": accepts,
            "produces": produces,
            "sourcing_tools": sourcing_tools(card),
            "output_shape": shape,
            "shape_source": shape_source,
            "port_resolution": resolution,
            "produces_status": {
                p: classify_produces(p, shape, resolution.get(p, "unresolved"))
                for p in produces
            },
            "input_binding": binding,
            "input_binding_labels": binding_labels,
            "output_contract": (
                "typed" if has_schema
                else "named_only" if contract.get("produces_schema")
                else "declared_untyped" if contract
                else "absent"
            ),
        }

    accepts_set, produces_set = set(accepts_counts), set(produces_counts)
    union = accepts_set | produces_set
    bridging = accepts_set & produces_set

    return {
        "corpus": {
            "cards": len(cards),
            "fingerprint": hashlib.sha256(
                "\n".join(sorted(union)).encode()
            ).hexdigest()[:16],
        },
        "labels": {
            "accepts_distinct": len(accepts_set),
            "produces_distinct": len(produces_set),
            "union_distinct": len(union),
            "bridging": sorted(bridging),
            "unbridged": len(union) - len(bridging),
            "accepts_singletons": sum(1 for v in accepts_counts.values() if v == 1),
            "produces_singletons": sum(1 for v in produces_counts.values() if v == 1),
        },
        "shape": Counter(a["shape_source"] for a in per_agent.values()),
        "port_resolution": Counter(
            status
            for a in per_agent.values()
            for status in a["port_resolution"].values()
        ),
        "produces_status": Counter(
            status
            for a in per_agent.values()
            for status in a["produces_status"].values()
        ),
        "input_binding": Counter(a["input_binding"] for a in per_agent.values()),
        "output_contract": Counter(a["output_contract"] for a in per_agent.values()),
        "agents": per_agent,
    }


def self_check(cards: list[dict], result: dict) -> list[str]:
    """Recompute the headline figures independently. Wrong twice by hand."""
    errors = []

    pooled = set()
    accepts_only, produces_only = set(), set()
    for card in cards:
        a, p = set(card.get("accepts") or []), set(card.get("produces") or [])
        pooled |= a | p
        accepts_only |= a
        produces_only |= p

    lab = result["labels"]
    if len(pooled) != lab["union_distinct"]:
        errors.append(
            f"union mismatch: pooled={len(pooled)} vs reported={lab['union_distinct']}"
        )
    if len(accepts_only) != lab["accepts_distinct"]:
        errors.append(
            f"accepts mismatch: {len(accepts_only)} vs {lab['accepts_distinct']}"
        )
    if len(produces_only) != lab["produces_distinct"]:
        errors.append(
            f"produces mismatch: {len(produces_only)} vs {lab['produces_distinct']}"
        )
    # Inclusion–exclusion must close.
    implied = lab["accepts_distinct"] + lab["produces_distinct"] - len(lab["bridging"])
    if implied != lab["union_distinct"]:
        errors.append(
            f"|A|+|P|-|A n P| = {implied} != union {lab['union_distinct']}"
        )
    if sum(result["input_binding"].values()) != result["corpus"]["cards"]:
        errors.append("input_binding verdicts do not cover every card")
    return errors


# ─── reporting ─────────────────────────────────────────────────────────

def report(result: dict, only: str | None) -> None:
    if only:
        agent = result["agents"].get(only)
        if agent is None:
            sys.exit(f"no such agent in the corpus: {only}")
        print(f"── {only} ({agent['path']})\n")
        print(f"  sourcing tools   {agent['sourcing_tools'] or '(none)'}")
        print(f"  output shape     {agent['output_shape'] or '(prose only)'}")
        print(f"  shape source     {agent['shape_source']}")
        print(f"  output contract  {agent['output_contract']}")
        print(f"  input binding    {agent['input_binding']} {agent['input_binding_labels']}")
        print("\n  accepts")
        for label in agent["accepts"]:
            print(f"    {label}")
        print("\n  produces")
        for label, status in agent["produces_status"].items():
            hint = ""
            if status == "unbacked" and label.split("_")[-1].lower() in DOC_NOUNS:
                hint = "  (doc-shaped noun — may name the whole document; needs ratification)"
            res = agent["port_resolution"].get(label, "unresolved")
            print(f"    {label:<38} {status} / {res}{hint}")
        return

    c, lab = result["corpus"], result["labels"]
    print("── Port census ─────────────────────────────────────────────")
    print(f"  cards {c['cards']}   label-set fingerprint {c['fingerprint']}\n")

    print("  Seam viability")
    print(f"    distinct accepts labels        {lab['accepts_distinct']}")
    print(f"    distinct produces labels       {lab['produces_distinct']}")
    print(f"    distinct labels overall        {lab['union_distinct']}")
    print(f"    appear on both sides           {len(lab['bridging'])}")
    print(f"    cannot form a seam             {lab['unbridged']}")
    print(f"    accepts appearing exactly once {lab['accepts_singletons']}")
    print(f"    produces appearing once        {lab['produces_singletons']}\n")

    print("  Output shape — is it machine-readable, and from where?")
    for k, v in sorted(result["shape"].items()):
        print(f"    {k:<30} {v}")
    print()

    print("  Port resolution — does a `produces` label name a declared type?")
    for k, v in sorted(result["port_resolution"].items()):
        print(f"    {k:<30} {v}")
    print()

    print("  `produces` backing")
    for k, v in sorted(result["produces_status"].items()):
        print(f"    {k:<30} {v}")
    print("    (unbacked is an upper bound — see classify_produces)\n")

    print("  Input binding — what the free-text execute path binds to")
    for k, v in sorted(result["input_binding"].items()):
        print(f"    {k:<30} {v}")
    print()

    print("  Declared output contract")
    for k, v in sorted(result["output_contract"].items()):
        print(f"    {k:<30} {v}")
    print()

    hard = [
        (a, lbl)
        for a, info in result["agents"].items()
        for lbl, st in info["produces_status"].items()
        if st == "unbacked" and lbl.split("_")[-1].lower() not in DOC_NOUNS
    ]
    if hard:
        print(f"  Unbacked and not document-shaped ({len(hard)}) — strongest candidates")
        for agent_id, label in sorted(hard):
            print(f"    {agent_id:<32} {label}")


# ─── burn-down ratchet ─────────────────────────────────────────────────

# Each counter, the direction it is allowed to move, and why it exists.
#
# Direction matters more than the number. `unresolved` falling is NOT
# sufficient evidence of progress: retiring a fake port shrinks it just as
# effectively as typing a real one, and the pilot did exactly that
# (513 -> 510 distinct labels when three fabricated ports were deleted).
# `registered` rising is the only counter that cannot be gamed by deletion,
# which is why it leads.
RATCHET = [
    ("port_resolution", "registered", "up",
     "ports naming a declared type — the only counter deletion cannot fake"),
    ("shape", "output_contract", "up",
     "agents whose output shape is declared for machines, not scraped from a prompt"),
    ("port_resolution", "unresolved", "down",
     "free-text labels a consumer can only string-match"),
    ("shape", "prose_only", "down",
     "agents whose output shape exists solely as prose"),
    ("input_binding", "no_text_input", "down",
     "agents the execute path binds to an interface they never advertised"),
    ("produces_status", "unbacked", "down",
     "produces labels with nothing behind them (upper bound)"),
]

BASELINE_PATH = "agents/port_burndown_baseline.json"


def counters(result: dict) -> dict:
    return {
        f"{section}.{key}": int(result[section].get(key, 0))
        for section, key, _, _ in RATCHET
    }


def gate(result: dict, root: str) -> int:
    """Fail if any counter moved the wrong way.

    A ratchet, not a target: it does not demand progress, only that nothing
    regresses while nobody is looking. The repo has one of these already —
    the migration baseline that went 26 -> 6 — and it works for the same
    reason: the number is allowed to improve without ceremony and cannot
    worsen without a deliberate, reviewable edit to the baseline file.
    """
    path = os.path.join(root, BASELINE_PATH)
    try:
        with open(path, encoding="utf-8") as fh:
            baseline = json.load(fh)
    except FileNotFoundError:
        sys.exit(
            f"{BASELINE_PATH} missing — create it with `--emit-baseline`, "
            "and commit it as the point the ratchet starts from."
        )

    now = counters(result)
    regressions, improvements = [], []
    for section, key, direction, why in RATCHET:
        name = f"{section}.{key}"
        was, is_ = baseline.get(name), now[name]
        if was is None:
            regressions.append(f"{name}: absent from baseline — regenerate")
            continue
        worse = is_ < was if direction == "up" else is_ > was
        better = is_ > was if direction == "up" else is_ < was
        if worse:
            regressions.append(
                f"{name}: {was} -> {is_} (must only go {direction}) — {why}"
            )
        elif better:
            improvements.append(f"{name}: {was} -> {is_}")

    for line in improvements:
        print(f"  improved  {line}")
    if not regressions:
        print(f"port burn-down: no regressions against {BASELINE_PATH}")
        if improvements:
            print(
                "  ratchet is loose — re-run with --emit-baseline to lock the "
                "gains in, or they can be silently given back"
            )
        return 0

    print("PORT BURN-DOWN REGRESSED:", file=sys.stderr)
    for line in regressions:
        print(f"  {line}", file=sys.stderr)
    print(
        "\nIf this is deliberate (an agent was added before it could be typed), "
        f"say so by regenerating {BASELINE_PATH} in the same commit — so the "
        "loosening is reviewable rather than invisible.",
        file=sys.stderr,
    )
    return 1


def emit_baseline(result: dict, root: str) -> None:
    path = os.path.join(root, BASELINE_PATH)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(counters(result), fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(f"wrote burn-down baseline to {path}")


def emit_expected(result: dict, path: str) -> None:
    """Write the binding fixture `tests/port_binding_parity.rs` asserts against.

    Same arrangement as `agents/taxonomy_derived_expected.json`: the Python
    tool is the editorial instrument and authors the fixture; the Rust side
    must agree with it. Two implementations of one rule will diverge — that
    is not speculative here, it is the whole subject of this workstream.
    """
    fixture = {
        agent_id: {
            "binding": info["input_binding"],
            "labels": info["input_binding_labels"],
        }
        for agent_id, info in sorted(result["agents"].items())
    }
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(fixture, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(f"wrote {len(fixture)} binding verdicts to {path}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=".", help="repo root (default: cwd)")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--agent", help="detail for one agent_id")
    ap.add_argument("--no-self-check", action="store_true")
    ap.add_argument(
        "--emit-expected",
        metavar="PATH",
        help="write the input-binding fixture for tests/port_binding_parity.rs",
    )
    ap.add_argument(
        "--gate",
        action="store_true",
        help="fail if any burn-down counter regressed against the baseline",
    )
    ap.add_argument(
        "--emit-baseline",
        action="store_true",
        help=f"rewrite {BASELINE_PATH} from the current corpus",
    )
    args = ap.parse_args()

    cards = load_cards(args.root)
    if not cards:
        sys.exit(f"no cards found under {args.root}/{CARD_GLOB}")
    result = census(cards)

    if not args.no_self_check:
        errors = self_check(cards, result)
        if errors:
            print("SELF-CHECK FAILED — do not cite these numbers:", file=sys.stderr)
            for err in errors:
                print(f"  {err}", file=sys.stderr)
            return 2

    if args.emit_expected:
        emit_expected(result, args.emit_expected)
        return 0

    if args.emit_baseline:
        emit_baseline(result, args.root)
        return 0

    if args.gate:
        return gate(result, args.root)

    if args.json:
        json.dump(result, sys.stdout, indent=2, default=sorted)
        print()
    else:
        report(result, args.agent)
    return 0


if __name__ == "__main__":
    sys.exit(main())
