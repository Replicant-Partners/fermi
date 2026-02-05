# Code Actions - Quick Fixes & Refactoring

## Overview

The Fermi LSP now provides **Code Actions** - intelligent quick fixes and refactorings that appear when you have warnings or errors in your FPL code.

## What Are Code Actions?

Code actions are context-aware suggestions that appear as:
- 💡 Light bulb icons in your editor
- Quick fix suggestions
- Refactoring options
- Code improvements

They let you fix issues with a single click!

## Available Code Actions

### 1. Add Evidence Block

**Triggers:** When you see the warning:
```
⚠️ Consider adding evidence to support your forecast
```

**Action:** "Add evidence block"

**What it does:** Inserts a template evidence block:
```fpl
evidence source_name {
    source: "Source citation"
    summary: "Brief summary of the evidence"
    relevance: 0.8
    date: 2026-01-01
}
```

**How to use:**
1. See the warning about missing evidence
2. Click the 💡 light bulb icon (or press `Cmd+.` / `Ctrl+.`)
3. Select "Add evidence block"
4. Template is inserted - fill in the details!

## How to Access Code Actions

### In Zed:
- **Keyboard:** `Cmd+.` (Mac) or `Ctrl+.` (Linux/Windows)
- **Mouse:** Click the 💡 light bulb icon that appears near warnings/errors

### In VS Code:
- **Keyboard:** `Ctrl+.` or `Cmd+.`
- **Mouse:** Click the light bulb icon

### Visual Indicators:
- 💡 Yellow light bulb = Code action available
- Squiggly underlines = Diagnostic with potential fixes

## Example Workflow

### Before:
```fpl
question "What will revenue be?"

driver revenue continuous {
    distribution: triangular(100, 200, 300)
    unit: "USD"
}

model: revenue

simulate 10000 iterations
```

**Warning appears:**
```
⚠️ Consider adding evidence to support your forecast
```

### After (using code action):
```fpl
question "What will revenue be?"

driver revenue continuous {
    distribution: triangular(100, 200, 300)
    unit: "USD"
}

evidence source_name {
    source: "Source citation"
    summary: "Brief summary of the evidence"
    relevance: 0.8
    date: 2026-01-01
}

model: revenue

simulate 10000 iterations
```

Now just fill in the template!

## Future Code Actions (Coming Soon)

### Planned Quick Fixes:
- ✅ Add evidence block (DONE!)
- 🔜 Add rationale field
- 🔜 Add display_name field
- 🔜 Add description field
- 🔜 Add unit field
- 🔜 Fix missing distribution
- 🔜 Fix missing probability
- 🔜 Fix missing values/weights
- 🔜 Normalize discrete weights to sum to 1.0

### Planned Refactorings:
- 🔜 Convert continuous → binary driver
- 🔜 Convert continuous → discrete driver
- 🔜 Extract driver from model expression
- 🔜 Inline driver into model
- 🔜 Add if-then-else for binary driver

## Benefits

### 1. Faster Development
- Fix issues in one click
- No manual typing of boilerplate
- Consistent code structure

### 2. Learn Best Practices
- Code actions teach you the right way
- Templates include all recommended fields
- Documentation built-in

### 3. Fewer Errors
- Guided fixes prevent typos
- Proper structure guaranteed
- Validation happens automatically

## Tips

1. **Watch for the light bulb** 💡 - It means help is available
2. **Use keyboard shortcuts** - Much faster than clicking
3. **Review the change** - Code actions show preview before applying
4. **Combine with autocomplete** - Use both for maximum efficiency

## Technical Details

### How It Works

1. **LSP analyzes your code** and generates diagnostics
2. **Code action provider** checks diagnostics for fixable issues
3. **Actions are computed** based on diagnostic type
4. **Editor shows 💡 icon** when actions available
5. **User selects action** - LSP applies the fix

### Supported Diagnostic Types

- ⚠️ **Warnings** → Quick fixes available
- ❌ **Errors** → Quick fixes available (when possible)
- ℹ️ **Info** → Refactoring suggestions

## FAQ

**Q: Why don't I see the light bulb?**
A: Make sure you're using Zed or VS Code with the Fermi LSP installed and running.

**Q: Can I undo a code action?**
A: Yes! Use `Cmd+Z` / `Ctrl+Z` to undo like any other edit.

**Q: Do code actions work offline?**
A: Yes! All processing is local in the LSP server.

**Q: Can I add my own code actions?**
A: Not yet, but this is planned for future versions!

## Status

**Current:** ✅ 1 code action implemented
**Next Sprint:** 🎯 5-10 more code actions
**Future:** 🚀 Full refactoring suite

---

**Enjoy smarter coding with Code Actions! 💡**
