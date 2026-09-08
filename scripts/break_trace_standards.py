#!/usr/bin/env python3
"""Break the artifact trace ten ways, and watch the guard go red for each.

The properties in `scripts/check_trace_standards.js` are about what a reader is
SHOWN. `field_state` proves in Rust that a shortfall and a fault are different
findings with different tones; that guarantee is worth nothing if the page then
paints them the same, and no Rust test can see the difference.

So each mutation reinstates one of the four collapses the user actually
reported, and the harness must fail.

    python3 scripts/break_trace_standards.py
"""

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAGE = ROOT / "templates" / "trace.html"
HARNESS = ROOT / "scripts" / "check_trace_standards.js"

# (name, what it reinstates, find, replace)
BREAKS = [
    (
        "a shortfall painted with the fault colour",
        "the reported defect itself: Q3 read `e.empty === 0 ? \"ok\" : \"bad\"`, "
        "separating owed / no_data / excused carefully in the prose one line "
        "above and then throwing the distinction away in the colour.",
        '        tone: e.total === 0 ? "na"\n            : e.empty === 0 ? "ok"\n'
        '            : toneOf(worstOf(f => owedPaths.has(f.name))),',
        '        tone: e.total === 0 ? "na" : e.empty === 0 ? "ok" : "bad",',
    ),
    (
        "the field chips all wear one tone",
        "eleven omissions and one fabrication in one red, which is how the "
        "smallest number on the page came to be drowned by the largest.",
        'f-${esc(f.finding_tone || "neutral")}"',
        'f-red"',
    ),
    (
        "the two standards collapse back into one strip",
        "legality and ambition interleaved under one colour ramp, so a "
        "designer-grade disappointment reads as a caller-grade defect.",
        '      <div class="std std-a">',
        '      <div class="notstd std-a">',
    ),
    (
        "row A loses its name",
        "a reader who cannot tell which standard they are reading. The split "
        "only works if both halves are named.",
        '<span class="std-n">Fit to send</span>',
        '<span class="std-n">Questions</span>',
    ),
    (
        "the distribution carries a verdict again",
        "`weakSourced > 0 ? \"bad\"` — a verdict smuggled into a description. "
        "Strength 0 is CORRECT for a field the contract says nothing can "
        "source, and red for that alone painted three compliant fields and a "
        "prose summary as a fault.",
        '      <div class="dist">',
        '      <div class="dist t-bad">',
    ),
    (
        "`records only` returns",
        "four declared enforcement modes collapsed into two, so `amend` — the "
        "one mode that actually removes the bad part — reads as impotent and "
        "`report` is indistinguishable from `metric`.",
        'amend:   { chip: "removes the bad part", tone: "",     full:',
        'amend:   { chip: "records only", tone: "",     full:',
    ),
    (
        "`why_not_control` is discarded again",
        "the platform receiving its own mandatory explanation and printing a "
        "word that sounds like a shrug. "
        "`a_gate_that_cannot_refuse_explains_itself` panics without the "
        "sentence; the page threw it away.",
        "              + (a.grounding.why_not_control\n"
        "                  ? ` It cannot refuse the run, and the platform's own reason is:\n"
        "                      <i>${esc(a.grounding.why_not_control)}</i>`\n"
        '                  : "")',
        '              + ""',
    ),
    (
        "the run-state legend goes behind a fold",
        "a token whose gloss is folded has no gloss — house rule 2 honoured in "
        "the letter and broken in effect.",
        '    return `<div class="obs-leg">',
        '    return `<details><div class="obs-leg">',
    ),
    (
        "the legend orders faults last",
        "a fault a reader meets after four states that are nobody's fault, "
        "which is the ordering that buried it.",
        "        (RANK_OF[y] ?? 0) - (RANK_OF[x] ?? 0)).map(t => {",
        "        (RANK_OF[x] ?? 0) - (RANK_OF[y] ?? 0)).map(t => {",
    ),
    (
        "a state word is spelled on the client again",
        "the nine words the page used to derive - `platform-computed`, `prose`, "
        "`a judgement`, `no tool exists`, `asked, empty`, `never asked`, `tool "
        "unused` - every one of them a state the platform already owned, under "
        "a different name.",
        '      html: `<span class="obs f-${esc(f.finding_tone || "neutral")}"',
        '      html: `<span>${"a judgement"}</span><span class="obs f-${esc(f.finding_tone || "neutral")}"',
    ),
    (
        "the contract clock is dropped from the row",
        "the trace spelling its own words for what the contract says, which is "
        "how `unsourced` came to mean a declared kind on one panel and a "
        "violation on the next.",
        "        + (f.declared\n",
        "        + (false\n",
    ),
    (
        "`pending` is marked as a defect",
        "an agent's ambition reported as its failure. `pending` means no tool "
        "exists for this yet, the field MUST be null, and pruning it to reach "
        "green would delete the ambition the contract exists to record.",
        'class="dec${dec.is_defect ? " d-defect" : ""}"',
        'class="dec d-defect"',
    ),
    (
        "the byte count is dropped from the row",
        "`the tool answered with nothing` and `the tool answered at length and "
        "the agent dropped the result` collapsing into one state. The number is "
        "the whole difference between a capability gap and a discarded result.",
        "      body: f.evidence_bytes !== null && f.evidence_bytes !== undefined",
        "      body: false && f.evidence_bytes !== undefined",
    ),
    (
        "the caption drops the pruning sentence",
        "a reader whose remedy for an amber row is to delete the contract "
        "entry — which deletes the ambition the contract exists to record.",
        "          ? ` Pruning the contract to reach green would delete the ambition it\n"
        "              exists to record.`",
        '          ? ""',
    ),
    (
        "the badge loses its label",
        "a verdict that reads as being about the artifact entire. `reading()` "
        "returns Fault on violations and nothing else, so the badge reports one "
        "row of one standard - it said 1, question 3 said 1, they were different "
        "1s, and eleven omissions sat beside them in the same colour family.",
        '<span class="badge-of">fit to send</span>',
        "",
    ),
    (
        "the badge stops saying what it omits",
        "the silence that was the whole defect: the one number that should alarm "
        "a reader was the smallest on the page and nothing said which question "
        "it answered.",
        "says nothing about whether the agent did what",
        "covers whether the agent did what",
    ),
]


def run():
    r = subprocess.run(
        ["node", str(HARNESS)], capture_output=True, text=True, cwd=str(ROOT)
    )
    return r.returncode, r.stdout + r.stderr


def first_reason(out):
    for line in out.splitlines():
        s = line.strip()
        if s.startswith("\u2716"):
            return s[1:].strip()
    return "(no reason line \u2014 did the page fail to parse?)"


def main():
    backup = PAGE.with_suffix(".html.trace-break-backup")
    shutil.copy2(PAGE, backup)
    original = PAGE.read_text()

    code, out = run()
    if code != 0:
        print("the guard is red BEFORE any mutation, so nothing below means anything:")
        print(out)
        shutil.copy2(backup, PAGE)
        backup.unlink()
        return 2
    print(f"baseline green.  {len(BREAKS)} mutations to try.\n")

    missed = []
    try:
        for i, (name, why, find, repl) in enumerate(BREAKS, 1):
            if find not in original:
                print(f"  {i:2}. SKIPPED      {name}\n      the pattern is not in the "
                      f"page, so the mutation is a no-op \u2014 which is "
                      f"indistinguishable from a guard that did not fire.\n")
                missed.append(name + " (pattern absent)")
                continue
            PAGE.write_text(original.replace(find, repl, 1))
            code, out = run()
            if code == 0:
                print(f"  {i:2}. STILL GREEN  {name}\n      the guard cannot see this. {why}\n")
                missed.append(name)
            else:
                print(f"  {i:2}. red          {name}\n      {first_reason(out)[:150]}\n")
            PAGE.write_text(original)
    finally:
        PAGE.write_text(original)

    code, out = run()
    if code != 0:
        print("RESTORE FAILED \u2014 the page is not back to green:")
        print(out)
        return 3
    backup.unlink()

    if missed:
        print(f"\n{len(missed)} mutation(s) the guard could not catch:")
        for m in missed:
            print("  \u2716 " + m)
        print("\nFix the check, not the break \u2014 unless the break was the no-op.")
        return 1
    print(f"\nall {len(BREAKS)} breaks were caught, and the page is green again.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
