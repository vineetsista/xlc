#!/usr/bin/env bash
# gate(0): Bootstrap and corpus (process gate — Law 12 exempts it from a
# numeric threshold, but every check here is still machine-verified).
#
# Asserts:
#   1. corpus/Makefile pins a sha256 for every remote source (no PENDING).
#   2. Any fetched raw archive matches its pinned sha256.
#   3. corpus/manifest.json is committed, lists exactly 500 workbooks.
#   4. Every subset workbook exists and matches its manifest sha256.
#   5. Every subset workbook loads (calamine open + >=1 formula) via
#      `xlc corpus-verify` — "committed and loads".
#   6. The dispatcher returns 2 for an unwritten gate (self-test).
set -u
cd "$(dirname "$0")/../.."
fail=0
say() { echo "gate(0): $*"; }

# 1. Pinned checksums present.
if grep -qE 'SHA256[A-Z_]* *:?= *(PENDING|TODO|)$' corpus/Makefile; then
    say "FAIL: corpus/Makefile has unpinned sha256 entries"; fail=1
else
    say "ok: all corpus sources have pinned sha256"
fi

# 2. Raw archives, if present, match their pins.
if ! make -C corpus -s verify-raw; then
    say "FAIL: fetched raw archives do not match pinned sha256"; fail=1
else
    say "ok: raw archives verify (or are absent — fetched on demand)"
fi

# 3. Manifest committed with exactly 500 entries.
if [ ! -f corpus/manifest.json ]; then
    say "FAIL: corpus/manifest.json missing"; fail=1
else
    n=$(python3 -c "import json;print(len(json.load(open('corpus/manifest.json'))['workbooks']))" 2>/dev/null || echo 0)
    if [ "$n" -ne 500 ]; then say "FAIL: manifest lists $n workbooks, want 500"; fail=1
    else say "ok: manifest lists 500 workbooks"; fi
fi

# 4. Subset files exist and hash-match the manifest.
if [ -f corpus/manifest.json ]; then
    if python3 scripts/verify_manifest.py corpus/manifest.json corpus/subset; then
        say "ok: all subset files present with matching sha256"
    else
        say "FAIL: subset files missing or hash mismatch"; fail=1
    fi
fi

# 5. Subset loads: calamine opens every file and finds >=1 formula cell.
if [ -x target/release/xlc ] || cargo build --release -p xlc-cli -q; then
    if target/release/xlc corpus-verify corpus/subset > /dev/null; then
        say "ok: all 500 subset workbooks load with >=1 formula cell"
    else
        say "FAIL: xlc corpus-verify rejected subset workbooks"; fail=1
    fi
else
    say "FAIL: could not build xlc-cli"; fail=1
fi

# 6. Dispatcher self-test: a gate that does not exist must return 2.
bash scripts/gate.sh 99 > /dev/null 2>&1
if [ $? -eq 2 ]; then say "ok: dispatcher returns 2 for unwritten gates"
else say "FAIL: dispatcher did not return 2 for an unwritten gate"; fail=1; fi

exit $fail
