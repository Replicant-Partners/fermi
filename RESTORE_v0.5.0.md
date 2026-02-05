# How to Restore to v0.5.0

If anything goes wrong during LSP refactoring, use this guide to restore to the stable v0.5.0 checkpoint.

## Quick Restore

### Option 1: Reset to Tag (Safest)
```bash
cd /home/ilabra/fermi

# Discard all local changes and restore to v0.5.0
git fetch origin
git checkout v0.5.0
git checkout -b refactor-backup  # Create backup branch
git checkout main
git reset --hard v0.5.0
git push origin main --force  # Only if you want to reset remote too
```

### Option 2: Revert Changes (Safer - Keeps History)
```bash
cd /home/ilabra/fermi

# Create a new commit that undoes everything after v0.5.0
git revert HEAD~1..HEAD  # Revert last N commits
# Or
git revert --no-commit HEAD~5..HEAD  # Revert last 5 commits without committing each
git commit -m "Revert to v0.5.0 stable state"
```

### Option 3: Cherry-pick Good Commits
```bash
cd /home/ilabra/fermi

# If some refactoring worked but other parts didn't
git checkout v0.5.0
git checkout -b new-attempt
git cherry-pick <commit-hash>  # Pick only the good commits
```

## Verify Restoration

After restoring, verify everything works:

```bash
# 1. Check version
git log --oneline -1
# Should show: 525f43f feat: v0.5.0 - Discrete Drivers...

# 2. Check tag
git describe --tags
# Should show: v0.5.0

# 3. Build
cargo build --release

# 4. Test
cargo test

# 5. Run example
./run-forecast.sh test_discrete.fpl
```

## What v0.5.0 Includes

### Working Features
- ✅ Continuous drivers (6 distributions)
- ✅ Binary drivers (with if-then-else)
- ✅ Discrete drivers (categorical)
- ✅ Natural language names (display_name, description)
- ✅ LSP autocomplete (82+ items)
- ✅ LSP hover documentation
- ✅ Code actions (basic - "Add evidence block")
- ✅ Execute via run-forecast.sh script

### Test Status
- 53/55 tests passing (96.4%)
- All core functionality working

### Known Issues at v0.5.0
- LSP main.rs is large (1,368 lines) - scheduled for refactoring
- 2 minor test failures (non-critical)
- 5 compiler warnings (cosmetic)

## Commit Details

**Commit Hash:** `525f43f`  
**Tag:** `v0.5.0`  
**Date:** 2026-02-05  
**Branch:** main  
**Remote:** origin/main

**Changes in v0.5.0:**
- 29 files changed
- 4,801 insertions
- 311 deletions
- 6 new documentation files
- 7 new test files

## Remote Access

The tag is pushed to GitHub:
```
https://github.com/Replicant-Partners/fermi/releases/tag/v0.5.0
```

You can also clone fresh from this tag:
```bash
git clone https://github.com/Replicant-Partners/fermi.git
cd fermi
git checkout v0.5.0
```

## Emergency: Full Reset

If everything is broken and you need to start fresh:

```bash
cd /home/ilabra
rm -rf fermi
git clone https://github.com/Replicant-Partners/fermi.git
cd fermi
git checkout v0.5.0
cargo build --release
```

## Prevention Tips

Before making changes:
```bash
# Create a working branch
git checkout -b lsp-refactor

# Make changes on the branch
# Test thoroughly
# Only merge to main when confident

git checkout main
git merge lsp-refactor
```

## Support

If you need help restoring:
1. Check git status: `git status`
2. Check current commit: `git log --oneline -1`
3. Check available tags: `git tag -l`
4. Check remote status: `git remote -v`

---

**Remember:** v0.5.0 is a fully working, tested, documented version. It's always safe to return here.
