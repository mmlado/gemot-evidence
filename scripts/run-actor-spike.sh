#!/usr/bin/env bash
# Section 9: the execute as a 0.4 actor. Public-path cycles, refusals and the ceiling in dev mode, then the real private proofs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
cd "$WORK/actor"
export CARGO_TARGET_DIR="$PWD/target"
echo "== public path under the 10M cap, refusals and the ceiling: cycle counts need no proof, so dev mode"
RISC0_DEV_MODE=1 cargo test --release -p lee --lib rfp005_actor -- --nocapture --test-threads=1 2>&1 | tee "$OUT/actor-public.log" | grep -E "RFP005-0.4|test result"
[ "${SKIP_PROVE:-0}" = 1 ] && { echo "SKIP_PROVE=1, done"; exit 0; }
unset RISC0_DEV_MODE
TIME=""; [ -x /usr/bin/time ] && TIME="/usr/bin/time -v"
run() {
  local name=$1 m=$2 recipient=$3
  echo "== real proof $name $(date -Iseconds)"
  RFP005_M=$m RFP005_RECIPIENT=$recipient $TIME cargo test --release -p lee --lib state::tests::rfp005_actor::rfp005_actor_private_execute -- --ignored --exact --nocapture > "$OUT/actor-proof-$name.log" 2>&1 || true
  grep -E "RFP005-0.4|Maximum resident|Elapsed \(wall|test result" "$OUT/actor-proof-$name.log" || true
}
run m5-private 5 private
run m5-public 5 public
run m1-private 1 private
run m10-private 10 private
