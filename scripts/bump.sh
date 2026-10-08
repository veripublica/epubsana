#!/usr/bin/env bash
#
# Step 1 of a release: the edits the 0.22.0 release commit made by hand.
# Both manifests (the publish workflows refuse a tag that differs from either),
# Cargo.lock, and the CHANGELOG heading the release body is taken from.
#
# Usage: scripts/bump.sh 0.25.0 [YYYY-MM-DD]   (date defaults to today)
#
# Writes only those three files and the lockfile; commits nothing. The README
# and docs/USAGE.md sweep for stale counts stays a reading job.

set -euo pipefail
cd "$(dirname "$0")/.."

V="${1:?usage: scripts/bump.sh X.Y.Z [YYYY-MM-DD]}"
D="${2:-$(date +%F)}"
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "not a version: $V" >&2; exit 2; }

grep -q '^## \[Unreleased\]' CHANGELOG.md \
  || { echo "CHANGELOG.md has no [Unreleased] section to release" >&2; exit 1; }
! grep -q "^## \[$V\]" CHANGELOG.md \
  || { echo "CHANGELOG.md already has a [$V] section" >&2; exit 1; }

# The first `version =` in each manifest is the package's own.
for m in Cargo.toml epubsana-wasm/Cargo.toml; do
  awk -v v="$V" '!done && /^version = "/ { print "version = \"" v "\""; done=1; next } { print }' \
    "$m" > "$m.tmp" && mv "$m.tmp" "$m"
done
sed -i '' "s/^## \[Unreleased\]/## [$V] - $D/" CHANGELOG.md

cargo build --workspace -q

grep -H '^version' Cargo.toml epubsana-wasm/Cargo.toml
grep -n "^## \[$V\]" CHANGELOG.md
git diff --stat
