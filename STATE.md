# XLC — STATE
Updated: 2026-08-09   |   LAUNCH: awaiting human steps (HUMAN_TODO.md)   |   Week 1
Phase: 9 — COMPLETE. All ten gates PASS (`make gate-all` green 0-9).

## Done / In flight / Blocked
- Every phase 0-9 complete; every gate written before its code (Law 12).
- Blocked: launch itself — human-only steps in HUMAN_TODO.md (domain,
  hosting web/dist, posting docs/launch/*, LibreOffice-UNO install for
  oracle adjudication). Nothing blocks further engineering.

## Next action
Human: work through HUMAN_TODO.md. Agent (post-launch backlog, spec
"Later" section + measured gaps): TEXT function number-format engine
(21,936 census cells), INDIRECT/OFFSET static-resolvable subset,
LET/LAMBDA if census justifies, pow-precision class (~2.3k cells),
LO-UNO adjudication of the stale-cache mismatch list, wasm threads
behind COOP/COEP, more detectors through the Law-7 audit loop,
range-axis tiling for very-wide live sets, Cranelift fusion (v2 slide).

## Numbers of Record
corpus: 16,167 wbs / 7,003,444 formula cells | parse 99.9970% RT, 0 panics
receipt: 99.35% subset / 98.81% full corpus (verifiable), 0 panics | ULP policy: bit|1ulp|sig15
functions: ~75 implemented | 99th-percentile function cut-off: 75 (top-75 = 99% of cell-mentions)
refusal rate: 5.7% | partial-compile rate: 14.1% (projected, census)
detectors: ref-error 100% (200) · off-by-one 100% (34 census) · slipped-ref 90.8% (65 census)
analyzer: 58,389 formulas 0.52s native / 4.19s wasm | IR coarsening 11.20x, 0 drift
scenario: N=1 oracle 21,047/21,047 | moments <=1.6 sigma | (seed,k) reproducible
bytes/scenario 1.00x min (residency witness 2.5MB/8MB) | fast path 1.45e8 cell-scen/s synthetic
incremental 12.6ms @ 40k formulas x 1e5 | AD 8.0e-7 max rel err (232 gradients) | diff witness exact
tests: 96 | clippy: 0 warnings | gates: 10/10 PASS

## Outside conversations
last: never | count: 0 — launch drafts ready; posting is human-only.
