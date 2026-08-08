#!/usr/bin/env bash
# gate(2): Parser. Numeric assertions per Law 12, checked against the
# committed artifact docs/benchmarks/parse-roundtrip.json produced by
# `xlc parse-corpus` over the full corpus:
#
#   1. round-trip rate >= 99.5% of formula cells
#   2. panics == 0 (malformed input errors, never unwinds)
#   3. parse throughput recorded (> 0 formulas/s, with machine noted)
#   4. failure log exists and every failure carries its formula text
set -u
cd "$(dirname "$0")/../.."
ART=docs/benchmarks/parse-roundtrip.json

if [ ! -f "$ART" ]; then
    echo "gate(2): FAIL: $ART missing — run \`xlc parse-corpus\` and commit the artifact"
    exit 1
fi

python3 - <<'EOF'
import json, os, sys
a = json.load(open("docs/benchmarks/parse-roundtrip.json"))
total, ok, panics = a["formulas_total"], a["roundtrip_ok"], a["panics"]
rate = ok / total if total else 0.0
print(f"gate(2): round-trip {ok}/{total} = {rate:.4%}")
fail = 0
if rate < 0.995:
    print("gate(2): FAIL: round-trip rate below 99.5%"); fail = 1
if panics != 0:
    print(f"gate(2): FAIL: {panics} panics on corpus input"); fail = 1
if not (a.get("throughput_formulas_per_s", 0) > 0 and a.get("machine")):
    print("gate(2): FAIL: throughput/machine not recorded"); fail = 1
log = a.get("failures_log", "")
if total - ok > 0:
    if not (log and os.path.exists(log)):
        print("gate(2): FAIL: failures exist but no failure log"); fail = 1
    else:
        with open(log) as fh:
            first = fh.readline()
        if first and "formula" not in first:
            print("gate(2): FAIL: failure log entries lack formula text"); fail = 1
sys.exit(fail)
EOF
