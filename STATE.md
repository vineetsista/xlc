# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: TBD (set after Phase 3 receipt trend)   |   Week 1
Phase: 3 — Graph, evaluator, THE RECEIPT          Gate: FAILING (expected — 9 functions implemented; baseline below)

## Done / In flight / Blocked
- Gate 0 PASS: corpus reproducible; 500-workbook subset committed (manifest,
  sha256 per file). Filter: 249,376 → 10,703 OOXML → 3,640 with formulas.
- Gate 1 PASS: census 16,167 wbs, 7,003,444 formula cells, 207 functions.
  Refusal 5.7% | partial 14.1% | full 80.3% | top-75 fns = 99% of mentions.
- Gate 2 PASS: parser round-trip 99.9970% byte-exact over 7M formulas,
  0 panics, 253k formulas/s. Remaining 212 = pasted curly-quote garbage.
- Phase 3 baseline (receipt, per-cell cached-context mode, subset):
  73.90% pass of 921,095 verifiable cells | 117,011 excl unimplemented |
  114,939 excl external-ref (Law 9) | 9,277 no-cached-value |
  7,962 numeric + 455 type mismatches | 0 panics.
- Blocked: none.

## Next action
Phase 3 function grind in census frequency order (IF✓ SUM✓ ISBLANK VLOOKUP
AVERAGE✓ IFERROR COUNTIFS TIME ISTEXT LEFT HLOOKUP TEXTAFTER ROUND✓ …),
re-running the subset receipt each batch; the per-function table in
corpus/work/receipt-baseline.json ranks impact. Then: defined-name ingest
(names currently #NAME?), Table-ref resolution, full-recompute mode via
xlc-graph schedule. DECIDE + record in decisions.md: gate-3 denominator =
verifiable cells (external-ref exclusions reported alongside, never hidden).

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 73.90% (subset baseline, 9 fns) | functions implemented: 9
detectors shipped: 0 | precision per detector: — | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 functions cover 99% of function-mentioning cells; ~180 budget will exceed 99.5%)
projected full-compile rate: 80.3% | projected cell coverage under top-75: 85.7%
parse throughput: 253k formulas/s (AMD/Intel per /proc — see docs/benchmarks/parse-roundtrip.json)
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
