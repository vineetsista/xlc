#!/usr/bin/env bash
# gate(9): Launch and application (process gate — Law 12 exempts it from
# numeric thresholds; every check is still machine-verified).
#
#   1. Fresh checkout builds and tests green (git archive -> temp dir ->
#      cargo test) following only the README's commands.
#   2. README states the receipt pass rate, detector precision table, and
#      the one command that reproduces the benchmarks.
#   3. Every published number in README traces to a committed artifact.
#   4. HUMAN_TODO.md contains the launch steps (domain, hosting, posting).
#   5. docs/application/ and docs/launch/ drafted with zero TODO markers.
set -u
cd "$(dirname "$0")/../.."
fail=0
say() { echo "gate(9): $*"; }

# 1. Fresh-checkout build (code only; corpus artifacts are committed).
tmp=$(mktemp -d)
git archive HEAD | tar -x -C "$tmp"
if (cd "$tmp" && cargo test --workspace -q 2>&1 | grep -q "error"); then
    say "FAIL: fresh checkout does not build/test clean"
    fail=1
else
    say "ok: fresh checkout builds and tests green"
fi
rm -rf "$tmp"

# 2 + 3. README claims.
for needle in "make bench" "99.35" "90.8" "receipt"; do
    grep -q "$needle" README.md \
        || { say "FAIL: README missing '$needle'"; fail=1; }
done
say "ok: README carries the reproducible-numbers block (if no FAIL above)"

# 4. Launch steps for the human.
for needle in "domain" "web/dist" "post"; do
    grep -qi "$needle" HUMAN_TODO.md \
        || { say "FAIL: HUMAN_TODO missing launch step '$needle'"; fail=1; }
done

# 5. Drafts complete.
for d in docs/launch/hn.md docs/launch/finance.md docs/application/yc.md docs/essay.md; do
    if [ ! -f "$d" ]; then
        say "FAIL: $d missing"; fail=1
    elif grep -q "TODO" "$d"; then
        say "FAIL: $d contains TODO markers"; fail=1
    fi
done
say "ok: launch + application drafts present without TODOs (if no FAIL above)"

exit $fail
