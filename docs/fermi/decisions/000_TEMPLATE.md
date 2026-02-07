# ADR-000: [Title] (TEMPLATE)

**Date:** YYYY-MM-DD  
**Status:** Proposed | Accepted | Superseded | Rejected  
**Deciders:** [Who was involved]  
**Related:** [Links to other ADRs, issues, docs]

---

## Context

**What's the issue we're facing?**

Describe the problem, constraint, or requirement that necessitates a decision.

Example:
"We need incremental parsing for the FPL LSP to provide real-time diagnostics as users type. Full re-parsing on every keystroke would be too slow (>500ms). We need sub-100ms latency for good UX."

---

## Decision

**What did we decide?**

State the decision clearly and concisely.

Example:
"We will use salsa for incremental parsing in the FPL Language Server."

---

## Consequences

**What are the trade-offs?**

List positive, negative, and neutral consequences of this decision.

### Positive Consequences
- Benefit 1
- Benefit 2

### Negative Consequences  
- Drawback 1
- Drawback 2

### Neutral Consequences
- Thing that changes but isn't clearly good/bad
- New complexity introduced

---

## Alternatives Considered

**What else did we look at? Why didn't we choose them?**

### Alternative 1: [Name]
- **Pros:** What's good about this option
- **Cons:** What's bad about this option  
- **Why not:** Specific reason we rejected it

### Alternative 2: [Name]
- **Pros:** ...
- **Cons:** ...
- **Why not:** ...

---

## Implementation Notes

**How will this decision be implemented?**

- Technical details
- Migration plan (if replacing something)
- Testing strategy
- Rollout plan

---

## References

- [Link to benchmark results]
- [Link to relevant documentation]
- [Link to discussion thread]
- [Link to prototype code]

---

## Revision History

- **YYYY-MM-DD:** Initial version (status: Proposed)
- **YYYY-MM-DD:** Accepted after team review
- **YYYY-MM-DD:** Superseded by ADR-XXX (if applicable)
