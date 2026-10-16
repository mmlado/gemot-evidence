#!/usr/bin/env bash
# Schnorr approval verification with the crates LEZ ships, under the 10M cap on a charged public transaction.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
BUDGET=10000000
cd "$WORK/stock"
export CARGO_TARGET_DIR="$PWD/target"
cargo build --release -p multisig_bench --features prove
B=./target/release/multisig_bench
export RISC0_DEV_MODE=0
echo "== sweep"
$B --scheme schnorr --m 1,2,3,5,7 --out "$OUT/stock-sweep.json"
echo "== M=1 under the cap, expected to pass"
$B --scheme schnorr --m 1 --budget $BUDGET --out "$OUT/stock-m1-budget.json"
echo "== M=2 under the cap, expected to fail with either toolchain"
if $B --scheme schnorr --m 2 --budget $BUDGET --out "$OUT/stock-m2-budget.json"; then echo "UNEXPECTED: M=2 fits the cap"; else echo "as expected, M=2 exceeds the cap"; fi
echo "== one flipped signature byte, expected to fail"
if $B --scheme schnorr --m 1 --tamper --out "$OUT/stock-tamper.json"; then echo "UNEXPECTED: tampered signature accepted"; else echo "as expected, rejected"; fi
