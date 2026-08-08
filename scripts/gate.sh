#!/usr/bin/env bash
# Gate dispatcher. Usage: gate.sh <N>
# Exit: 0 pass | 1 fail | 2 unwritten | 3 unmeasurable here
set -u
N="${1:?usage: gate.sh <phase-number>}"
GATE="$(dirname "$0")/gates/gate${N}.sh"

if [ ! -f "$GATE" ]; then
    echo "gate(${N}): unwritten — write scripts/gates/gate${N}.sh first (Law 12)"
    exit 2
fi
bash "$GATE"
