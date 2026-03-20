# Rust Development Rules

Focus on code quality, safety, security, and reliability.

This project enforces Rust best practices using nightly tooling, aggressive linting, undefined behavior detection, dependency auditing (with `cargo-deny`), comprehensive CI, and Licensing.

## Prerequisites

### cargo-llvm-cov (Required)

`cargo-llvm-cov` is **mandatory** for this project. The pre-push hook and `smoke-test.sh` will **hard-fail** if it is not installed.

**Install via cargo:**

```bash
cargo +nightly install cargo-llvm-cov
```

**Install via Homebrew (macOS/Linux):**

```bash
brew install cargo-llvm-cov
```

**Verify installation:**

```bash
cargo llvm-cov --version
```

For more options, see: https://github.com/taiki-e/cargo-llvm-cov#installation

> ⚠️ Without `cargo-llvm-cov` installed, you will **not** be able to push commits. This is intentional — coverage checking is a hard requirement, not optional.

### TruffleHog (Required)

TruffleHog is **mandatory** for this project. The pre-push hook and `smoke-test.sh` will **hard-fail** if it is not installed.

**Install via Homebrew (macOS/Linux):**

```bash
brew install trufflehog
```

**Install via install script (Linux):**

```bash
curl -sSfL https://raw.githubusercontent.com/trufflesecurity/trufflehog/main/scripts/install.sh | sh -s -- -b /usr/local/bin
```

**Verify installation:**

```bash
trufflehog --version
```

For more options, see: https://github.com/trufflesecurity/trufflehog#installation

> ⚠️ Without `trufflehog` installed, you will **not** be able to push commits. This is intentional — secret scanning is a hard requirement, not optional.

## Key Features

- **Nightly Rust toolchain** with `rustfmt`, `clippy`, and `miri` (`rust-toolchain.toml`)
- **Strict Clippy rules** banning `unwrap()`, `expect()`, and requiring documentation on `unsafe` blocks
- **Miri** for detecting undefined behavior in tests
- **cargo-deny** for license compliance, vulnerability scanning, yanked crate blocking, and source restrictions
- **Opinionated rustfmt** configuration for consistent code style
- **Optimized GitHub Actions CI** that runs checks only on changed `.rs` files
- **Compile-time denials** for common anti-patterns (configured in `lib.rs`)
- **Git pre-push hook** that automatically runs formatting, tests, Miri, Clippy, and deny checks before pushing commits
- **Git pre-commit hook** that checks for latest rust_template changes
- **Comprehensive .gitignore** to exclude build artifacts, temporary files, environment files, Docker outputs, and IDE/editor settings
- **License support** via `--private <name>` flag (creates proprietary LICENSE file) **or** `--name <name>` flag (creates MIT LICENSE file)
  - optionally set `RUST_OWNER` to use as the `<name>`

## Tooling Details

### Rust Toolchain (`rust-toolchain.toml`)

Pins the project to the latest `nightly` channel with essential components:

```toml
channel = "nightly"
components = ["rustfmt", "clippy", "miri"]
profile = "minimal"
```

### rustfmt (`rustfmt.toml`)

Enforces consistent formatting:

- 4-space indentation
- 100 character line width
- Trailing commas where possible
- Unix newlines
- Reordered imports

### Clippy (`clippy.toml` + `lib.rs`)

Bans footguns via `disallowed-methods`:

- `Option::unwrap` / `expect`
- `Result::unwrap` / `expect`

Additional compile-time denials in `lib.rs`:

```rust
#![deny(clippy::disallowed_methods)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]
```

### Miri

Miri interprets Rust MIR to catch undefined behavior (UB) such as invalid memory access, uninitialized reads, and more.

The CI runs:

```bash
cargo +nightly miri test --all-targets --all-features
```

**Resources**:
- Official repository: https://github.com/rust-lang/miri
- Documentation & usage: https://github.com/rust-lang/miri/blob/master/README.md
- Undefined Behavior in Rust: https://doc.rust-lang.org/reference/behavior-considered-undefined.html

### Test Utilities (`mae::testing`)

The template no longer ships a local `tests/must.rs` helper.

Use the test helper utilities provided by the `mae` library (`mae::testing`) instead.
This keeps test helper behavior centralized and avoids template-specific duplication.

### cargo-deny (`deny.toml` & `deny.exceptions.toml`)

Enforces workspace-wide dependency policy:

- Allowed licenses: `MIT`, `Apache-2.0`, `BSD-3-Clause`, `ISC`, `MPL-2.0`
- Blocks known vulnerabilities and yanked crates
- Restricts sources to `crates.io` and your GitHub repositories

[read the `cargo-deny` docs](https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html)

### GitHub Actions CI (`.github/workflows/ci.yml`)

Triggers on PR to `main` or `production`. Runs four parallel jobs after **Mission Brief** (config parsing):

- **🔐 Secret Scan** — trufflehog scan of the last commit
- **🦀 Ferris Says No Bugs** — rustfmt, clippy, llvm-cov coverage, cargo-deny
- **🔌 Connection Check** → **⚙️ Integration Gauntlet** — connectivity probe then integration tests via `bash scripts/int-test.sh --ci`

Coverage threshold is read from `.ci/ci_env.toml` (`coverage_threshold` key, default 45%).

#### Configuring service-specific secrets

Service-specific host secrets use **standard names** set per-repo by the ansible `k8s-bootstrap` playbook. The CI workflow injects them directly — no declarations needed in `.ci/ci_env.toml`.

`.ci/ci_env.toml` only needs the local dev flags:

```toml
coverage_threshold = 45
engine = "nextest"
env = [
  "MAE_TESTCONTAINERS=1",
]
flags = ["--no-pager", "--features", "integration-testing", "--all-features", "--run-ignored", "all"]
```

#### Required GitHub Secrets

| Secret | Scope | Set by | Maps to |
|---|---|---|---|
| `CI_STAGE_SERVICE_POSTGRES_HOST` | **per-repo** | ansible k8s-bootstrap | `APP_DATABASE__HOST` → `database.host` |
| `CI_STAGE_SERVICE_NEO4J_HOST` | **per-repo** | ansible k8s-bootstrap | `APP_GRAPHDB__HOST` → `graphdb.host` |
| `CI_REDIS_URL` | org-global | ansible k8s-bootstrap | `APP_REDIS_URI` → `redis_uri` |
| `CI_RABBITMQ_HOST` | org-global | ansible k8s-bootstrap | `RABBITMQ_HOST` |

The per-repo secrets are provisioned by running the `ci_staging` tag of the bootstrap playbook:

```bash
ansible-playbook -i inventories/hosts.ini playbooks/k8s-bootstrap.yml \
  -e @secrets/k8s-bootstrap.vars.yml --tags ci_staging
```

> **config-rs env var naming:** prefix `APP`, prefix_separator `_`, separator `__`.
> `APP_DATABASE__HOST` → strips `APP_` → `database__host` → `database.host`.
> Flat fields (no `__`): `APP_REDIS_URI` → `redis_uri`.

#### Local development

Running `bash scripts/int-test.sh` (no flags) automatically sets `MAE_TESTCONTAINERS=1` via `.ci/ci_env.toml`, spinning up docker containers for all services. No manual env setup needed.

## Pre-Push Hook

### Git Pre-Push Hook (`.git/hooks/pre-push`)

This project includes a pre-push hook to enforce Rust quality checks **before any `git push`**. It runs automatically when you push commits and ensures your code passes formatting, linting, testing, Miri, and dependency checks.

* **Automatic execution**: `.git/hooks/pre-push`
* **Bypass options**:

  * `SKIP_RUST_GATE=1` — skips all Rust checks globally
  * `FAST_PUSH=1` — skips Miri tests but still runs other checks
  * `SKIP_TEST=1` - skips all tests
* **Requirements**: Only runs if all of these files exist in your Rust project:

  * `clippy.toml`
  * `deny.toml`
  * `rustfmt.toml`
  * `rust-toolchain.toml`

### Behavior

1. Checks formatting:

```bash
cargo fmt -- --check
```

2. Runs all tests:

```bash
cargo test
```

3. Runs Miri unless `FAST_PUSH` is set:

```bash
cargo miri test
```

4. Enforces strict linting:

```bash
cargo clippy -- -D warnings
```

6. Verifies workspace policies:

```bash
cargo deny check
```

If any check fails, the push is aborted.

---

## Git Ignore (`.gitignore`)

This project uses a comprehensive `.gitignore` to prevent committing unnecessary or sensitive files:

```gitignore
# Rust / Cargo
/target/
**/*.rs.bk
**/*.rs.orig
**/*.rs.tmp
**/debug/
**/release/
*.rs.meta
rust-project.json

# Environment files
.env
.env.*
.env.local
.env.production
.env.development

# Docker
docker-compose.override.yml
Dockerfile.*
.dockerignore
*.dockerfile
*.log
*.pid
docker-volume-*
docker-container-*

# IDEs / Editors
.vscode/
.idea/
*.swp
*.swo
*.bak
*.tmp
```

### Licensing (`--private <name>` or `--name <name>`)

The `--private` flag allows you to generate a proprietary LICENSE file for your project. This is useful when your Rust crate or program is **not intended for public or open-source use**.

The `--name` flag allows you to generate a MIT LICENSE file for your project.

#### Usage

```bash
# Create a LICENSE file for yourself
sync-rust-template --private "Your Name"

# Or combine with --force
sync-rust-template --force --private "Your Name"

# Set your Environment
echo 'export RUST_OWNER="your name"' >> /.zshrc
source /.zshrc

sync-rust-template --private # --name
```

#### Behavior

* Creates a `LICENSE` file in the current directory.
* Includes the **current year** and the **provided name**.
* Marks the project as **all rights reserved / proprietary** or **MIT**.
* updates `Cargo.toml` file.
* Does **not overwrite** an existing LICENSE unless combined with `--force`.

#### Example Private LICENSE Content

```
Copyright (c) 2026 Your Name

All rights reserved.

This software is proprietary and may not be used, copied, modified,
or distributed without explicit permission from Your Name.
```

## Getting Started

```bash
# Create a new project, then:
cargo new your-project
cd your-project
git init
sync-rust-template --name <your-name>

# Toolchain auto-selected via rust-toolchain.toml
cargo build
cargo test
```

## Adding Dependencies

All new dependencies must pass `cargo deny check`. Update `Cargo.toml` and run:

```bash
cargo deny check
```

If there are unique usecases to allow licenses from specific crates, add lines to your `./deny.exceptions.toml` file.
#### Example:
```toml
exceptions = [
{crate = "atomic-wait", allow = ["BSD-2-Clause"]},
{crate = "unicode-ident", allow = ["MIT", "Apache-2.0", "Unicode-3.0"]}
]
```

## Local Checks

Run these commands to verify code quality locally:

```bash
cargo +nightly fmt -- --check                    # Formatting
cargo +nightly clippy -- -D warnings              # Strict linting
cargo +nightly miri test                          # Undefined behavior
cargo deny check                                  # Licenses / sources / bans
```

## Syncing Template Configs & Documentation

This project includes a small helper script called `sync-rust-template` that lets you easily bring in the latest configuration files, documentation standards, and CI workflow from your rust_template directory into any Rust project.

What the script does

When run from the root of a Rust project (must contain `Cargo.toml`), it:

- Copies these config files (fails if they exist unless `--force` is used):
  - `clippy.toml` (strict linting rules)
  - `deny.toml` (license/vulnerability/source checks via cargo-deny)
  - `rust-toolchain.toml` (nightly Rust + rustfmt/clippy/miri)
  - `rustfmt.toml` (opinionated code formatting)

- Copies the GitHub Actions workflow:
  - `.github/workflows/cooked-crab.yaml`  
    (creates the `.github/workflows/` directory if missing; skips if the file exists unless `--force` is used)

- Handles `DEVELOPMENT.md`:
  - Copies the template's `README.md` to `DEVELOPMENT.md`  
  - Normal mode: skips if `DEVELOPMENT.md` already exists  
  - With `--force`: overwrites `DEVELOPMENT.md` if it exists

- Handles `README.md`:
  - If missing → creates minimal version: `For development rules, see [DEVELOPMENT.md](DEVELOPMENT.md)`
  - If exists → prepends the above link (only once, idempotent check)

- Appends header to `src/lib.rs` from template (skips if header already present via content check)

- licensing:
  - Using `--private "Your Name"` generates a LICENSE file with current year and your name
    - Marks the project as proprietary (all rights reserved)
  - Using `--name "Your Name"` generates an MIT file with current year and your name
  - Skips creation if LICENSE already exists unless --force is used

Safety features:
- Fails immediately if not in a Rust project (no `Cargo.toml`)
- Pre-checks: refuses to overwrite config files without `--force`
- Skips missing template files with clear warnings
- Idempotent: won't duplicate headers or README links
- `--force` also enables overwriting `DEVELOPMENT.md` and the workflow file

Prerequisites

1. Set the environment variable pointing to your template directory:

   ```bash
   export RUST_TEMPLATE_DIR="/path/to/your/rust_template"
   ```

   Make it permanent (add to `~/.bashrc`, `~/.zshrc`, or `~/.profile`):

   ```bash
   echo 'export RUST_TEMPLATE_DIR="/path/to/your/rust_template"' >> ~/.zshrc
   source ~/.zshrc
   ```

2. Verify the template directory exists:

   ```bash
   ls "$RUST_TEMPLATE_DIR" # Should show clippy.toml, deny.toml, .github/workflows/cooked-crab.yaml, etc.
   ```

Installation & Setup

1. Save the script to a directory in your `$PATH`:

   ```bash
   mkdir -p ~/.local/bin
   # Save script as ~/.local/bin/sync-rust-template
   ```

2. Make executable (chmod privileges):

   ```bash
   chmod +x ~/.local/bin/sync-rust-template
   ```

   Verify:

   ```bash
   ls -l ~/.local/bin/sync-rust-template # Should show -rwxr-xr-x
   ```

3. Add script directory to PATH (if not already):

   ```bash
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
   source ~/.zshrc
   ```

Usage

From any Rust project root:

```bash
# Safe mode (fails if config files exist; skips DEVELOPMENT.md & workflow if present)
sync-rust-template

# Force mode: overwrites config files, DEVELOPMENT.md, and workflow file if they exist
sync-rust-template --force
# or
sync-rust-template -f

# Add a private license.
# Use this if your project is explicitly private and should not be shared (ie - proprietary)
sync-rust-template --private your_name

# Show help
sync-rust-template --help
```

Example workflow:

```bash
cd ~/projects/my-rust-app
export RUST_TEMPLATE_DIR="$HOME/templates/rust_template"
sync-rust-template --force

# Check results
ls -l clippy.toml deny.toml rust-toolchain.toml rustfmt.toml DEVELOPMENT.md
ls -l .github/workflows/cooked-crab.yaml
head -n 20 src/lib.rs # Should show template header
head -n 5 README.md   # Should show DEVELOPMENT.md link

# Commit
git add clippy.toml deny.toml rust-toolchain.toml rustfmt.toml \
       DEVELOPMENT.md README.md src/lib.rs \
       .github/workflows/cooked-crab.yaml
git commit -m "chore: sync rust_template configs, workflow, and development docs"
```

Expected Output (first run with --force)

```
Syncing from template: /path/to/your/rust_template
Target directory:     /home/user/projects/my-rust-app
(FORCE mode: will overwrite existing config files + DEVELOPMENT.md)

'/path/to/your/rust_template/clippy.toml' -> './clippy.toml'
'/path/to/your/rust_template/deny.toml' -> './deny.toml'
'/path/to/your/rust_template/rust-toolchain.toml' -> './rust-toolchain.toml'
'/path/to/your/rust_template/rustfmt.toml' -> './rustfmt.toml'
'/path/to/your/rust_template/.github/workflows/cooked-crab.yaml' -> './.github/workflows/cooked-crab.yaml'
Created workflow file: .github/workflows/cooked-crab.yaml
'/path/to/your/rust_template/README.md' -> './DEVELOPMENT.md'
Overwriting DEVELOPMENT.md (with --force)
Created/Updated DEVELOPMENT.md from template README.md
Created minimal README.md pointing to DEVELOPMENT.md
Appended header to src/lib.rs

Done:
  • 7 new file(s) created/copied
  • 1 file(s) overwritten (with --force)
  • 1 file(s) updated (header or README pointer)
```

Troubleshooting

- `RUST_TEMPLATE_DIR is not set` → run the export command
- `Template directory not found` → check path with `ls "$RUST_TEMPLATE_DIR"`
- `Permission denied` → `chmod +x sync-rust-template`
- `command not found` → add script dir to `$PATH` and `source ~/.zshrc`
- `Not a Rust project` → run from directory containing `Cargo.toml`
- `Files already exist` → use `--force` flag
-  `No --private, --name, or RUST_OWNER environment variable provided.` → set the `RUST_OWNER` in your environment, use `--name <your/company name>` or use `--private <your/company name>`
    - with `RUST_OWNER` set, `--name` defaults to `MIT` license, use `--private` for proprietary projects

After Syncing

Run these to verify everything works:

```bash
cargo +nightly fmt -- --check
cargo +nightly clippy -- -D warnings
cargo deny check
cargo +nightly miri test
```

Pro tip: Add an alias to your shell for convenience:

```bash
echo 'alias rust-sync="sync-rust-template --force"' >> ~/.zshrc
# Then just: rust-sync
```

Enjoy consistent, production-grade Rust tooling and integrity checks across all your projects! 🚀

---

## Local Dev with Docker Compose Watch

This template includes a fast local development loop powered by **Docker Compose Watch** and **cargo-watch** inside a dev container.

### How it works

- `Dockerfile.dev` pre-cooks dependencies using **cargo-chef**, so they are cached as a Docker layer and only rebuilt when `Cargo.lock` changes.
- `scripts/boot.sh` runs **cargo-watch** inside the container, watching `src/` and `Cargo.toml` for changes and automatically triggering an incremental recompile + restart.
- The `develop.watch` config in your compose file syncs host-side file changes into the running container via **Docker Compose Watch**.

### Starting the dev environment

```bash
docker compose up --watch
```

File changes in `src/` are synced into the container automatically. cargo-watch detects the sync and triggers an incremental recompile + restart.

### Expected cycle times

| Change type | Expected time |
|---|---|
| Source file (`src/`) | ~5–30s (incremental recompile) |
| `Cargo.toml` | ~5–30s (incremental recompile) |
| `Cargo.lock` (dep change) | ~2–5 min (full image rebuild) |

### Syncing dev files to a service repo

`Dockerfile.dev`, `scripts/boot.sh`, and `.dockerignore` are distributed to service repos via `sync-rust-template`. Run it from any service repo root to pick up the latest dev workflow files.
