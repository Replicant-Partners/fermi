# Prompt: generate the ABW Target System Design (as intended, not as built)

Hand this whole file to a fresh session as its task. It is self-contained.

---

## Role and output

You are a principal systems architect. Write **one** document,
`docs/architecture/ABW_TARGET_SYSTEM_DESIGN.md`. It is the normative low-level design of the ABW
(Agent Bestiary Workspace) platform **as it is intended to work** once remediation is complete. The
reader is a senior C++ systems engineer who will hold implementers to it. It must explain, in
physical terms, how each logical requirement is met:

- which process does the work;
- which transaction it happens in;
- which table and which bytes;
- which lock and which index.

**Do not modify any other file. Do not write code. Do not open PRs or issues.**

## Source documents and their authority

Read these first, in this order. When they conflict, the higher one wins.

| Rank | Document | Role |
|---|---|---|
| 1 | `docs/architecture/ABW_VERIFICATION_LATTICE_SPEC_v0.1.md` | **The design.** Normative for claims, dimensions, verdicts, judge, routing, events and loops |
| 2 | `docs/architecture/ABW_CODING_AGENT_DIRECTIVE.md` | **Rules and phasing.** §3 hard rules and §5 phase boundaries are invariants of the target. Its §4 dispositions decide what is in scope |
| 3 | `docs/specs/SPEC_36_INTEGRITY_REMEDIATION.md` | Detailed mechanisms (schemas, APIs, algorithms). Use it only where it agrees with ranks 1–2. Where it conflicts (in-process `CommandBus`, an outbox outside the ledger crate, ladder rung names, the R0.8 permit type), **follow the directive** and record the conflict in Appendix "Divergence from SPEC 36" |
| 4 | `docs/ABW_LOW_LEVEL_DESIGN.md` | **Current state only.** Use it to name every gap the target closes (G01–G33) and as a fact base for components that keep their current design. Never copy its "as built" behaviour into the target unless the directive keeps it |
| 5 | The code | Ground truth for anything the target **keeps unchanged** (FPL engine, executor loop shape, tools, Apps, MCP/A2A wire formats). Cite `path:line` when you rely on it |

Also skim, for vocabulary only:

- `docs/AGENT_MODEL.md`
- `docs/GLOSSARY.md`
- `docs/EMBEDDING_PROVENANCE.md`
- `docs/specs/01_APP_PRIMITIVE.md`
- `docs/A2A_DEVELOPER_GUIDE.md`

## Non-negotiable properties of the target

The design you write MUST make each of these true and show **physically** how:

1. **The write boundary is a crate.** `abw-ledger` is the only code holding a write-capable DB handle. Handlers receive a read-only handle. A handler that writes directly does not compile.
2. **Trust history is events.** Streams use per-stream sequence numbers and expected-version append. Artifacts are content-addressed. Hashing is JCS (RFC 8785), with one prefixed format: `sha256:<hex>`. Event envelopes are typed and versioned.
3. **Projections update in the append transaction.** Old mutable tables become projections; nothing else writes them.
4. **The database enforces immutability**, not comments:
   - `BEFORE UPDATE OR DELETE` triggers;
   - `ON DELETE RESTRICT` foreign keys;
   - an owner role for migrations and an app role that has `INSERT` only on events.
5. **A record is atomic with its effect.** Decisions, charges, versions and verdicts are written in the same transaction as the effect they describe. There is no write-behind, no detached task and no fire-and-forget for these records.
6. **A control refuses or it does not exist.** Anything that only records is typed and named as an *observation*, and records `Undetermined` with a reason class.
7. **Evidence is separate from verdict.** Evidence collection and verdict computation are different modules. No function both stamps provenance and amends a claim.
8. **Interpretive is never provenance.** LLM-judge output is typed `interpretive` and is unreachable in any Grounding walk. The database enforces this.
9. **One word, one meaning.** Presence, Liveness, Truth, Grounding and Binding refer only to the Lattice's claim-level dimensions. The platform-health checks are named `platform_health` with these rungs: `schema_present`, `write_path_live`, `rollup_consistent`, `field_tool_sourceable`, `input_port_match`.
10. **Money is a ledger.** The balance is derived from ledger events. Credit is reserved before work and settled in the episode-close transaction. Retries are idempotent.
11. **Verdicts are reproducible.** A verdict vector is a pure function of `(claim, artifacts, stream heads, provenance graph, θ version)` and can be replayed bit-for-bit. θ is versioned policy data, and `theta_version` is part of the vector.
12. **The verdict vocabulary is closed.** Every dimension returns exactly one of `pass | fail | unsupported | not_evaluated`. A missing substrate yields `unsupported` or `not_evaluated`, never `pass`.

## Required structure

Use these sections, in this order:

0. **Reading guide.**
   - Status line: `Status: Target` (normative, not implemented).
   - The authority table above.
   - Legend: **[S]** taken from the Lattice spec, **[R]** from the directive, **[M]** from a SPEC 36 mechanism, **[K]** kept from current code (with `path:line`), **[P]** your proposal where all sources are silent. Every [P] must also appear in the open-decisions section.
1. **System context and invariants.**
   - Number the invariants `T1…Tn`. Map each to the Lattice spec principle and directive rule it implements.
   - Show which phase (directive §5) first makes each invariant true.
2. **Physical architecture.**
   - Processes, tokio runtime, pools: a read-only pool for handlers and a write pool owned by `abw-ledger`.
   - Postgres roles, git and object storage, background workers and who owns them.
   - The single-replica constraint until Phase 3 completes, with the path to horizontal scale.
   - A deployment diagram.
3. **The ledger crate (`abw-ledger`).** This is the centre of the document, so give it the most depth:
   - DDL for `streams`, `events`, `event_types`, `artifacts`, `projector_checkpoints`, and `policy` / θ versions.
   - The append algorithm with expected-version compare-and-swap, as step-by-step SQL with lock analysis.
   - How a stream is read and folded (aggregates), plus snapshots.
   - Inline projections in the same transaction: which projections, and why each is inline.
   - Read-side visibility for anything asynchronous (the `xid8` / snapshot fence).
   - JCS canonicalisation and the hash format, with exact byte rules and test vectors.
   - The Rust API surface, as signatures only.
   - How the type system makes direct writes impossible.
   - Failure and crash semantics at every statement boundary.
4. **What an agent is, in the target.**
   - The resolved card, identified by `card_sha`; the version stream; seed-on-insert-only.
   - The per-agent tool allowlist.
   - The execution pipeline as a sequence diagram, showing which work happens in which transaction (reserve → run → close+settle), plus the drop guard that always closes an episode.
   - Show the episode as a stream, not a mutable row.
5. **Claims and the Verification Lattice.** Implement the spec exactly:
   - claim envelope;
   - the five dimensions, each with its physical instrument (table, index, query) and complexity;
   - dependencies (Truth needs Presence; Truth is qualified by Grounding);
   - verdict states;
   - the `lattice.evaluated` event;
   - residual risk;
   - the judge report and its independence rules (model family, prompt lineage, injection-safe delimiting);
   - the routing state machine (Proceed / Sign-off / Correct / Endorse / Park);
   - correction budget, endorsement and sign-off packets;
   - the full event catalogue from the spec, with payload schemas.

   Answer each of the spec's open questions, either from SPEC 36 §15 or as a [P].
6. **Provenance (the spec's ΞPROV).**
   - The node and edge model (PROV-DM relations) and the `epistemic` class.
   - The DB constraint that makes interpretive nodes unreachable.
   - External sources and DOI/URL resolution through the outbound guard.
   - The field-level `quotedFrom` edges produced from evidence.
   - The chain-walk algorithm (recursive CTE, depth bound, cycle detection, indexes) and its cost.
   - How derived knowledge (semantic rules) inherits a provenance floor along edges.
7. **Evidence-based grounding.**
   - Evidence collection from the run's own tool-call artifacts.
   - Match methods and the total verdict table.
   - The demotion rules, with the bridge from today's `tool_verified` content proxy (LLD §5.4).
   - The measure → shadow → enforce rollout required by directive rule 5.
8. **Gates as events.**
   - Every gate decision is an event in its effect's transaction, including detail (γ, θ, schema errors).
   - Replay versus re-evaluation for two-reviewer consensus.
   - Reason classes (`policy`, `infra_error`, `advisory`, `insufficient_input`).
   - Observations kept distinct from controls.
9. **Money.**
   - Ledger events, holds, settlement inside episode close, royalties, idempotency keys.
   - Stripe, App spawn funding, and the platform-sponsorship ledger entry for template budgets.
   - Why the balance can never drift from the ledger.
10. **Vertical slices.**
    - **Fermi (Phase 2):** forecasts as claims, Binding and Brier at outcome, calibration loops.
    - **DPP / carbon (Phase 3):** Presence and Truth against stored artifacts, episode lineage, signed passports with an offline verifier.

    For each slice, give the end-to-end event flow as a sequence diagram and the exact consistency boundaries.
11. **Loops and policy.**
    - Inner, middle and outer loops with their projections and metrics.
    - θ as versioned data.
    - Human-load ceilings.
    - The outer loop proposes; a human accepts.
12. **Extension surfaces in the target.**
    - Tools (with `origin()` declaration and allowlist), evaluators, feeds.
    - Apps, MCP client and server, A2A (HMAC-signed webhooks, encrypted credentials).
    - The outbound SSRF guard.
    - Event-type versioning as the contract-versioning mechanism (G33).
13. **Security model.**
    - Tenancy (404 on unauthorised by-ID reads, everywhere).
    - Database roles.
    - Secrets, signing-key custody, outbound guard.
    - Rate limiting classified by handler.
14. **Migration from current state.**
    - The strangler plan by phase, with each phase's acceptance gate (directive §5) restated as checkable tests.
    - Which current tables become projections, and when.
    - Backfills that cannot be perfect and how they are marked.
    - The CI ratchets (`scripts/ratchet.py`) and their required trend.
15. **Conformance test catalogue.** Every invariant T1…Tn and every phase acceptance item, each with:
    - the test name;
    - what it asserts;
    - how it fails on today's code.
16. **Open decisions.** Every [P], plus anything the sources leave ambiguous. Give each the options and a recommendation; do not silently decide.
- **Appendix A:** Gap closure map. For each of G01–G33: the directive disposition and the section of this document that closes it, or "Deferred: reason".
- **Appendix B:** Divergence from SPEC 36, item by item, with the reason.
- **Appendix C:** Rust → C++ mental model, for any new idioms the target introduces.

## Diagrams

Use Mermaid only, in fenced code blocks. Supported types: flowchart, sequence, state, ER, class, gantt.

Constraints:
- No HTML in labels.
- No `%%{init}%%` directives.
- No hard-coded colours.
- No colons inside state-transition labels (they break the parser).

Use **numbered sequence diagrams** to show time-ordered flows frame by frame. At minimum:

- deployment;
- the ledger append under contention, showing two writers with the same expected version and one loser;
- episode reserve / run / close+settle;
- claim lifecycle (state machine);
- one claim end to end across lattice, judge, route and outcome;
- the provenance chain walk;
- the Fermi slice;
- the DPP slice;
- crash-point analysis.

If a Mermaid CLI (`mmdc`) is available, validate every diagram and fix the failures. If Chromium will not start in a sandbox, pass a puppeteer config with `--no-sandbox`.

## Quality bar

- **Normative voice.** Use MUST / MUST NOT / SHOULD (RFC 2119). Never describe target behaviour as already existing.
- **Cite every [K] claim** as `path:line`, and every [S], [R] or [M] claim by section.
- **Physical precision.** Use concrete DDL, index definitions, transaction boundaries, isolation level, lock order and complexity, not prose such as "is stored securely".
- **Close every gap.** Do not leave any LLD gap without a disposition.
- **Stay internally consistent.** Names, table names, event types and invariant numbers must match across all sections. Before finishing, grep your own document for each event type and table name to confirm.
- **Respect the directive's freeze list.** The target design must not contain:
  - command declarations without routing;
  - counters presented as fixes;
  - an outbox outside `abw-ledger`;
  - advisory checks presented as controls.

## Finish with

A short summary of your work containing:

- the document path and its length;
- the number of invariants;
- the number of open decisions;
- the diagrams validated;
- every place where you had to choose between conflicting sources.
