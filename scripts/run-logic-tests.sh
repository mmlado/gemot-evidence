#!/usr/bin/env bash
# The private-path rules, the admin call and the whole-transaction shapes, in dev mode: these check logic, not proving cost.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
export RISC0_DEV_MODE=1
cd "$WORK/stock"
echo "== our tests: shared-account conflict, one initialization, public part, admin call"
cargo test --release -p lee --lib rfp005_ -- --nocapture --test-threads=1 2>&1 | tee "$OUT/logic-stock.log" | grep -E "^test |RFP005 [a-z]|test result"
echo "== LEZ's tests: private-path windows, immutable headers, undisclosed programs, PDA signers"
cargo test --release -p lee --lib -- validity_window_works_in_privacy_preserving_transactions private_claim_ create_header_ update_header_flip a_signer_entry_does_not_authorize_a_private_pda 2>&1 | tee "$OUT/logic-lez.log" | grep -E "test result"
cargo test --release -p program_loader_core -- update_header 2>&1 | tee -a "$OUT/logic-lez.log" | grep -E "test result"
cargo test --release -p key_protocol --lib group_key_holder 2>&1 | tee -a "$OUT/logic-lez.log" | grep -E "test result"
cd "$WORK/accel"
echo "== whole-transaction shapes, settled inside and rejected outside their window"
cargo test --release -p lee --lib rfp005_ -- --nocapture --test-threads=1 2>&1 | tee "$OUT/logic-accel.log" | grep -E "^test |RFP005 [a-z]|settled|outside|test result"
