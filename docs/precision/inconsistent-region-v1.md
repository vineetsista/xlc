# inconsistent-region v1 — REJECTED by its own audit (Law 7)

First corpus run: 6,565 findings. Audit of the first 50 of 200 sampled
findings measured precision at roughly 25–30% — far below the 90% shipping
floor — with a crisp false-positive taxonomy:

| class | examples | share of audited fps |
|---|---|---|
| subtotal/summary row embedded in a run | `O107=SUM(O49:O106)` inside a `P49+Q49` family | ~20% |
| alternating two-pattern regions (run windowing manufactures a fake majority) | `(M332+M384+M436)/3` rows interleaved with `AVERAGE(L332,...)` rows | ~50% |
| seed cells / anchor pinning | `D12=$C$12*(1+$B$12)` heading a `*(1+$B$12)` family | ~5% |
| deliberate plugs and overrides | `+B11-7-1`, `SUMIF(...)*1.5` | ~15% |

The true positives were almost exclusively one class: **same structure,
slipped reference** (`H1306/G1306` in a `H1300/F1300` family). v2 ships
that class only, with three added guards: structural equality, no
relative↔absolute anchor flips, and minority-shape-unique-per-sheet.
Raw v1 samples preserved in `inconsistent-region-v1-rejected.json.bak`.

This document exists because Law 7 requires precision to be a measured,
gated artifact — including the failures.
