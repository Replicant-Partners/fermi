#!/usr/bin/env python3
"""Agent taxonomy — audit, propose, and validate.

See docs/specs/SPEC_30_AGENT_TAXONOMY.md for the scheme this enforces.

WHY THIS EXISTS
───────────────
47 of 96 curated cards carried a seven-rank Linnaean taxonomy and nothing
read it, so nothing kept it honest. Measured before this tool existed:

    rank      values  singletons  mean group
    kingdom        9           5         5.2
    phylum        19          15         2.5
    class         22          18         2.1
    order         37          31         1.3   <- labels, not a rank
    family        18          15         2.6   <- the only coherent one
    genus         35          30         1.3   <- labels, not a rank

A rank averaging 1.3 members per value groups nothing; it is a second name
for the agent. `phylum` mixed four incompatible suffix conventions
(-vora, -ales, -ria, -ia). `class` did not track any observable property:
`agent_type=research` alone spanned 11 classes, and `Processoria` appeared
under both `research` and `strategist`.

So simply filling the 49 gaps would have added 49 more singletons and left
the taxonomy decorative. The scheme needed reform first, which is what the
spec does: four ranks are now DERIVED from card structure (machine-assigned
and lint-enforceable) and three remain EDITORIAL but drawn from a
controlled vocabulary.

USAGE
─────
    scripts/taxonomy.py audit              report conformance, exit 1 on error
    scripts/taxonomy.py propose            write review file for unclassified
    scripts/taxonomy.py apply --derived    write ONLY derived ranks into cards
    scripts/taxonomy.py apply --from FILE  apply a reviewed proposal file

`apply --derived` is safe to run repeatedly: it only writes ranks this tool
can determine from the card itself, and never invents an editorial name.
Editorial ranks (kingdom, family, genus) always require a human, because a
name is a claim about kinship and a generator would only be guessing.
"""

import argparse
import glob
import json
import os
import re
import sys
from collections import Counter, defaultdict

RANKS = ["kingdom", "phylum", "class", "order", "family", "genus", "species"]
DERIVED = ["phylum", "class", "order", "species"]
EDITORIAL = ["kingdom", "family", "genus"]

# ── Suffix conventions (SPEC_30 §3) ────────────────────────────────
SUFFIX = {
    "kingdom": r"a$",       # Quantitativa, Spatiala
    "phylum": r"a$",        # Composita, Instrumenta, Solitaria
    "class": r"ia$",        # Researchia, Creativia
    "order": r"ales$",      # Evidentiales, Analyticales
    "family": r"idae$",     # Investigatidae — already 100% conformant
    "genus": r"(us|or|is|ix)$",
}

# ── class ← agent_type (derived, SPEC_30 §4.2) ─────────────────────
CLASS_BY_TYPE = {
    "research": "Researchia",
    "creative": "Creativia",
    "meta": "Metaria",
    "observability": "Vigilia",
    "coherence": "Cohaerentia",
    "commerce": "Mercatia",
    "strategist": "Strategia",
    "compound": "Compositia",
    "coordination": "Coordinia",
    "game": "Ludia",
    "companion": "Compandia",
    "analytics": "Analytia",
    "forecast": "Prognostia",
}

# ── order ← output MODALITY, by pattern (derived, SPEC_30 §4.3) ─────
#
# NOT an enumeration of `produces`. That field is free text: 267 distinct
# values across 321 declarations, 234 of them singletons. Enumerating it
# would break on every new card — and it is almost certainly why the
# original `order` had 31 singletons across 37 values. The pathology was
# inherited from the field it was derived from.
#
# So `order` buckets outputs by what KIND of thing they are. Six buckets
# over ~96 agents groups meaningfully, and pattern matching survives new
# vocabulary. Evaluated in list order — earlier patterns win, so the more
# specific modalities are checked before the general ones.
ORDER_PATTERNS = [
    # Forward-looking estimates.
    ("Prognosticales", r"forecast|predict|projection|estimate|probabilit|multiplier|"
                       r"scenario|outlook|base.?rate"),
    # Alarms and judgements about health, risk or quality.
    ("Diagnosticales", r"diagnos|anomal|alert|flag|warning|risk|health|verdict|"
                       r"incident|conservation|score"),
    # Prescriptions — what to do next.
    ("Consiliales", r"recommend|plan|advice|advisor|strateg|guide|suggestion|"
                    r"proposal|ranking|formulation"),
    # Generated artefacts and media.
    ("Imaginales", r"image|avatar|scene|render|art|visual|media|video|audio|voice|"
                   r"placement|choreograph"),
    # Prose for humans. `narrat` deliberately, so it catches both
    # `narrative` and `narration`.
    ("Narrativales", r"narrat|story|post|synopsis|prose|dream|message|response|"
                     r"summary|brief|note"),
    # Plumbing: state, transforms, machine-to-machine payloads.
    ("Operationales", r"state|config|transform|input|action|command|operation|task|"
                      r"workflow|packaged|payload|schema|manifest|rules|ontolog|"
                      r"embedding|routing|listing|match|position|metric"),
    # Epistemic output — the broadest bucket, so it goes last.
    ("Evidentiales", r"evidence|analys|assess|report|profile|finding|research|"
                     r"rating|comparison|audit|scan|intel|catalog|fact"),
]

VOCAB_PATH = "agents/taxonomy_vocab.json"


# `agents/templates/` holds the authoring template and worked examples, not
# real agents. Excluded from the corpus for a concrete reason: the examples
# reuse real agent_ids (`sentiment_analyzer`, `market_research`), so keying
# anything by agent_id across the whole tree silently collides. That was
# diagnosed by the Rust parity test reporting two "rule mismatches" that
# turned out to be last-wins collisions between a curated card and its
# example twin — the two tools happened to walk the tree in different
# orders and so picked different winners.
#
# The production registry already loads from `agents/curated` only
# ($AGENTS_DIR), so nothing shadows a real card at runtime.
EXCLUDE_DIRS = ("agents/templates/",)


def load_cards():
    out = []
    seen = {}
    for p in sorted(glob.glob("agents/**/agent_card.json", recursive=True)):
        if any(x in p for x in EXCLUDE_DIRS):
            continue
        try:
            with open(p) as f:
                card = json.load(f)
        except Exception as e:
            print(f"  WARN unreadable {p}: {e}", file=sys.stderr)
            continue
        aid = card.get("agent_id")
        if aid and aid in seen:
            # Two real cards claiming one identity is a corpus defect, not
            # something to resolve silently by ordering.
            print(f"  WARN duplicate agent_id {aid!r}: {seen[aid]} and {p}", file=sys.stderr)
            continue
        if aid:
            seen[aid] = p
        out.append((p, card))
    return out


def load_vocab():
    if os.path.exists(VOCAB_PATH):
        with open(VOCAB_PATH) as f:
            return json.load(f)
    return {"kingdom": [], "family": [], "genus": []}


# ── Derivation ─────────────────────────────────────────────────────
def derive(card):
    """Ranks determinable from the card itself. Never guesses a name."""
    caps = card.get("capabilities") or {}
    deps = card.get("dependencies") or {}
    out = {}

    # phylum — mode of operation. Three values, so it actually partitions.
    if deps.get("required"):
        out["phylum"] = "Composita"      # orchestrates other agents
    elif caps.get("mcp_servers") or caps.get("mcp_tools") or caps.get("skills"):
        out["phylum"] = "Instrumenta"    # reaches for external instruments
    else:
        out["phylum"] = "Solitaria"      # works from its own prompt alone

    # class — agent_type, one-to-one. Makes the rank verifiable.
    t = (card.get("agent_type") or "").lower()
    if t in CLASS_BY_TYPE:
        out["class"] = CLASS_BY_TYPE[t]

    # order — output modality, from the first `produces` entry that matches
    # a modality pattern. Left unset when nothing matches, so the audit
    # reports it as needing attention rather than silently defaulting.
    blob = " ".join(card.get("produces") or []).lower()
    for name, pat in ORDER_PATTERNS:
        if re.search(pat, blob):
            out["order"] = name
            break

    out["species"] = card.get("agent_id")
    return out


# ── Audit ──────────────────────────────────────────────────────────
def audit(cards, vocab, verbose=True):
    """Returns (errors, warnings).

    `errors` are machine-checkable defects: a rank contradicting its own
    card, a broken suffix convention, a species that isn't the agent_id.
    `warnings` are editorial gaps — a missing or off-vocabulary kingdom,
    family or genus. Those need a human and cannot be auto-resolved, so
    they must not fail a build; see `--gate`.
    """
    errors, warnings = [], []
    classified = [(p, c) for p, c in cards if (c.get("metadata") or {}).get("taxonomy")]

    for path, card in cards:
        aid = card.get("agent_id", path)
        tax = (card.get("metadata") or {}).get("taxonomy") or {}
        if not tax:
            warnings.append((aid, "unclassified", "no taxonomy block"))
            continue

        for rank in RANKS:
            val = tax.get(rank)
            bucket = warnings if rank in EDITORIAL else errors
            if not val:
                # `order` derives from `produces`. A card that declares no
                # outputs at all gives nothing to derive from, so this is an
                # authoring gap in the card, not a taxonomy defect — it must
                # not fail a build that the taxonomy tooling cannot fix.
                if rank == "order" and not (card.get("produces") or []):
                    warnings.append(
                        (aid, rank, "cannot derive: card declares no `produces`")
                    )
                    continue
                bucket.append((aid, rank, "missing"))
                continue
            if rank == "species":
                if val != card.get("agent_id"):
                    errors.append((aid, rank, f"species must equal agent_id, got {val!r}"))
                continue
            if not re.search(SUFFIX[rank], val):
                bucket.append((aid, rank, f"{val!r} breaks the /{SUFFIX[rank]}/ convention"))
            if rank in EDITORIAL and vocab.get(rank) and val not in vocab[rank]:
                warnings.append((aid, rank, f"{val!r} not in the controlled vocabulary"))

        # Derived ranks must agree with the card, or the taxonomy is a lie.
        d = derive(card)
        for rank in ("phylum", "class", "order"):
            if rank in d and tax.get(rank) and tax[rank] != d[rank]:
                errors.append(
                    (aid, rank, f"{tax[rank]!r} contradicts the card (derives to {d[rank]!r})")
                )

    if verbose:
        print(f"cards {len(cards)} · classified {len(classified)} · "
              f"unclassified {len(cards) - len(classified)}")
        print(f"machine-checkable errors {len(errors)} · "
              f"editorial gaps {len(warnings)}\n")

        def show(title, items, limit=6):
            if not items:
                return
            by_rank = defaultdict(list)
            for aid, rank, msg in items:
                by_rank[rank].append((aid, msg))
            print(title)
            for rank in RANKS + ["unclassified"]:
                got = by_rank.get(rank, [])
                for aid, msg in got[:limit]:
                    print(f"  {rank:8s} {aid:28s} {msg}")
                if len(got) > limit:
                    print(f"  {rank:8s} … and {len(got) - limit} more")
            print()

        show("ERRORS (must fix — the card contradicts itself)", errors)
        show("EDITORIAL GAPS (need a human + agents/taxonomy_vocab.json)", warnings, 4)

        # Informativeness — the metric that exposed the original problem.
        if classified:
            print("RANK INFORMATIVENESS (mean group size; ~1.0 means the rank groups nothing)")
            for rank in RANKS[:-1]:
                vals = Counter((c.get("metadata") or {}).get("taxonomy", {}).get(rank)
                               for _, c in classified)
                singles = sum(1 for v in vals.values() if v == 1)
                mean = len(classified) / max(len(vals), 1)
                flag = "  <-- uninformative" if mean < 1.6 else ""
                print(f"  {rank:8s} {len(vals):3d} values · {singles:3d} singleton(s) · "
                      f"mean {mean:.1f}{flag}")
    return errors, warnings


# ── Propose ────────────────────────────────────────────────────────
def propose(cards, out_path):
    """Derive what we can; leave editorial ranks blank for a human."""
    items = []
    for path, card in cards:
        tax = (card.get("metadata") or {}).get("taxonomy") or {}
        d = derive(card)
        need = {r: tax.get(r, "") for r in EDITORIAL}
        if all(need.values()) and all(tax.get(r) for r in DERIVED):
            continue  # already complete
        items.append({
            "agent_id": card.get("agent_id"),
            "path": path,
            "agent_type": card.get("agent_type"),
            "produces": card.get("produces") or [],
            "tags": (card.get("metadata") or {}).get("tags") or [],
            "description": (card.get("description") or "")[:120],
            "derived": d,
            "editorial_TODO": need,
        })
    with open(out_path, "w") as f:
        json.dump({"proposals": items}, f, indent=2)
    print(f"wrote {len(items)} proposal(s) -> {out_path}")
    print("Derived ranks are filled in. Fill `editorial_TODO` (kingdom, family, genus)")
    print("from agents/taxonomy_vocab.json, then: scripts/taxonomy.py apply --from " + out_path)


# ── Apply ──────────────────────────────────────────────────────────
def apply(cards, derived_only, from_file):
    reviewed = {}
    if from_file:
        with open(from_file) as f:
            for it in json.load(f)["proposals"]:
                reviewed[it["agent_id"]] = it

    changed = 0
    for path, card in cards:
        aid = card.get("agent_id")
        md = card.setdefault("metadata", {})
        tax = md.setdefault("taxonomy", {})
        before = dict(tax)

        tax.update(derive(card))          # derived ranks always refreshed
        if not derived_only and aid in reviewed:
            for rank, val in (reviewed[aid].get("editorial_TODO") or {}).items():
                if val:
                    tax[rank] = val

        # Keep the canonical rank order so diffs stay readable.
        md["taxonomy"] = {r: tax[r] for r in RANKS if r in tax}
        if md["taxonomy"] != before:
            if write_taxonomy_block(path, md["taxonomy"]):
                changed += 1
    print(f"updated {changed} card(s)")


def write_taxonomy_block(path, taxonomy):
    """Replace (or insert) just the `taxonomy` block, in place, as text.

    A full `json.load`/`json.dump` round-trip would be far simpler, and it
    is what this did first. Two problems made it unusable at corpus scale:

      * `ensure_ascii=True` (the default) escapes every non-ASCII character,
        turning the em-dashes and arrows that fill these prompts into
        `\\u2014` and `\\u2192`. One card produced a 31-line diff of pure
        escaping noise.
      * `indent=2` expands every inline array, so `"required": ["x"]`
        becomes three lines. Measured across the corpus: a `--derived`
        pass rewrote **1650 lines in 97 files** when the actual taxonomy
        edit accounts for a few hundred. The rest was formatting churn that
        nobody asked for, that buries the real change in review, and that
        collides with anyone else editing cards concurrently.

    So this edits the text surgically. The taxonomy block is a flat
    string->string map with no nesting, which makes it safely matchable
    without a JSON parser. Indentation is taken from the existing block so
    the result matches whatever style the card already uses.

    Returns True if the file was modified.
    """
    with open(path) as f:
        text = f.read()

    # Flat map: no nested braces, so `[^{}]*` is sufficient and safe.
    m = re.search(r'([ \t]*)"taxonomy"\s*:\s*\{[^{}]*\}', text)
    if m:
        indent = m.group(1)
    else:
        # No taxonomy yet — insert as the first key inside `metadata`.
        m = re.search(r'([ \t]*)"metadata"\s*:\s*\{', text)
        if not m:
            print(f"  SKIP {path}: no metadata block to insert into")
            return False
        indent = m.group(1) + "  "

    inner = ",\n".join(
        f'{indent}  "{r}": {json.dumps(v, ensure_ascii=False)}'
        for r, v in taxonomy.items()
    )
    block = f'{indent}"taxonomy": {{\n{inner}\n{indent}}}'

    if re.search(r'[ \t]*"taxonomy"\s*:\s*\{[^{}]*\}', text):
        new = re.sub(r'[ \t]*"taxonomy"\s*:\s*\{[^{}]*\}', lambda _: block, text, count=1)
    else:
        anchor = m.group(0)
        new = text.replace(anchor, f"{anchor}\n{block},", 1)

    if new == text:
        return False

    # Never leave a card unparseable: validate before committing the write.
    try:
        json.loads(new)
    except json.JSONDecodeError as e:
        print(f"  ABORT {path}: surgical edit produced invalid JSON ({e}); left untouched")
        return False

    with open(path, "w") as f:
        f.write(new)
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["audit", "propose", "apply"])
    ap.add_argument("--derived", action="store_true", help="apply only derived ranks")
    ap.add_argument("--from", dest="from_file", help="reviewed proposal file")
    ap.add_argument("--out", default="agents/taxonomy_proposals.json")
    ap.add_argument("--emit-expected", metavar="PATH",
                    help="audit: also write derived ranks for every card to PATH, "
                         "the fixture tests/taxonomy_parity.rs asserts the Rust "
                         "implementation against. Regenerate whenever a rule changes.")
    ap.add_argument("--gate", choices=["derived", "all"], default="derived",
                    help="audit: which findings fail the build. `derived` (default) "
                         "fails only on machine-checkable defects, so CI can enforce "
                         "the automated half while editorial naming is still in "
                         "progress. Switch to `all` once the vocabulary is filled in.")
    a = ap.parse_args()

    cards = load_cards()
    if a.command == "audit":
        if a.emit_expected:
            # Fixture for the Rust parity test. Two implementations of one
            # rule will drift; this is what notices.
            payload = {
                card.get("agent_id"): derive(card)
                for _, card in cards
                if card.get("agent_id")
            }
            with open(a.emit_expected, "w") as f:
                json.dump(payload, f, indent=2, sort_keys=True, ensure_ascii=False)
                f.write("\n")
            print(f"wrote derived-rank fixture for {len(payload)} card(s) -> {a.emit_expected}")
        errors, warnings = audit(cards, load_vocab())
        bad = bool(errors) or (a.gate == "all" and bool(warnings))
        if errors:
            print(f"FAIL: {len(errors)} machine-checkable error(s). "
                  f"Run `scripts/taxonomy.py apply --derived` to fix derived ranks.")
        elif warnings and a.gate == "derived":
            print(f"PASS (gate=derived): 0 errors. {len(warnings)} editorial gap(s) "
                  f"outstanding — see docs/specs/SPEC_30_AGENT_TAXONOMY.md §4.")
        sys.exit(1 if bad else 0)
    if a.command == "propose":
        propose(cards, a.out)
    if a.command == "apply":
        if not a.derived and not a.from_file:
            sys.exit("apply needs --derived or --from FILE")
        apply(cards, a.derived, a.from_file)


if __name__ == "__main__":
    main()
