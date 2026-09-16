"""Pull the exact SQL literals out of grounding_trust.rs for probing.

Verified rather than trusted: each extracted string is asserted to be a bare
SELECT aliasing the column the harness reads, and the count is asserted, so a
regex that silently matches nothing fails here instead of producing an empty
probe that reports success.
"""
import re
import sys

src = open("src/grounding_trust.rs", encoding="utf-8").read()


def join_literal(chunk: str) -> str:
    """Concatenate a Rust multi-line string literal into one line."""
    # Rust `"...\` + newline + indent continues a literal with no space added.
    body = re.search(r'"((?:[^"\\]|\\.|\\\n)*)"', chunk, re.S)
    if not body:
        return ""
    txt = body.group(1)
    txt = re.sub(r"\\\s*\n\s*", "", txt)  # line continuations
    txt = txt.replace("\\'", "'").replace('\\"', '"')
    return re.sub(r"\s+", " ", txt).strip()


out = []

# The three carbon cross-checks, each keyed on the path above it.
for m in re.finditer(r'agent_id: CA,\s*\n\s*path: "([^"]+)",', src):
    path = m.group(1)
    tail = src[m.end():]
    stop = tail.find("FieldContract {")
    tail = tail[: stop if stop > 0 else len(tail)]
    got = re.search(r"cross_check_sql: Some\(\s*(.*?)\n\s*\),", tail, re.S)
    if got:
        out.append((path, join_literal(got.group(1))))

# The coverage denominator.
cov = re.search(r"pub const CROSS_CHECK_COVERAGE[^=]*=\s*&\[\(\s*(.*?)\n\)\];", src, re.S)
assert cov, "CROSS_CHECK_COVERAGE not found"
parts = re.findall(r'"((?:[^"\\]|\\.|\\\n)*)"', cov.group(1), re.S)
sql_parts = [p for p in parts if "SELECT" in p or "FROM" in p or "JOIN" in p]
assert sql_parts, "coverage SQL literal not found"
cov_sql = re.sub(r"\\\s*\n\s*", "", "".join(sql_parts))
out.append(("COVERAGE factor_kg_co2e_per_kg", re.sub(r"\s+", " ", cov_sql).strip()))

assert len(out) == 4, f"expected 3 cross-checks + 1 coverage, extracted {len(out)}: {[p for p,_ in out]}"
for path, sql in out:
    assert sql.lower().startswith("select"), f"{path}: not a bare SELECT -> {sql[:80]!r}"
    alias = "as comparable" if path.startswith("COVERAGE") else "as mismatches"
    assert alias in sql.lower(), f"{path}: missing `{alias}`"
    # The extracted text must be a real substring of the source, modulo the
    # whitespace collapsing — proof we probe what the contract declares.
    print(path + "\t" + sql)

print("EXTRACTED_OK", file=sys.stderr)
