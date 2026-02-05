# Session: Complete Extension Fix - 2026-02-05

## Starting Context

User reported that the extension hadn't been working since directory structure changes, and nothing had been pushed to GitHub for 6 hours. The working version from https://github.com/Replicant-Partners/fermi was from commit `7f55d49` but local had diverged significantly.

## Problems Found and Fixed

### 1. Wrong Installation Directory
**Problem**: Install script was installing to `~/.config/zed/extensions` but Zed looks in `~/.local/share/zed/extensions/installed/`

**Fix**: Updated `scripts/install-extension.sh`:
```bash
-ZED_EXTENSIONS_DIR="$HOME/.config/zed/extensions"
+ZED_EXTENSIONS_DIR="$HOME/.local/share/zed/extensions/installed"
```

### 2. Invalid extension.toml - Missing Grammar Section
**Problem**: Removed `[grammars.fpl]` section entirely, causing "no such grammar fpl" error

**Fix**: Added grammar section with repository reference:
```toml
[grammars.fpl]
repository = "https://github.com/Replicant-Partners/fermi"
commit = "9daa7c9"
```

Note: Even though grammar is bundled locally in `grammars/fpl.wasm`, Zed requires the grammar section in extension.toml.

### 3. Invalid languages/fpl/config.toml - Block Comment Format
**Problem**: `block_comment = { start = "/*", end = "*/" }` was causing TOML parse error:
```
TOML parse error at line 5, column 17
data did not match any variant of untagged enum BlockCommentConfigHelper
```

**Fix**: Removed `block_comment` line entirely (optional field). Final working config:
```toml
name = "FPL"
grammar = "fpl"
path_suffixes = ["fpl"]
line_comments = ["// "]
brackets = [
    { start = "{", end = "}", close = true, newline = true },
    { start = "[", end = "]", close = true, newline = true },
    { start = "(", end = ")", close = true, newline = true },
    { start = "\"", end = "\"", close = true, newline = false, not_in = ["comment", "string"] },
]
```

### 4. Invalid languages/fpl/indents.scm - Wrong Capture Names
**Problem**: Used `@indent.begin` and `@indent.end` but Zed expects `@indent`:
```
ERROR [language] missing required capture(s) in FPL indents TreeSitter query: indent
```

**Fix**: Simplified indents.scm:
```scm
; Indentation rules for FPL

("{" @indent
 "}" @indent)
```

### 5. Critical Bug in executor.rs - Statistics Unpacking
**Problem**: `calculate_statistics()` returns `(mean, stddev, p10, p50, p90)` but code was unpacking as `(mean, median, std_dev, min, max)`. This caused:
- `median` was getting `stddev` value
- `std_dev` was getting `p10`
- `min` was getting `p50` (median!)
- `max` was getting `p90`

**Fix**: Correct unpacking and calculate min/max from sorted samples:
```rust
// calculate_statistics returns: (mean, stddev, p10, p50, p90)
let (mean, std_dev, _p10, median, _p90) = calculate_statistics(&samples);

// Calculate additional percentiles
let mut sorted = samples.clone();
sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
let min = sorted[0];
let max = sorted[sorted.len() - 1];
```

### 6. Outdated Tests in executor.rs
**Problem**: Tests used `ForecastStmt` (doesn't exist) and missing required `DriverStmt` fields

**Fix**: Updated to use `QuestionStmt` and added all required fields:
```rust
Statement::Question(QuestionStmt {
    text: "test".to_string(),
    target_date: None,
    resolution_criteria: None,
}),
Statement::Driver(DriverStmt {
    name: "x".to_string(),
    driver_type: DriverType::Continuous,
    distribution: Some(Distribution::Triangular { ... }),
    probability: None,
    impact_multiplier: None,
    unit: None,
    rationale: None,
    constraints: vec![],
    evidence_refs: vec![],
}),
```

## Current State

### ✅ Working
- Syntax highlighting in Zed for `.fpl` files
- FPL language recognized (shows "FPL" in status bar)
- All executor tests pass
- Extension properly installed and symlinked
- All changes committed and pushed to GitHub

### ⚠️ Needs Full Zed Restart
- **Autocomplete/LSP**: Requires full Zed restart (quit application, not just reload extensions)
- Zed caches language configurations aggressively
- After full restart, LSP should start and autocomplete will work

## File Structure (Current)

```
/home/ilabra/fermi/extensions/fermi/
├── extension.toml          ✅ Fixed - has [grammars.fpl] section
├── extension.wasm          ✅ 91KB compiled
├── grammars/
│   ├── fpl.wasm           ✅ 23KB tree-sitter grammar
│   └── fpl/               ✅ Source grammar files
│       └── queries/
│           └── highlights.scm
├── languages/
│   └── fpl/
│       ├── config.toml    ✅ Fixed - no block_comment
│       ├── highlights.scm ✅ Synced from grammars
│       └── indents.scm    ✅ Fixed - uses @indent
└── src/
    └── lib.rs             ✅ Extension code with LSP path

Symlinked to:
~/.local/share/zed/extensions/installed/fermi -> /home/ilabra/fermi/extensions/fermi
```

## Version History

- `7f55d49` - Last working version on GitHub (6 hours ago)
- `aa46344` - Fixed install directory and extension.toml 
- `0c28d79` - Fixed executor.rs tests and statistics bug
- `9daa7c9` - **CURRENT** - Fixed extension config (indents, config.toml, extension.toml)

## LSP Configuration

**Extension provides LSP at**: `/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp` (4.7MB)

**Configured in** `extensions/fermi/src/lib.rs`:
```rust
fn language_server_command(
    &mut self,
    _language_server_id: &LanguageServerId,
    _worktree: &zed::Worktree,
) -> Result<zed::Command> {
    let lsp_path = "/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp";
    Ok(zed::Command {
        command: lsp_path.to_string(),
        args: vec![],
        env: Default::default(),
    })
}
```

**Registered in** `extensions/fermi/extension.toml`:
```toml
[language_servers.fermi-lsp]
language = "FPL"
languages = ["FPL"]

[language_servers.fermi-lsp.language_ids]
FPL = "fpl"
```

## How to Verify Everything Works

### 1. Check Installation
```bash
cat /home/ilabra/fermi/extensions/fermi/.version
# Should show: version=20260205-030622-9daa7c9 (or newer)

ls -la ~/.local/share/zed/extensions/installed/fermi
# Should be symlink to /home/ilabra/fermi/extensions/fermi
```

### 2. Full Zed Restart (REQUIRED for LSP)
```bash
# Quit Zed completely (not just close window)
# Then restart Zed
```

### 3. Open Test File
```bash
# In Zed, open:
/home/ilabra/fermi/test_basic.fpl
```

### 4. Verify Features
- **Status bar** shows "FPL" (bottom-right)
- **Syntax highlighting**:
  - `question` keyword colored
  - `driver` keyword colored
  - `continuous` type colored
  - `distribution:` property colored
  - `triangular()`, `normal()` function calls colored
- **Autocomplete**: Type `con` + Tab → should complete to `continuous`
- **LSP hover**: Hover over keywords for documentation

### 5. Check Logs If Issues
```bash
tail -50 ~/.local/share/zed/logs/Zed.log | grep -i "fermi\|fpl"
```

Look for:
- ✅ `INFO [lsp] starting language server process. binary path: ".../fermi-lsp"`
- ❌ `ERROR` messages about extension loading
- ❌ `failed to load language FPL`

## Key Lessons Learned

1. **Zed extension directory**: Must be `~/.local/share/zed/extensions/installed/` not `~/.config/zed/extensions/`

2. **Grammar registration**: Even with bundled grammar WASM, need `[grammars.fpl]` section with `repository` and `commit` in extension.toml

3. **Block comment format**: Optional field, better to omit if not sure of exact format Zed expects

4. **Indents query**: Use `@indent` not `@indent.begin`/`@indent.end`

5. **Full restart required**: Zed aggressively caches language configurations - "Reload Extensions" is NOT enough, need full quit/restart

6. **Extension symlinks**: Work great for development, changes are immediately visible (after restart)

7. **Statistics function signatures**: Always check return types carefully - the executor bug was subtle but broke all Monte Carlo results

## Next Steps

After full Zed restart:
1. ✅ Verify syntax highlighting works
2. ✅ Verify autocomplete works
3. ✅ Test LSP features (hover, diagnostics)
4. 📝 Update documentation with installation instructions
5. 🚀 Consider packaging extension for Zed extension marketplace

## Troubleshooting

### Syntax Highlighting Not Working
- Check `tail ~/.local/share/zed/logs/Zed.log | grep "FPL"`
- Look for "failed to load language FPL" errors
- Verify `grammars/fpl.wasm` exists (23KB)
- Verify `languages/fpl/highlights.scm` exists (2.3KB)

### LSP/Autocomplete Not Working
- **First**: Do full Zed restart (quit application completely)
- Check LSP binary exists: `ls -lh /home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp`
- Check logs: `tail ~/.local/share/zed/logs/Zed.log | grep "fermi-lsp"`
- Should see: "starting language server process. binary path: ..."

### Extension Won't Load
- Check extension.toml syntax: `cd extensions/fermi && cat extension.toml`
- Verify no TOML parse errors in logs
- Rebuild extension: `./scripts/install-extension.sh`

### Tests Failing
```bash
cd /home/ilabra/fermi
cargo test --lib executor
```
All 3 tests should pass.

## Files Modified This Session

1. `scripts/install-extension.sh` - Fixed installation directory
2. `extensions/fermi/extension.toml` - Added [grammars.fpl] section
3. `extensions/fermi/languages/fpl/config.toml` - Removed invalid block_comment
4. `extensions/fermi/languages/fpl/indents.scm` - Fixed capture names
5. `src/executor.rs` - Fixed statistics bug and updated tests

## Git Commits This Session

```bash
aa46344 - Fix Zed extension installation and configuration
0c28d79 - Fix executor.rs tests and statistics calculation bug
9daa7c9 - Fix Zed extension configuration to work properly
```

All pushed to: https://github.com/Replicant-Partners/fermi

---

**Session Status**: Extension working with syntax highlighting. LSP requires full Zed restart to activate.

**Action Required**: User needs to quit Zed completely and restart for autocomplete/LSP to work.
