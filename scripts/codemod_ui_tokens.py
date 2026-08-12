#!/usr/bin/env python3
"""One-shot codemod: hardcoded `px(..)` -> scalable design tokens.

Rewrites `Styled` setter calls in the fermi-console binary so every length
in the UI resolves through `ui::s` / `ui::TEXT_*` (and therefore through the
global UI scale) instead of being frozen at authoring size.

    .py(px(6.0))         ->  .py(ui::s(6.0))
    .text_size(px(11.0)) ->  .text_size(ui::TEXT_BASE)

Deliberately conservative. It only touches:

  * a fixed whitelist of `Styled` setters (METHODS), so `px()` used for
    window bounds, canvas coordinates, `Point`/`Size` construction or
    stroke widths is left alone;
  * calls whose argument is a bare numeric literal. Non-literal arguments
    (`px(chart_w)`, `px(x - 4.0)`) are computed from values that are
    themselves scaled at their definition, so rewriting them here would
    double-scale.

`src/viz/` is skipped entirely: it sizes wrappers to match hand-computed
canvas geometry and must stay in real pixels. See `ui.rs`.

Idempotent, and a no-op on anything it does not recognise. Run from the
repo root:

    python3 scripts/codemod_ui_tokens.py --check   # report only
    python3 scripts/codemod_ui_tokens.py           # apply
"""

import argparse
import collections
import glob
import re
import sys

ROOT = "crates/fermi-console/src"

# Skipped: paints vector geometry at coordinates derived from a spec, so
# its wrapper divs must be sized in real pixels that match that spec.
SKIP_DIRS = ("/viz/",)

# `Styled` setters that take a length. Anything not listed keeps its `px()`.
METHODS = {
    # box model
    "w", "h", "size", "min_w", "max_w", "min_h", "max_h", "flex_basis",
    # spacing
    "gap", "gap_x", "gap_y",
    "p", "px", "py", "pt", "pb", "pl", "pr",
    "m", "mx", "my", "mt", "mb", "ml", "mr",
    # positioning
    "top", "left", "right", "bottom", "inset",
    # decoration
    "rounded", "rounded_t", "rounded_b", "rounded_l", "rounded_r",
    "rounded_tl", "rounded_tr", "rounded_bl", "rounded_br",
    "border", "border_t", "border_b", "border_l", "border_r",
    "border_x", "border_y",
    # type
    "text_size",
}

# Legacy font size -> type-scale token. Lossy on purpose: 9/9.5 and
# 10/10.5 were half-pixel distinctions nobody could see, so they fold
# together. See `uiscale::TYPE_SCALE_PX`.
TEXT_TOKENS = {
    8.0: "TEXT_MICRO",
    9.0: "TEXT_XS",
    9.5: "TEXT_XS",
    10.0: "TEXT_SM",
    10.5: "TEXT_SM",
    11.0: "TEXT_BASE",
    12.0: "TEXT_MD",
    13.0: "TEXT_LG",
    14.0: "TEXT_XL",
    16.0: "TEXT_2XL",
    18.0: "TEXT_3XL",
    20.0: "TEXT_4XL",
    22.0: "TEXT_5XL",
    24.0: "TEXT_6XL",
    28.0: "TEXT_7XL",
    32.0: "TEXT_8XL",
    36.0: "TEXT_9XL",
}

CALL = re.compile(r"\.([a-z_0-9]+)\(\s*px\((-?[0-9]+(?:\.[0-9]+)?)\)\s*\)")


def rewrite(src, stats, path):
    def repl(m):
        method, raw = m.group(1), m.group(2)
        if method not in METHODS:
            stats["skipped-method"][method] += 1
            return m.group(0)
        value = float(raw)
        if method == "text_size":
            token = TEXT_TOKENS.get(value)
            if token is None:
                # An unmapped size means the type scale grew a tier
                # without this table being updated. Refuse rather than
                # silently freezing it at authoring size.
                stats["unmapped-text"][raw] += 1
                return m.group(0)
            stats["text"][token] += 1
            return f".text_size(ui::{token})"
        stats["length"][method] += 1
        return f".{method}(ui::s({raw}))"

    return CALL.sub(repl, src)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="report without writing")
    args = ap.parse_args()

    stats = collections.defaultdict(collections.Counter)
    touched = []

    for path in sorted(glob.glob(f"{ROOT}/**/*.rs", recursive=True)):
        if any(d in path for d in SKIP_DIRS):
            continue
        original = open(path).read()
        updated = rewrite(original, stats, path)
        if updated != original:
            touched.append((path, sum(1 for _ in CALL.finditer(original))))
            if not args.check:
                open(path, "w").write(updated)

    for path, n in touched:
        print(f"{'would rewrite' if args.check else 'rewrote'} {path} ({n} call sites)")

    print(f"\nlengths -> ui::s()   {sum(stats['length'].values())}")
    print(f"type    -> ui::TEXT_* {sum(stats['text'].values())}")
    for token, n in stats["text"].most_common():
        print(f"    {n:5d}  {token}")

    if stats["unmapped-text"]:
        print("\nUNMAPPED font sizes (left as-is, add them to TEXT_TOKENS):")
        for raw, n in stats["unmapped-text"].most_common():
            print(f"    {n:5d}  px({raw})")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
