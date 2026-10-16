#!/usr/bin/env bash
# The same bench with RISC Zero's accelerated k256, sha2 and crypto-bigint, then a real proof of the verification guest at M of 5.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
BUDGET=10000000
THREADS="${THREADS:-16 8}"
cd "$WORK/accel"
export CARGO_TARGET_DIR="$PWD/target"
cargo build --release -p multisig_bench --features prove
B=./target/release/multisig_bench
export RISC0_DEV_MODE=0
echo "== sweep"
$B --scheme schnorr --m 1,2,3,5,7,9,12,20 --out "$OUT/accel-sweep.json"
echo "== ceiling under the cap, all expected to pass"
$B --scheme schnorr --m 20,30,32,33,34 --budget $BUDGET --out "$OUT/accel-ceiling.json"
echo "== M=35 under the cap: over it with RISC Zero Rust 1.91.1, inside it with Rust 1.97.0"
if $B --scheme schnorr --m 35 --budget $BUDGET --out "$OUT/accel-m35.json"; then echo "M=35 fits the cap with this toolchain"; else echo "M=35 exceeds the cap with this toolchain"; fi
echo "== M=36 under the cap, expected to fail with either toolchain"
if $B --scheme schnorr --m 36 --budget $BUDGET --out "$OUT/accel-m36.json"; then echo "UNEXPECTED: M=36 fits the cap"; else echo "as expected, M=36 exceeds the cap"; fi
echo "== one flipped signature byte, expected to fail"
if $B --scheme schnorr --m 1 --tamper --out "$OUT/accel-tamper.json"; then echo "UNEXPECTED: tampered signature accepted"; else echo "as expected, rejected"; fi
[ "${SKIP_PROVE:-0}" = 1 ] && { echo "SKIP_PROVE=1, done"; exit 0; }
TIME=""; [ -x /usr/bin/time ] && TIME="/usr/bin/time -v"
for t in $THREADS; do
  echo "== real proof at M of 5, $t threads"
  RAYON_NUM_THREADS=$t $TIME $B --scheme schnorr --m 5 --prove --budget $BUDGET --out "$OUT/accel-prove-m5-t$t.json" 2>&1 \
    | tee "$OUT/accel-prove-m5-t$t.log" | grep -E "^schnorr|proved in|Maximum resident|Elapsed \(wall"
  cp target/receipt_schnorr_m5_plan.json "$OUT/accel-receipt-m5-t$t.json"
done
