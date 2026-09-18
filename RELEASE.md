# Release Process

This document describes how to release new versions of `ograf-core` to both GitHub and crates.io.

## Semantic Versioning (SemVer)

`ograf-core` follows [Semantic Versioning 2.0.0](https://semver.org/):

```
MAJOR.MINOR.PATCH  (e.g., 0.1.0, 1.2.3, 2.0.0)
```

### Version Components

- **MAJOR** (X.0.0) — Incompatible API changes
- **MINOR** (0.X.0) — New functionality, backwards-compatible
- **PATCH** (0.0.X) — Bug fixes, backwards-compatible

### When to Bump Which Version

#### MAJOR (Breaking Changes)
Bump when you make **incompatible** API changes:
- Remove public functions, types, or modules
- Change function signatures (parameters, return types)
- Rename public items
- Change behavior in ways that break existing code

**Examples:**
- `0.1.0` → `1.0.0` — First stable release
- `1.2.3` → `2.0.0` — Removed `AllowAllAccessControl::new()`
- `2.1.0` → `3.0.0` — Changed `build_router()` signature

#### MINOR (New Features)
Bump when you add **backwards-compatible** functionality:
- Add new public functions, types, or modules
- Add new optional parameters (with defaults)
- Add new trait methods with default implementations
- Add new fields to non-exhaustive structs

**Examples:**
- `0.1.0` → `0.2.0` — Added GraphQL support
- `1.0.0` → `1.1.0` — Added new `CustomAccessControl` trait impl
- `1.1.0` → `1.2.0` — Added metrics endpoint

#### PATCH (Bug Fixes)
Bump when you make **backwards-compatible** bug fixes:
- Fix bugs without changing public API
- Update documentation
- Improve error messages
- Performance improvements
- Internal refactoring

**Examples:**
- `0.1.0` → `0.1.1` — Fixed memory leak in renderer registry
- `1.0.0` → `1.0.1` — Fixed panic on malformed WebSocket message
- `1.2.0` → `1.2.1` — Updated README examples

### Pre-1.0 Versions (0.x.x)

Before `1.0.0`, breaking changes are allowed in **MINOR** versions:
- `0.1.0` → `0.2.0` can include breaking changes
- Reserve PATCH for critical bug fixes only

Once you hit `1.0.0`, you commit to SemVer stability.

## Release Checklist

### 1. Prepare the Release

- [ ] All changes committed and pushed to `main`
- [ ] All tests passing (`cargo test`)
- [ ] Code compiles without warnings (`cargo clippy`)
- [ ] Documentation up-to-date (README.md, SPEC_COMPLIANCE.md)
- [ ] CHANGELOG.md updated (if you maintain one)

### 2. Bump Version

Edit `Cargo.toml`:

```toml
[package]
version = "0.2.0"  # ← Update this
```

Commit the version bump:

```bash
git add Cargo.toml
git commit -m "chore: bump version to 0.2.0"
```

### 3. Create Git Tag

```bash
# Annotated tag with message
git tag -a v0.2.0 -m "Release v0.2.0 - Add GraphQL support"

# Verify tag created
git tag -l
```

### 4. Push to GitHub

```bash
# Push commits
git push origin main

# Push tag
git push origin v0.2.0
```

### 5. Publish to crates.io

```bash
# Dry-run first (verify packaging)
cargo publish --dry-run

# If dry-run succeeds, publish
cargo publish
```

Wait 2-5 minutes for the crate to appear on crates.io.

### 6. Verify Publication

**Check crates.io:**
```bash
xdg-open https://crates.io/crates/ograf-core
```

**Check docs.rs** (builds automatically in 5-10 minutes):
```bash
xdg-open https://docs.rs/ograf-core
```

**Test installation:**
```bash
cargo new test-project
cd test-project
cargo add ograf-core@0.2.0
cargo build
```

### 7. Create GitHub Release (Optional)

1. Go to https://github.com/HeineFro/ograf-core/releases
2. Click "Draft a new release"
3. Select tag `v0.2.0`
4. Title: `v0.2.0 - Add GraphQL Support`
5. Description: Release notes (what's new, breaking changes, fixes)
6. Click "Publish release"

Or via CLI:

```bash
gh release create v0.2.0 \
  --title "v0.2.0 - Add GraphQL Support" \
  --notes "## What's New
- Added GraphQL endpoint
- Improved error messages

## Breaking Changes
None

## Bug Fixes
- Fixed renderer timeout issue"
```

## Quick Reference

### Patch Release (Bug Fix)

```bash
# 1. Update version
vim Cargo.toml  # 0.1.0 → 0.1.1

# 2. Commit, tag, push
git add Cargo.toml
git commit -m "chore: bump version to 0.1.1"
git tag -a v0.1.1 -m "Release v0.1.1 - Bug fixes"
git push origin main
git push origin v0.1.1

# 3. Publish
cargo publish
```

### Minor Release (New Feature)

```bash
# 1. Update version
vim Cargo.toml  # 0.1.1 → 0.2.0

# 2. Commit, tag, push
git add Cargo.toml
git commit -m "chore: bump version to 0.2.0"
git tag -a v0.2.0 -m "Release v0.2.0 - Add metrics endpoint"
git push origin main
git push origin v0.2.0

# 3. Publish
cargo publish
```

### Major Release (Breaking Change)

```bash
# 1. Update version
vim Cargo.toml  # 0.2.0 → 1.0.0

# 2. Commit, tag, push
git add Cargo.toml
git commit -m "chore: bump version to 1.0.0"
git tag -a v1.0.0 -m "Release v1.0.0 - First stable release"
git push origin main
git push origin v1.0.0

# 3. Publish
cargo publish
```

## Troubleshooting

### `cargo publish` fails with "crate already exists"

You've already published this version. Bump the version number in `Cargo.toml` and try again.

### `cargo publish` fails with "authentication failed"

Run `cargo login` again and enter a fresh API token from https://crates.io/settings/tokens

### Tag already exists

Delete the tag locally and remotely:

```bash
git tag -d v0.2.0                    # Delete local
git push origin :refs/tags/v0.2.0   # Delete remote
```

Then recreate it.

### Pushed wrong version

If you haven't run `cargo publish` yet:

1. Delete the tag (see above)
2. Amend the commit: `git commit --amend`
3. Force-push: `git push origin main --force-with-lease`
4. Recreate tag and push

**If you already published to crates.io:** You cannot unpublish. You must yank the version and publish a new patch:

```bash
cargo yank --vers 0.2.0              # Yank bad version
# Fix, bump to 0.2.1, and publish
```

## Best Practices

1. **Never yank a version unless it's seriously broken** (security issue, completely unusable)
2. **Always test with `--dry-run` first**
3. **Write good release notes** in git tags and GitHub releases
4. **Keep a CHANGELOG.md** for user-facing changes
5. **Use conventional commits** for clear git history:
   - `feat:` — New feature (MINOR bump)
   - `fix:` — Bug fix (PATCH bump)
   - `docs:` — Documentation only
   - `chore:` — Maintenance (version bumps, deps)
   - `refactor:` — Code change (no behavior change)
   - `BREAKING CHANGE:` — Breaking change (MAJOR bump)

## Automation (Future)

Consider setting up GitHub Actions to:
- Run tests on every push
- Automatically publish to crates.io on tag push
- Generate CHANGELOG from conventional commits
- Create GitHub releases automatically

Example workflow: https://github.com/actions-rs/cargo
