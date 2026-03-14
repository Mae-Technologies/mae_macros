#!/usr/bin/env bash
set -euo pipefail

########################################
# 🎨 Color Support (Cargo colors untouched)
########################################
if command -v tput >/dev/null 2>&1 && [ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]; then
  RED="$(tput setaf 1)"
  GREEN="$(tput setaf 2)"
  YELLOW="$(tput setaf 3)"
  BLUE="$(tput setaf 4)"
  CYAN="$(tput setaf 6)"
  BOLD="$(tput bold)"
  RESET="$(tput sgr0)"
else
  RED=""
  GREEN=""
  YELLOW=""
  BLUE=""
  CYAN=""
  BOLD=""
  RESET=""
fi

info() { echo "${BLUE}$*${RESET}"; }
ok() { echo "${GREEN}$*${RESET}"; }
warn() { echo "${YELLOW}$*${RESET}"; }
err() { echo "${RED}$*${RESET}" >&2; }

########################################
# 1️⃣ RUST PRE-PUSH CHECKS
########################################

# Global bypass
if [[ -n "${SKIP_GUARD:-}" ]]; then
  warn "⚠️  SKIP_GUARD set — skipping Rust quality gate"
  exit 0
fi

# Only allow pushing over ssh
remote_name="${1:-}"
remote_url="${2:-}"

case "$remote_url" in
https://* | http://*)
  echo "ERROR: HTTPS push blocked. Use SSH remote URLs (git@...)." >&2
  echo "Remote: $remote_name  URL: $remote_url" >&2
  exit 1
  ;;
esac

# Drain stdin (required by git pre-push hook protocol)
# Guard: only drain when stdin is a pipe (not a terminal), so the script
# doesn't hang when a developer runs it directly from the command line.
if [[ ! -t 0 ]]; then
  while read -r local_ref local_sha remote_ref remote_sha; do
    : # checks run on every branch — no branch filtering
  done
fi

# Detect Rust project by policy files
required_files=(
  clippy.toml
  deny.toml
  rustfmt.toml
  rust-toolchain.toml
)

for f in "${required_files[@]}"; do
  [[ -f "$f" ]] || exit 0
done

info "🦀  Rust quality gate detected — running pre-push checks"
echo

run() {
  local label="$1"
  shift
  info "▶ ${label}"
  "$@" || {
    err "❌ ${label} failed — aborting push"
    exit 1
  }
}

########################################
# ✨ Always run formatting
########################################
run "rustfmt" cargo fmt -- --check

########################################
# 🧪 Tests (optional fast-paths)
########################################
if [[ -z "${SKIP_TESTS:-}" ]]; then
  run "tests" bash "$(dirname "$0")/int-test.sh"
  ok "✔  Tests completed successfully"
else
  warn "⚡ SKIP_TESTS set — skipping tests"
fi

########################################
# 📊 Coverage (cargo llvm-cov)
########################################
if ! command -v cargo-llvm-cov >/dev/null 2>&1 && ! cargo llvm-cov --version >/dev/null 2>&1; then
  err "❌ cargo-llvm-cov is not installed — coverage checking is mandatory."
  err "   Install it: see DEVELOPMENT.md or https://github.com/taiki-e/cargo-llvm-cov#installation"
  exit 1
fi

# Read coverage threshold from CI config (fall back to 45%)
COV_THRESHOLD=45
CI_ENV_TOML="$(dirname "$0")/../.ci/ci_env.toml"
if [[ -f "$CI_ENV_TOML" ]]; then
  _parsed=$(python3 -c "
import tomllib, sys, pathlib
d = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
print(d.get('coverage_threshold', ''))
" "$CI_ENV_TOML" 2>/dev/null || true)
  if [[ -n "$_parsed" ]]; then
    COV_THRESHOLD="$_parsed"
  fi
fi

run "coverage (≥${COV_THRESHOLD}% lines)" cargo +nightly llvm-cov --lib --fail-under-lines "$COV_THRESHOLD"
ok "✔  Coverage threshold met"

########################################
# 🔍 Lint / Security / Policy
########################################
run "clippy" cargo clippy -- -D warnings
run "cargo deny" cargo deny check

########################################
# 🔐 Secret Scanning (TruffleHog)
#
# Scans the most recent commit for verified secrets (API keys, tokens, etc.).
# Uses TruffleHog (https://github.com/trufflesecurity/trufflehog).
#
# Install: see DEVELOPMENT.md or run sync_rust_template.sh which bootstraps it.
#
# False-positive handling:
#   - Add patterns to .trufflehog-ignore (one regex per line, matched against
#     the detector name + file path).
#   - Alternatively use --exclude-paths=<file> with a list of paths to skip.
#   - For test fixtures / example keys, add them to .trufflehog-ignore.
#
# Skip: set SKIP_SECRET_SCAN=1 to bypass this step.
########################################
if ! command -v trufflehog >/dev/null 2>&1; then
  err "❌ trufflehog is not installed — secret scanning is mandatory."
  err "   Install it: see DEVELOPMENT.md or https://github.com/trufflesecurity/trufflehog#installation"
  exit 1
fi

trufflehog_args=(git "file://." --since-commit HEAD~1 --only-verified --fail)
if [[ -f ".trufflehog-ignore" ]]; then
  trufflehog_args+=(--exclude-paths .trufflehog-ignore)
fi
run "secret scan (trufflehog)" trufflehog "${trufflehog_args[@]}"
ok "✔  No secrets detected"

echo
ok "✅  All Rust checks passed — continuing push"
