#!/usr/bin/env bash
# gate(5): Typed IR and coarsening (§8.5). Numeric assertions per Law 12,
# against docs/benchmarks/ir.json produced by `xlc ir-verify corpus/subset`:
#
#   1. NO SEMANTIC DRIFT: the IR interpreter's per-cell results equal the
#      scalar interpreter's bit-for-bit on every formula cell of the
#      committed 500-workbook subset (mismatches == 0). This is the §8.5
#      invariant: the IR must still reproduce the receipt exactly.
#   2. Coarsening ratio recorded and real: formula cells / vector nodes
#      >= 2.0 across the subset (copied families are the norm in real
#      workbooks; a ratio near 1 means coarsening is not happening).
#   3. CSE/DCE reduction measured: insts_before / insts_after recorded,
#      with per-workbook coarsening ratios present.
set -u
cd "$(dirname "$0")/../.."
ART=docs/benchmarks/ir.json

if [ ! -f "$ART" ]; then
    echo "gate(5): FAIL: $ART missing — run \`xlc ir-verify corpus/subset\` and commit the artifact"
    exit 1
fi

python3 - <<'PYEOF'
import json, sys
a = json.load(open("docs/benchmarks/ir.json"))
bad = 0
cells, mism = a["cells_compared"], a["mismatches"]
print(f"gate(5): IR vs scalar: {cells - mism}/{cells} bit-identical")
if mism != 0:
    print(f"gate(5): FAIL: {mism} semantic drift cells"); bad = 1
if cells < 500_000:
    print(f"gate(5): FAIL: only {cells} cells compared — subset not covered"); bad = 1
ratio = a["coarsening"]["cells"] / max(a["coarsening"]["nodes"], 1)
print(f"gate(5): coarsening {a['coarsening']['cells']} cells -> {a['coarsening']['nodes']} nodes (ratio {ratio:.2f})")
if ratio < 2.0:
    print("gate(5): FAIL: coarsening ratio below 2.0"); bad = 1
if not a.get("per_workbook_ratios_recorded"):
    print("gate(5): FAIL: per-workbook coarsening ratios not recorded"); bad = 1
ib, ia = a["cse"]["insts_before"], a["cse"]["insts_after"]
if not (ib > 0 and 0 < ia <= ib):
    print("gate(5): FAIL: CSE/DCE inst counts not recorded sanely"); bad = 1
else:
    print(f"gate(5): CSE/DCE: {ib} -> {ia} insts ({100*(ib-ia)/ib:.1f}% reduction)")
sys.exit(bad)
PYEOF
