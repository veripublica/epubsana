#!/usr/bin/env bash
#
# Step 6 of a release: verify from the artefacts, not the workflows. Read-only:
# it downloads into a temporary directory and changes nothing anywhere.
#
# Usage: scripts/release-verify.sh 0.25.0
#
# Prints one line per item (ok / FAIL) and a verdict; exit 0 only if all pass.
# npm lags its own publish by minutes (every release so far), so the npm
# tarball is polled for up to 10 minutes before it counts as missing.

set -uo pipefail
cd "$(dirname "$0")/.."

V="${1:?usage: scripts/release-verify.sh X.Y.Z}"
REPO=veripublica/epubsana
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
PASS=0; FAIL=0
ok()  { echo "  ok    $1"; PASS=$((PASS + 1)); }
bad() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }

# ------------------------------------------------------------------ crates.io
if curl -sfL -A "epubsana-release (github.com/veripublica/epubsana)" \
     -o "$W/c.crate" "https://static.crates.io/crates/epubsana/epubsana-$V.crate"; then
  ok "crates.io serves epubsana $V"
  LEAK=$(tar tzf "$W/c.crate" | grep -E "^epubsana-$V/(CLAUDE\.md|docs/(SPIKE|API|COVERAGE)\.md)$" || true)
  [ -z "$LEAK" ] && ok "crate has no local-only doc" || bad "crate ships: $LEAK"
else
  bad "crates.io does not serve epubsana $V"
fi

# ------------------------------------------------------------------ npm
TGZ="https://registry.npmjs.org/@veripublica/epubsana-wasm/-/epubsana-wasm-$V.tgz"
for _ in $(seq 1 "${NPM_POLLS:-40}"); do curl -sfL -o "$W/n.tgz" "$TGZ" && break; sleep 15; done
if [ -s "$W/n.tgz" ]; then
  ok "npm serves @veripublica/epubsana-wasm $V"
  L=$(tar tzf "$W/n.tgz")
  for f in package/LICENSE package/LICENSE-COMMERCIAL.md package/epubsana_wasm.d.ts; do
    grep -qx "$f" <<< "$L" && ok "npm tarball has ${f#package/}" || bad "npm tarball lacks ${f#package/}"
  done
  # The tarball can be served before the dist-tag moves.
  LATEST=$(curl -sf "https://registry.npmjs.org/@veripublica/epubsana-wasm" | jq -r '."dist-tags".latest')
  [ "$LATEST" = "$V" ] && ok "npm latest is $V" || bad "npm latest is $LATEST, not $V"
else
  bad "npm tarball not served after $(( ${NPM_POLLS:-40} * 15 ))s: $TGZ"
fi

# ------------------------------------------------------------------ GitHub release
N=$(gh release view "v$V" --repo "$REPO" --json assets -q '.assets | length' 2>/dev/null || echo 0)
[ "$N" = 9 ] && ok "release v$V has 9 assets" || bad "release v$V has $N assets, not 9"

TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
A="epubsana-$TRIPLE.tar.gz"
if gh release download "v$V" --repo "$REPO" -D "$W" -p SHA256SUMS.txt -p "$A" 2>/dev/null; then
  (cd "$W" && shasum -a 256 -c SHA256SUMS.txt --ignore-missing >/dev/null 2>&1) \
    && ok "$A matches SHA256SUMS.txt" || bad "$A does not match SHA256SUMS.txt"
  gh attestation verify "$W/$A" --repo "$REPO" >/dev/null 2>&1 \
    && ok "$A provenance attestation verifies" || bad "$A attestation does not verify"
  mkdir -p "$W/x" && tar xzf "$W/$A" -C "$W/x"
  BIN=$(find "$W/x" -type f -name epubsana | head -1)
  GOT=$("$BIN" --version 2>/dev/null || echo "no binary")
  if [[ "$GOT" == "epubsana $V+"* && "$GOT" != *dirty* ]]; then ok "binary reports '$GOT'"
  else bad "binary reports '$GOT' (want epubsana $V+<sha>, no .dirty)"; fi
else
  bad "could not download $A and SHA256SUMS.txt"
fi

echo
echo "$PASS ok, $FAIL failed"
[ "$FAIL" = 0 ] && echo "VERIFIED: epubsana $V" || { echo "NOT VERIFIED"; exit 1; }
