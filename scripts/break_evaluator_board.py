#!/usr/bin/env python3
"""Break the evaluator board eight ways, and watch the guard go red for each.

A check never seen to fail has not been shown to work. Three checks in this
codebase's history turned out to be incapable of catching the case they were
written for, which is why this is a rule and not a habit.

Every mutation asserts its pattern is present before replacing it: a
`str.replace` that silently matched nothing is indistinguishable from a guard
that did not fire, and only one of those is a problem.

    python3 scripts/break_evaluator_board.py
"""

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAGE = ROOT / "templates" / "loops.html"
HARNESS = ROOT / "scripts" / "check_evaluator_board.js"

# (name, what it simulates, find, replace)
BREAKS = [
    (
        "unknown token falls through to healthy",
        "the exact upstream defect: a new token in a closed set that grew, "
        "landing on a benign default. This is how `nothing has been watched` "
        "came to display as `the system is idle`.",
        'const evRead = t => EV_TOKEN[t] || EV_UNRECOGNISED;',
        'const evRead = t => EV_TOKEN[t] || EV_TOKEN.healthy;',
    ),
    (
        "inconclusive coloured as a pass",
        "three of six evaluators are usually inconclusive, so this reports a "
        "healthy platform on every fresh boot.",
        '    inconclusive: ["Nothing could be concluded:", "unknown",',
        '    inconclusive: ["Nothing could be concluded:", "idle",',
    ),
    (
        "notice folded into the fault colour",
        "a control that never fires and one that is not wired produce identical "
        "observations, so asserting a fault asserts that violations must exist.",
        '    notice:   ["Reported:", "unknown",',
        '    notice:   ["Reported:", "fault",',
    ),
    (
        "the caveat is not rendered",
        "every check is narrower than the claim it serves. A tick rendered "
        "without `does_not_show` is the kind of lie that is very hard to notice.",
        '          ${e.caveat ? `<div class="ev-cav">',
        '          ${false ? `<div class="ev-cav">',
    ),
    (
        "a passing verdict with a caveat is not flagged",
        "`loop_stalled_in_code` says the remaining loops are idle rather than "
        "broken about four loops classified `unknown`. A caveat under a green "
        "row that nothing points at is a caveat nobody reads.",
        '      const narrow = !!e.caveat && r[1] === "idle";',
        '      const narrow = false;',
    ),
    (
        "the tally collapses to one number",
        '"1 of 6 passing" invites the reader to conclude five are broken and '
        '"0 findings" invites the opposite. Both are wrong.',
        "      + `<div class=\"contract\">${esc(EVALS.contract || \"\")}</div>`\n      + evLegend()",
        "      + `<div class=\"contract\">${esc(EVALS.contract || \"\")}</div>`\n      + evLegend0()",
    ),
    (
        "the empty door list is hidden",
        "a reader shown no control has to be told why, not left to conclude a "
        "finding is unactionable.",
        '      : `<div class="nodoors">\n          <b style="color:var(--fg3)">Nothing to do here, and that is the point.</b>',
        '      : `<div class="hidden-nodoors">\n          <b style="color:var(--fg3)">Nothing to do here, and that is the point.</b>',
    ),
    (
        "a subject that is a sentence is linked anyway",
        "a link built by guessing sends a reader to a page that is not about "
        "the thing the finding named.",
        '    return `<span class="ev-s">${esc(s)}</span>`;',
        '    return `<a class="ev-s" href="/gate/${encodeURIComponent(lead)}">${esc(s)}</a>`;',
    ),
]

# The sixth break needs the tally gone, not the legend renamed. Rewritten as a
# pair so the mutation is the thing being simulated rather than a syntax error.
BREAKS[5] = (
    BREAKS[5][0],
    BREAKS[5][1],
    "    return tally(EVALS.tally, EVAL_BUCKETS)",
    "    return `<div class=\"tally\"><div class=\"tb\"><div class=\"tb-v\">1</div>"
    "<div class=\"tb-l\">of 6 passing</div></div></div>`",
)


def run_harness():
    r = subprocess.run(
        ["node", str(HARNESS)], capture_output=True, text=True, cwd=str(ROOT)
    )
    return r.returncode, (r.stdout + r.stderr)


def main():
    backup = PAGE.with_suffix(".html.break-backup")
    shutil.copy2(PAGE, backup)
    original = PAGE.read_text()

    code, out = run_harness()
    if code != 0:
        print("the guard is red BEFORE any mutation, so nothing below means anything:")
        print(out)
        shutil.copy2(backup, PAGE)
        backup.unlink()
        return 2

    print(f"baseline green.  {len(BREAKS)} mutations to try.\n")
    green_when_broken = []
    try:
        for i, (name, why, find, repl) in enumerate(BREAKS, 1):
            if find not in original:
                print(f"  {i}. {name}\n     SKIPPED: the pattern is not in the page. "
                      f"The mutation would have been a no-op, which is "
                      f"indistinguishable from a guard that did not fire.\n")
                green_when_broken.append(name + " (pattern absent)")
                continue
            PAGE.write_text(original.replace(find, repl, 1))
            code, out = run_harness()
            if code == 0:
                print(f"  {i}. {name}\n     STILL GREEN \u2014 the guard cannot see this. {why}\n")
                green_when_broken.append(name)
            else:
                first = next(
                    (l.strip() for l in out.splitlines() if l.strip().startswith("\u2716")),
                    "(no reason line)",
                )
                print(f"  {i}. {name}\n     red.  {first[:150]}\n")
            PAGE.write_text(original)
    finally:
        PAGE.write_text(original)

    code, out = run_harness()
    if code != 0:
        print("RESTORE FAILED \u2014 the page is not back to green:")
        print(out)
        return 3
    backup.unlink()

    if green_when_broken:
        print(f"\n{len(green_when_broken)} mutation(s) the guard could not catch:")
        for n in green_when_broken:
            print("  \u2716 " + n)
        print("\nFix the check, not the break.")
        return 1
    print(f"\nall {len(BREAKS)} breaks were caught, and the page is green again.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
