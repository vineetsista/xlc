#!/usr/bin/env bash
# gate(3): Graph, evaluator, THE RECEIPT. Numeric assertions per Law 12,
# against the committed artifact docs/benchmarks/receipt.json produced by
# `xlc receipt corpus/subset`:
#
#   1. receipt pass rate >= 97% of VERIFIABLE formula cells across the
#      committed 500-workbook subset. Verifiable = cells whose oracle
#      exists and which XLC compiles; Law 9 exclusions (external refs —
#      their oracle lives in a workbook we don't have — nondeterministic
#      volatiles, legacy array formulas, unimplemented functions) are
#      REPORTED beside the rate, never hidden, and may not exceed half
#      the subset (docs/decisions.md 2026-08-08).
#   2. per-function pass-rate table present and non-empty
#   3. every mismatch categorized (mismatch_classes non-empty if pass < 100%)
#   4. zero panics, including on a full-corpus receipt run
#      (docs/benchmarks/receipt-fullcorpus.json)
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

python3 - <<'PYEOF' || fail=1
import json, sys
a = json.load(open("docs/benchmarks/receipt.json"))
total, ok, panics = a["cells_total"], a["cells_pass"], a["panics"]
exc = a.get("excluded", {})
excluded_sum = sum(exc.values())
verifiable = total - excluded_sum
rate = ok / verifiable if verifiable else 0.0
print(f"gate(3): receipt {ok}/{verifiable} verifiable = {rate:.4%}")
print(f"gate(3): exclusions (reported, Law 9): {json.dumps(exc)}")
bad = 0
if rate < 0.97:
    print("gate(3): FAIL: receipt below 97% of verifiable cells"); bad = 1
if not exc or "external_ref" not in exc:
    print("gate(3): FAIL: exclusions not reported in artifact"); bad = 1
if verifiable < total * 0.5:
    print("gate(3): FAIL: over half the subset excluded — denominator gamed"); bad = 1
if panics != 0:
    print(f"gate(3): FAIL: {panics} panics"); bad = 1
if not a.get("per_function"):
    print("gate(3): FAIL: per-function pass-rate table missing/empty"); bad = 1
if ok < verifiable and not a.get("mismatch_classes"):
    print("gate(3): FAIL: mismatches exist but are not categorized"); bad = 1
if not a.get("ulp_policy"):
    print("gate(3): FAIL: ULP policy not documented in artifact"); bad = 1
sys.exit(bad)
PYEOF

# Full-corpus stability: a full-corpus receipt run must have completed
# panic-free (artifact committed by that run).
if [ -f docs/benchmarks/receipt-fullcorpus.json ]; then
    fp=$(python3 -c "import json;print(json.load(open('docs/benchmarks/receipt-fullcorpus.json'))['panics'])")
    if [ "$fp" != "0" ]; then say "FAIL: full-corpus receipt had $fp panics"; fail=1
    else say "ok: full-corpus receipt panic-free"; fi
else
    say "FAIL: docs/benchmarks/receipt-fullcorpus.json missing (full-corpus run required)"
    fail=1
fi

scc_out=$(cargo test -q -p xlc-graph scc 2>&1)
if echo "$scc_out" | grep -q "FAILED"; then
    say "FAIL: SCC tests failing"
    fail=1
elif echo "$scc_out" | grep -qE "test result: ok\. [1-9]"; then
    say "ok: SCC tests pass"
else
    say "FAIL: SCC tests absent"
    fail=1
fi

exit $fail
