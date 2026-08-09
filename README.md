# xlc — an optimizing compiler for Excel

**Repo:** https://github.com/vineetsista/xlc · **Live demo:** https://vineetsista.github.io/xlc/ (deploys from `main`)

Excel is the most widely used programming language on Earth, and it is the
only one with no compiler, no type checker, no linter, no test framework,
and no CI. xlc is that missing toolchain: it parses a `.xlsx` into a typed
dataflow IR, **proves** it re-derives the values Excel itself cached,
**finds real defects** with machine-checkable evidence, and runs
**scenario sweeps** across a vectorized scenario axis — on CPU, in your
browser or CLI, with nothing ever uploaded.

## The numbers (all reproducible via `make bench`)

Measured against 16,167 real-world workbooks (7,003,444 formula cells)
from the FUSE Common Crawl corpus + SpreadsheetBench. Machine and ISA are
stamped into every artifact in `docs/benchmarks/`.

| claim | number | artifact |
|---|---|---|
| parser round-trip (byte-exact) | 99.9970% of 7,003,444 formulas, 0 panics | `parse-roundtrip.json` |
| **the receipt** (re-derive Excel's cached values, bit-exact under a stated ULP policy) | **99.35%** of 802,705 verifiable subset cells · 98.81% of 4.68M corpus-wide · 0 panics | `receipt.json` |
| detector precision (hand-audited, Law-7 gated) | ref-error 200/200 · range-off-by-one 34/34 (census) · slipped-reference 59/65 = 90.8% (census) | `docs/precision/` |
| analyze 58,389 formulas | 0.52 s native · 4.19 s browser wasm (single-thread) | `analyze-timing.json` |
| IR coarsening | 930,372 cells → 83,095 vector nodes (11.2×), bit-identical to scalar | `ir.json` |
| scenario engine N=1 oracle | 21,047/21,047 bit-identical to the interpreter | `scenario.json` |
| five distributions vs analytic moments (N=10⁶) | all within 1.6σ | `scenario.json` |
| incremental recompute (one input, 40k-formula model, N=10⁵) | 12.6 ms | `phase7.json` |
| reverse-mode AD vs central differences (50 smooth models) | max rel err 8.0×10⁻⁷ | `phase7.json` |
| vectorized fast path (synthetic chain) | 1.45×10⁸ cell-scenarios/s | `scenario.json` |
| secondary-oracle adjudication of every disputed cell (LibreOffice via UNO) | 49.6% of residual mismatches are stale-cache oracle noise (LO sides with xlc); 41.5% real gaps = 0.27% of verifiable cells | `oracle-adjudication.json` |

Exclusions are always printed beside every rate, never hidden: external-
workbook refs (their oracle lives in files we don't have), nondeterministic
volatiles, legacy CSE array formulas, unimplemented functions. See
`docs/methodology.md`, including "The oracle's noise floor" and "What XLC
cannot tell you."

## Build and run

```
git clone https://github.com/vineetsista/xlc && cd xlc
cargo test --workspace          # 96 tests
cargo build --release -p xlc-cli

target/release/xlc check  model.xlsx            # detector diagnostics (+ --ci, --baseline)
target/release/xlc monte  model.xlsx --scenarios 100000 \
    --input "Sheet1!B2=normal(50,5)" --watch "Sheet1!B40"
target/release/xlc diff   v1.xlsx v2.xlsx --output "Sheet1!B40"

make corpus                     # reproduce the corpus from pinned sha256 sources
make bench                      # re-derive every number above on your machine
```

Browser build: `wasm-pack build crates/xlc-wasm --target web --release
--out-dir ../../web/pkg && cd web && npm install && npm run build`, then
`node verify.mjs` drives the page in headless Chromium.

## Design

- `xlc-parse` — hand-written Pratt parser, whitespace-preserving AST
- `xlc-eval` — the scalar interpreter: Excel's hostile semantics, paid for
  one at a time (decimal-faithful ROUND, the 1900 leap-year bug, per-
  operator coercion, blank≠0≠"")
- `xlc-ir` — copied-formula families coarsened to vector nodes
- `xlc-scenario` — Philox counter-based RNG (scenario *k* reproducible
  from `(seed, k)` on any machine), five distributions, tiled sweep with
  a cache-residency witness
- `xlc-ad` / `xlc-diff` / `xlc-lint` — gradients, semantic diff with
  witness input vectors, audited detectors
- No JIT (a `pulp`-vectorized interpreter over the coarsened IR gets real
  AVX-512/NEON that Cranelift cannot emit). Stable Rust only.

Everything runs locally. The only server this product will ever have is a
CDN serving static files.

## Live demo

Drop a workbook at the hosted page (nothing uploads — the engine runs in
your tab), or click **try the sample** for a one-click tour. Findings
render as compiler diagnostics — rule, cell, caret under the offending
range, machine-checkable proof. The **scenario lab** picks your
highest-impact inputs, scrubs a what-if slider with microsecond re-evals
(the engine re-runs only the dependency cone), draws the response curve
and a **tornado sensitivity chart**, inverts the model with **goal
seek**, and runs a 10,000-scenario Monte-Carlo with histogram/CDF views
and p5/p50/p95 — all in the tab. Drop a second version of the file to
get a semantic diff with a concrete witness input. `ctrl-k` opens a
command palette; **export report** writes the whole audit to markdown,
locally. Build your own from source:
`make deploy-package` produces a static site zip;
`.github/workflows/deploy-pages.yml` deploys to GitHub Pages on push.

## Pricing — the promise

**Everything described above is free, forever, for everyone — including
commercial use at work.** Not a trial, not a freemium tease, not
"free for personal use". Specifically and permanently free:

- the entire browser tool — audit, receipt, findings with proofs,
  suppression, scenario lab, sensitivity, goal seek, Monte-Carlo,
  version diff, report export;
- the entire CLI — `xlc check`, `xlc monte`, `xlc diff`, including
  `--ci` on your own machine;
- no signup, no account, no telemetry, no file-size cap, no usage
  metering, no seat counting, no nag.

It stays free because it costs nothing to serve: there is no server.
Your workbook never leaves your machine (Law 1), so there is no bill
for me to pass on to you.

**What will eventually cost money:** one thing — **XLC CI**, for an
*organization* that wants the auditor enforcing on a shared build
pipeline: hosted-free license administration, baseline management across
repos, and support with an SLA. The planned price is **$950/year, flat,
per organization** — not per seat, unlimited users, unlimited repos,
published openly rather than hidden behind "contact sales", and verified
by an **offline** license key that never phones home. It does not exist
yet; when it does, none of the free capabilities above move behind it.

**Always free, no questions asked, even for CI:** open-source projects,
students and educators, academic research, nonprofits, and any
individual using it for their own work.

Two commitments, so you can build on this without worrying: **nothing
that is free today will ever become paid**, and the free tier will never
require an account. New paid capabilities only ever appear alongside
what already exists.

## License and attribution

Code: MIT (see LICENSE). The committed 500-workbook regression subset in
`corpus/subset/` is derived from the **FUSE corpus** (Barik, Lubick,
Smith, Slankas, Murphy-Hill — "FUSE: A Reproducible, Extendable, Internet-
Scale Corpus of Spreadsheets", MSR 2015; DOI 10.5281/zenodo.581678),
licensed **CC-BY-4.0**; redistribution here with attribution per that
license. SpreadsheetBench (CC-BY-SA-4.0) is used for testing only and is
never vendored into this repository or any shipped artifact.
