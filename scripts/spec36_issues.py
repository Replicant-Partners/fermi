#!/usr/bin/env python3
"""Create the SPEC 36 GitHub issues from the spec text itself.

One issue per `### Rn.m` requirement, one tracking epic per workstream, one
master epic. Idempotent: an issue whose title already exists (open or closed)
is reused, not duplicated. Writes the id map to scripts/spec36_issue_map.json.
"""
import json, re, subprocess, sys, pathlib

REPO = "Replicant-Partners/fermi"
SPEC = pathlib.Path("docs/specs/SPEC_36_INTEGRITY_REMEDIATION.md")
BRANCH_URL = f"https://github.com/{REPO}/blob/integrity/spec-36/{SPEC}"
OUT = pathlib.Path("scripts/spec36_issue_map.json")

def gh(*args, input=None):
    r = subprocess.run(["gh", *args], capture_output=True, text=True, input=input)
    if r.returncode != 0:
        sys.exit(f"gh {' '.join(args[:3])} failed: {r.stderr}")
    return r.stdout

LABELS = {
    "spec-36": ("5319e7", "SPEC 36 integrity remediation"),
    "epic": ("3e4b9e", "Tracking issue"),
    "P0": ("b60205", "Exploitable or money-losing now"),
    "P1": ("d93f0b", "Silent corruption or crash risk"),
    "P2": ("fbca04", "Architectural gap blocking a stated requirement"),
    "P3": ("c5def5", "Hygiene / drift"),
}
WS = {
    "0": ("ws0-stop-bleeding", "Phase 0 — stop the bleeding"),
    "1": ("ws1-money", "WS1 — Money atomicity"),
    "2": ("ws2-cqrs", "WS2 — Command boundary and event store (CQRS + immutability)"),
    "3": ("ws3-artifacts", "WS3 — Artifact store and content addressing"),
    "4": ("ws4-grounding", "WS4 — Evidence-based grounding"),
    "5": ("ws5-provenance", "WS5 — Provenance graph (ΞPROV)"),
    "6": ("ws6-gates", "WS6 — Gate ledger"),
    "7": ("ws7-lattice", "WS7 — Verification Lattice v1"),
    "8": ("ws8-claims", "WS8 — Claims discipline"),
    "A": ("ws-other", "Appendix A — remaining register items"),
}
# Severity per requirement (defaults by workstream; overrides from the gap register).
SEV_DEFAULT = {"0": "P1", "1": "P0", "2": "P1", "3": "P2", "4": "P1", "5": "P2", "6": "P1", "7": "P2", "8": "P2", "A": "P2"}
SEV = {"R0.1": "P0", "R0.2": "P0", "R0.3": "P0", "R0.9": "P3", "R8.3": "P1", "R8.5": "P1",
       "R3.5": "P1", "R3.7": "P2", "R2.3": "P1", "R2.8": "P1", "R7.1": "P2", "R8.7": "P3"}

# Appendix A items without an Rn.m of their own.
EXTRA = [
    ("A", "G21", "P2", "Process-local state: singleton guard now, externalise before scale-out",
     "Spec §13.3.2 / Appendix A G21. Boot acquires `pg_try_advisory_lock(hashtext('abw-api-singleton'))`; a second replica refuses to start with an explicit error. Follow-up spec to externalise rate limits, counters, caches and git storage.\n\n**Done when** a second `api-server` against the same DB exits non-zero with a message naming the singleton guard."),
    ("A", "G24", "P2", "Per-agent tool allowlist (every agent currently gets every builtin tool)",
     "Appendix A G24. Card `tools_allowed` is enforced in `PlatformToolRegistry::execute` and in the tool list offered to the model; defaults to the current set for existing cards; part of `card_sha`.\n\n**Done when** a test card with `tools_allowed: [\"get_agent\"]` is offered exactly one tool and a forged call to another returns an error."),
    ("A", "G26", "P1", "Image attachments are dropped on the tool loop",
     "Appendix A G26 / LLD §3.2. `execute_anthropic_loop` must build the first user message from `context.attachments`.\n\n**Done when** a unit test with a 1×1 PNG attachment shows an image block in the first request body of the tool loop."),
    ("A", "G27", "P3", "CLI login: key in redirect query string; role scopes unioned into every key",
     "Appendix A G27. Code-exchange with PKCE (`POST /auth/cli/token`), CSPRNG state; key scopes intersect with role instead of union.\n\n**Done when** the redirect carries only a one-time code and a `cli`-scoped key held by an admin cannot call an admin route."),
    ("A", "G28", "P3", "Consolidation lease: extend never called; cleanup_expired placeholder",
     "Appendix A G28 (also #26). Worker heartbeats `extend` every TTL/3; `cleanup_expired` implemented.\n\n**Done when** a test holding a lease past TTL with heartbeats is not stolen, and `cleanup_expired` deletes an expired row and returns 1."),
    ("A", "G31", "P3", "SSE handler opens a PgListener per client outside the pool",
     "Appendix A G31. Use the shared `pg_notify` broadcast in `handlers/workspace/messages.rs` SSE.\n\n**Done when** grep finds no `PgListener::connect` in handler code and the SSE test still receives notifications."),
    ("A", "G32", "P3", "Editor tooling drift: tree-sitter grammar and hard-coded LSP path",
     "Appendix A G32. Generate keyword set from `lexer.rs`; resolve `fermi-lsp` from PATH.\n\n**Done when** a test asserts the grammar keyword list equals the lexer's and the extension contains no absolute path."),
    ("A", "G33", "P3", "No API / manifest / tool contract versioning",
     "Appendix A G33. `/api/v1` for new routes; `manifest_version` in App manifests; `PlatformTool::version()`.\n\n**Done when** abw-apps-core rejects a manifest without `manifest_version` (with a migration path) and every tool reports a version."),
    ("A", "G16", "P1", "Migration runner: skip applied, fail on checksum change, fail boot on failure",
     "Spec §12 runner change. Skip a migration whose recorded `content_sha256` matches with `first_succeeded_at`; changed checksum on an applied migration fails boot; any failure fails boot; `SCHEMA_STRICT` becomes default; delete `ensure_critical_schema` after moving its statements into a numbered migration.\n\n**Done when** `tests/migration_ledger.rs` covers skip / checksum-change / failure-aborts against a real DB."),
]

def parse_reqs(text):
    reqs = []
    heads = list(re.finditer(r"^### (R(\d)\.\d+b?): (.+)$", text, re.M))
    for i, m in enumerate(heads):
        end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
        body = text[m.end():end]
        body = re.split(r"\n---\n|\n## ", body)[0].strip()
        reqs.append((m.group(1), m.group(2), m.group(3).strip(), body))
    return reqs

def existing():
    out = gh("issue", "list", "-R", REPO, "--state", "all", "--limit", "500", "--label", "spec-36",
             "--json", "number,title")
    return {i["title"]: i["number"] for i in json.loads(out)}

def create(title, body, labels, have):
    if title in have:
        return have[title]
    url = gh("issue", "create", "-R", REPO, "--title", title, "--body-file", "-",
             *sum([["--label", l] for l in labels], []), input=body).strip()
    n = int(url.rsplit("/", 1)[1])
    have[title] = n
    print(f"#{n} {title}")
    return n

def main():
    for name, (color, desc) in LABELS.items():
        gh("label", "create", name, "-R", REPO, "--color", color, "--description", desc, "--force")
    for _, (name, desc) in WS.items():
        gh("label", "create", name, "-R", REPO, "--color", "ededed", "--description", desc, "--force")

    text = SPEC.read_text()
    have = existing()
    reqs = parse_reqs(text)
    idmap = {}
    for rid, ws, title, body in reqs:
        sev = SEV.get(rid, SEV_DEFAULT[ws])
        t = f"[SPEC-36 {rid}] {title}"
        b = (f"Requirement **{rid}** of [SPEC 36]({BRANCH_URL}) — {WS[ws][1]}.\n\n{body}\n\n---\n"
             f"Close only when the **Done when** test exists, fails on the pre-change code, and passes on the change (SPEC 36 §17).")
        idmap[rid] = create(t, b, ["spec-36", WS[ws][0], sev], have)
    for ws, gid, sev, title, body in EXTRA:
        t = f"[SPEC-36 {gid}] {title}"
        b = f"[SPEC 36]({BRANCH_URL}) — {WS[ws][1]}.\n\n{body}\n\n---\nClose only when the Done-when test passes."
        idmap[gid] = create(t, b, ["spec-36", WS[ws][0], sev], have)

    epics = {}
    for ws, (lab, name) in WS.items():
        members = [(k, v) for k, v in idmap.items()
                   if (k.startswith(f"R{ws}.") or (ws == "A" and k.startswith("G")))]
        if not members:
            continue
        lines = "\n".join(f"- [ ] #{n} {k}" for k, n in members)
        b = f"Tracking issue for **{name}** of [SPEC 36]({BRANCH_URL}).\n\n{lines}\n"
        epics[ws] = create(f"[SPEC-36 EPIC] {name}", b, ["spec-36", "epic", lab], have)
    lines = "\n".join(f"- [ ] #{n} {WS[ws][1]}" for ws, n in epics.items())
    prior = "#22 #23 #24 #25 #26 #27 #28"
    b = (f"Master tracking issue for [SPEC 36 — Integrity Remediation]({BRANCH_URL}).\n\n"
         f"The low-level design (`docs/ABW_LOW_LEVEL_DESIGN.md`) found the trust-domain architecture specified "
         f"since early 2026 — CQRS command boundary, immutable append-only history, provenance chain, the "
         f"Verification Lattice — was largely not built. This epic tracks building it.\n\n{lines}\n\n"
         f"Related earlier audit issues: {prior}.\n\nPolicy: every requirement closes on a test that fails before "
         f"and passes after; documents claim only what tests prove (WS8).")
    epics["master"] = create("[SPEC-36 EPIC] Integrity remediation — build what was specified", b,
                             ["spec-36", "epic"], have)
    OUT.write_text(json.dumps({"requirements": idmap, "epics": epics}, indent=2) + "\n")
    print(f"{len(idmap)} requirement issues, {len(epics)} epics -> {OUT}")

if __name__ == "__main__":
    main()
