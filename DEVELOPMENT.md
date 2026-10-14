# Rust Development Rules

Code quality, safety, security, and reliability — no exceptions.

---

## Prerequisites

### cargo-llvm-cov (Required)

`cargo-llvm-cov` and `cargo-nextest` are **mandatory**. The pre-push hook and `smoke-test.sh` hard-fail without llvm-cov; coverage uses `llvm-cov nextest`.

```bash
cargo +nightly install cargo-llvm-cov
# or
brew install cargo-llvm-cov
```

Verify: `cargo llvm-cov --version`

See: https://github.com/taiki-e/cargo-llvm-cov#installation

### TruffleHog (Required)

TruffleHog is **mandatory**. The pre-push hook and `smoke-test.sh` hard-fail without it.

```bash
brew install trufflehog
# or
curl -sSfL https://raw.githubusercontent.com/trufflesecurity/trufflehog/main/scripts/install.sh | sh -s -- -b /usr/local/bin
```

Verify: `trufflehog --version`

See: https://github.com/trufflesecurity/trufflehog#installation

---

## Tooling

### Rust Toolchain (`rust-toolchain.toml`)

All services pin to nightly with these components:

```toml
channel = "nightly"
components = ["rustfmt", "clippy", "miri"]
profile = "minimal"
```

### Clippy (`clippy.toml` + `Cargo.toml [lints.clippy]`)

Unwrap/expect are denied via `[lints.clippy]` in `Cargo.toml` — this only fires on user-written code, not external macro expansions:

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
```

Test code is exempt via `clippy.toml`:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
```

In test code, prefer `mae::testing::must::{Must, MustExpect}` over raw `.unwrap()` / `.expect()`.

### Miri

Detects undefined behavior. Run `--lib` only — integration tests requiring docker/postgres cannot run under Miri.

```bash
cargo +nightly miri test --lib
```

Skip tests incompatible with Miri:

```rust
#[cfg_attr(miri, ignore)]
```

### cargo-deny (`deny.toml` + `deny.exceptions.toml`)

Enforces workspace-wide dependency policy:

- Allowed licenses: `MIT`, `Apache-2.0`, `BSD-3-Clause`, `ISC`, `MPL-2.0`
- Blocks known vulnerabilities (RUSTSEC advisories) and yanked crates
- Restricts sources to `crates.io` and Mae-Technologies GitHub repos

Add exceptions to `deny.exceptions.toml` with justification:

```toml
exceptions = [
  { crate = "some-crate", allow = ["BSD-2-Clause"] },
]
```

### Test Utilities (`mae::testing`)

Use `mae::testing::must::{Must, MustExpect}` for fallible operations in tests instead of `.unwrap()` / `.expect()`. These traits are enabled via the `test-utils` feature:

```toml
[dev-dependencies]
mae = { version = "...", features = ["test-utils"] }
```

---

## Pre-Push Checks (Mandatory)

The pre-push hook runs `scripts/smoke-test.sh` before every push. All checks must pass:

```bash
cargo +nightly fmt -- --check
unset MAE_TESTCONTAINERS
cargo +nightly llvm-cov nextest --no-pager --no-tests=warn --fail-under-lines <threshold>
cargo +nightly clippy -- -D warnings
cargo deny check
trufflehog git file://. --since-commit HEAD~1 --only-verified --fail
```

The hook runs that suite once. Docker e2e is not part of the hook. Run `bash scripts/int-test.sh` when a change needs the container tests.

**Coverage:** line coverage of `src/` from the **`tests/` suite**, not inline `#[cfg(test)]`. Put new unit tests in `tests/unit_coverage.rs`, `tests/coverage/`, or `tests/unit/`. Docker-gated tests skip during llvm-cov so local and GitHub Actions measure the same number. Template `coverage_threshold` is **85** (healthy 80–90 band). Existing crates may override downward in `.ci/ci_env.toml` with a comment to raise back to 85. `coverage_threshold = 0` skips llvm-cov (`mae_macros`).

**Remote CI** (`.github/workflows/ci.yml`, `ubuntu-latest`): fmt, that llvm-cov command, clippy, deny, TruffleHog. No Helm’s Deep / in-cluster CI Postgres/Neo4j.

**Skip options (use sparingly):**

- `SKIP_GUARD=1` — bypass the entire smoke-test gate
- `SKIP_TESTS=1` — skip the test step only

---

## Git Hygiene

- All branches: `{type}/{issue}-{short-kebab}` — e.g. `bugfix/42-fix-login`
- Branch from `main` (or `sandbox` when directed by the manager):
  ```bash
  git fetch origin main
  git checkout -b {type}/{issue}-{kebab} origin/main
  ```
- Always rebase, never merge from main:
  ```bash
  git fetch origin main && git rebase origin/main && git push --force-with-lease
  ```
- **Never push to `main`** — all changes go through PRs. Carter is the only one who merges to main.

---

## Pull Requests

Every PR must include in its body:

```markdown
## Summary
[What this PR does]

## Changes
- [List of changes]

Closes #<issue>

## Test Results
- cargo +nightly fmt ✅/❌
- cargo +nightly clippy ✅/❌
- cargo miri test --lib ✅/❌
- cargo test --features integration-testing ✅/❌
- cargo deny check ✅/❌
- cargo llvm-cov ✅/❌ (XX%)
- scripts/smoke-test.sh ✅/❌
```

Reply to **every** review comment thread before pushing fixes. Unreplied threads = unfinished work.

---

## CI Pipeline (`ci.yml`)

Runs on PRs targeting `main` or `production` using self-hosted `mae-runner` ARC nodes.

| Job | Purpose |
|-----|---------|
| `config` | Reads `configuration/base.yaml` + `configuration/test.yaml` and exports service credentials as job outputs |
| `integrity` | Runs `scripts/smoke-test.sh` (format, clippy, coverage, deny, TruffleHog) |
| `integration` | Spins up Postgres, Neo4j, RabbitMQ, Redis; runs `scripts/int-test.sh --ci` |

**`.ci/ci_env.toml`** configures the test runner:

```toml
coverage_threshold = 65
engine = "nextest"
env = [
  "MAE_TESTCONTAINERS=1",
]
flags = ["--no-pager", "--features", "integration-testing", "--all-features", "--run-ignored", "all"]
```

In CI, `MAE_TESTCONTAINERS` is unset by `int-test.sh --ci` so docker-gated tests skip automatically.

---

## Local Dev with Docker Compose Watch

Start the dev environment:

```bash
docker compose up --watch
```

The dev container runs `scripts/dev-boot.sh` → `cargo fetch` → `cargo watch` → `scripts/dev-run.sh`.

`dev-run.sh` runs `scripts/migrate.sh` then `cargo run`. If migrations fail, the watcher stays alive and retries on the next file change.

Watch targets: `src/`, `Cargo.toml`, `migrations/`, `../lib` (shared lib when mounted).

**Expected cycle times:**

| Change | Time |
|--------|------|
| Source file | ~5–30s (incremental) |
| `Cargo.toml` | ~5–30s (incremental) |
| `Cargo.lock` (new dep) | ~2–5 min (image rebuild) |

---

## Syncing from Template

```bash
# First-time or update
export RUST_TEMPLATE_DIR=/path/to/rust_template
cd /path/to/my-service
bash $RUST_TEMPLATE_DIR/sync_rust_template.sh --force --private "1000482371 ONTARIO CORPORATION"
```

What gets synced: config files, workflow, `configuration/` YAMLs, `.ci/`, `scripts/`, pre-push hook, DEVELOPMENT.md, `[lints.clippy]`, LICENSE, mae dep bump.

Use `--lib` for library crates (skips `Dockerfile.*`). Use `--skip-mae-bump` to skip the mae version bump.

---

## Adding Dependencies

All new dependencies must pass `cargo deny check`. Add exceptions with justification to `deny.exceptions.toml`.

---

## Local Checks Quick Reference

```bash
cargo +nightly fmt -- --check
cargo +nightly clippy --all-targets --all-features -- -D warnings
cargo +nightly miri test --lib
cargo deny check
bash scripts/smoke-test.sh
MAE_TESTCONTAINERS=1 bash scripts/int-test.sh
```
