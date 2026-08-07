# Coding Agent Task: Business Rule Execution Audit Instrumentation

## Context (give this to the agent verbatim, in addition to the schema-drift-harness prompt)

We've built (or are building) drift instrumentation for event/table schema. A related
and partially overlapping problem: **business rules we believe are executing may be
silently failing** — not throwing, not logging, just not doing what we assume. This is
often *caused by* schema drift (a validator checks a field that got renamed, so it
always passes trivially) but can also happen independently (swallowed errors,
short-circuited guards, rules that got wired up once and then orphaned when a code
path changed).

Your job is to make "is this rule actually running, and is it actually capable of
failing when it should" a directly observable, queryable property of the system — not
something we discover by noticing a bad row in production three weeks later.

## The specific failure modes to instrument against

1. **Swallowed errors.** `.ok()`, `let _ = result`, `unwrap_or_default()`, or a
   catch-all `match` arm that discards an `Err` without logging it, inside any function
   that's supposed to enforce a business rule.
2. **Trivially-always-true guards.** A validator/guard that references a field or shape
   that schema drift changed, so the check still runs but can never actually fail
   (e.g., checking `status == "pending"` after the field was renamed to `state`, so
   the comparison is always false against a stale variant and the branch that should
   gate on it never triggers).
3. **Orphaned rules.** A rule is defined and unit-tested in isolation but is no longer
   actually wired into the live invocation path — a refactor moved the call site and
   nobody deleted the now-dead rule, or nobody re-wired it either. It exists, passes
   its own tests, and never runs in production.
4. **Short-circuited guards.** An early return, `?` propagation, or branch ordering
   change causes a rule to be skipped for a subset of inputs it should apply to (e.g.,
   a fast-path added later that bypasses the validation layer entirely for a specific
   input shape).
5. **Precondition drift.** The rule fires correctly but its precondition (the event or
   state that should trigger it) itself became unreachable due to upstream event
   schema/routing changes — the rule is fine, it's just never invoked anymore.

## Deliverables

### 1. Business Rule Registry
- A canonical, code-adjacent manifest of business rules: rule ID, human description,
  the invariant it enforces, which code path/function implements it, and which
  event type(s)/operation(s) should trigger its evaluation.
- This is the rule-layer equivalent of the event schema registry — a declared,
  versioned "this rule should fire under these conditions" statement that instrumentation
  can be checked against, rather than inferring expected behavior from code alone.
- Add a CI check: any function tagged as implementing a registered rule must retain a
  detectable call site; flag (don't silently allow) removal or relocation of that call
  site without a corresponding registry update.

### 2. Execution Tracing ("did it actually run")
- Wrap every registered rule's invocation with a lightweight trace emission: rule ID,
  timestamp, input summary (not full payload unless needed — enough to reconstruct
  intent), outcome (`pass` / `fail` / `error-swallowed-detected` / `not-reached`).
- Emit this into the same provenance trail as the drift alarm log from the schema-drift
  harness (ΞPROV), tagged with the schema version in effect at execution time. This is
  the critical link: when a rule's pass rate suspiciously jumps to 100%, you want to be
  able to cross-reference that against a schema version change in the same window.
- Rules that should fire per-event-type but show zero executions over a rolling window
  are surfaced as **orphaned-rule alarms** — this is your detector for failure mode #3
  and #5 above.

### 3. Swallowed-Error Static Analysis Pass
- A lint/analysis step (via `clippy` custom lints, or a scripted AST pass if clippy
  doesn't cover it) that flags: `.ok()`, `let _ =`, `unwrap_or_default()`, and bare
  catch-all match arms on `Result`/`Option` specifically within functions tagged as
  rule-implementations in the registry from #1.
- This is scoped narrowly (only rule-implementing functions) so it doesn't produce
  noise across the whole codebase — the point is business-rule integrity, not a
  blanket ban on these patterns everywhere.
- Findings feed into the same drift/rule alarm log, tagged by severity (a swallowed
  error in a rule-implementation function is high severity by definition).

### 4. Mutation Testing for Rule Reachability
- For each registered rule, add a mutation test: deliberately feed an input that
  *should* fail the rule and assert that it does. This catches failure mode #2
  (trivially-always-true guards) directly — a rule that can't be made to fail by any
  known-bad input is flagged as suspect regardless of whether it's "passing" its
  existing tests.
- Suggest `cargo-mutants` or a hand-rolled property-based harness (`proptest`) generating
  known-invalid variants per rule, run as part of the same CI gate as the schema
  migration diff check — rule integrity and schema integrity should be checked in the
  same pass since they're causally linked.

### 5. Correlation Report: Rule Outcome vs. Schema Version
- A query/report (CLI or simple dashboard) that answers: "for rule X, show pass/fail
  rate over time, overlaid with schema version changes in the relevant domain."
- This turns "why did this rule stop catching bad data three weeks ago" from an
  archaeology exercise into a direct lookup: correlate the outcome-rate discontinuity
  against the nearest schema version bump or migration timestamp.

### 6. Coverage Assertion at Task-Completion
- Any agent (human-directed or autonomous) completing a task that touches a
  rule-implementing function must, as a completion gate, confirm: (a) the rule's
  registry entry still points to a live call site, (b) the mutation test for that rule
  still fails on known-bad input, (c) no new swallowed-error pattern was introduced per
  #3. Fold this into the same "cargo check + schema introspection" completion checklist
  from the schema-drift harness rather than treating it as a separate step.

## Acceptance criteria

- [ ] Can I list every registered business rule and see its live execution count and
      pass/fail rate over the last N days without reading code?
- [ ] If a rule's pass rate silently goes to 100%, does something alert on that, or do
      I only find out when bad data surfaces downstream?
- [ ] Can I distinguish "this rule is orphaned/not being called" from "this rule is
      called and legitimately always passing because the data is actually always
      valid"? (Mutation testing from #4 is what makes this distinguishable.)
- [ ] If a rule's behavior changed, can I tell whether it was a deliberate logic change
      or a side effect of upstream schema drift, without manual investigation?

## Sequencing

Build after the schema-drift harness's Drift Alarm Log (#4 in that document) exists,
since this depends on it for correlation. Order: (1) Rule Registry → (2) Execution
Tracing → (3) Swallowed-Error Lint Pass → (4) Mutation Testing → (5) Correlation
Report → (6) Task-completion gate integration.

## What NOT to do

- Do not instrument every function in the codebase — scope strictly to functions
  registered as business-rule implementations. Blanket tracing produces noise that
  gets ignored, which defeats the purpose.
- Do not treat "rule passes 100% of the time" as inherently suspicious without the
  mutation test — some rules legitimately almost never fail in practice. The mutation
  test, not the raw pass rate, is what proves the rule is actually capable of catching
  a violation.
- Do not build this as a separate provenance system from ΞPROV/the drift alarm log —
  the value is in the correlation, which requires a shared timeline.
