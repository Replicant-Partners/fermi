# ADR-003: Hybrid Fermi Coaching Integration

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** Project team  
**Related:** Module 1 (FPL LSP), [QUESTIONS_BY_MODULE.md](../QUESTIONS_BY_MODULE.md#q13-fermi-coaching-integration)

---

## Context

Fermi is the inline AI coach that provides real-time suggestions as users write forecasts. We need to decide how Fermi's coaching messages appear in the Zed editor through the LSP.

**Requirements:**
- Errors must be clearly visible (standard LSP diagnostics)
- Suggestions should be helpful but not intrusive
- Coaching should work in any LSP-compatible editor (future-proofing)
- Need to distinguish between "must fix" errors and "could improve" suggestions

**User Experience Goal:**
- Errors block execution → must be fixed → standard diagnostics
- Suggestions improve quality → optional → richer presentation

---

## Decision

We will use a **Hybrid approach**:

1. **Errors/Warnings** → Standard LSP diagnostics
   - Type errors, validation failures, syntax errors
   - Appear as red/yellow squiggles inline
   - Block execution until fixed

2. **Coaching Suggestions** → Separate LSP extension messages
   - Fermi-specific suggestions (e.g., "Consider widening this range")
   - Appear as info/hints with special icon
   - Don't block execution, optional to follow

**Technical Implementation:**
```rust
// Standard diagnostics for errors
Diagnostic {
    severity: DiagnosticSeverity::ERROR,
    message: "Triangular ordering violated: p5 > p50",
    ...
}

// Custom extension for coaching
FermiCoachingMessage {
    severity: CoachingSeverity::SUGGESTION,
    message: "This range is very narrow (±2%). Historical data suggests ±30%.",
    coaching_type: "overconfidence_warning",
    ...
}
```

---

## Consequences

### Positive Consequences

✅ **Best of Both Worlds**
- Standard diagnostics work in any LSP client
- Rich coaching can have custom UI in Zed
- Clear separation: errors vs. suggestions

✅ **User Control**
- Users can disable coaching without disabling error checking
- Can tune coaching verbosity independently
- Errors are always visible (not buried in suggestions)

✅ **Future-Proof**
- Standard diagnostics ensure basic functionality everywhere
- Custom coaching can evolve without breaking compatibility
- Other editors can implement coaching UI if they want

✅ **Better UX**
- Errors are unambiguous (must fix)
- Suggestions are clearly optional (nice to have)
- Can show different icons/colors for each type

### Negative Consequences

❌ **More Complexity**
- Two parallel messaging systems to maintain
- Need to implement custom LSP extension
- Zed needs to handle both message types

❌ **Inconsistent Experience**
- In editors that don't support custom extensions, only see errors
- Need fallback: show coaching as info-level diagnostics in non-Zed editors

### Neutral Consequences

⚖️ **Implementation Effort**
- Standard diagnostics are straightforward (already in LSP)
- Custom extension requires Zed-specific protocol
- Total effort: ~20% more than pure diagnostics approach

---

## Alternatives Considered

### Alternative A: Pure LSP Diagnostics

Use only standard LSP diagnostics for everything (errors + coaching).

**Example:**
```rust
// Coaching as info-level diagnostic
Diagnostic {
    severity: DiagnosticSeverity::INFORMATION,
    message: "[Fermi] This range is very narrow...",
}
```

**Pros:**
- Simplest implementation
- Works in all LSP clients
- No custom protocol needed

**Cons:**
- Can't distinguish errors from suggestions visually
- Limited control over presentation
- Coaching clutters diagnostic list
- No way to disable coaching without disabling all info diagnostics

**Why not:** Users need clear distinction between "must fix" and "could improve". Pure diagnostics mix them together.

---

### Alternative B: Separate LSP Extension Only

Use custom LSP extension for all Fermi messages (errors + coaching).

**Example:**
```rust
// Everything goes through custom protocol
FermiMessage {
    severity: ERROR | WARNING | SUGGESTION,
    message: "...",
    coaching_type: Option<CoachingType>,
}
```

**Pros:**
- Unified Fermi-specific protocol
- Full control over presentation
- Can add rich metadata (explanations, links, etc.)

**Cons:**
- Doesn't work in non-Zed editors
- Errors won't show up in other LSP clients
- Non-standard approach
- Breaks compatibility

**Why not:** Errors should always be visible, even in basic LSP clients. We don't want to lock users into Zed for basic functionality.

---

### Alternative C: Separate Service

Run coaching as a separate service (not through LSP at all).

**Example:**
```
Zed Extension → FPL LSP (errors only)
              → Fermi Coaching Service (suggestions)
```

**Pros:**
- Complete separation of concerns
- Coaching can be more sophisticated (heavy ML models, etc.)
- Can scale coaching independently

**Cons:**
- Two separate connections to manage
- Coaching doesn't know about parse state (would need to duplicate)
- More latency (separate request)
- Over-engineered for current needs

**Why not:** Coaching needs access to parse tree and semantic info, which LSP already has. Duplicating that state in a separate service is wasteful.

---

## Implementation Notes

### Error Messages (Standard Diagnostics)

**When to use:**
- Syntax errors (can't parse)
- Type errors (type mismatch)
- Validation errors (triangular ordering, probability range)
- Undefined symbols
- Any issue that blocks execution

**Example:**
```rust
pub fn emit_error_diagnostic(&self, span: Span, message: String) -> Diagnostic {
    Diagnostic {
        range: span_to_lsp_range(span),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("fermi-lsp".to_string()),
        message,
        ..Default::default()
    }
}
```

---

### Coaching Messages (Custom Extension)

**When to use:**
- Overconfidence warnings (range too narrow)
- Missing evidence suggestions
- Calibration feedback
- Best practice recommendations
- "Did you consider...?" prompts

**LSP Extension Protocol:**
```typescript
// Custom notification from server to client
interface FermiCoachingMessage {
    range: Range;
    severity: 'suggestion' | 'tip' | 'recommendation';
    message: string;
    coaching_type: string;
    explanation?: string;
    learn_more_url?: string;
}

// Client registers for notifications
client.onNotification('fermi/coaching', (message: FermiCoachingMessage) => {
    // Zed displays with special UI
});
```

**Zed Display:**
- Icon: 🤖 (Fermi avatar)
- Color: Blue (distinct from errors/warnings)
- Action: "Learn more" button if explanation exists
- Dismissible: Users can dismiss per-message or globally

---

### Adaptive Verbosity (Future)

This hybrid approach enables adaptive coaching (ADR-004):
- Track which suggestions users accept/dismiss
- Learn user preference over time
- Adjust coaching verbosity automatically
- Errors always shown (not affected by verbosity)

---

### Fallback for Non-Zed Editors

If a client doesn't support the custom extension:
- Send coaching as **info-level diagnostics**
- Prefix with "[Fermi]" to identify source
- Users get basic coaching, just less pretty

```rust
pub fn emit_coaching_message(&self, span: Span, message: String) -> Message {
    if client_supports_custom_extension() {
        // Send as custom message
        FermiCoachingMessage { ... }
    } else {
        // Fallback to info diagnostic
        Diagnostic {
            severity: Some(DiagnosticSeverity::INFORMATION),
            message: format!("[Fermi] {}", message),
            ...
        }
    }
}
```

---

## Testing Strategy

**Error Diagnostics:**
- Unit tests: Parser/semantic errors generate correct diagnostics
- LSP tests: Diagnostics sent correctly over protocol
- Zed tests: Errors display with red squiggles

**Coaching Messages:**
- Unit tests: Coaching logic generates appropriate suggestions
- LSP tests: Custom messages sent correctly
- Zed tests: Coaching displays with custom UI
- Fallback tests: Info diagnostics work in mock "basic" client

---

## References

- LSP Diagnostic specification: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnostic
- Zed extension documentation: https://zed.dev/docs/extensions
- Related: ADR-004 (Adaptive Coaching Verbosity)
- Question answered: Q1.3 in QUESTIONS_BY_MODULE.md

---

## Revision History

- **2026-02-04:** Initial version - Status: Accepted
