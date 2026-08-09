# Compiling Excel

*The engineering essay — assembled from docs/build-log.md; every number
traces to a committed artifact reproducible via `make bench`.*

Excel is the most widely used programming language on Earth, and it is the
only one with no compiler. No type checker, no linter, no test framework,
no CI. A meaningful fraction of the world's financial decisions flow
through interpreted, cell-at-a-time recalculation of files nobody can
diff, test, or prove correct. We built the missing compiler. This is how,
and what 16,167 real workbooks taught us on the way.

## The oracle was inside the file all along

Every `.xlsx` stores, next to each formula, the value Excel last computed
for it. That turns compiler verification — normally the hardest part —
into a data problem: recompute every cell and bit-compare against Excel's
own answers, across a corpus too large to argue with. Ours is 16,167
workbooks and 7,003,444 formula cells pulled from Common Crawl. The
result is what we call the receipt: 99.35% of verifiable cells on our
committed 500-workbook regression subset re-derive bit-exactly (98.81%
across the full corpus), under a stated policy — bit-identical, one ULP,
or equal at 15 significant decimal digits, the precision at which Excel
writes its caches. Exclusions are counted and printed beside the rate,
never hidden.

## Excel's semantics are hostile, and the corpus bills you for each one

The census came first: 75 functions cover 99% of real-world cell
mentions, VBA appears in six workbooks out of sixteen thousand, and the
projected refusal rate — the number our business model hangs on — is
5.7%, not the 50% we feared. Then the corpus started teaching. Stored
formulas keep every space the user typed, so byte-exact round-trip
(99.9970% of 7M formulas) required an AST that treats whitespace as data
— fitting, since in Excel a lone space is the intersection operator.
ROUND is decimal-faithful: the f64 nearest 1.275 sits *below* it, yet
ROUND(x,2) is 1.28, because Excel rounds the 15-digit decimal rendering,
not the binary value — we learned that from a 57-sheet workbook of
ROUND(MEDIAN('1:57'!F46),2) cells. SUMIF criteria coerce numeric-looking
text ("003607" matches the text '003607' and the number 3607); VLOOKUP's
exact mode does the opposite. SUM(IF(range,...)) stored as a plain
formula evaluates elementwise — modern dynamic-array semantics with no
marker in the file. And the oracle itself has a noise floor: workbooks
saved with stale caches, where the stored inputs contradict the stored
results, and writers that cache display-rounded values. There is no way
to learn any of this except to be graded by real files, seven million
times per run.

## A linter that measures its own precision — and rejects itself

Real models are full of deliberate irregularity, so a spreadsheet linter
lives or dies on precision. We gated ours on hand-audited precision over
corpus findings, and the audit killed our first two detectors: v1
measured ~27% (embedded subtotals, alternating layouts that manufacture
fake majorities, anchored seed cells, deliberate plugs), v2 measured
~72%. What ships is v4, which claims exactly one thing — a copied
formula whose references slipped — at 90.8% on a full-population census,
alongside range-off-by-one (34/34, every finding backed by boundary-cell
evidence like a sum skipping the row where its siblings include 59,384)
and ref-error (200/200). Both corpses are preserved in the repo, because
a precision number nobody can re-derive is marketing.

## No JIT, and the bandwidth wall

The scenario engine vectorizes across scenarios, not cells: copied-
formula families — 930,372 subset cells coarsen into 83,095 vector nodes,
11.2× — evaluate as tiled buffers with a counter-based RNG (Philox), so
scenario k is a pure function of (seed, k) on any machine. We did not
build a JIT: Cranelift cannot emit AVX-512, and a vectorized interpreter
over the coarsened IR amortizes dispatch to noise at scenario widths of
10⁵–10⁶. The N=1 oracle — the engine must reproduce the interpreter
bit-for-bit — caught two real bugs in one afternoon, both
floating-point summation-order regroupings invisible to any test
smaller than a corpus. Memory traffic is the real ceiling at a million
scenarios, so the artifact records a cache-residency witness: peak live
buffers against a stated budget, with stream-read totals published so
nobody has to trust the model. A single-input change on a 40k-formula
model re-runs its cone across 100,000 scenarios in 12.6 ms — that is
what makes a slider feel like a slider. The topologically-ordered cone
doubles as an AD tape: 232 gradients across 50 random smooth models
match central differences to 8×10⁻⁷, with non-differentiable calls
reported as structural boundaries instead of silently zeroed. And
because two versions of a workbook can run over identical scenario
tiles, "diff" means something now: not "4,000 cells changed" but a
concrete input vector where v1 says 552.37 and v2 says 554.69.

## What it is

A free, no-signup page: drop a workbook, watch three lines — compiled
58,389 formulas (half a second native, four in the browser), receipt
green, defects listed as compiler diagnostics with proofs. Nothing
uploads; the entire engine is an 817 KB wasm module and the only server
is a CDN. The same engine is a CLI with a CI mode that fails builds on
new findings. Every claim in this essay is a committed artifact in the
repo, and `make bench` re-derives all of them on your machine.
