# HANDOFF — resume point after context clear

Written 2026-08-09, mid-verification of the web v2 release. Read this,
then `STATE.md`, then work. (`XLC.md` is the constitution; the Session
Protocol in it still applies.)

## Where things stand right now

Everything through **web v2 is committed AND pushed** (last commit:
"web v2: scenario lab …"). The push triggered the `deploy-pages`
workflow; I was interrupted while watching it. **The live site's v2
deploy is the one unverified thing.**

Shipped and verified before the interruption:

- **Web v2 features** (28/28 headless-Chromium checks green locally,
  `web/verify.mjs`): one-click embedded sample button; count-up acts;
  clickable receipt line expanding the exact/1-ulp/sig15 + exclusions
  breakdown; detector filter chips; copy-proof buttons; `j/k/x/c`
  keyboard triage; **scenario lab** — input picker ranked by impact,
  live what-if slider with µs readout, canvas response curve with hover
  tooltip, Monte-Carlo (10k scenarios) with histogram + bin tooltips +
  p5/p50/p95 stats; **compare versions** — drop a second file, get
  divergence % and a witness input vector.
- **wasm `Session`** (crates/xlc-wasm): persistent workbook + prepared
  engine across calls — `prepare()` builds cone/schedule once,
  `what_if()` re-evaluates only the cone (that's why the slider is
  instant). New engine API: `Engine::eval_with_inputs` (explicit input
  values, no dist sampling); `auto_inputs` moved to
  `xlc_scenario::engine` (CLI updated). `diff_books()` free function.
  NOTE: Session uses a documented self-referential borrow
  (engine borrows Box<Workbook>, engine declared first so it drops
  first) — see the SAFETY comment before touching it.
- **Artifact demo updated + verified** (single-file build via
  `vite.single.config.js` → `web/dist-single/`, republished at the SAME
  URL): https://claude.ai/code/artifact/02d27d90-befe-44db-8195-d2e8525e6595
- Chart colors follow the dataviz method; mark hue `#2F81F7` validated
  against the `#0D1117` surface.
- Diff test fixture: `tests/cases/diff-b-sample.xlsx` (sample with
  Contingency 25k→40k, cache-consistent).

## Next session — do these in order

1. Check the Pages deploy:
   `gh run list --repo vineetsista/xlc --limit 3` — expect the "web v2"
   run `completed success`. If it failed, `gh run view <id> --log-failed`.
2. Smoke the LIVE site https://vineetsista.github.io/xlc/ with
   playwright (pattern in `web/verify.mjs`, or reuse the inline live
   test from the session log): click `#try-sample`, expect
   `2 defects found`; click `#run-monte`, expect stats with `mean`;
   move `#whatif`, expect the readout to change. Zero console errors.
3. `make gate-all` — expect 10/10 (gate-8 reads the regenerated
   `docs/benchmarks/browser-coop.json`, now 28 checks). Fix anything red.
4. Update `STATE.md` + `docs/build-log.md` with the web v2 entry
   (NOT yet written), commit, push.
5. Optional polish captured but not done: mention the scenario lab in
   README's demo section; consider a short GIF for the repo README
   (human can record, or skip).

## The user asked mid-turn: "what else should I do?" — answer to deliver

Strategic list (beyond the checklist in HUMAN_TODO.md):
- **Before posting HN**: run 2–3 real workbooks from your own life
  through the live site; the first comment will be "did you try real
  files?" — have an anecdote.
- **Collect the first testimonial**: send the link to one finance
  friend with the sample workbook; screenshot their reaction.
- **Set up GitHub repo niceties**: enable Issues, add the
  `docs/essay.md` link to the repo About field (gh api or web UI).
- **Engineering backlog** (in STATE.md): ^-precision class (the 41.5%
  of adjudicated real gaps), TEXT function, wasm threads under the
  already-shipped COOP/COEP headers, more detectors through the Law-7
  audit loop, INDIRECT/OFFSET static subset.
- **Consider**: a 60-second screen recording of drop→receipt→slider for
  the HN comment thread; recordings outperform screenshots there.

## Key files touched this session (all committed)

- `crates/xlc-wasm/src/lib.rs` — Session, diff_books
- `crates/xlc-scenario/src/engine.rs` — eval_with_inputs, auto_inputs
- `crates/xlc-cli/src/monte_verify.rs` — uses shared auto_inputs
- `web/index.html`, `web/src/style.css`, `web/src/main.ts`,
  `web/src/sample.ts` (embedded sample), `web/verify.mjs` (28 checks),
  `web/vite.single.config.js`, `web/tsconfig.json` (lib: ESNext)
- `tests/cases/diff-b-sample.xlsx`

## Invariants to keep (constitution digest)

Gates before code (`gate(N):` commits) · nothing leaves the user's
machine · exclusions always printed next to any rate · detectors ship
only ≥90% audited precision · no JIT · stable Rust · posting/accounts/
money remain human-only (see HUMAN_TODO.md).
