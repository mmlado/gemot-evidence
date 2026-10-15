#!/usr/bin/env bash
# LEZ's own private PDA tests. Dev mode is fine here: they check authorization logic, not proving cost.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
cd "$WORK/stock"
RISC0_DEV_MODE=1 cargo test --release -p lee --lib -- pda --skip rfp005 2>&1 | tee "$OUT/vault-tests.log" | grep -E "^test result"
