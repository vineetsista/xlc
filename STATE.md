# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: Phase 4 ships the first public product   |   Week 1
Phase: 6 — The scenario axis (the hard one)          Gate: UNWRITTEN (write gate6.sh first, Law 12)

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
Write gate6.sh (Law 12), then §8.6 in full: pulp SIMD kernels over the
coarsened IR, two-axis tiling with liveness-based buffer reuse (budget a
full run; bytes-moved-per-scenario is the metric), counter-based RNG
(Philox), vectorized inverse-normal (AS241) + exp/ln, five distributions
(normal/lognormal/triangular/uniform/PERT), auto-detect RAND/NORM.INV
cells + auto-propose hardcoded assumptions. Oracles: N=1 bit-identical to
receipt; deterministic mean == scalar; moments match analytic.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 99.35% subset / 98.81% full (verifiable) | functions implemented: ~75
detectors SHIPPING: ref-error 100% (200/200) · range-off-by-one 100% (34/34 census) · inconsistent-region v4 90.8% (59/65 census; v1 rejected 27%, v2 72%) | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
parse throughput: 253k formulas/s | receipt full-corpus: 0 panics
analyzer: 58,389 formulas native 0.52s / browser wasm 4.19s (single-thread)
IR: 930,372/930,372 bit-identical vs scalar | coarsening 11.20x (83,095 nodes) | CSE 0.8%
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
