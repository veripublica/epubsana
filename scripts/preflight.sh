#!/usr/bin/env bash
#
# Step 2 of a release: everything CI runs, plus what the publish workflows
# check after the tag, moved to before it. epubveri's scripts/preflight.sh is
# the model. Main and the tag are pushed together (Baris, 2026-10-08), so this
# is the last gate before the irreversible step: crates.io never reuses a
# version number.
#
# Usage:
#   scripts/preflight.sh          # check the version in Cargo.toml
#   scripts/preflight.sh 0.25.0   # check a version you are about to tag
#
# Every check runs even after one fails. The last line is the verdict:
# READY or NOT READY. Exit 0 = safe to `git push origin main vX.Y.Z`.
#
# Not here, on purpose: the shelf measurement (plan digest, regression_audit).
# It belongs to the commit that changes behaviour, and is recorded there.

set -uo pipefail
cd "$(dirname "$0")/.."

PASS=0; FAIL=0; FAILED=()
T=$SECONDS
took() { local d=$((SECONDS - T)); T=$SECONDS; printf ' [%ds]' "$d"; }
ok()   { printf '  ok    %s' "$1"; took; echo; PASS=$((PASS + 1)); }
bad()  { printf '  FAIL  %s' "$1"; took; echo; FAIL=$((FAIL + 1)); FAILED+=("$1"); }
skip() { printf '  --    %s' "$1"; took; echo; }

# check <label> <command...>: runs it, prints the tail only on failure.
check() {
  local label="$1"; shift
  local out
  if out=$("$@" 2>&1); then ok "$label"; else
    bad "$label"; printf '%s\n' "$out" | tail -15 | sed 's/^/        /'
  fi
}

ver() { grep -m1 '^version = "' "$1" | cut -d'"' -f2; }
CRATE_V=$(ver Cargo.toml); WASM_V=$(ver epubsana-wasm/Cargo.toml)
V="${1:-$CRATE_V}"
echo "version $V (crate $CRATE_V, wasm $WASM_V)"

# ------------------------------------------------------------------ version
[ "$CRATE_V" = "$V" ] && ok "Cargo.toml is $V" || bad "Cargo.toml is $CRATE_V, not $V"
[ "$WASM_V" = "$V" ] && ok "epubsana-wasm/Cargo.toml is $V" || bad "epubsana-wasm/Cargo.toml is $WASM_V, not $V"
grep -q "^## \[$V\]" CHANGELOG.md && ok "CHANGELOG.md has [$V] (the release body)" \
  || bad "CHANGELOG.md has no [$V] section"
grep -q '^## \[Unreleased\]' CHANGELOG.md && bad "CHANGELOG.md still has [Unreleased]" \
  || ok "no [Unreleased] left in CHANGELOG.md"

# ------------------------------------------------------------------ tree
if [ -z "$(git status --porcelain)" ]; then ok "working tree clean (build.rs stamps .dirty)"
else bad "working tree dirty"; git status --short | sed 's/^/        /'; fi

git rev-parse -q --verify "refs/tags/v$V" >/dev/null && bad "tag v$V exists locally" || ok "tag v$V free locally"
if git fetch -q origin 2>/dev/null; then
  B=$(git rev-list --count HEAD..origin/main)
  [ "$B" = 0 ] && ok "main is not behind origin" || bad "main is $B commit(s) behind origin/main"
  git ls-remote -q --exit-code --tags origin "v$V" >/dev/null && bad "tag v$V exists on origin" \
    || ok "tag v$V free on origin"
else
  bad "git fetch failed"
fi

# The CLAUDE.md rule, checked rather than remembered.
LAST=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
if git log --format='%B' ${LAST:+$LAST..}HEAD | grep -qiE 'claude|anthropic|co-authored-by'; then
  bad "a commit since ${LAST:-the root} names Claude/Anthropic or has a Co-Authored-By"
else
  ok "no attribution in commits since ${LAST:-the root}"
fi

# ------------------------------------------------------------------ registries
curl -sf -A "epubsana-release (github.com/veripublica/epubsana)" \
  "https://crates.io/api/v1/crates/epubsana/$V" >/dev/null \
  && bad "crates.io already has $V" || ok "crates.io does not have $V"
curl -sf "https://registry.npmjs.org/@veripublica/epubsana-wasm/$V" >/dev/null \
  && bad "npm already has $V" || ok "npm does not have $V"

# ------------------------------------------------------------------ CI gates
check "cargo fmt --all --check" cargo fmt --all --check
check "cargo build --workspace --locked" cargo build --workspace --locked
if out=$(cargo test --workspace --locked 2>&1); then
  # Every `test result:` line, summed: a cut output once hid a failing crate.
  sum=$(grep '^test result:' <<< "$out" | awk '{p+=$4; f+=$6; i+=$8} END {print p" passed, "f" failed, "i" ignored"}')
  ok "cargo test --workspace --locked ($sum)"
else
  bad "cargo test --workspace --locked"
  grep -E '^test .* FAILED|^failures:|panicked' <<< "$out" | head -15 | sed 's/^/        /'
fi
check "cargo clippy (-D warnings)" cargo clippy --workspace --all-targets --locked -- -D warnings
check "cargo doc (-D warnings)" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
check "core lib for wasm32" cargo build --lib --target wasm32-unknown-unknown --locked
check "epubsana-wasm for wasm32" cargo build -p epubsana-wasm --target wasm32-unknown-unknown --locked

# CI's declared-minimum step. The lockfile is copied aside first: the plain
# `git checkout -- Cargo.lock` CI uses discards an uncommitted lock bump
# (hit 2026-10-02).
MIN=$(grep -E '^epubveri = "' Cargo.toml | sed -E 's/^epubveri = "\^?([0-9]+(\.[0-9]+){1,2})".*/\1/')
case "$MIN" in *.*.*) ;; *.*) MIN="$MIN.0" ;; esac
cp Cargo.lock "${TMPDIR:-/tmp}/epubsana-preflight.lock"
check "builds against the declared minimum epubveri ($MIN)" \
  bash -c "cargo update -q -p epubveri --precise '$MIN' && cargo check -q --workspace"
cp "${TMPDIR:-/tmp}/epubsana-preflight.lock" Cargo.lock

# CI's `audit` job runs on the push, which races the tag, and its verdict
# depends on the advisory database, not on our tree: three zip 8 advisories
# entered it between 0.30.0 and 0.31.0 with nothing here changing. epubveri
# added the same check after its 0.29.0 shipped red (2026-10-17). A missing
# cargo-audit is a failure, not a skip.
if command -v cargo-audit >/dev/null; then
  check "cargo audit (advisory database fetched now)" cargo audit --file Cargo.lock
else
  bad "cargo audit is not installed: cargo install cargo-audit --locked"
fi

MSRV=$(grep -m1 '^rust-version' Cargo.toml | cut -d'"' -f2)
if rustup run "$MSRV" rustc -V >/dev/null 2>&1; then
  check "compiles at the MSRV ($MSRV)" rustup run "$MSRV" cargo check --workspace --locked
else
  skip "MSRV $MSRV not installed here (CI gates it)"
fi

# ------------------------------------------------------------------ package
# `cargo package` refuses a dirty tree; one cause should raise one alarm.
if [ -n "$(git status --porcelain)" ]; then
  skip "crate contents: needs a clean tree (see above)"
elif PKG=$(cargo package --list --locked 2>/dev/null); then
  STRAY=$(grep -E '^(CLAUDE\.md|docs/(SPIKE|API|COVERAGE)\.md)$|(^|/)[^/]* [0-9]+(\.[A-Za-z0-9]+)?$' <<< "$PKG" || true)
  [ -z "$STRAY" ] && ok "crate has no local-only doc or conflict copy ($(wc -l <<< "$PKG" | tr -d ' ') files)" \
    || { bad "stray files would ship"; printf '%s\n' "$STRAY" | sed 's/^/        /'; }
else
  bad "cargo package --list failed"
fi

echo "  note: sweep README.md and docs/USAGE.md for stale counts (fixers, floor)"
echo
echo "$PASS passed, $FAIL failed, in $((SECONDS / 60)) min $((SECONDS % 60)) s"
if [ "$FAIL" -gt 0 ]; then
  echo "NOT READY:"; for f in "${FAILED[@]}"; do echo "  - $f"; done; exit 1
fi
echo "READY: git push origin main v$V"
