#!/usr/bin/env python3
"""Integrity ratchet (Directive §5 Phase 0).

    scripts/ratchet.py [REF]                 print counts for REF (default: working tree)
    scripts/ratchet.py --check [BASELINE]    exit 1 unless counts == baseline exactly

`--check` fails when a count **rises** (regression) and also when it **falls**
without the baseline being lowered in the same change — so an improvement is
locked in and cannot be silently spent by a later PR.

Scope: production Rust in the workspace members. Excluded: `tests/` dirs,
files named `tests.rs`, `#[cfg(test)]`-gated items (brace-matched), and
`//` / `/* */` comments. String literals are kept: SQL lives in them.
"""
import re, subprocess, sys

ROOTS = ["src", "crates", "agent-bestiary", "fermi-auth", "fermi-lsp"]

METRICS = {
    # SQL that mutates canonical state in place. Case-insensitive, any
    # whitespace, optional quoting. `ON CONFLICT ... DO UPDATE` is counted as
    # an UPDATE: it is one.
    "update_ident": re.compile(r'(?i)\bUPDATE\s+"?[a-z_][a-z0-9_."]*\s+SET\b|\bDO\s+UPDATE\b'),
    "delete_from": re.compile(r'(?i)\bDELETE\s+FROM\b'),
    # Discarded results: `let _ =` (any spacing, across newlines) and bare `_ = expr;`.
    "let_underscore": re.compile(r'(?m)(?:\blet\s+_\s*=|^\s*_\s*=\s)'),
    # Of those, the ones discarding a sqlx call.
    "let_underscore_sqlx": re.compile(r'(?:\blet\s+_\s*=|^\s*_\s*=\s)\s*sqlx::', re.M),
    # Panicking row access: `row.get(..)` panics on decode error.
    "row_get": re.compile(r'\b(?:row|r|rec|record)\s*\.\s*get\s*(?:::<[^>]*>)?\s*\('),
}
TRY_GET = re.compile(r"\btry_get\s*(::<[^>]*>)?\s*\(")


def files(ref):
    args = (["git", "ls-tree", "-r", "--name-only", ref, "--", *ROOTS] if ref
            else ["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *ROOTS])
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout
    for f in out.split():
        if (f.endswith(".rs") and "/tests/" not in f and not f.startswith("tests/")
                and not f.endswith("/tests.rs") and "/target/" not in f):
            yield f


def read(ref, f):
    if ref:
        return subprocess.run(["git", "show", f"{ref}:{f}"], capture_output=True, text=True).stdout
    return open(f, encoding="utf-8", errors="replace").read()


def strip_comments(t):
    """Remove // and /* */ comments, keeping string and char literals intact."""
    out, i, n = [], 0, len(t)
    while i < n:
        c = t[i]
        if t.startswith("//", i):
            j = t.find("\n", i)
            i = n if j < 0 else j
        elif t.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if t.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif t.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif c == "r" and re.match(r'r#*"', t[i:i + 8]) and (i == 0 or not (t[i-1].isalnum() or t[i-1] == "_")):
            m = re.match(r'r(#*)"', t[i:])
            close = '"' + m.group(1)
            j = t.find(close, i + len(m.group(0)))
            j = n if j < 0 else j + len(close)
            out.append(t[i:j]); i = j
        elif c == '"':
            j = i + 1
            while j < n and t[j] != '"':
                j += 2 if t[j] == "\\" else 1
            out.append(t[i:j + 1]); i = j + 1
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", t[i:i + 4]):
            m = re.match(r"'(\\.|[^\\'])'", t[i:])
            out.append(m.group(0)); i += len(m.group(0))
        else:
            out.append(c); i += 1
    return "".join(out)


def strip_cfg_test(text):
    out, i = [], 0
    while True:
        j = text.find("#[cfg(test)]", i)
        if j < 0:
            out.append(text[i:])
            return "".join(out)
        out.append(text[i:j])
        k, semi = text.find("{", j), text.find(";", j)
        if k < 0 or (0 <= semi < k):
            i = (semi + 1) if semi >= 0 else len(text)
            continue
        depth, p = 0, k
        while p < len(text):
            if text[p] == "{":
                depth += 1
            elif text[p] == "}":
                depth -= 1
                if depth == 0:
                    break
            p += 1
        i = p + 1


def decode_unwraps(text):
    n = 0
    for m in TRY_GET.finditer(text):
        end = text.find(";", m.end())
        stmt = text[m.end(): end if end >= 0 else len(text)]
        if re.match(r'[^)]*\)\s*(\?\s*)?(\.\s*parse\s*\(\s*\)\s*)?\.\s*(unwrap|expect)\s*\(', stmt, re.S):
            n += 1
    return n


def count(ref):
    c = {k: 0 for k in METRICS}
    c["db_decode_unwrap"] = 0
    for f in files(ref):
        t = strip_comments(strip_cfg_test(read(ref, f)))
        for k, rx in METRICS.items():
            c[k] += len(rx.findall(t))
        c["db_decode_unwrap"] += decode_unwraps(t)
    return c


def load(path):
    base = {}
    for line in open(path):
        line = line.strip()
        if line and not line.startswith("#"):
            k, v = line.split("=")
            base[k] = int(v)
    return base


def main():
    if sys.argv[1:2] == ["--check"]:
        path = sys.argv[2] if len(sys.argv) > 2 else "scripts/ratchet_baseline.txt"
        base, cur = load(path), count(None)
        rc = 0
        for k in sorted(set(base) | set(cur)):
            b, v = base.get(k), cur.get(k)
            if b is None:
                print(f"RATCHET FAIL {k}: not in baseline (current {v})"); rc = 1
            elif v > b:
                print(f"RATCHET FAIL {k}: {v} > baseline {b} (regression)"); rc = 1
            elif v < b:
                print(f"RATCHET FAIL {k}: {v} < baseline {b} (improvement: lower the baseline to {v} in this change)"); rc = 1
            else:
                print(f"ok {k}={v}")
        sys.exit(rc)
    for k, v in count(sys.argv[1] if len(sys.argv) > 1 else None).items():
        print(f"{k}={v}")


if __name__ == "__main__":
    main()
