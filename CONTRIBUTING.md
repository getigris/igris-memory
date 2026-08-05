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

CI also runs a mutation-testing gate (`cargo mutants --in-diff diff.txt --in-place`) that fails a PR if it introduces code with no test capable of detecting a behavioral mutation. Check your own diff before pushing with the fast, scoped local equivalent (a few minutes, vs. CI's much longer full-diff run):

```bash
cargo mutants --file <path>
```

`.cargo/mutants.toml` controls `exclude_globs`, for files with nothing meaningful to mutate (e.g. `src/tui/ui.rs`'s rendering code). If you find a mutant you believe is genuinely equivalent (no test could ever observe a behavioral difference), prefer restructuring the code to remove the ambiguous operator (e.g. `.min()`/`.max()` instead of `if a < b {...} else {...}` when the values are guaranteed distinct) over skipping it. Reserve `#[cfg_attr(test, mutants::skip)]` — never a bare `#[mutants::skip]`, since `mutants` is a dev-only dependency and a bare attribute breaks `cargo build --release` — for cases where no such restructuring is possible, e.g. a process entrypoint like `main()` or an idempotent migration-version gate. Note that `cargo mutants` exit code 3 (a run timed out) fails the CI step exactly like exit code 2 (a mutant survived) — a test that can hang under a plausible mutation is just as gate-blocking as one that can't catch it.

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
- Run `cargo mutants --file <path>` on the file(s) you changed to catch gaps the CI mutation-testing gate would otherwise flag
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
