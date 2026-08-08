#!/usr/bin/env bash
# gate(3): Graph, evaluator, THE RECEIPT. Numeric assertions per Law 12,
# against the committed artifact docs/benchmarks/receipt.json produced by
# `xlc receipt corpus/subset`:
#
#   1. receipt pass rate >= 97% of formula cells across the committed
#      500-workbook subset (bit-identical under the documented ULP policy)
#   2. per-function pass-rate table present and non-empty
#   3. every mismatch categorized (mismatch_classes non-empty if pass < 100%)
#   4. zero panics across the full run
#   5. SCC unit tests pass (cargo test -p xlc-graph scc)
set -u
cd "$(dirname "$0")/../.."
ART=docs/benchmarks/receipt.json
fail=0
say() { echo "gate(3): $*"; }

if [ ! -f "$ART" ]; then
    say "FAIL: $ART missing — run \`xlc receipt corpus/subset\` and commit the artifact"
    exit 1
fi

python3 - <<'EOF' || fail=1
import json, sys
a = json.load(open("docs/benchmarks/receipt.json"))
total, ok, panics = a["cells_total"], a["cells_pass"], a["panics"]
rate = ok / total if total else 0.0
print(f"gate(3): receipt {ok}/{total} = {rate:.4%}")
bad = 0
if rate < 0.97:
    print("gate(3): FAIL: receipt below 97% on the committed subset"); bad = 1
if panics != 0:
    print(f"gate(3): FAIL: {panics} panics"); bad = 1
if not a.get("per_function"):
    print("gate(3): FAIL: per-function pass-rate table missing/empty"); bad = 1
if ok < total and not a.get("mismatch_classes"):
    print("gate(3): FAIL: mismatches exist but are not categorized"); bad = 1
if not a.get("ulp_policy"):
    print("gate(3): FAIL: ULP policy not documented in artifact"); bad = 1
sys.exit(bad)
EOF

if cargo test -q -p xlc-graph scc 2>&1 | tail -1 | grep -q "test result: ok"; then
    say "ok: SCC tests pass"
else
    say "FAIL: SCC tests failing or absent"
    fail=1
fi

exit $fail
