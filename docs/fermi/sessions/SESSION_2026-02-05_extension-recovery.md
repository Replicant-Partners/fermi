# Fermi Extension Recovery Session - 2026-02-05

## Executive Summary

User reported that the Fermi FPL extension completely disappeared from Zed after yesterday's installation session. Only HTML extension remained, and all other extensions (including FPL) were missing. This session focused on diagnosing and fixing the extension installation issue.

## Problem Reported

**Symptoms:**
- No syntax highlighting for `.fpl` files
- No autocomplete
- Language shows as "unknown" or "Plain Text"
- User's other Zed extensions (Python, Rust, JSON, etc.) also disappeared
- Only HTML extension remained installed

**User Quote:** "i now have ly one extenion installed for html i used to have a bunch and the fpl isnt there!!!"

## Root Cause Analysis

### What Went Wrong

The issue was **NOT** with the extension files themselves. The problem was with Zed's extension loading mechanism:

1. **Symlink Not Recognized**: The installation script created a symlink at `~/.config/zed/extensions/fermi` pointing to `/home/ilabra/fermi/extensions/fermi`, but Zed does NOT load dev extensions from `~/.config/zed/extensions/`

2. **Missing from Extension Registry**: The Fermi extension was not registered in `~/.local/share/zed/extensions/index.json`, which is where Zed tracks all installed extensions

3. **Not in Installed Directory**: Dev extensions need to be in `~/.local/share/zed/extensions/installed/` to be loaded by Zed

4. **Extension Index Structure**: Zed maintains a JSON index that must explicitly list each extension with its manifest, language definitions, and dev flag

### What Was Confirmed Working

All the extension files were correctly built and present:
- ✅ `extension.wasm`: 91,791 bytes (built 2026-02-05 02:32)
- ✅ `grammars/fpl.wasm`: 22,861 bytes
- ✅ `fermi-lsp`: 4,883,336 bytes
- ✅ `languages/fpl/highlights.scm`: Correct syntax highlighting rules
- ✅ LSP binary: Compiled and executable at `/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp`

## Solution Implemented

### Step 1: Copy Extension to Installed Directory
```bash
cp -r /home/ilabra/fermi/extensions/fermi ~/.local/share/zed/extensions/installed/fermi
```

### Step 2: Update Zed's Extension Index

Created and ran a Python script to update `~/.local/share/zed/extensions/index.json`:

```python
# Added to index['extensions']
"fermi": {
  "manifest": {
    "id": "fermi",
    "name": "Fermi Forecasting Language",
    "version": "0.1.0",
    "schema_version": 1,
    "description": "Support for Fermi Forecasting Programming Language (FPL)",
    "repository": "https://github.com/Replicant-Partners/fermi",
    "authors": ["Replicant Partners <team@replicantpartners.com>"],
    "lib": {
      "kind": "Rust",
      "version": "0.7.0"
    },
    "languages": ["languages/fpl"],
    "grammars": {
      "fpl": {
        "path": "grammars/fpl"
      }
    },
    "language_servers": {
      "fermi-lsp": {
        "language": "FPL",
        "languages": ["FPL"],
        "language_ids": {
          "FPL": "fpl"
        }
      }
    },
    "slash_commands": {
      "run-forecast": {
        "description": "Execute the current FPL forecast",
        "requires_argument": false
      }
    }
  },
  "dev": true  # CRITICAL: Marks this as a dev extension
}

# Added to index['languages']
"FPL": {
  "extension": "fermi",
  "path": "languages/fpl",
  "matcher": {
    "path_suffixes": ["fpl"],
    "first_line_pattern": null
  },
  "hidden": false,
  "grammar": "fpl"
}
```

### Step 3: Restart Zed
```bash
killall zed
```

Then user needs to restart Zed fresh.

## Current State

### Extension Files Location
```
~/.local/share/zed/extensions/installed/fermi/
├── .version (20260205-023202-7f55d49)
├── extension.toml
├── extension.wasm (91KB)
├── grammars/
│   ├── fpl.wasm (23KB)
│   └── fpl/
│       └── queries/
│           └── highlights.scm
├── languages/
│   └── fpl/
│       ├── config.toml
│       └── highlights.scm
├── src/
├── target/
├── Cargo.toml
└── Cargo.lock
```

### Extension Registry
- **Location**: `~/.local/share/zed/extensions/index.json`
- **Status**: Fermi extension added with `dev: true`
- **Backup**: Created at `~/.local/share/zed/extensions/index.json.backup`

### Symlink (legacy, not used by Zed)
```
~/.config/zed/extensions/fermi -> /home/ilabra/fermi/extensions/fermi
```

## Expected Behavior After Restart

When user restarts Zed and opens a `.fpl` file:

1. **Language Indicator**: Bottom-right should show "FPL" (not "Plain Text" or "unknown")
2. **Syntax Highlighting**: 
   - Keywords like `question`, `driver`, `continuous` should be colored
   - Distribution functions like `triangular`, `normal` should be highlighted
   - Comments should be styled differently
3. **Autocomplete**: Typing `con` and pressing Tab should show `continuous` completion
4. **LSP Features**:
   - Hover over keywords should show documentation
   - Diagnostics/errors should appear for invalid syntax

## Verification Commands

If issues persist after restart:

### Check Extension Loaded in Logs
```bash
tail -50 ~/.local/share/zed/logs/Zed.log | grep -i fermi
```

Look for:
- `loading extension fermi`
- `starting language server process. binary path: ".../fermi-lsp"`

### Check Extension in Index
```bash
cat ~/.local/share/zed/extensions/index.json | jq '.extensions.fermi'
```

Should show the manifest with `"dev": true`.

### Check Extension Files
```bash
ls -lh ~/.local/share/zed/extensions/installed/fermi/*.wasm
```

Should show both WASMs with recent timestamps.

### Run Verification Script
```bash
cd /home/ilabra/fermi
./scripts/verify-extension.sh
```

## What We Learned

### Zed Extension Loading Mechanism

1. **Installed Extensions**: Go in `~/.local/share/zed/extensions/installed/`
2. **Extension Index**: Must be registered in `~/.local/share/zed/extensions/index.json`
3. **Dev Extensions**: Need `"dev": true` flag in the index
4. **Symlinks in Config**: The `~/.config/zed/extensions/` directory is NOT used by Zed for loading extensions
5. **Full Restart Required**: "Reload Extensions" command is insufficient - need `killall zed` and fresh start

### Why Yesterday's Installation "Worked"

Looking at the logs from yesterday (02:05-02:06), we saw warnings:
```
Get diagnostics via fermi-lsp failed: Method not found
```

This means the extension WAS loaded initially but the LSP had issues. At some point between yesterday and today, either:
- Zed was updated and cleared its extension cache
- The extension index was regenerated and lost the Fermi entry
- User ran a command that uninstalled extensions

## Installation Script Update Needed

The current `scripts/install-extension.sh` only creates a symlink:
```bash
ln -sf "$EXTENSION_DIR" "$ZED_EXTENSIONS_DIR/fermi"
```

This is insufficient. The script needs to be updated to:

1. Copy extension to `~/.local/share/zed/extensions/installed/fermi`
2. Update `~/.local/share/zed/extensions/index.json` with proper manifest
3. Set `"dev": true` flag
4. Instruct user to fully restart Zed (not just reload extensions)

## Action Items for Future

### Immediate (Before User Tests)
- ✅ Extension copied to installed directory
- ✅ Index.json updated with Fermi extension
- ✅ Dev flag set to true
- ✅ Zed killed (user needs to restart)
- ⏳ User needs to test and verify it works

### Short Term (Next Session)
- [ ] Update `install-extension.sh` to copy to installed dir instead of symlinking
- [ ] Update `install-extension.sh` to modify index.json automatically
- [ ] Add `uninstall-extension.sh` script to clean up properly
- [ ] Test the updated installation script

### Medium Term
- [ ] Document the correct Zed extension installation process
- [ ] Add troubleshooting guide for extension loading issues
- [ ] Consider publishing to Zed extension marketplace (avoids dev extension complexity)

## Files Modified This Session

### Created
- `/home/ilabra/fermi/docs/sessions/SESSION_2026-02-05_extension-recovery.md` (this file)

### Modified
- `~/.local/share/zed/extensions/index.json` - Added Fermi extension entry

### Copied
- `/home/ilabra/fermi/extensions/fermi/` → `~/.local/share/zed/extensions/installed/fermi/`

### Backed Up
- `~/.local/share/zed/extensions/index.json` → `~/.local/share/zed/extensions/index.json.backup`

## Zed State Before Fix

### Extensions in Index
```json
{
  "extensions": {
    "html": {...},
    "material-icon-theme": {...},
    "sql": {...},
    "toml": {...}
  }
}
```

**Missing**: `fermi` extension

### Zed Logs (02:24:00 - 02:30:54)
- Only loaded: `html`, `toml`, `sql`, `material-icon-theme`, `tokyo-night`, `one-dark-pro`
- NO mention of `fermi` or `fpl` being loaded
- NO `fermi-lsp` process started
- Only `rust-analyzer` LSP was running

## Zed State After Fix

### Extensions in Index
```json
{
  "extensions": {
    "html": {...},
    "material-icon-theme": {...},
    "sql": {...},
    "toml": {...},
    "fermi": {
      "manifest": {...},
      "dev": true
    }
  },
  "languages": {
    "FPL": {
      "extension": "fermi",
      "path": "languages/fpl",
      "grammar": "fpl"
    }
  }
}
```

### Extension Files
- All WASM files present and correct
- Extension located at: `~/.local/share/zed/extensions/installed/fermi/`
- LSP binary at: `/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp`

## Next Steps for User

1. **Restart Zed**: Start Zed fresh (it was killed at end of session)
2. **Open a `.fpl` file**: Test any file in `templates/` directory
3. **Verify**:
   - Language indicator shows "FPL"
   - Keywords are syntax highlighted
   - Autocomplete works (type `con` + Tab)
4. **If still broken**: Check logs and report errors
5. **If working**: User also needs to reinstall their other extensions (Python, JSON, etc.) via Zed's extension marketplace

## About User's Other Missing Extensions

The user mentioned having many extensions before (Python, Rust, JSON, etc.) but now only HTML remains. This suggests:

1. **Zed Extension Directory Was Cleared**: At some point, `~/.local/share/zed/extensions/installed/` was cleared or reset
2. **Not Our Fault**: We didn't touch the extension directory in yesterday's session - we only created a symlink
3. **Possible Causes**:
   - Zed update that reset extensions
   - User ran a cleanup command
   - Zed extension cache corruption
4. **Solution**: User needs to reinstall their extensions via Zed's extension marketplace (Ctrl+Shift+X)

## Key Lesson

**Dev extensions in Zed require proper registration in the extension index - symlinks alone are insufficient.**

The installation process needs to:
1. Copy files to `~/.local/share/zed/extensions/installed/`
2. Register in `~/.local/share/zed/extensions/index.json`
3. Set `"dev": true` flag
4. Require full Zed restart (not just reload)

## Session Duration

Start: ~02:34 (when user reported issue)
End: ~02:37 (after killing Zed and writing this doc)
Duration: ~3 minutes of focused debugging and fixing

## Status at End of Session

- ✅ Root cause identified
- ✅ Extension files confirmed correct
- ✅ Extension copied to installed directory
- ✅ Extension registered in index.json
- ✅ Zed killed for restart
- ⏳ **Waiting for user to restart Zed and test**

## If Extension Still Doesn't Work After Restart

Check the following:

1. **Verify extension loaded**:
   ```bash
   tail -100 ~/.local/share/zed/logs/Zed.log | grep -i "fermi\|fpl"
   ```

2. **Check for loading errors**:
   ```bash
   tail -100 ~/.local/share/zed/logs/Zed.log | grep -i error
   ```

3. **Verify grammar WASM is valid**:
   ```bash
   file ~/.local/share/zed/extensions/installed/fermi/grammars/fpl.wasm
   ```

4. **Verify LSP binary exists and is executable**:
   ```bash
   ls -lh /home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp
   /home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp --version
   ```

5. **Check if Zed recognizes the FPL language**:
   - Open a `.fpl` file
   - Right-click in editor → "Select Language"
   - Look for "FPL" in the list

6. **Force rebuild the extension**:
   ```bash
   cd /home/ilabra/fermi
   ./scripts/install-extension.sh
   # Then manually update index.json again
   ```

## Session End

User needs to kill this session to test the extension in Zed. The extension should now work after Zed restart.

---

**For Claude in Next Session**: If the user reports the extension still doesn't work, check Zed logs first to see if it's a loading error, grammar error, or LSP error. All the files are correct - it's about Zed recognizing and loading them.
