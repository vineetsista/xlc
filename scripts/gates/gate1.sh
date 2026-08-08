#!/usr/bin/env bash
# gate(1): The census. Numeric assertions per Law 12.
#
#   1. corpus/census.json exists and covers >=95% of corpus workbooks
#      (examined / total, where total = all candidate OOXML workbooks found).
#   2. The function frequency table is present, non-empty, and rank-ordered.
#   3. STATE.md records the 99th-percentile function cut-off.
#   4. STATE.md records projected refusal rate AND projected partial-compile
#      rate as percentages.
set -u
cd "$(dirname "$0")/../.."
fail=0
say() { echo "gate(1): $*"; }

if [ ! -f corpus/census.json ]; then
    say "FAIL: corpus/census.json missing"
    exit 1
fi

python3 - <<'EOF' || fail=1
import json, sys
c = json.load(open("corpus/census.json"))
total, examined = c["workbooks_total"], c["workbooks_examined"]
cov = examined / total if total else 0.0
print(f"gate(1): coverage {examined}/{total} = {cov:.1%}")
if cov < 0.95:
    print("gate(1): FAIL: census coverage below 95%"); sys.exit(1)
funcs = c["functions"]
if not funcs:
    print("gate(1): FAIL: function frequency table empty"); sys.exit(1)
ranked = sorted(funcs.items(), key=lambda kv: -kv[1]["cells"])
if list(funcs) != [k for k, _ in ranked]:
    print("gate(1): FAIL: function table not stored rank-ordered"); sys.exit(1)
print(f"gate(1): ok: {len(funcs)} distinct functions, top = {ranked[0][0]} ({ranked[0][1]['cells']} cells)")
EOF

grep -qE "99th-percentile function cut-off: *[0-9]+" STATE.md \
    && say "ok: 99th-percentile cut-off recorded in STATE.md" \
    || { say "FAIL: 99th-percentile function cut-off not in STATE.md"; fail=1; }

grep -qE "refusal rate: *[0-9.]+%" STATE.md \
    && say "ok: projected refusal rate recorded" \
    || { say "FAIL: projected refusal rate not in STATE.md"; fail=1; }

grep -qE "partial-compile rate: *[0-9.]+%" STATE.md \
    && say "ok: projected partial-compile rate recorded" \
    || { say "FAIL: projected partial-compile rate not in STATE.md"; fail=1; }

exit $fail
