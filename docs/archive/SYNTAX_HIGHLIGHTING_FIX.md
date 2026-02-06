# Syntax Highlighting Fix - Root Cause Found

## Date: 2026-02-05

## The Real Problem

The syntax highlighting issue where `question` and `continuous` keywords were NOT highlighting was **NOT** a grammar or theme issue.

### Root Cause

Zed was reading **stale cached files** from a previous installation located at:
```
~/.local/share/zed/extensions/installed/fermi/grammars/fpl/extensions/fermi/languages/fpl/highlights.scm
```

This old file contained:
- Only 3 keywords: `forecast`, `driver`, `estimate` 
- Did NOT have `question`, `continuous`, or most other keywords
- Was from an earlier version of the FPL grammar

### Why Some Keywords Highlighted

Keywords like `triangular` and `driver` appeared to work because:
- `triangular` was in the old cache file as `@function.builtin`
- `driver` was in the old cache file as `@keyword`
- These happened to match entries that existed in the stale cache

### Why Others Didn't

Keywords like `question` and `continuous` did NOT highlight because:
- They weren't in the old cached file at all
- Zed was preferring the cached version over the fresh installation

## The Solution

1. **Removed ALL cached extension files**:
   ```bash
   rm -rf ~/.local/share/zed/extensions/installed/fermi
   rm -rf ~/.config/zed/extensions/fermi
   rm -rf ~/.cache/zed/*
   ```

2. **Reinstalled the extension cleanly**:
   ```bash
   cd /home/ilabra/fermi
   bash scripts/install-extension.sh
   ```

3. **Restart Zed completely** (not just "reload extensions")

## Verification

After clean installation, the correct highlights file is now at:
- `/home/ilabra/.config/zed/extensions/fermi/grammars/fpl/queries/highlights.scm`
- `/home/ilabra/.config/zed/extensions/fermi/languages/fpl/highlights.scm`

Both contain the full, up-to-date keyword list with proper `@keyword.control` captures.

## Important Notes

1. **Cache clearing is critical**: When debugging Zed extensions, ALWAYS clear:
   - `~/.local/share/zed/extensions/`
   - `~/.config/zed/extensions/`
   - `~/.cache/zed/`

2. **Full restart required**: The "zed: reload extensions" command does NOT always clear all caches. You must close and reopen Zed completely.

3. **Check for stale paths**: Use this command to find all extension-related files:
   ```bash
   find ~/.local/share/zed ~/.config/zed ~/.cache/zed -name "*fermi*" -o -name "*fpl*" 2>/dev/null
   ```

## Current Capture Types Used

Our highlights.scm now correctly uses:
- `@keyword.control` - For all statement and control keywords (question, driver, continuous, etc.)
- `@function.builtin` - For distribution functions (triangular, normal, etc.)
- `@operator` - For mathematical operators
- `@punctuation.delimiter` - For punctuation
- `@string`, `@number` - For literals
- `@comment` - For comments
- `@variable.parameter` - For identifiers in specific contexts
- `@variable` - For general identifiers

All of these are officially supported Zed capture types and work across all themes.

## Extension Version

Fixed in version: `20260205-020651-7f55d49`

## What Was Misleading

The previous debugging sessions were thorough and technically correct:
- The grammar.js was correct
- The highlights.scm was correct  
- The tree-sitter queries were correct
- The capture types were valid

The issue was simply that Zed was reading old cached files instead of the new ones, making it appear as if the queries weren't working.
