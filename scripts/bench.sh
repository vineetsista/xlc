#!/usr/bin/env bash
# One-command benchmark suite (Gate 8): re-runs every headline measurement
# and assembles docs/benchmarks/bench.json with machine + ISA recorded.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p xlc-cli -q
target/release/xlc receipt corpus/subset --out docs/benchmarks/receipt.json
target/release/xlc monte-verify --out docs/benchmarks/scenario.json
target/release/xlc phase7-verify --out docs/benchmarks/phase7.json
target/release/xlc check corpus/work/fuse-bins/cc-binaries/56cbca12-9c02-49e5-a064-53129669df80 \
    --timing-out docs/benchmarks/analyze-timing.json > /dev/null
python3 - <<'PYEOF'
import json, platform, re
cpu = open('/proc/cpuinfo').read()
model = re.search(r'model name\s*:\s*(.+)', cpu).group(1)
flags = re.search(r'flags\s*:\s*(.+)', cpu).group(1).split()
isa = [f for f in ('sse2','avx','avx2','fma','avx512f','avx512vl') if f in flags]
bench = {
  "machine": model,
  "kernel": platform.release(),
  "isa_features": isa,
  "command": "make bench",
  "artifacts": {
    "receipt": "docs/benchmarks/receipt.json",
    "scenario": "docs/benchmarks/scenario.json",
    "phase7": "docs/benchmarks/phase7.json",
    "analyze_timing": "docs/benchmarks/analyze-timing.json",
    "parse_roundtrip": "docs/benchmarks/parse-roundtrip.json",
    "ir": "docs/benchmarks/ir.json",
  },
}
json.dump(bench, open('docs/benchmarks/bench.json','w'), indent=1)
print("bench.json written:", model, isa)
PYEOF
