# XLC — STATE
Updated: 2026-08-09   |   LAUNCH: awaiting human steps (HUMAN_TODO.md)   |   Week 1
Phase: 9 — COMPLETE + SHIPPED. All ten gates PASS. Repo public:
https://github.com/vineetsista/xlc · Pages: https://vineetsista.github.io/xlc/

## Done / In flight / Blocked
- Every phase 0-9 complete; every gate written before its code (Law 12).
- Repo public + GitHub Pages enabled via API (deploys web/dist from main).
- Web v2 shipped AND live-verified (2026-08-09): one-click sample,
  receipt breakdown, findings triage (chips/copy/keyboard/suppression),
  scenario lab (wasm Session: prepare-once cone re-eval, what-if slider,
  response curve, 10k Monte-Carlo histogram), version diff with witness.
- Web v3 (2026-08-09): caret diagnostics (§9 cashed), sticky nav +
  scrollspy, ctrl-k command palette, help overlay, tornado sensitivity
  (±10% per ranked input, validated 2-hue pair), goal seek (bisection
  on the cone), histogram/CDF toggle + p5/p50/p95 markers, markdown
  report export, a11y pass (aria-modal + focus trap, combobox wiring,
  chart aria-labels, aria-live toast). Adversarial review (3 lenses ×
  refuters) found 18 real issues — all fixed. Suite 28 → 49 checks.
- Live demo + essay published as artifacts (private until shared).
- LibreOffice installed user-space (no sudo); all 5,194 disputed cells
  adjudicated: 49.6% oracle noise / 41.5% real gaps / 8.9% three-way.
- Blocked (human-only, Law 13): posting the launch drafts; custom domain;
  payment account when pricing exists. That is the whole list.

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
tests: 96 | clippy: 0 | rustfmt: clean | gates: 10/10 PASS | oracle adjudication: 2575/2156/463 (noise/real/3-way)

## Outside conversations
last: never | count: 0 — launch drafts ready; posting is human-only.
