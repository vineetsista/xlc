#!/usr/bin/env bash
# gate(7): Incremental recompute, AD, semantic diff (§8.7, §8.9, §8.10).
# Numeric assertions per Law 12, against docs/benchmarks/phase7.json:
#
#   1. INCREMENTAL: single-input-change recompute latency < 50 ms on a
#      >=40k-formula model at N=10^5 (schedule prebuilt; the slider case).
#   2. AD: reverse-mode gradients match central finite differences to
#      1e-6 relative on 50 random smooth models (max relative error and
#      model count recorded).
#   3. DIFF: on two workbook versions with a planted change, the semantic
#      diff reports divergence AND produces a concrete input vector with
#      both versions' outputs at that vector.
set -u
cd "$(dirname "$0")/../.."
ART=docs/benchmarks/phase7.json

if [ ! -f "$ART" ]; then
    echo "gate(7): FAIL: $ART missing — run \`xlc phase7-verify\` and commit the artifact"
    exit 1
fi

python3 - <<'PYEOF'
import json, sys
a = json.load(open("docs/benchmarks/phase7.json"))
bad = 0

inc = a["incremental"]
print(f"gate(7): incremental {inc['latency_ms']:.1f} ms | model {inc['model_formulas']} formulas | cone {inc['cone_cells']} | N={inc['scenarios']}")
if inc["latency_ms"] >= 50.0:
    print("gate(7): FAIL: incremental latency >= 50 ms"); bad = 1
if inc["model_formulas"] < 40_000 or inc["scenarios"] < 100_000:
    print("gate(7): FAIL: model/scenario scale below spec"); bad = 1

ad = a["ad"]
print(f"gate(7): AD {ad['models']} models, max rel err {ad['max_rel_err']:.2e}, {ad['gradients_checked']} gradients")
if ad["models"] < 50 or ad["max_rel_err"] > 1e-6:
    print("gate(7): FAIL: AD accuracy/count below spec"); bad = 1

d = a["diff"]
if not (d["divergence_detected"] and d.get("witness_scenario") is not None
        and d.get("v1_output") != d.get("v2_output")):
    print("gate(7): FAIL: semantic diff did not produce a divergent witness"); bad = 1
else:
    print(f"gate(7): diff witness: scenario {d['witness_scenario']}: v1 {d['v1_output']} vs v2 {d['v2_output']} ({d['divergent_pct']:.1f}% of sampled space diverges)")

sys.exit(bad)
PYEOF
