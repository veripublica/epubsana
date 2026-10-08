#!/usr/bin/env bash
#
# Waits for every workflow run on one commit, then prints them and exits 0
# only if all succeeded. Run it in the background after the push.
#
# Usage: scripts/wait-runs.sh <commit> [expected-runs] [timeout-seconds]
#
# The release push sends main and the tag together, so one commit carries
# five runs: CI and Pages (main), crates.io, npm and binaries (the tag). The
# default expects five; a plain push to main expects 2.
#
# Written after 0.24.0, whose hand-rolled wait loop never saw its runs finish
# and sat until its 10-minute timeout, 6.5 minutes after CI was green.

set -euo pipefail
SHA=$(git rev-parse "${1:?usage: scripts/wait-runs.sh <commit> [expected-runs] [timeout-seconds]}")
N="${2:-5}"
LIMIT="${3:-1500}"
REPO=veripublica/epubsana

runs() { gh run list --repo "$REPO" --commit "$SHA" --limit 20 --json name,status,conclusion,databaseId; }

START=$SECONDS
while :; do
  J=$(runs)
  have=$(jq length <<< "$J")
  open=$(jq '[.[] | select(.status != "completed")] | length' <<< "$J")
  [ "$have" -ge "$N" ] && [ "$open" = 0 ] && break
  if [ $((SECONDS - START)) -ge "$LIMIT" ]; then
    echo "timed out after ${LIMIT}s: $have of $N runs seen, $open still running"
    jq -r '.[] | "  \(.status)\t\(.conclusion)\t\(.name)\t\(.databaseId)"' <<< "$J"
    exit 2
  fi
  sleep 15
done

echo "$have runs on ${SHA:0:7}, $((SECONDS - START))s waited:"
jq -r '.[] | "  \(.conclusion)\t\(.name)\t\(.databaseId)"' <<< "$J"
[ "$(jq '[.[] | select(.conclusion != "success")] | length' <<< "$J")" = 0 ]
