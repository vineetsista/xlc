# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: Phase 4 ships the first public product   |   Week 1
Phase: 4 — The analyzer, first launchable product          Gate: UNWRITTEN (write gate4.sh first, Law 12)

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
v2 lint sweep in flight (tightened slipped-reference detector; v1 was
REJECTED by its own audit at ~25-30% — docs/precision/inconsistent-region-v1.md).
When it lands: audit fresh inconsistent-region samples + re-verify
off-by-one population (34/34 tp recorded) + ref-error (200/200 recorded).
Then: amend gate4 for full-population audits (<200 findings), run gate-4
precision checks. Remaining after: web drag-and-drop surface (xlc-wasm).
Timing artifact done: 58,389 formulas in 0.51s.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 99.35% subset / 98.81% full (verifiable) | functions implemented: ~75
detectors: ref-error 100% (200/200) · range-off-by-one 100% (34/34, full population) · inconsistent-region v2 audit pending (v1 rejected ~27%) | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
parse throughput: 253k formulas/s | receipt full-corpus: 0 panics
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
