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
Corpus-wide lint run in flight (detached, monitored). When it lands:
AUDIT the 200 samples per detector in docs/precision/*.json (verdict
tp/fp per sample, evidence-based), then `make gate-4`'s precision checks.
Remaining for gate 4 after audit: web drag-and-drop surface (xlc-wasm +
web/) with the three-act sequence. Timing artifact done: 58,389 formulas
in 0.51s. Phase 5 note: full-recompute receipt is the IR invariant.

## Numbers of Record
corpus workbooks: 16167 (6682 with formulas; 7003444 formula cells) | parse rate: 99.9970% | receipt pass rate: 99.35% subset / 98.81% full (verifiable) | functions implemented: ~75
detectors built: 3 (audit pending) | precision per detector: audit pending | refusal rate: 5.7% | partial-compile rate: 14.1%
99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
parse throughput: 253k formulas/s | receipt full-corpus: 0 panics
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
