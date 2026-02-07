# Syntax Highlighting Regression Investigation
**Date:** 2026-02-05 02:22
**Status:** TROUBLESHOOTING - Extension broken after repo cleanup

## Problem Summary
After cleaning up the nested `/fermi/fermi/` directory structure, FPL language support completely stopped working in Zed:
- No syntax highlighting
- No autocomplete
- No language recognition for .fpl files
- Extension appears not to load at all

Previously working features before cleanup:
- LSP autocomplete ✓
- Some syntax highlighting (triangular, driver worked; question, continuous didn't)
- .fpl file type association ✓

## Timeline of Events

### What Was Working (Before Cleanup)
1. Extension installed in nested structure: `/fermi/fermi/extensions/fermi/`
2. LSP providing autocomplete
3. Basic language recognition
4. Partial syntax highlighting (some keywords worked)

### The Cleanup (Root of Problem)
1. Removed nested `/fermi/fermi/` directory
2. Kept only `/fermi/extensions/fermi/`
3. Updated install script to use new paths
4. Created backup branch: `backup-before-cleanup-20260205-012758`

### What Broke
- Complete loss of FPL language support
- Zed doesn't recognize .fpl files
- No highlighting, no autocomplete, no LSP

## Current State

### Files Verified Present
```bash
~/.config/zed/extensions/fermi/
├── .version (20260205-022203-7f55d49)
├── extension.wasm (91,791 bytes)
├── extension.toml (correct)
├── grammars/
│   ├── fpl.wasm (22,861 bytes)
│   └── fpl/
│       ├── src/
│       │   ├── parser.c
│       │   ├── node-types.json
│       │   └── grammar.json
│       └── queries/
│           └── highlights.scm (@keyword.control captures)
└── languages/
    └── fpl/
        ├── config.toml
        └── highlights.scm (synced)
```

### Symlink Structure
```bash
~/.config/zed/extensions/fermi -> /home/ilabra/fermi/extensions/fermi
```

### Extension Configuration
**extension.toml** (verified correct):
```toml
id = "fermi"
name = "Fermi Forecasting Language"
description = "Support for Fermi Forecasting Programming Language (FPL)"
version = "0.1.0"
schema_version = 1

[[languages]]
name = "FPL"
grammar = "fpl"
path_suffixes = ["fpl"]
line_comments = ["// "]
block_comment = ["/* ", " */"]

[[language_servers]]
name = "fermi-lsp"
languages = ["FPL"]

[grammars.fpl]
path = "grammars/fpl"
```

### LSP Configuration
**src/lib.rs** - Hardcoded path (line 14):
```rust
let lsp_path = "/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp";
```
LSP binary exists and is executable: ✓

### Git Status
```
D fermi  <-- deleted symlink or file (unclear what this was)
M extensions/fermi/extension.toml
M extensions/fermi/languages/fpl/highlights.scm
M extensions/fermi/README.md
... (other modified files)
```

## Actions Taken (Did Not Fix)

1. ✗ Full Zed restart (multiple times)
2. ✗ "Reload Extensions" command
3. ✗ Cleared `~/.cache/zed/`
4. ✗ Cleared `~/.local/share/zed/languages/`
5. ✗ Cleared `~/.local/share/zed/extensions/`
6. ✗ Cleared `~/.cache/zed/extensions/`
7. ✗ Reinstalled extension (version 20260205-022203-7f55d49)
8. ✗ Verified all files present
9. ✗ Changed highlight captures to `@keyword.control`

## Theories

### Theory 1: Deleted 'fermi' File
Git shows `D fermi` in status. This might have been:
- A symlink needed by Zed
- The main binary symlink
- Something the extension references

**Need to investigate:** What was this file and does the extension expect it?

### Theory 2: Extension Index Corruption
Zed's extension index might be corrupted and not recognizing the extension at all.

**Evidence:** Zed logs show NO errors about fermi/FPL at all - extension appears invisible.

### Theory 3: Schema or Structure Change
The nested directory structure might have been intentional or expected by Zed's extension loader.

### Theory 4: Extension WASM Not Loading
The extension.wasm might not be loading due to:
- Build issues
- Permission problems
- Missing dependencies in the WASM

## Zed Logs

Recent logs show NO mention of fermi/FPL:
```
2026-02-05T02:15:46+01:00 ERROR [project::context_server_store] Failed to create context server configuration...
2026-02-05T02:15:46+01:00 INFO  [lsp] starting language server process... discord-presence-lsp
2026-02-05T02:15:47+01:00 INFO  [lsp] starting language server process... rust-analyzer
2026-02-05T02:17:08+01:00 INFO  [extension_host] rebuilt extension index in 6.14362ms
```

**Key observation:** No fermi-lsp startup, no FPL language loading, no errors about the extension.

## Test File Created
```
/home/ilabra/fermi/test_basic.fpl
```

## Next Steps to Debug

### 1. Check Zed Extension List
In Zed:
- Cmd/Ctrl+Shift+P → "zed: extensions"
- Is "Fermi Forecasting Language" listed?
- Is it enabled?
- Does it show as loaded?

### 2. Check File Association
Open `test_basic.fpl`:
- What does bottom-right language indicator show?
- "Plain Text" = extension not loaded
- "FPL" = extension loaded but features broken

### 3. Check Zed Logs During File Open
```bash
tail -f ~/.local/share/zed/logs/Zed.log
```
Then open a .fpl file and watch for errors.

### 4. Verify Extension WASM Loads
Check if Zed can even load the extension:
```bash
# Look for extension loading errors
grep -i "fermi\|extension.*load\|wasm.*error" ~/.local/share/zed/logs/Zed.log
```

### 5. Compare with Working Extension
Look at a working extension structure:
```bash
ls -la ~/.config/zed/extensions/
ls -la ~/.local/share/zed/extensions/installed/
```

### 6. Check if Symlink is the Issue
Try copying instead of symlinking:
```bash
rm ~/.config/zed/extensions/fermi
cp -r /home/ilabra/fermi/extensions/fermi ~/.config/zed/extensions/fermi
```

### 7. Restore from Backup
As last resort, restore the working nested structure:
```bash
git checkout backup-before-cleanup-20260205-012758 -- fermi extensions/
```

## Critical Files to Check

1. `/home/ilabra/fermi/extensions/fermi/extension.wasm` - Does it load?
2. `/home/ilabra/fermi/extensions/fermi/grammars/fpl.wasm` - Is grammar valid?
3. What was the deleted `fermi` file in git status?

## Install Script Current Version
```bash
/home/ilabra/fermi/scripts/install-extension.sh
```
- Builds tree-sitter grammar ✓
- Builds extension WASM ✓
- Builds LSP ✓
- Syncs highlights.scm ✓
- Creates symlink ✓

## Build Commands Used
```bash
cd extensions/fermi/grammars/fpl
tree-sitter generate
tree-sitter build --wasm

cd extensions/fermi
cargo build --target wasm32-wasip1 --release

cd fermi-lsp
cargo build --release
```

All builds completed successfully without errors.

## Current Working Directory Structure
```
/home/ilabra/fermi/
├── extensions/
│   └── fermi/
│       ├── extension.toml
│       ├── extension.wasm
│       ├── grammars/
│       │   ├── fpl.wasm
│       │   └── fpl/ (full tree-sitter grammar)
│       ├── languages/
│       │   └── fpl/
│       │       ├── config.toml
│       │       └── highlights.scm
│       └── src/
│           └── lib.rs (extension code)
├── fermi-lsp/
│   └── target/release/fermi-lsp (exists, executable)
├── src/ (main fermi binary source)
├── templates/ (working .fpl files)
└── test_basic.fpl (new test file)
```

## Questions to Answer

1. **What was the deleted `fermi` file?**
   - Symlink? Binary? Something else?
   - Does the extension expect it?

2. **Why doesn't Zed see the extension at all?**
   - No logs, no errors, complete silence
   - Extension invisible to Zed

3. **Did the nested structure serve a purpose?**
   - Was `/fermi/fermi/` intentional?
   - Does Zed expect a specific structure?

4. **Is the symlink approach correct?**
   - Should we copy files instead?
   - Does Zed follow symlinks properly?

## Backup Information
- Branch: `backup-before-cleanup-20260205-012758`
- Contains working (partial) version
- Can restore if needed

## Session Context
User is testing in Zed now. Will report back with:
- Whether extension appears in extension list
- Whether .fpl files show as "FPL" or "Plain Text"
- Any error messages in Zed
- Log output when opening .fpl files
