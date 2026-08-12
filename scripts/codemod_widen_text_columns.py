#!/usr/bin/env python3
"""Follow-up to `codemod_ui_tokens.py`: widen boxes that were sized to fit
their own text.

Raising the type scale's floor grew the smallest tiers by 18-25% relative
to their containers. Containers scale too, so nothing *global* breaks --
but a `.w(52.0)` that was measured to fit an 8-character timestamp at 9px
does not fit it at 11px, and the label truncates.

This bumps each such box by exactly the ratio its own type tier grew, so
the fit is preserved rather than re-guessed. Boxes are only touched when
they set both a fixed width and a small type tier in the same element
chain; `w(0.0)` / `min_w(0.0)` (the flexbox "allow me to shrink" idiom)
are left alone.

    python3 scripts/codemod_widen_text_columns.py --check
    python3 scripts/codemod_widen_text_columns.py
"""

import argparse
import glob
import re
import sys

# Each tier's new size over its old one -- see TEXT_TOKENS in
# codemod_ui_tokens.py for the old -> new mapping.
GROWTH = {
    "TEXT_MICRO": 10 / 8,
    "TEXT_XS": 11 / 9,
    "TEXT_SM": 12 / 10,
    "TEXT_BASE": 13 / 11,
}

WIDTH = re.compile(r"\.(w|min_w)\(ui::s\((\d+(?:\.\d+)?)\)\)")
TIER = re.compile(r"\.text_size\(ui::(TEXT_MICRO|TEXT_XS|TEXT_SM|TEXT_BASE)\)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    total = 0
    for path in sorted(glob.glob("crates/fermi-console/src/**/*.rs", recursive=True)):
        src = open(path).read()
        out, cursor, changed = [], 0, 0

        for m in WIDTH.finditer(src):
            value = float(m.group(2))
            # Not a measured width: the flexbox idiom for "may shrink".
            if value == 0.0:
                continue
            # Only boxes narrow enough for a text overflow to matter.
            if value > 110:
                continue
            # Look ahead within this element's own chain, stopping before
            # any nested child so we don't attribute a child's type tier
            # to the parent's width.
            chain = src[m.start(): m.start() + 420].split(".child(")[0]
            tier = TIER.search(chain)
            if not tier:
                continue

            widened = round(value * GROWTH[tier.group(1)])
            if widened == value:
                continue

            out.append(src[cursor:m.start()])
            out.append(f".{m.group(1)}(ui::s({widened}.0))")
            cursor = m.end()
            changed += 1
            line = src[:m.start()].count("\n") + 1
            print(
                f"{path.split('/')[-1]}:{line}  "
                f".{m.group(1)}({value}) -> {widened}  ({tier.group(1)})"
            )

        if changed:
            out.append(src[cursor:])
            total += changed
            if not args.check:
                open(path, "w").write("".join(out))

    print(f"\n{'would widen' if args.check else 'widened'} {total} boxes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
