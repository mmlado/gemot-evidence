#!/usr/bin/env bash
# Two checkouts of LEZ v0.3.0 with this repository's patches: stock crypto crates, and RISC Zero's accelerated forks.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
REV=db66590ab
mkdir -p "$WORK"
[ -d "$WORK/lez" ] || git clone https://github.com/logos-blockchain/logos-execution-zone.git "$WORK/lez"
for v in stock accel; do
  [ -d "$WORK/$v" ] || git -C "$WORK/lez" worktree add --detach "$WORK/$v" "$REV"
done
apply() {
  if git -C "$1" apply --check "$2" 2>/dev/null; then git -C "$1" apply "$2"; else echo "$(basename "$2") already applied in $1"; fi
}
apply "$WORK/stock" "$ROOT/patches/01-bench.patch"
apply "$WORK/stock" "$ROOT/patches/03-private-path-tests.patch"
apply "$WORK/accel" "$ROOT/patches/01-bench.patch"
apply "$WORK/accel" "$ROOT/patches/02-accelerated-forks.patch"
apply "$WORK/accel" "$ROOT/patches/04-whole-transaction-tests.patch"
echo "ready: $WORK/stock and $WORK/accel"
