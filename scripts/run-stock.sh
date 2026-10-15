#!/usr/bin/env bash
# Schnorr approval verification with the crates LEZ ships, under LEZ's public cycle budget.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
BUDGET=33554432
cd "$WORK/stock"
export CARGO_TARGET_DIR="$PWD/target"
cargo build --release -p multisig_bench --features prove
B=./target/release/multisig_bench
export RISC0_DEV_MODE=0
echo "== sweep"
$B --scheme schnorr --m 1,2,3,5,7 --out "$OUT/stock-sweep.json"
echo "== M=5 under the budget, expected to pass"
$B --scheme schnorr --m 5 --budget $BUDGET --out "$OUT/stock-m5-budget.json"
echo "== M=6 under the budget: just over it with RISC Zero Rust 1.91.1, just under it with Rust 1.97.0"
if $B --scheme schnorr --m 6 --budget $BUDGET --out "$OUT/stock-m6-budget.json"; then echo "M=6 fits the budget with this toolchain"; else echo "M=6 exceeds the budget with this toolchain"; fi
echo "== one flipped signature byte, expected to fail"
if $B --scheme schnorr --m 1 --tamper --out "$OUT/stock-tamper.json"; then echo "UNEXPECTED: tampered signature accepted"; else echo "as expected, rejected"; fi
