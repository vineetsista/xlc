# HN launch draft (Law 11: the compiler story)

**Title:** Show HN: XLC – an optimizing compiler for Excel

**Body:**

Excel is the most widely used programming language on Earth and the only
one with no compiler, no linter, no tests, and no CI. I built the missing
toolchain in Rust: parse a .xlsx into a typed dataflow IR, prove it
re-derives the values Excel itself cached, find copy-paste defects with
machine-checkable proofs, and run 100k+ Monte-Carlo scenarios by
vectorizing across a scenario axis — all local, in the browser (817 KB
wasm) or CLI. Nothing uploads, ever.

The part I'd want to read about: every xlsx stores Excel's own computed
value next to each formula, which turns compiler verification into a data
problem. Against 16,167 Common-Crawl workbooks (7M formula cells): parser
round-trips 99.997% byte-exact; the evaluator re-derives 99.35% of
verifiable cells bit-exactly (exclusions printed, never hidden); the
corpus taught us that ROUND is decimal-faithful, that SUMIF coerces
"003607" to match both text and number while VLOOKUP won't, and that some
files' caches are simply stale — the oracle has a noise floor. The linter
gates itself on hand-audited precision and rejected its own first two
versions (27%, then 72%) before shipping at 90.8–100%. No JIT — a
pulp-vectorized interpreter over coarsened copied-formula families gets
AVX-512 that Cranelift can't emit; a single-input change on a 40k-formula
model re-simulates 100k scenarios in 12.6 ms.

Try it: https://vineetsista.github.io/xlc/ · Repo (MIT, every artifact committed): https://github.com/vineetsista/xlc ·
Essay: https://github.com/vineetsista/xlc/blob/main/docs/essay.md. Every number is reproducible
with `make bench`.

---

## Posting notes (channel rules live-verified 2026-08-09)

- **Window:** Tue–Thu, 13:00–16:00 UTC (9am–12pm ET) — where this
  niche's winners posted (Excel-to-Python compiler 79 pts, Probly 171,
  WASM-compiler posts 137–225, while "Excel productivity tool" framings
  died at 2–5). Low-competition alternative: Sunday 11–16 UTC.
- **Title variant with the hooks explicit:** "Show HN: XLC – an
  optimizing compiler for Excel (Rust → WASM, runs entirely in your
  browser)".
- **Before posting:** open the live site and click the sample workbook
  once. Show HN requires something people can play with instantly —
  nobody will upload their own file first.
- **Immediately after posting:** add the first comment below. Answer
  every comment for the first 90 minutes. Never defensive. Never ask
  anyone to upvote — voting-ring detection kills posts and accounts.
  If asked about money, answer honestly: browser tool and CLI free
  forever; a paid per-organization CI tier is planned.
- Reposts are only acceptable after ~a year with no traction — treat
  this as the one shot and don't post until the sample flow is verified.

**Prepared first comment:**

Author here. I built this because Excel is the most-used programming
language on Earth with none of the toolchain we take for granted — and
because .xlsx files secretly contain a perfect test oracle: Excel
stores its own computed value next to every formula, so a compiler can
grade itself against 7M real cells with no human in the loop.

Architecture in one paragraph: calamine reads the file; a hand-written
Pratt parser round-trips 99.997% of 7M corpus formulas byte-exactly
(whitespace is data — a lone space is Excel's intersection operator);
Tarjan SCC isolates circular blocks; a scalar interpreter re-derives
Excel's cached values (99.35% of verifiable cells on the committed
500-workbook subset, exclusions printed, never hidden); then formulas
are coarsened into copied-family vector nodes (11.2×) and a
pulp-vectorized interpreter runs the scenario axis. No JIT — Cranelift
can't emit the wide SIMD this needs, and at 10⁵–10⁶ scenarios dispatch
amortizes to noise anyway.

Honest limitations: ~75 functions implemented (that covers 99% of
cell-mentions in the corpus, but TEXT's format engine, INDIRECT/OFFSET,
and LET/LAMBDA aren't in yet — excluded cells are reported per-cell);
the oracle itself has a noise floor (some real files are saved with
stale caches, and we publish the adjudication); and the browser build
is currently single-threaded, so native is ~8× faster. MIT; every
number reproduces with `make bench`.
