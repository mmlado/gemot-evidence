#!/usr/bin/env bash
# Every real proof in FINDINGS.md section 2, one at a time. About two and a half hours on 16 threads.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
unset RISC0_DEV_MODE
TIME=""; [ -x /usr/bin/time ] && TIME="/usr/bin/time -v"
run() {
  local dir=$1 name=$2 path=$3
  echo "== $name $(date -Iseconds)"
  (cd "$WORK/$dir" && $TIME cargo test --release -p lee --lib "$path" -- --exact --nocapture) > "$OUT/proof-$name.log" 2>&1
  grep -E "RFP005 [a-z]|proof_bytes|settled|Maximum resident|Elapsed \(wall|test result" "$OUT/proof-$name.log" || true
}
run stock minimal-transfer state::tests::rfp005_spike::rfp005_public_part_of_a_private_transaction
run stock admin-call state::tests::rfp005_spike::rfp005_admin_write_real
run accel whole-m5 state::tests::rfp005_whole_tx::rfp005_whole_multisig_tx_m5
run accel curve-m1 state::tests::rfp005_whole_tx::rfp005_curve_m1
run accel curve-m3 state::tests::rfp005_whole_tx::rfp005_curve_m3
run accel curve-m10 state::tests::rfp005_whole_tx::rfp005_curve_m10
run accel wallet-shape-m5 state::tests::rfp005_whole_tx::rfp005_wallet_shape_m5
run accel fallback-queue-m5 state::tests::rfp005_whole_tx::rfp005_fallback_queue_m5
RAYON_NUM_THREADS=8 run accel whole-m5-8-threads state::tests::rfp005_whole_tx::rfp005_whole_multisig_tx_m5
