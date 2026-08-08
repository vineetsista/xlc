# XLC — Master Build Prompt

> **How to use this file.** Save it as `XLC.md` in an empty directory. Open Claude Code there and paste, verbatim:
>
> ```
> Read XLC.md in full. It is the complete specification for a project we are building
> together across many long sessions. Follow its Session Protocol exactly. Begin at
> Phase 0. Do not skip gates. Do not ask permission to continue — work until you hit a
> gate you cannot pass or an item that belongs in HUMAN_TODO.md, then say precisely why.
> ```
>
> Every later session opens with the same sentence; the Session Protocol recovers state from disk.
>
> **This specification was adversarially verified before you received it.** A red-team agent fetched every dependency live, searched GitHub and Hacker News for prior art, and fetched four competitors' pricing pages. It found three factual errors in the original proposal and one architectural claim that was impossible. All are corrected below. Where this document says something surprising — *don't JIT*, *lead with the analyzer not the simulation*, *measure the refusal rate before building anything fun* — it is surprising because the naive version was checked and shown to fail.

---

# PART 0 — RUNNING THIS IN CLAUDE CODE

**Why this project is unusually well-suited to autonomous building: the oracle is inside the input.** Every `.xlsx` stores the computed value of every formula cell alongside the formula. `calamine` v0.36.1 exposes both in one struct (`XlsxCellFormula` — *"containing both cached/literal value and expanded formula text"*, verified). So every run can compile a workbook, recompute it, and bit-diff against Excel's own answers across 16,000 real files, with no human in the loop. Nothing here needs a human to say "looks right."

**Repo setup.** Keep `CLAUDE.md` under ~200 lines: build commands (`make gate`, `make corpus`, `make receipt`), unchanging conventions, and a pointer to this file. `STATE.md` holds mutable state and is re-read every session. Pre-approve `cargo build/test/clippy/bench`, `make`, and `git` in `.claude/settings.json` so long runs aren't interrupted.

**Compaction.** Run `/compact` at phase boundaries, never mid-debug. End sessions cleanly — state written, committed — rather than pushing until quality degrades. `claude --continue` the next day.

**Parallelism.** This build is sequential with gates, which is the right shape. Use subagents for bounded read-heavy work (auditing a crate, researching Excel's coercion rules for one function family, reviewing a diff). Worktrees only where work is genuinely independent — the analyzer and the scenario engine can proceed in parallel once the IR lands in Phase 4.

**No GPU anywhere, by design.** Every perf gate runs on CPU and is therefore fully machine-checkable in a container. Unlike a graphics project, there is no unverifiable-without-hardware hole here — but do record which machine produced each benchmark, since AVX-512 and NEON differ.

**Calibration.** Anthropic's 16-agent project produced ~100,000 lines of Rust in ~2 weeks for ~$20k. This is comparable in scale and far better-oracled. Work in 60–90 minute sessions ending at gates.

---

# PART I — THE CONSTITUTION

## 1. Mission

Build **XLC**: an optimizing compiler for Excel.

It parses a `.xlsx` into a typed dataflow IR, **proves** it re-derives every value Excel cached, **finds real defects** with machine-checkable evidence, and then runs **a million full-workbook scenarios** by adding a vectorized scenario axis to the IR — all on CPU, all in the browser or a CLI, with nothing ever uploaded.

**Positioning, in one sentence:** *Excel is the most widely used programming language on Earth, and it is the only one with no compiler, no type checker, no linter, no test framework, and no CI.*

**The reaction we're engineering for:** *one person should not have been able to build this* — from an audience that then immediately drops their own workbook onto it.

## 2. The Fourteen Laws

1. **Nothing ever leaves the user's machine.** No upload, no backend, no server-side compute, no telemetry beyond an explicit opt-in anonymous counter. This is simultaneously the margin structure, the trial-friction advantage, the answer to every security question, and a moat against anyone competing with a cloud service. The only server is a CDN serving a `.wasm` blob and a payment checkout.
2. **The receipt is the spine.** Before any performance work, an obviously-correct scalar interpreter must bit-diff against Excel's cached values across the full corpus. It is the build oracle, the trust artifact, and the acceptance test for every later optimization. **Ship nothing until it is green.**
3. **Never claim a number the demo can't reproduce on someone else's machine.** Publish native and browser figures separately.
4. **Only true claims ship.** Specifically: **Excel is NOT single-threaded** — Multithreaded Recalculation shipped in Excel 2007 with up to 1024 threads across independent cell chains. The true and still-devastating claim is that Excel's recalc is *interpreted, cell-at-a-time, and re-executes the entire dependency graph from scratch on every scenario*, while XLC compiles once and vectorizes across the scenario axis.
5. **No JIT in v1.** Cranelift's backends lower the 128-bit WASM SIMD subset; wasmtime issue #4418 (open since 2022) confirms wide-vector support is unsolved, and there is no AVX-512 lowering work. A `pulp`-vectorized interpreter over the coarsened IR amortizes dispatch to noise at scenario widths of 10⁵–10⁶ **and** gets real AVX-512/NEON that Cranelift cannot emit. Cranelift is a v2 fusion backend and a roadmap slide, nothing more.
6. **SIMD is `pulp`, on stable Rust.** `std::simd` is still nightly-only after five years (`portable_simd` #86656). Highway is C++ and would drag a C++ toolchain into a pure-Rust build.
7. **A detector ships only above ~90% measured precision.** Real models are full of *deliberate* irregularities. A linter that flags 40 sites of which 33 are intentional is worse than no linter. Precision is a gated build artifact, hand-audited on sampled findings — never an aspiration.
8. **Every finding carries a machine-checkable proof.** *"Columns E–O all sum D5:D20; column D sums D5:D19."* Never report a count without the evidence. Ship one-click "this is intentional" suppression.
9. **Partial compilation, never refusal.** *"I audited 90% of your model and here are 5 bugs"* is a product. *"Unsupported"* is a bounce. Compile the compilable cone; report exactly which cells were excluded and why, per cell, per feature.
10. **Never ask the user to leave Excel.** Every well-funded attempt to replace the spreadsheet for finance people has failed or exited early (Causal → Lucanet). XLC makes the `.xlsx` better; it does not replace it. **The day the roadmap says "and then you edit in XLC," this becomes Causal.**
11. **Two messages, two surfaces.** The compiler internals are the HN and engineering-blog story. The found bug is the r/excel, r/FPandA and finance-LinkedIn story. HN upvotes compilers and does not buy financial modeling tools — a joke LLVM→Excel compiler scored 226 points while every finance-flavored spreadsheet post scored 1–7.
12. **Gates are written before the code they check**, in a commit prefixed `gate(N):`, asserting a numeric threshold against a committed fixture. Gates 0 and 9 are process gates and exempt; every other gate needs at least one numeric assertion.
13. **The agent never spends money, registers domains, creates accounts, accepts terms, or posts publicly.** Those go in `HUMAN_TODO.md` with exact steps. Work continues around them.
14. **`docs/methodology.md` is written continuously**, including a section titled "What XLC cannot tell you."

## 3. Session Protocol

**Start of session:** read `STATE.md` → read `HUMAN_TODO.md` → read the current phase spec → run `make gate` (**0** = pass, advance; **1** = fail, the output is your task list; **2** = gate unwritten, write it first per Law 12; **3** = ran but unmeasurable here, record as unverified and advance) → `git log --oneline -20` → announce phase, gate status, next action in one line. Then work.

**Every ~90 minutes and at session end:** update `STATE.md`; append a dated 3–8 sentence entry to `docs/build-log.md` written for a human reader (this becomes launch content — the engineering essay is assembled from it); commit.

```markdown
# XLC — STATE
Updated: <ISO date>   |   LAUNCH TARGET: <date>   |   Week <n>
Phase: <N> — <name>          Gate: <PASSING | FAILING: reason | UNWRITTEN>

## Done / In flight / Blocked
- blocked items carry: entered-at timestamp, Never-Stall rung, timebox expiry

## Next action
<one concrete sentence>

## Numbers of Record
corpus workbooks: <n> | parse rate: <%> | receipt pass rate: <%> | functions implemented: <n>
detectors shipped: <n> | precision per detector: <...> | refusal rate: <%> | partial-compile rate: <%>
scenario throughput native: <formulas×scenarios/s> | browser: <...> | bytes moved per scenario: <n>
peak RSS: <MB> | incremental recompute latency: <ms>

## Outside conversations
last: <date> | count: <n>
# if >1 week: draft three outreach messages into HUMAN_TODO.md and flag at top of STATE.md.
```

## 4. Definition of Done

`make gate-<N>` exits 0 **and** `make gate-all` exits 0 — no phase may silently break an earlier one. The gate sequence is exactly `0 1 2 3 4 5 6 7 8 9`. Visual/UX criteria are met in a real browser. `STATE.md` and `docs/build-log.md` updated. Committed.

## 5. Never-Stall Protocol

Timebox every blocker in `STATE.md` when entered: 2 hours for rungs 1–4, 6 hours for rung 6.

1. **Dependency won't build** → pin last known-good, record in `docs/decisions.md`, move on.
2. **A corpus file is malformed or a URL 404s** → skip the file, log it, continue. Never let one bad workbook stop a corpus run; the corpus is real-world data and *will* contain garbage. Track the skip rate as a metric.
3. **A function's semantics are wrong** → shrink to the smallest failing workbook, add it to `tests/cases/`, fix against that. If still wrong after a genuine attempt, mark the function unimplemented (Law 9 handles it gracefully), log it, move on. **Correct-and-narrow beats wrong-and-broad beats blocked.**
4. **A perf target is missed** → record the real number, ship it, continue. Optimize in the phase that owns it.
5. **Design ambiguity** → choose what produces a measurable result sooner, write the tradeoff in `docs/decisions.md`, move on.
6. **Structurally blocked** → write it in `STATE.md`, then switch to any unblocked task. There is always parallel work: more functions, more corpus coverage, docs, the CLI.
7. **Escalation.** Gate failing >12 cumulative hours, or the same blocker re-entered three times, or the item needs money/identity/an account → write it in `HUMAN_TODO.md` with exact reproduction steps, flag it at the top of `STATE.md`, and say so plainly. Silent grinding past 12 hours is worse than asking.

**You do not need permission to continue. Continue.**

---

# PART II — ARCHITECTURE

## 6. Repo shape

```
xlc/
├── STATE.md · HUMAN_TODO.md · CLAUDE.md · Makefile
├── crates/
│   ├── xlc-parse/      # Excel expression grammar → AST. Pure, no I/O.
│   ├── xlc-graph/      # dep graph, Tarjan SCC, topological schedule
│   ├── xlc-eval/       # scalar interpreter + Excel semantics. THE SPINE.
│   ├── xlc-ir/         # typed dataflow IR, range coarsening, CSE/DCE
│   ├── xlc-scenario/   # scenario axis, pulp SIMD kernels, tiling scheduler
│   ├── xlc-ad/         # reverse-mode automatic differentiation
│   ├── xlc-lint/       # defect detectors + precision harness
│   ├── xlc-diff/       # semantic workbook diff
│   ├── xlc-cli/        # `xlc` binary
│   └── xlc-wasm/       # wasm-bindgen surface
├── web/                # TypeScript + Vite, drag-and-drop, Web Workers
├── corpus/
│   ├── Makefile        # pinned URLs + sha256; `make corpus` is reproducible
│   ├── manifest.json   # the committed 500-workbook regression subset
│   └── census.json     # feature frequency — generated in Phase 1, drives all scope
├── tests/cases/        # minimal workbooks with hand-verified expected values
└── docs/ methodology.md · build-log.md · decisions.md · precision/ · benchmarks/
```

## 7. Pinned stack

| Layer | Choice | Notes |
|---|---|---|
| Language | **Rust, stable** | No nightly anywhere. |
| xlsx reading | **`calamine` 0.36.1** (MIT) | `worksheet_formula()`, `XlsxCellFormula` (formula **+ cached value**), `expand_shared_formula_into()`, defined names, Tables, VBA detection. **This crate is why the project works.** |
| SIMD | **`pulp` 0.22.3** (MIT) | Stable, runtime dispatch: AVX-512 (`f64x8`), NEON, WASM SIMD128. The SIMD layer under `faer`. |
| Parallelism | **`rayon`** | Scenario tiles across cores. Browser: Web Workers + `SharedArrayBuffer` behind COOP/COEP headers. |
| Secondary oracle | **LibreOffice via UNO** (`python3-uno`) | **NOT `soffice --convert-to`** — see §8.4, it silently emits cached values. |
| Frontend | TypeScript + Vite, no framework in the hero path | |
| JIT | **None in v1.** Cranelift 0.134.3 is a v2 fusion backend. | Law 5. |

## 8. The engine

### 8.1 Ingest and the capability report

`calamine` reads the workbook. Per cell, pull: formula text, **Excel's cached value**, number format (needed for date/currency typing), plus defined names, Tables, and the VBA-project flag. Expand shared formulas via `expand_shared_formula_into`.

Immediately emit a **capability report**: which cells contain VBA, external links, Power Query, pivot tables, volatile functions (`NOW`, `TODAY`, `RAND`, `RANDBETWEEN`, `OFFSET`, `INDIRECT`, `CELL`, `INFO`), or unimplemented functions. Per feature, per cell, never a blanket refusal (Law 9).

### 8.2 Parse

Hand-written recursive-descent parser with Pratt operator precedence. Must handle: A1 and R1C1, absolute/relative anchors, ranges, whole-column refs, 3D refs, structured Table refs (`Table1[[#Headers],[Col]]`), defined names, external workbook refs, **intersection (space) and union (comma) operators**, `%`, `^`, `&`, array literals `{1,2;3,4}`, and every error literal (`#DIV/0!`, `#N/A`, `#VALUE!`, `#REF!`, `#NAME?`, `#NUM!`, `#NULL!`, `#SPILL!`, `#CALC!`).

**Oracle:** round-trip printing must reproduce the original formula string across all ~6.8M formula cells in the corpus. Log every failure with its formula text — failures are self-reporting, so this converges fast.

### 8.3 Dependency graph

Cell-level graph → **Tarjan SCC** to isolate genuine circular blocks (become iterative-calc nodes honoring the workbook's `maxIterations`/`maxChange`, or a reported exclusion) → topological schedule of the condensation → dead-cell elimination from the output cone.

### 8.4 The scalar evaluator and the receipt — the spine

A plain, obviously-correct, cell-at-a-time interpreter over the top ~180 functions (the census in Phase 1 determines exactly which). Then the receipt: recompute every formula cell, bit-diff against Excel's cached value with an explicit ULP policy, print a green/red report.

**Excel's hostile semantics, each of which must be paid for explicitly:**
- IEEE-754 f64 with Excel's 15-significant-digit *display* truncation and its cosmetic rounding near zero
- The 1900 leap-year bug
- Text↔number coercion rules, which differ per operator
- Error propagation and precedence
- `ROUND` is **half-away-from-zero**, not banker's rounding
- Blank vs. zero vs. empty-string distinctions
- Boolean coercion rules

**Secondary oracle trap — put this in the code comments.** `soffice --headless --convert-to csv` **silently emits cached values rather than recomputing**, because LibreOffice's OOXML recalc-on-load setting defaults to "Never recalculate" and has no command-line flag. This would produce a silently-passing oracle and waste an entire run. Drive LibreOffice through **UNO**: load the document, call `document.calculateAll()`, then read cells — or pre-write `Calc/Formula/Load/OOXMLRecalcMode` into `registrymodifications.xcu` in a throwaway profile.

Maintain a **per-function pass-rate dashboard** in `docs/benchmarks/`. It only goes up.

### 8.5 Typed IR and range coarsening

Lower the AST forest into a typed SSA-ish dataflow IR. Types: `Scalar(f64) | Bool | Text | Error | Date | Array(shape) | Ref`, with shape inference over ranges.

**Range coarsening is the key move.** Detect copied-formula families — one formula replicated down a column with consistent relative offsets — and represent the whole family as **one vector node of width W** rather than W graph nodes. `SUM(A1:A10000)` becomes a single reduction. Then constant folding, CSE and DCE on the IR.

**Invariant: the IR interpreter must still reproduce the receipt exactly.**

### 8.6 The scenario axis

Every IR value becomes a buffer of shape `[range_width × N]` in **structure-of-arrays** layout, vectorized with `pulp`, tiles parallelized with `rayon`.

**The scheduling problem that decides whether this project is fast or mediocre.** 41,388 formulas × 10⁶ scenarios is 4.1×10¹⁰ f64 operations. Streamed naively, intermediates generate **~330 GB of memory traffic** — six seconds at laptop bandwidth before any arithmetic happens. A naive implementation lands at 10–30 s, not 1.4 s.

**Two-axis tiling with liveness-based buffer reuse.** Choose a scenario tile size T (start 256–1024) and tile the range axis too where ranges are wide, so the live set fits in L2. Compute liveness on the IR, allocate a small pool of reusable tile buffers, execute the whole DAG per tile before moving on. **Budget a full run for this alone, with bytes-moved-per-scenario as the metric.**

**RNG: counter-based** (Philox 4×32-10 or Threefry) so scenario *k* is reproducible from `(seed, k)` with no stored state — which makes tiling trivially parallel *and* makes results reproducible across machines, which matters enormously for a trust product. Vectorized inverse-normal via Wichura AS241 or Acklam; vectorized `exp`/`ln`/`pow` via Cephes-style polynomial kernels, because WASM has no SIMD transcendentals.

**Oracles:** at N=1 the scenario engine must reproduce the receipt bit-for-bit. At large N, the mean of a deterministic model must equal the scalar result. Property-test distributions against known analytic moments.

### 8.7 Reverse-mode AD

The topologically-ordered IR *is* the tape. One backward sweep gives ∂output/∂every-input at ~2–4× a forward pass.

**Handle non-smoothness honestly.** `IF`/`MIN`/`MAX`/`ABS` give subgradients — pick a convention and document it. `VLOOKUP`/`INDEX`/`MATCH`/`CHOOSE` are differentiable in the value returned but **not in the index** — propagate through the selected branch and mark the edge *structural*. Present two lists: continuous sensitivities (ranked, exact) and structural dependencies ("this input changes which branch you take").

**Oracle:** central finite differences on 50 random inputs match the AD gradient to ~1e-6 relative on smooth models.

### 8.8 The analyzer

Detectors over the IR, not over text. Candidates, roughly in descending order of achievable precision:

1. Inconsistent formula within a copied region (sibling rows/columns whose normalized formula shape differs)
2. Range off-by-one relative to siblings — `SUM(D5:D19)` where eleven sibling columns use `D5:D20`
3. Reference into a `#REF!` or deleted cone
4. Range excludes adjacent populated cells, or includes the header
5. Hardcoded numeric constant embedded in a formula inside an otherwise-uniform region
6. Unintended circular reference
7. Type/unit inconsistency — a rate summed with a currency, inferred from number formats and magnitudes
8. Dead input — a cell clearly intended as an assumption that reaches no output
9. Sign/direction anomaly in a copied series

**Calibrate on the corpus:** run all workbooks, randomly sample 200 flagged sites per detector, hand-audit precision, record it in `docs/precision/`. **Ship only detectors above ~90%** (Law 7). Start with 1–3, which are near-tautological.

### 8.9 Semantic workbook diff

Structurally align two IRs and report not *"4,000 cells changed"* but: *"v2 changed the dependency cone of NPV. On 3.1% of the sampled input space the two models disagree by more than 1%. Here is an input vector where v1 = 38.7 and v2 = 41.2."*

Nearly free once the IR and scenario axis exist — run both IRs over the same scenario tiles and report divergence. **Do not skip this.** It is equivalence checking applied to the artifact a board approves, and it's the most consequential verb in the product.

### 8.10 Incremental recompute

Mark the dirty cone from a changed input, re-run only that sub-DAG across the scenario axis, reuse cached upstream tiles. This is what makes the slider scrub at 40 ms.

## 9. Interface design

**Aesthetic: a compiler, not a dashboard.** The reference points are `tsc --watch`, `cargo build`, and a good profiler — not a BI tool. Monospace, dense, fast, factual. The output looks like a build log because it *is* one, and that's the joke that makes engineers love it.

- **Palette.** Near-black or paper-white, pick one and commit. Compiler-diagnostic colors only: green for the receipt, amber for warnings, red for errors, dim grey for elided detail. One accent. No gradients, no cards, no rounded dashboard tiles, no purple.
- **Typography.** A real mono with character — **IBM Plex Mono**, Berkeley Mono if licensed, JetBrains Mono — with **tabular figures** so counters don't jitter. One display face at most. Never Inter, Roboto, Arial, or system-ui.
- **The findings render as compiler diagnostics.** Filename, cell reference, caret, the rule that fired, the evidence line, the suggested fix. Anyone who has read a Rust error message will feel at home instantly, and that familiarity *is* the design.
- **Motion is functional only.** Numbers count up as work completes. The receipt turns green. The slider scrubs at 40 ms and that responsiveness is the whole feeling — nothing else animates.
- **Forbidden:** anything that looks like Power BI, stock-photo finance imagery, AI-app aesthetics, or a marketing landing page above the drop zone. The tool *is* the landing page.

---

# PART III — THE BUILD

## Phase 0 — Bootstrap and corpus

**Tasks.** Scaffold per §6, `git init`, write `STATE.md` and empty `HUMAN_TODO.md`. Write `corpus/Makefile` with pinned URLs and sha256 for every source. Fetch and verify **FUSE** (9.4 GB, 249,376 spreadsheets from 1.9 PB of Common Crawl, CC-BY-4.0, no login — `https://zenodo.org/records/581678`) and **SpreadsheetBench** (~100 MB, 5,426 workbooks, 912 real Excel-forum questions, CC-BY-SA-4.0 — *testing only, do not vendor*). Filter FUSE to valid OOXML workbooks with formulas (expect ~10,500; budget ~25 GB free disk). Commit a deterministic **500-workbook regression subset** with a SHA256 manifest — CI runs against that, never re-downloading. Write the `make gate` dispatcher (exit 0/1/2/3) and `make gate-all`. Start `docs/methodology.md`.

**Gate 0.** `make corpus` succeeds from clean checkout; every entry has a matching sha256; the 500-workbook subset is committed and loads; the dispatcher returns 2 for unwritten gates.

---

## Phase 1 — The census (do this before anything fun)

**Goal.** Learn the true shape of the problem before scoping a single function. **Highest value-per-hour in the entire build.**

**Tasks.** Wire `calamine`. Across all ~16,000 corpus workbooks, produce `corpus/census.json`: what percentage contain VBA, external links, Power Query, pivot tables, each volatile function, and **each of the ~600 Excel functions by frequency**. Emit the capability report per workbook.

**Why this is first:** it determines the ~180-function scope, and it tells you the true **refusal rate** — the number that quietly decides whether the frictionless-trial business model works at all (Kill Risk 2 in the dossier). If 50% of real workbooks contain a blocker, you need to know that in week one, not month three.

**Gate 1.** `census.json` exists covering ≥95% of corpus workbooks; function frequency table is ranked; the 99th-percentile function cut-off is recorded in `STATE.md`; **projected refusal rate and projected partial-compile rate are both recorded.**

---

## Phase 2 — Parser

**Tasks.** Full expression grammar per §8.2. **Oracle:** round-trip print reproduces the original string on all ~6.8M formula cells.

**Gate 2.** ≥99.5% round-trip parse rate across the corpus; every failure logged with its formula text; parse throughput recorded; zero panics on the full corpus (malformed input returns an error, never unwinds).

---

## Phase 3 — Graph, evaluator, and THE RECEIPT

**This is ~40% of total effort and everything downstream is worthless without it.** It is also the most Claude-Code-friendly work in existence: each function is a small independently testable unit with a machine-checkable oracle and a pass-rate dashboard that only goes up.

**Tasks.** Dependency graph, Tarjan SCC, topological schedule (§8.3). The scalar interpreter over the census-determined function set. Every hostile Excel semantic from §8.4, each with its own regression case. The receipt with an explicit ULP policy. The per-function pass-rate dashboard. The LibreOffice-UNO secondary oracle (**never `--convert-to`**).

**Gate 3.** Receipt green — bit-identical within the documented ULP policy — on **≥97% of formula cells across the committed 500-workbook subset**, with a published per-function pass-rate table; every mismatch class categorized in `docs/benchmarks/`; SCC detection correct on hand-built circular test cases; full-corpus run completes without panics.

---

## Phase 4 — The analyzer, and the first launchable product

**Goal.** Ship something publicly on its own, before any of the simulation work exists.

**Tasks.** Detectors per §8.8, plus the precision-calibration harness. Diagnostic-style output with per-finding proof (Law 8). One-click "intentional" suppression persisted alongside the workbook. Web drag-and-drop surface with the three-act ten-second sequence: *compiled N formulas* → *receipt green* → *N defects found*.

**Gate 4.** ≥3 detectors at **≥90% hand-audited precision** on 200 sampled findings each, with the audit recorded in `docs/precision/`; every finding carries a proof string; end-to-end drag-to-findings under **2.5 s** on a 40k-formula workbook; partial compilation reports excluded cells per feature rather than refusing.

**This is the first public launch.** A free, no-signup Excel bug finder that runs entirely in your browser is a complete product.

---

## Phase 5 — Typed IR and coarsening

**Tasks.** §8.5. **Invariant: the IR interpreter reproduces the receipt exactly.**

**Gate 5.** IR interpreter matches scalar receipt bit-for-bit across the 500-workbook subset; coarsening ratio recorded (formulas → vector nodes) per workbook; CSE/DCE reduction measured; no semantic drift.

---

## Phase 6 — The scenario axis (the hard one)

**Tasks.** §8.6 in full: `pulp` kernels, **two-axis tiling with liveness-based buffer reuse**, counter-based RNG, vectorized inverse-normal and transcendentals. Auto-detection of existing `RAND`/`RANDBETWEEN`/`NORM.INV` cells and auto-*proposal* of hardcoded assumption cells in the output cone as uncertain inputs at ±10% triangular, one click to accept.

Ship exactly five distributions — normal, lognormal, triangular, uniform, PERT — and independence. **Resist @RISK feature parity.** XLC competes on the engine, not the stats menu.

**Gate 6.** At N=1, bit-identical to the receipt; mean of a deterministic model equals the scalar result; distributions match analytic moments; **bytes-moved-per-scenario recorded and within 3× of the theoretical minimum for the tiling schedule**; native and browser throughput recorded separately on a named public workbook.

---

## Phase 7 — Incremental recompute, AD, semantic diff

**Tasks.** §8.10 (the 40 ms slider), §8.7 (reverse-mode AD with honest structural-edge handling), §8.9 (semantic workbook diff).

**Gate 7.** Incremental recompute latency under **50 ms** for a single-input change on a 40k-formula model at N=10⁵; AD matches central finite differences to 1e-6 relative on 50 random smooth models; semantic diff produces a concrete divergent input vector on two hand-constructed workbook versions with a known planted difference.

---

## Phase 8 — Surfaces and evidence

**Tasks.** CLI (`xlc check model.xlsx`, `xlc monte --scenarios 1000000`, `xlc diff a.xlsx b.xlsx`) and a CI mode — this is the stickiest revenue tier and it's cheaper than a polished desktop app. Native binary with full-width AVX-512/NEON. Cookieless analytics on the marketing surface only, never in the tool (Law 1). Payment via a third-party checkout — a `HUMAN_TODO.md` item.

**Gate 8.** CLI exits non-zero on regressions; benchmark suite reproducible via one command with machine and ISA recorded; browser build works under COOP/COEP with a documented fallback when `SharedArrayBuffer` is unavailable.

---

## Phase 9 — Launch and application

**Tasks.** README with the receipt pass rate, detector precision table, and honest benchmark numbers. **The engineering essay** assembled from `docs/build-log.md`: the coarsened-IR design, why the dense JIT was the wrong call, Excel's hostile semantics one at a time, the bandwidth wall and the tiling schedule that beat it. This post is the credibility asset — a *joke* LLVM→Excel compiler scored 226 points on HN, so a real one should do better.

**Two launch messages on two surfaces** (Law 11), both prepared by the agent and posted by the human. Draft the YC application from the build log with real numbers pasted in.

**Gate 9.** Fresh clone builds and runs following only the README; all published numbers trace to a reproducible benchmark command; `HUMAN_TODO.md` contains every human-only step; `docs/application/` drafted with zero `TODO` markers.

---

## Later

N-way workbook diff and a git-style history view · distribution fitting from historical data · correlation and copulas · LET and LAMBDA support if the census justifies it · a Cranelift fusion backend (the roadmap slide, finally cashed) · an Excel add-in that calls the engine in-process.

---

# PART IV — VERIFIED DEPENDENCIES

All fetched live, August 2026.

**FUSE spreadsheet corpus** — `https://zenodo.org/records/581678` — the primary oracle corpus. Single `fuse.zip`, **9.4 GB**, **249,376 spreadsheets** extracted from 1.9 PB of Common Crawl, plus a 2.1M-URL web-analysis dataset. **CC-BY-4.0**, redistribution permitted with attribution. No login. DOI 10.5281/zenodo.581678. Filter to the ~10,500 valid OOXML workbooks with formulas; budget ~25 GB disk.

**SpreadsheetBench** — `https://github.com/RUCKBReasoning/SpreadsheetBench`, data at `data/all_data_912.tar.gz` — ~100 MB, **5,426 workbooks**, 912 real Excel-forum questions, 2,729 test cases. **CC-BY-SA-4.0 — share-alike, so use for testing only; do not vendor into the shipped artifact.**

**calamine 0.36.1** (MIT, released 2026-07-27) — `https://docs.rs/calamine/latest/calamine/` — **the crate the project depends on existing.** Verified: `worksheet_formula()`, `XlsxCellFormula` *"containing both cached/literal value and expanded formula text"*, `expand_shared_formula()` / `expand_shared_formula_into()`, `XlsxCellFormulaMetadataRecord`, defined names, Tables, hyperlinks, VBA-project detection.

**pulp 0.22.3** (MIT) — `https://docs.rs/pulp/latest/pulp/` — stable-Rust safe SIMD with **runtime feature dispatch**; x86-64 including 512-bit (`f64x8`, `f32x16`), aarch64 NEON including apple-darwin, WASM SIMD via the `pulp-wasm-simd-flag` feature. The SIMD layer under `faer`.

**Enron spreadsheet corpus** — `https://github.com/SheetJS/enron_xls` — a second real financial-model corpus, mostly legacy `.xls` so lower priority. *(The figshare original returns 403; use the GitHub mirror.)*

**LibreOffice Calc** (MPL-2.0) — secondary oracle **via UNO only**. See §8.4 for the silent-failure trap.

**SEC EDGAR Financial Statement Data Sets** — `https://www.sec.gov/files/dera/data/financial-statement-data-sets/2026q1.zip` (81.31 MB; 68 quarterly releases 2009Q1→2026Q1, keyless). **Honest downgrade:** XBRL numeric facts, not workbooks with formulas. Useful as a marketing artifact and for realistic financial magnitudes. **Never on the correctness critical path.**

**Cranelift 0.134.3** (Apache-2.0 with LLVM exception) — v2 only, per Law 5.

**Read but do not depend on:** **Formualizer** (`https://github.com/psu3d0/formualizer`, 156 stars, MIT/Apache) — the closest substrate, Arrow columnar storage, incremental dep tracking, 400+ functions, *no SIMD, no JIT, no Monte Carlo, no AD, no auditing*. It is a free map of the Excel-semantics minefield; read its source, own your own IR. **IronCalc** (`https://www.ironcalc.com/`) — EU-funded, roadmap is charts and collaboration. **pycel** (629 stars) — compiles Excel to Python at Python speed. **Peter Sestoft's Funcalc lineage** (ITU Copenhagen, *Spreadsheet Implementation Technology*, MIT Press 2014; the `popular-parallel-programming` GitHub org) — read the papers; compiled parallel recalculation was proven feasible academically and abandoned in 2018. Research risk retired, commercial space vacant.

---

# PART V — ANTI-PATTERNS

- **Optimizing before the receipt is green.** Every performance number is meaningless if the semantics are wrong. Law 2.
- **Materializing a dense JIT.** Cranelift can't emit AVX-512. Law 5.
- **Shipping a noisy linter.** 33 false positives out of 40 findings and this buyer never returns. Law 7.
- **Refusing a workbook.** Partial compilation with a per-cell explanation. Law 9.
- **Chasing @RISK's stats menu.** Five distributions. The engine is the product.
- **Building an editor.** The day users edit in XLC, this is Causal. Law 10.
- **Saying "Excel is single-threaded."** It has been multithreaded since 2007. Law 4.
- **Streaming intermediates to memory.** 330 GB of traffic and a 30-second demo. §8.6.

---

# PART VI — THE BAR

1. **Would a compiler engineer respect the IR?** If not, the gap is in coarsening or type inference, not in the UI.
2. **Would a CFO forward the findings email?** If not, the detectors aren't precise enough or the proofs aren't legible.
3. **Does the receipt make the whole thing trustworthy without argument?** That green bit-diff is the single best idea in this project. If a change weakens it, undo the change.

Excel runs a meaningful fraction of the world's financial decisions and nobody has ever compiled it. Build the compiler.
