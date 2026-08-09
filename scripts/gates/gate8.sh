#!/usr/bin/env bash
# gate(8): Surfaces and evidence. Numeric/behavioral assertions per Law 12:
#
#   1. CI MODE: `xlc check --ci` exits non-zero on a workbook with
#      findings and zero on a clean one; with --baseline, known findings
#      are accepted and NEW findings still fail.
#   2. BENCH: `make bench` produces docs/benchmarks/bench.json in one
#      command, recording machine, ISA feature set, and every headline
#      number's source artifact.
#   3. BROWSER: the built page serves and passes its checks under
#      COOP/COEP headers (docs/benchmarks/browser-coop.json written by
#      web/verify.mjs --coop), and the SharedArrayBuffer fallback is
#      documented in docs/methodology.md.
set -u
cd "$(dirname "$0")/../.."
fail=0
say() { echo "gate(8): $*"; }

BIN=target/release/xlc
[ -x "$BIN" ] || cargo build --release -p xlc-cli -q

# 1. CI exit codes.
if $BIN check tests/cases/slipped-ref.xlsx --ci > /dev/null 2>&1; then
    say "FAIL: --ci exited 0 on a workbook with findings"; fail=1
else
    say "ok: --ci exits non-zero on findings"
fi
if $BIN check tests/cases/basic-sum.xlsx --ci > /dev/null 2>&1; then
    say "ok: --ci exits 0 on a clean workbook"
else
    say "FAIL: --ci exited non-zero on a clean workbook"; fail=1
fi
tmp=$(mktemp)
$BIN check tests/cases/slipped-ref.xlsx --write-baseline "$tmp" > /dev/null 2>&1
if $BIN check tests/cases/slipped-ref.xlsx --ci --baseline "$tmp" > /dev/null 2>&1; then
    say "ok: --baseline accepts known findings"
else
    say "FAIL: --baseline did not accept known findings"; fail=1
fi
rm -f "$tmp"

# 2. Bench artifact.
if [ -f docs/benchmarks/bench.json ]; then
    python3 - <<'PYEOF' || fail=1
import json, sys
b = json.load(open("docs/benchmarks/bench.json"))
bad = 0
if not b.get("machine") or not b.get("isa_features"):
    print("gate(8): FAIL: machine/ISA not recorded"); bad = 1
for k in ("receipt", "scenario", "phase7", "analyze_timing"):
    if k not in b.get("artifacts", {}):
        print(f"gate(8): FAIL: bench missing source artifact {k}"); bad = 1
if bad == 0:
    print(f"gate(8): ok: bench.json on {b['machine'][:40]} [{','.join(b['isa_features'][:4])}...]")
sys.exit(bad)
PYEOF
else
    say "FAIL: docs/benchmarks/bench.json missing (run \`make bench\`)"; fail=1
fi

# 3. Browser under COOP/COEP.
if [ -f docs/benchmarks/browser-coop.json ]; then
    ok=$(python3 -c "import json;b=json.load(open('docs/benchmarks/browser-coop.json'));print(int(b.get('checks_failed',1)==0 and b.get('coop_coep_headers_present')))")
    if [ "$ok" = "1" ]; then say "ok: browser checks pass under COOP/COEP"; else say "FAIL: browser COOP/COEP checks"; fail=1; fi
else
    say "FAIL: docs/benchmarks/browser-coop.json missing (run web/verify.mjs --coop)"; fail=1
fi
grep -q "SharedArrayBuffer" docs/methodology.md \
    && say "ok: SharedArrayBuffer fallback documented" \
    || { say "FAIL: SharedArrayBuffer fallback not documented"; fail=1; }

exit $fail
