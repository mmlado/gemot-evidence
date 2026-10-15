#!/usr/bin/env bash
# LEZ's own privacy-circuit benchmark, at the commit its published numbers come from and at v0.3.0.
# Two local edits to the bench: drop the chain-call depth sweep, which takes hours on a CPU, and at v0.3.0
# make --ppe skip the executor table, which panics at that release (fixed in v0.3.1, commit 37f971055).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${1:-$ROOT/work}"
OUT="$ROOT/results"; mkdir -p "$OUT"
[ -d "$WORK/lez" ] || git clone https://github.com/logos-blockchain/logos-execution-zone.git "$WORK/lez"
for pair in "bench-0901 2811d0601" "bench-v030 db66590ab"; do
  set -- $pair; d="$WORK/$1"
  [ -d "$d" ] || git -C "$WORK/lez" worktree add --detach "$d" "$2"
  python3 - "$d/tools/cycle_bench/src/ppe.rs" "$d/tools/cycle_bench/src/main.rs" "$1" <<'PY'
import sys
ppe, main, name = sys.argv[1:4]
s = open(ppe).read()
s = s.replace("for depth in [1_u32, 3, 5, 9] {", "for depth in [0_u32; 0] {")
open(ppe, "w").write(s)
if name == "bench-v030":
    m = open(main).read()
    guard = '    #[cfg(feature = "ppe")]\n    if cli.ppe {\n        let results = ppe::run_all();\n        ppe::print_table(&results);\n        return Ok(());\n    }\n'
    if guard not in m:
        m = m.replace("    let cli = Cli::parse();\n", "    let cli = Cli::parse();\n" + guard, 1)
        open(main, "w").write(m)
PY
  (cd "$d" && export CARGO_TARGET_DIR="$PWD/target" && unset RISC0_DEV_MODE && cargo run --release -p cycle_bench --features ppe -- --ppe) 2>&1 | tee "$OUT/lez-ppe-$1.log" | grep -E "in PPE|label"
done
