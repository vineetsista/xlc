#!/usr/bin/env python3
"""Phase 1 projections from the census (XLC.md Gate 1).

Reads census.json + census-workbooks.jsonl and reports:
  - the 99th-percentile function cut-off K (smallest top-K-by-cells set
    covering >=99% of function-mentioning formula cells)
  - projected refusal rate: workbooks with formulas where NO formula cell
    is compilable under S = top-K (a cell is blocked if its combo uses a
    function outside S or references an external workbook)
  - projected partial-compile rate: workbooks with some-but-not-all cells
    compilable
  - projected cell coverage: compilable cells / all formula cells

Methodology caveat (docs/methodology.md): per-cell feature detection with
no dependency-cone propagation — a blocked upstream cell also poisons its
downstream cone at compile time, so true partial-compile coverage is lower;
conversely volatile functions are treated as compilable (fixed-seed policy).

Usage: projection.py census.json census-workbooks.jsonl [--coverage 0.99]
"""
import json
import sys

EXTREF = "[EXTREF]"


def main() -> int:
    census_path, jsonl_path = sys.argv[1], sys.argv[2]
    coverage_target = 0.99
    if "--coverage" in sys.argv:
        coverage_target = float(sys.argv[sys.argv.index("--coverage") + 1])

    census = json.load(open(census_path))
    funcs = census["functions"]  # rank-ordered by cells desc
    total_mentions = sum(v["cells"] for v in funcs.values())
    cum, cutoff = 0, 0
    implemented = set()
    for name, v in funcs.items():
        if total_mentions and cum / total_mentions >= coverage_target:
            break
        implemented.add(name)
        cum += v["cells"]
        cutoff += 1

    n_formula_wbs = 0
    full = partial = refusal = 0
    cells_total = cells_ok = 0
    for line in open(jsonl_path):
        r = json.loads(line)
        if r["status"] != "ok" or r["formula_cells"] == 0:
            continue
        n_formula_wbs += 1
        ok = blocked = 0
        for combo in r["combos"]:
            compilable = all(f in implemented for f in combo["funcs"] if f != EXTREF) and (
                EXTREF not in combo["funcs"]
            )
            if compilable:
                ok += combo["cells"]
            else:
                blocked += combo["cells"]
        cells_total += ok + blocked
        cells_ok += ok
        if ok == 0:
            refusal += 1
        elif blocked == 0:
            full += 1
        else:
            partial += 1

    pct = lambda n, d: f"{100.0 * n / d:.1f}%" if d else "n/a"
    print(f"function cut-off for {coverage_target:.0%} cell coverage: {cutoff}")
    print(f"workbooks with formulas: {n_formula_wbs}")
    print(f"projected full-compile:  {full} ({pct(full, n_formula_wbs)})")
    print(f"projected partial-compile rate: {pct(partial, n_formula_wbs)}")
    print(f"projected refusal rate: {pct(refusal, n_formula_wbs)}")
    print(f"projected cell coverage under top-{cutoff}: {pct(cells_ok, cells_total)}")
    top = list(funcs.items())[:cutoff]
    tail = top[-5:] if cutoff >= 5 else top
    print("cut-off tail (least-frequent implemented):", ", ".join(n for n, _ in tail))
    return 0


if __name__ == "__main__":
    sys.exit(main())
