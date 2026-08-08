# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: TBD (set after Phase 3 receipt trend)   |   Week 1
Phase: 1 — The census          Gate: run `make gate-1`

## Done / In flight / Blocked
- Done Phase 0 (gate PASSING): corpus reproducible from clean checkout; FUSE
  md5-verified + sha256-pinned; 500-workbook subset committed with manifest;
  filter stats: 249,376 scanned → 10,703 valid OOXML → 3,640 with formulas,
  0 errors, 0 panics.
- Done Phase 1 census: 16,167 workbooks (fuse 10,703 + sb 5,464), 100%
  examined, 6,682 with formulas, 7,003,444 formula cells, 207 distinct
  functions. Features: VBA 6 wbs (!), external links 1,123, pivots 93,
  power-query machinery 825, 1904-epoch 240, volatile-any 377.
- Built ahead (Never-Stall rung 6, all tested): full Pratt parser +
  parse-corpus oracle (Phase 2), Tarjan SCC, value model, date serials,
  interpreter skeleton + 9 builtins, workbook model, receipt command
  (per-cell cached-context mode) (Phase 3).
- Blocked: none.

## Next action
Run `make gate-1`; commit; then Phase 2: parse-corpus over the full corpus,
fix top round-trip failures to ≥99.5%.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: — | receipt pass rate: — | functions implemented: 9
detectors shipped: 0 | precision per detector: — | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 functions cover 99% of function-mentioning cells; ~180 budget will exceed 99.5%)
projected full-compile rate: 80.3% | projected cell coverage under top-75: 85.7%
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
