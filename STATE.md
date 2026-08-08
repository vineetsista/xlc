# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: Phase 4 ships the first public product   |   Week 1
Phase: 5 — Typed IR and coarsening          Gate: UNWRITTEN (write gate5.sh first, Law 12)

## Done / In flight / Blocked
- Gates 0–3 ALL PASSING (`make gate-all` green end-to-end).
- Gate 3 receipt: 99.3529% of 802,705 verifiable subset cells bit-match
  Excel under the documented policy (bit-identical | 1 ULP | 15-sig-digit
  decimal). Full corpus: 98.81% of 4.68M verifiable cells, ZERO panics
  across 16,167 workbooks. Exclusions (Law 9, always printed): extref
  116,253 · array 6,967 · unimpl 1,716 · volatile 186.
- ~75 functions implemented; every function with ≥200 uses is ≥97%.
- Known residual classes (docs/decisions.md): oracle noise (stale caches,
  display-rounded caches — LO-UNO adjudicator in HUMAN_TODO), pow-precision
  (^ chains, ~2.3k cells), scattered tail.
- Blocked: none.

## Next action
Phase 4 DONE (all criteria incl. real-browser verification). Write
gate5.sh (Law 12): IR interpreter matches scalar receipt bit-for-bit on
the 500-subset; coarsening ratio recorded per workbook; CSE/DCE reduction
measured; no semantic drift. Then build xlc-ir: typed SSA-ish dataflow
IR, copied-family coarsening (one vector node of width W), constant
folding, CSE, DCE (§8.5). The full-recompute receipt (schedule-driven via
xlc-graph) is the natural first consumer.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 99.35% subset / 98.81% full (verifiable) | functions implemented: ~75
detectors SHIPPING: ref-error 100% (200/200) · range-off-by-one 100% (34/34 census) · inconsistent-region v4 90.8% (59/65 census; v1 rejected 27%, v2 72%) | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
parse throughput: 253k formulas/s | receipt full-corpus: 0 panics
analyzer: 58,389 formulas native 0.52s / browser wasm 4.19s (single-thread)
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
