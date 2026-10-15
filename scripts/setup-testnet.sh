#!/usr/bin/env bash
# A checkout of LEZ v0.3.1, the version the public testnet runs, with the testnet driver, the wallet and the guest built.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
TAG=v0.3.1
mkdir -p "$WORK"
[ -d "$WORK/lez" ] || git clone https://github.com/logos-blockchain/logos-execution-zone.git "$WORK/lez"
git -C "$WORK/lez" fetch --tags --quiet
[ -d "$WORK/testnet" ] || git -C "$WORK/lez" worktree add --detach "$WORK/testnet" "$TAG"
if git -C "$WORK/testnet" apply --check "$ROOT/patches/05-testnet-driver.patch" 2>/dev/null; then
  git -C "$WORK/testnet" apply "$ROOT/patches/05-testnet-driver.patch"
else
  echo "05-testnet-driver.patch already applied in $WORK/testnet"
fi
unset RISC0_DEV_MODE
(cd "$WORK/testnet" && CARGO_TARGET_DIR="$WORK/testnet/target" cargo build --release -p wallet -p test_methods -p rfp005_testnet)
echo "ready: $WORK/testnet"
echo "  wallet  $WORK/testnet/target/release/wallet"
echo "  driver  $WORK/testnet/target/release/rfp005_testnet"
echo "  guest   $WORK/testnet/target/riscv-guest/test_methods/test_methods_guests/riscv32im-risc0-zkvm-elf/release/rfp005_multisig_execute.bin"
