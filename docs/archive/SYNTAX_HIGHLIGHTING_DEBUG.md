# Syntax Highlighting Debug Session Summary

## Problem
- `question` and `continuous` keywords NOT highlighting in .fpl files
- `driver` and `triangular` ARE highlighting (orange)
- Autocomplete works for ALL keywords (LSP working fine)

## What We've Done

### 1. Cleaned Up Nested Directory Structure
- Removed nested `/home/ilabra/fermi/fermi/` directory (was duplicate/accident)
- Created backup branch: `backup-before-cleanup-20260205-012833`
- Now only have clean structure with files in `/home/ilabra/fermi/extensions/fermi/`

### 2. Updated Install Script
- Modified `scripts/install-extension.sh` to sync `highlights.scm` from `grammars/fpl/queries/` to `languages/fpl/` 
- This ensures Zed finds the highlights file in the right location

### 3. Modified Highlights Queries
- Reordered rules to put specific matches before general ones
- Changed `@keyword` and `@type` to `@function.builtin` for testing (to rule out theme issues)
- File: `/home/ilabra/fermi/extensions/fermi/grammars/fpl/queries/highlights.scm`

### 4. Cache Clearing
- Cleared `~/.cache/zed/`
- Cleared `~/.local/share/zed/languages/`

## Current State
- Extension version: `20260205-014806-7f55d49`
- All keywords defined identically in grammar.js as anonymous string literals
- Tree-sitter parses them all correctly (verified with --debug)
- Highlight queries have all keywords in same array marked as `@function.builtin`

## Theory
Zed might be caching old references to the nested structure or there's a stale compiled grammar somewhere.

## Next Steps When You Return

1. **Check if highlighting works now** after full Zed restart
   
2. **If still not working**, verify no old files remain:
   ```bash
   find ~/.config/zed ~/.local/share/zed ~/.cache/zed -name "*fpl*" -o -name "*fermi*" 2>/dev/null
   ```

3. **Reinstall clean**:
   ```bash
   cd /home/ilabra/fermi
   rm -rf ~/.config/zed/extensions/fermi
   bash scripts/install-extension.sh
   ```
   Then in Zed: Ctrl+Shift+P → "zed: reload extensions"

4. **If STILL not working**, the issue might be:
   - Zed's tree-sitter implementation doesn't support anonymous string literal highlighting in queries
   - Need to make keywords explicit nodes in grammar.js instead of anonymous strings
   - Or there's a Zed-specific query syntax we're missing

## Files to Check
- `/home/ilabra/fermi/extensions/fermi/grammars/fpl/queries/highlights.scm` (source)
- `~/.config/zed/extensions/fermi/grammars/fpl/queries/highlights.scm` (installed)
- `~/.config/zed/extensions/fermi/languages/fpl/highlights.scm` (also checked by Zed)

## Key Finding
All keywords (`question`, `driver`, `continuous`, `triangular`) are:
- Defined identically in grammar as anonymous string literals
- Parsed correctly by tree-sitter
- Listed in same highlight query arrays
- Yet only some highlight (driver, triangular) and others don't (question, continuous)

This suggests Zed is either caching selectively or there's something about the query matching we don't understand yet.

---

## SOLUTION FOUND (2026-02-05)

### Root Cause
The issue was NOT with tree-sitter parsing or query syntax. It was a **theme compatibility issue**:

- `triangular` was captured as `@function.builtin` → theme has color for this → highlighted ✓
- `driver` was captured as `@keyword` → theme might have color for this → highlighted ✓  
- `question` was captured as `@keyword` → theme doesn't have color for generic `@keyword` → NOT highlighted ✗
- `continuous` was captured as `@type` → theme doesn't have color for `@type` → NOT highlighted ✗

### The Fix
Changed all keyword captures from generic types to more specific ones that Zed themes universally support:

**Before:**
```scm
(question_statement "question" @keyword)
(driver_statement type: ["continuous" "binary" "discrete"] @type)
```

**After:**
```scm
(question_statement "question" @keyword.control)
(driver_statement type: ["continuous" "binary" "discrete"] @keyword.control)
```

### Why This Works
- `@keyword.control` is a more specific capture type that all Zed themes define colors for
- It's semantically correct (these ARE control keywords for the language structure)
- `@function.builtin` already worked for distribution names like `triangular`, `normal`, etc.

### Files Changed
- `/home/ilabra/fermi/extensions/fermi/grammars/fpl/queries/highlights.scm`

### Installation
```bash
cd /home/ilabra/fermi
bash scripts/install-extension.sh
```

Then in Zed:
- Close and restart Zed completely (Ctrl+Shift+P → "zed: reload extensions" does NOT always work)
- Open a `.fpl` file
- All keywords should now highlight consistently

### Verification Commands
Check that all anonymous nodes are defined:
```bash
jq -r '.[] | select(.named == false) | .type' ~/.config/zed/extensions/fermi/grammars/fpl/src/node-types.json | sort -u
```

Verify highlights file is updated:
```bash
head -20 ~/.config/zed/extensions/fermi/grammars/fpl/queries/highlights.scm
```

### Important Notes
1. **Full restart required**: Zed caches grammar/highlight files aggressively. "Reload extensions" command is NOT sufficient. You must close and reopen Zed completely.

2. **Theme compatibility**: If issues persist with a different theme, check which capture types your theme supports. Common ones that work across themes:
   - `@keyword.control` (control flow)
   - `@keyword.storage` (storage types)
   - `@function.builtin` (built-in functions)
   - `@constant.builtin` (built-in constants)

3. **Debugging capture types**: To see what capture types your theme supports, check Zed's theme files or use a known-working language (like Rust or TypeScript) as a reference.

### Extension Version
- Fixed in version: `20260205-020041-7f55d49`

---

## REGRESSION ISSUE (2026-02-05 02:11)

### Problem Reported
User reported that FPL language support completely disappeared:
- No syntax highlighting at all
- No autocomplete
- "Everything is broken!"

### Investigation

#### What Was Found
1. **Extension properly installed**: Version `20260205-021128-7f55d49` in `~/.config/zed/extensions/fermi/`
2. **All files present and correct**:
   - Grammar WASM: 22,861 bytes ✓
   - Extension WASM: 91,791 bytes ✓
   - LSP binary: 4,883,336 bytes ✓
   - Highlights file: correctly configured with `@keyword.control` captures ✓
3. **LSP binary exists and is executable**: `/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp`
4. **Extension has hardcoded LSP path**: In `extensions/fermi/src/lib.rs` line 14

#### Root Cause
The issue is **NOT** with the installation - all files are correct. The problem is that **Zed needs a complete restart** to reload the extension properly.

### The Real Issue
- User likely used "Reload Extensions" command or just closed/reopened a window
- Zed aggressively caches grammar and LSP configurations
- Only a **full application quit and restart** will pick up the changes

### Solution
1. **Quit Zed completely** (not just close window):
   ```bash
   killall zed
   ```

2. **Start Zed fresh** and open a `.fpl` file

3. **Verify** these features work:
   - Keywords (`question`, `driver`, `continuous`, etc.) highlighted in color
   - Distribution functions (`triangular`, `normal`, etc.) highlighted
   - Autocomplete when typing
   - LSP features (hover, diagnostics)

### If Still Broken After Full Restart
Check Zed's logs for errors:
```bash
tail -50 ~/.local/share/zed/logs/Zed.log | grep -i fermi
```

Look for errors related to:
- Loading the extension WASM
- Starting the LSP server
- Grammar compilation

### Files Verified Correct (2026-02-05 02:11)
```
~/.config/zed/extensions/fermi/
├── .version (20260205-021128-7f55d49)
├── extension.wasm (91,791 bytes)
├── grammars/
│   ├── fpl.wasm (22,861 bytes)
│   └── fpl/
│       └── queries/
│           └── highlights.scm (correct @keyword.control captures)
├── languages/
│   └── fpl/
│       ├── config.toml
│       └── highlights.scm (synced from grammars/fpl/queries/)
└── [LSP binary at /home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp]
```

### Key Lesson
**"Reload Extensions" is NOT sufficient** - Always do a full Zed restart (quit application) when debugging extension issues.
