#!/usr/bin/env bash
# gate(4): The analyzer — first launchable product. Numeric assertions per
# Law 12:
#
#   1. >=3 detectors, each with a precision audit in docs/precision/
#      <detector>.json: >=200 sampled findings, each sample carrying the
#      finding's proof string and an audit verdict; measured precision
#      >= 0.90.
#   2. Every finding in the committed sample files carries a non-empty
#      proof string (Law 8).
#   3. docs/benchmarks/analyze-timing.json records end-to-end
#      parse+analyze under 2.5 s on a >=40k-formula workbook, with the
#      workbook and machine named.
#   4. `xlc check` on a workbook with excluded cells reports exclusions
#      per feature (partial compilation, never refusal): the timing
#      artifact carries a capability_report with per-feature counts.
set -u
cd "$(dirname "$0")/../.."
fail=0
say() { echo "gate(4): $*"; }

python3 - <<'PYEOF' || fail=1
import glob, json, sys
bad = 0
audits = sorted(glob.glob("docs/precision/*.json"))
ok_detectors = 0
for path in audits:
    a = json.load(open(path))
    name = a.get("detector", path)
    samples = a.get("samples", [])
    n = len(samples)
    total = a.get("findings_total", n)
    # 200 samples required — unless the detector's entire corpus population
    # is smaller, in which case the audit must cover ALL of it (a census
    # beats a sample) and still comprise at least 20 findings
    # (docs/decisions.md 2026-08-08, full-population amendment).
    if n < 200 and not (n == total and n >= 20):
        print(f"gate(4): {name}: {n} samples of {total} findings — below the audit floor")
        continue
    missing_proof = sum(1 for s in samples if not s.get("proof"))
    unaudited = sum(1 for s in samples if s.get("verdict") not in ("tp", "fp"))
    tp = sum(1 for s in samples if s.get("verdict") == "tp")
    if missing_proof:
        print(f"gate(4): FAIL: {name}: {missing_proof} samples lack proof strings"); bad = 1
        continue
    if unaudited:
        print(f"gate(4): {name}: {unaudited} samples unaudited — not counted")
        continue
    precision = tp / n
    print(f"gate(4): {name}: precision {tp}/{n} = {precision:.1%}")
    if precision >= 0.90:
        ok_detectors += 1
    else:
        print(f"gate(4): {name}: below 90% — does not ship (Law 7)")
if ok_detectors < 3:
    print(f"gate(4): FAIL: {ok_detectors} detectors at >=90% precision (need 3)"); bad = 1
else:
    print(f"gate(4): ok: {ok_detectors} detectors ship at >=90% precision")
sys.exit(bad)
PYEOF

python3 - <<'PYEOF' || fail=1
import json, sys
try:
    t = json.load(open("docs/benchmarks/analyze-timing.json"))
except FileNotFoundError:
    print("gate(4): FAIL: docs/benchmarks/analyze-timing.json missing"); sys.exit(1)
bad = 0
if t.get("formula_cells", 0) < 40_000:
    print(f"gate(4): FAIL: timing workbook has {t.get('formula_cells')} formulas (<40k)"); bad = 1
if not (0 < t.get("elapsed_s", 99) < 2.5):
    print(f"gate(4): FAIL: analyze took {t.get('elapsed_s')}s (>=2.5s)"); bad = 1
if not t.get("machine") or not t.get("workbook"):
    print("gate(4): FAIL: machine/workbook not named in timing artifact"); bad = 1
cr = t.get("capability_report")
if not cr or not isinstance(cr, dict) or not cr:
    print("gate(4): FAIL: capability_report (per-feature exclusions) missing"); bad = 1
if bad == 0:
    print(f"gate(4): ok: {t['formula_cells']} formulas analyzed in {t['elapsed_s']:.2f}s on {t['machine'][:40]}")
sys.exit(bad)
PYEOF

exit $fail
