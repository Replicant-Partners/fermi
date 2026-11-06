#!/usr/bin/env python3
"""Integrity ratchet counts (Directive §5 Phase 0).

Usage: scripts/ratchet.py [REF]     # REF = git ref; omit for the working tree

Scope: production Rust in the workspace members (src, crates, agent-bestiary,
fermi-auth, fermi-lsp). Excluded: tests/ dirs, files named tests.rs, and
`#[cfg(test)]` modules (everything from the first `#[cfg(test)]` line of a
file onward — the repo convention places test modules last).
"""
import re, subprocess, sys

ROOTS = ["src", "crates", "agent-bestiary", "fermi-auth", "fermi-lsp"]

def files(ref):
    if ref:
        out = subprocess.run(["git", "ls-tree", "-r", "--name-only", ref, "--", *ROOTS],
                             capture_output=True, text=True, check=True).stdout
    else:
        out = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *ROOTS],
                             capture_output=True, text=True, check=True).stdout
    for f in out.split():
        if f.endswith(".rs") and "/tests/" not in f and not f.startswith("tests/") \
                and not f.endswith("/tests.rs") and "/target/" not in f:
            yield f

def read(ref, f):
    if ref:
        return subprocess.run(["git", "show", f"{ref}:{f}"], capture_output=True, text=True).stdout
    return open(f, encoding="utf-8", errors="replace").read()

def prod_part(text):
    """Remove each `#[cfg(test)]`-gated item (brace-matched), keep the rest.

    Brace matching ignores braces inside string literals only approximately
    (raw SQL strings rarely contain unbalanced braces); good enough for a
    ratchet, which must be stable rather than exact.
    """
    out, i = [], 0
    while True:
        j = text.find("#[cfg(test)]", i)
        if j < 0:
            out.append(text[i:])
            return "".join(out)
        out.append(text[i:j])
        k = text.find("{", j)
        semi = text.find(";", j)
        if k < 0 or (0 <= semi < k):      # `#[cfg(test)] use ...;` / `mod x;`
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

UPDATE = re.compile(r"UPDATE [a-z_]")
LET_SQLX = re.compile(r"let _ = sqlx::query")
LET_ANY = re.compile(r"^\s*let _ = ", re.M)
TRY_GET = re.compile(r"\btry_get(::<[^>]*>)?\(")

def decode_unwraps(text):
    n = 0
    for m in TRY_GET.finditer(text):
        stmt = text[m.end(): text.find(";", m.end())]
        # a decode whose Result is unwrapped/expected in the same statement
        if re.match(r'[^)]*\)\s*(\?\s*)?(\.\s*parse\(\)\s*)?\.\s*(unwrap|expect)\s*\(', stmt, re.S):
            n += 1
    return n

def check(counts, baseline_path):
    base = {}
    for line in open(baseline_path):
        line = line.strip()
        if line and not line.startswith("#"):
            k, v = line.split("=")
            base[k] = int(v)
    worse = [(k, base[k], counts[k]) for k in base if counts.get(k, 0) > base[k]]
    for k, b, c in worse:
        print(f"RATCHET FAIL {k}: {c} > baseline {b}")
    better = [(k, base[k], counts[k]) for k in base if counts.get(k, 0) < base[k]]
    for k, b, c in better:
        print(f"ratchet improved {k}: {c} < baseline {b} (lower the baseline in this PR)")
    return 1 if worse else 0

def main():
    if sys.argv[1:2] == ["--check"]:
        c = count(None)
        for k, v in c.items():
            print(f"{k}={v}")
        sys.exit(check(c, sys.argv[2] if len(sys.argv) > 2 else "scripts/ratchet_baseline.txt"))
    ref = sys.argv[1] if len(sys.argv) > 1 else None
    for k, v in count(ref).items():
        print(f"{k}={v}")

def count(ref):
    c = dict(update_ident=0, let_underscore_sqlx=0, let_underscore_src=0, db_decode_unwrap=0)
    for f in files(ref):
        t = prod_part(read(ref, f))
        c["update_ident"] += len(UPDATE.findall(t))
        c["let_underscore_sqlx"] += len(LET_SQLX.findall(t))
        if f.startswith("src/"):
            c["let_underscore_src"] += len(LET_ANY.findall(t))
        c["db_decode_unwrap"] += decode_unwraps(t)
    return c

if __name__ == "__main__":
    main()
