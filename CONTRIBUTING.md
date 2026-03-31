# Contributing to Igris Memory

Thanks for your interest in contributing to Igris Memory! This document outlines the process and guidelines.

## License

Igris Memory is licensed under the [Elastic License 2.0 (ELv2)](LICENSE). By submitting a contribution, you agree that your work will be licensed under the same terms.

## Getting Started

1. Fork the repository
2. Clone your fork and create a branch from `main`:
   ```bash
   git checkout -b feat/my-feature main
   ```
3. Set up the dev environment:
   ```bash
   cargo build
   cargo test
   git config core.hooksPath .githooks
   ```

## Branch Naming

Use prefixes to categorize your work:

| Prefix | Purpose |
|--------|---------|
| `feat/` | New features |
| `fix/` | Bug fixes |
| `docs/` | Documentation only |
| `refactor/` | Code restructuring without behavior change |
| `test/` | Adding or fixing tests |
| `ci/` | CI/CD changes |

## Making Changes

### Code Quality

All contributions must pass before merge:

```bash
cargo fmt --check              # Formatting
cargo clippy -- -D warnings    # Linting (zero warnings)
cargo test                     # All tests pass
```

Pre-commit hooks enforce this automatically if configured via `git config core.hooksPath .githooks`.

### Commit Messages

Write clear, descriptive commit messages:

```
<type>: <short summary>

<optional body explaining why, not what>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `ci`, `chore`

Examples:
- `feat: add rate limiting to HTTP server`
- `fix: topic_key upsert creating duplicates on concurrent writes`
- `docs: clarify SQLCipher setup instructions`

### Tests

- **Bug fixes**: Include a test that reproduces the bug and passes with the fix.
- **New features**: Include tests covering the main path and edge cases.
- **Refactors**: Existing tests must continue to pass without modification.

Run a specific test:
```bash
cargo test <test_name>
cargo test --test db_test
```

## Pull Requests

### Before Opening a PR

- Rebase on latest `main` to avoid conflicts
- Ensure all checks pass locally (`fmt`, `clippy`, `test`)
- Keep PRs focused — one feature or fix per PR

### PR Description

Use this template:

```markdown
## Summary
Brief description of what this PR does and why.

## Changes
- Bullet points of specific changes

## Testing
How you tested these changes.

## Related Issues
Closes #123 (if applicable)
```

### Review Process

- All PRs require approval from [@adiazblanco](https://github.com/adiazblanco) (code owner)
- CI must pass before merge
- Address review feedback with new commits (don't force-push during review)
- PRs are merged via squash-and-merge to keep `main` history clean

## Reporting Bugs

Open an issue with:

1. **What happened** — describe the unexpected behavior
2. **What you expected** — describe the correct behavior
3. **How to reproduce** — minimal steps, commands, or a test case
4. **Environment** — OS, Rust version (`rustc --version`), igmem version (`igmem --version`)

## Feature Requests

Open an issue with:

1. **Problem** — what limitation you're hitting
2. **Proposed solution** — how you'd like it to work
3. **Alternatives considered** — other approaches you thought about

Discussion before implementation avoids wasted effort. Wait for feedback before starting a PR for significant features.

## Architecture Notes

Before contributing, read [AGENTS.md](AGENTS.md) for the full module map and design patterns. Key things to know:

- **stdout is reserved** for MCP stdio transport — use `eprintln!` or `tracing` (which goes to stderr)
- **Thread safety** — database is behind `Arc<Mutex<Database>>`
- **Soft deletes** — never hard-delete in queries; use `deleted_at` filtering
- **FTS5 sync** — triggers handle it; don't manually update `observations_fts`

## Code of Conduct

Be respectful, constructive, and collaborative. We're all here to build something useful.
