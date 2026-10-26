#!/usr/bin/env bash
# A third checkout: LEZ dev at 7e3f7f7, the 0.4 actor model, with the actor-model spike patch.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
REV=7e3f7f7
mkdir -p "$WORK"
[ -d "$WORK/lez" ] || git clone https://github.com/logos-blockchain/logos-execution-zone.git "$WORK/lez"
git -C "$WORK/lez" fetch origin dev
[ -d "$WORK/actor" ] || git -C "$WORK/lez" worktree add --detach "$WORK/actor" "$REV"
if git -C "$WORK/actor" apply --check "$ROOT/patches/06-actor-model-spike.patch" 2>/dev/null; then
  git -C "$WORK/actor" apply "$ROOT/patches/06-actor-model-spike.patch"
else
  echo "06-actor-model-spike.patch already applied in $WORK/actor"
fi
echo "ready: $WORK/actor"
