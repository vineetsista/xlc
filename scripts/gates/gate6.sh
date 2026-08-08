#!/usr/bin/env bash
# gate(6): The scenario axis (§8.6). Numeric assertions per Law 12,
# against docs/benchmarks/scenario.json produced by `xlc monte-verify`:
#
#   1. N=1 ORACLE: with every distribution degenerate, scenario results
#      are bit-identical to the scalar interpreter on every cone cell of
#      the fixture set AND a corpus sample (mismatches == 0).
#   2. DETERMINISTIC MEAN: on a model with no uncertain inputs, the mean
#      across N=10_000 scenarios equals the scalar value exactly.
#   3. MOMENTS: each of the five distributions matches analytic mean and
#      variance within 5 sigma of the Monte Carlo standard error at
#      N=1_000_000 (recorded per distribution).
#   4. REPRODUCIBILITY: scenario k re-derived from (seed, k) alone equals
#      the value from the full sweep (counter-based RNG property).
#   5. bytes-moved-per-scenario recorded and within 3x the theoretical
#      minimum for the schedule (min = 8B * (cone writes + distinct
#      dep reads), measured against actual buffer traffic).
#   6. Native throughput recorded on a named public workbook
#      (cells x scenarios / s); browser figure recorded separately when
#      measured (absence marked, never blended).
set -u
cd "$(dirname "$0")/../.."
ART=docs/benchmarks/scenario.json

if [ ! -f "$ART" ]; then
    echo "gate(6): FAIL: $ART missing — run \`xlc monte-verify\` and commit the artifact"
    exit 1
fi

python3 - <<'PYEOF'
import json, sys
a = json.load(open("docs/benchmarks/scenario.json"))
bad = 0

n1 = a["n1_oracle"]
print(f"gate(6): N=1 oracle: {n1['cells'] - n1['mismatches']}/{n1['cells']} bit-identical")
if n1["mismatches"] != 0 or n1["cells"] < 1000:
    print("gate(6): FAIL: N=1 oracle not clean or too small"); bad = 1

dm = a["deterministic_mean"]
if not dm["exact"]:
    print("gate(6): FAIL: deterministic mean != scalar value"); bad = 1
else:
    print("gate(6): deterministic mean exact over", dm["scenarios"], "scenarios")

moments = a["moments"]
for name, m in moments.items():
    ok = m["mean_sigmas"] < 5 and m["var_sigmas"] < 5
    print(f"gate(6): {name}: mean {m['mean_sigmas']:.2f}σ, var {m['var_sigmas']:.2f}σ {'ok' if ok else 'FAIL'}")
    if not ok:
        bad = 1
if len(moments) < 5:
    print(f"gate(6): FAIL: only {len(moments)} distributions verified (need 5)"); bad = 1

if not a["reproducibility"]["exact"]:
    print("gate(6): FAIL: scenario k not reproducible from (seed, k)"); bad = 1
else:
    print("gate(6): reproducibility: scenario k re-derived exactly from (seed, k)")

bm = a["bytes_moved"]
ratio = bm["measured_per_scenario"] / max(bm["theoretical_min_per_scenario"], 1)
print(f"gate(6): bytes/scenario {bm['measured_per_scenario']:.0f} vs min {bm['theoretical_min_per_scenario']:.0f} ({ratio:.2f}x)")
if ratio > 3.0:
    print("gate(6): FAIL: bytes moved beyond 3x theoretical minimum"); bad = 1

tp = a["throughput"]
if not (tp.get("native_cells_x_scenarios_per_s", 0) > 0 and tp.get("workbook") and tp.get("machine")):
    print("gate(6): FAIL: native throughput not recorded with workbook+machine"); bad = 1
else:
    print(f"gate(6): native {tp['native_cells_x_scenarios_per_s']:.2e} cell-scenarios/s on {tp['workbook']}")
if "browser" not in tp:
    print("gate(6): note: browser figure absent — must be recorded before Phase 8 gate")

sys.exit(bad)
PYEOF
