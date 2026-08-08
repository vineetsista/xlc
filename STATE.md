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
Write gate4.sh (Law 12) — detectors ≥90% hand-audited precision on 200
samples each, proof strings, <2.5s drag-to-findings, partial-compile
reporting — then build detectors 1–3 (§8.8: inconsistent-formula-in-region,
range off-by-one vs siblings, #REF!-cone) over parsed formulas + the
precision harness. Phase 5 note: full-recompute receipt (schedule-driven)
is the IR invariant target; per-cell receipt is the Phase 3 artifact.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 99.35% subset / 98.81% full (verifiable) | functions implemented: ~75
detectors shipped: 0 | precision per detector: — | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
parse throughput: 253k formulas/s | receipt full-corpus: 0 panics
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
