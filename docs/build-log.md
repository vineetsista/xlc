# XLC build log

Dated entries written for a human reader; the Phase 9 engineering essay is
assembled from these.

## 2026-08-09 — Shipped: public repo, live site, and the oracle's confession

The last HUMAN_TODO items fell to stubbornness. LibreOffice does not need
sudo — the TDF deb bundle extracts with dpkg -x into a home directory,
its bundled python speaks UNO, and the adjudicator ran calculateAll()
over every one of the 5,194 cells where XLC and Excel's caches disagree.
The verdict vindicates the receipt's residual: on 49.6% of disputed
cells LibreOffice sides with XLC against the file's own cache —
confirmed stale-cache noise — while 41.5% are real gaps (0.27% of
verifiable cells, mostly the ^-precision chain) and 8.9% split all
three engines. The product shipped three ways in one evening: a
self-contained single-file demo (wasm and fonts embedded, verified in
Chromium before publishing), the public repository, and GitHub Pages
enabled by API so the site deploys from main with a relative base that
survives any mount point. What remains for a human is exactly what the
constitution reserves: pressing "post".

## 2026-08-09 — All ten gates green

The last three phases fell in one continuous run. The scenario engine
passed its cruelest oracle — at N=1 it must reproduce the interpreter
bit-for-bit, and it caught two floating-point summation-order bugs that
no smaller test could see — then posted five distributions within 1.6
sigma of analytic moments at a million draws, a cache-residency witness
for the bandwidth claim, and 1.45e8 cell-scenarios/s on the vectorized
path. Phase 7 turned the schedule into an AD tape (8e-7 against central
differences, non-smooth calls reported honestly as structural), made the
slider real (12.6 ms for 100k scenarios after an input change on a
40k-formula model), and gave diff a witness vector. Phase 8 shipped the
product verbs and a CI mode that fails builds on new findings, plus a
one-command bench that stamps machine and ISA into every artifact.
Phase 9 is the telling of it: a README where every number links to its
artifact, the essay, two launch drafts for two audiences, and an
application. From an empty directory to a ten-gate-green compiler for
the world's most-used programming language, with the receipts to prove
every sentence.

## 2026-08-09 — Gate 5: the compiler gets its IR, and a lesson about what "family" means

Six gates green. The typed IR lowers 930,372 subset formula cells into
83,095 vector nodes — an 11.2× coarsening, the number the whole scenario
engine will stand on — and its evaluation matches the scalar interpreter
bit-for-bit on every single cell. The invariant held partly by
construction (the IR calls the same operand-level primitives the scalar
interpreter uses; there is one implementation of Excel semantics, not
two) and partly by a bug the verifier caught in its first run: function
calls lowered as lazy black boxes carried the exemplar's AST, so every
lane of SUM(D12:D18) returned the exemplar's sum. The fix is the
definition of a copied family made literal — an exemplar expression plus
a per-lane offset, applied by rebasing every relative reference axis.
CSE turned out to be worth less than a percent inside real formulas
(they're mostly single calls); the coarsening is where the leverage
lives. Next: the hard one — the scenario axis.

## 2026-08-09 — Phase 4 complete: the product runs in a browser tab

The whole engine — parser, evaluator, receipt, detectors — compiles to an
817 KB wasm module (410 KB over the wire) and runs entirely inside the
browser tab. The page is the §9 aesthetic: near-black, JetBrains Mono,
diagnostic colors, and exactly three acts — compiled 58,389 formulas,
receipt green (that workbook re-derives 100.00% bit-exact in-browser),
findings as compiler diagnostics with an [intentional] button whose
suppressions persist keyed by workbook hash. Thirteen headless-Chromium
checks drive the real page against a clean fixture and a planted
slipped-reference bug. Getting there forced one real refactor — ingest
and the per-cell receipt moved to a shared crate consumed by CLI and
wasm, with a drift check proving the committed gate-3 artifact reproduces
number-for-number — and one dependency surprise: the zip crate's default
features drag in a C zstd library that wasm can't build, and xlsx never
needed anything but deflate anyway. Native and browser speeds published
separately, as the constitution demands: 0.52s native, 4.19s
single-threaded wasm.

## 2026-08-09 (early) — Three detectors ship, and the audit earns its keep

Gate 4's numeric checks pass: ref-error at 200/200, range-off-by-one at
34/34 (a full census — the corpus only contains 34 of them, and every one
came with boundary-cell evidence like a sum whose siblings include the
59,384 sitting in the row it skips), and inconsistent-region at 59/65 =
90.8% after two full rejection cycles. The v1 detector measured 27%
precision and died; v2 measured 72% and died; what ships is v4, which
flags exactly one thing: a copied formula whose references slipped. The
false positives that survived to the final audit are a museum of deliberate
irregularity — prior-year comparison columns pulling an external workbook,
baseline columns referencing a top-of-sheet assumption, counters that skip
separator rows. The audit trail keeps all of it, including both corpses,
because a precision number nobody can re-derive is just marketing. The
whole analyzer runs 58,389 formulas in half a second. Next: the browser.

## 2026-08-08 (late night) — The receipt learns what Excel actually does

From 71.96% to 99.35% of verifiable cells in one sitting, and every point of
it was a lesson delivered by real workbooks. The function grind (nine
builtins to about seventy-five, ordered strictly by blocked-cell count from
the census) did the bulk. But the last few points were the interesting ones.
A file full of `SUM(IF(ISBLANK(...)...))` taught the interpreter array
semantics — stored as plain formulas, no CSE marker, yet cached with
elementwise results, exactly how modern dynamic-array Excel evaluates them.
A 57-sheet workbook of `ROUND(MEDIAN('1:57'!F46),2)` cells proved Excel's
ROUND operates on the 15-digit decimal rendering, not the binary value: the
f64 nearest 1.275 sits below it, and Excel still says 1.28. A sheet of
`SUMIF` keyed on leading-zero IDs like '003607' showed the criteria
language coerces numeric-looking text on both sides — 4,400 cells flipped
with a five-line fix — while VLOOKUP's exact mode does the precise
opposite. And the receipt found the oracle's own noise floor: workbooks
saved with stale caches (the stored inputs contradict the stored results)
and writers that cache display-rounded values. The engine's remaining
mismatch list is now mostly Excel being wrong about itself, which is a
sentence that only a corpus of 16,000 real files lets you write with a
straight face.

## 2026-08-08 (night) — Three gates in one day, and the corpus writes the bug reports

Gate 2 fell the same evening: 99.9970% of 7,003,444 real-world formulas now
parse and print back byte-identical, at 253,000 formulas a second. The first
full-corpus run scored 94.85%, and the failure log — aggregated by formula
text, sorted by count — diagnosed itself: 360,131 of 360,762 mismatches were
pure whitespace, because stored xlsx files keep every space the user ever
typed around an operator. So the AST grew trivia fields (the intersection
operator now literally stores its own whitespace run, since in Excel the
space IS an operator), booleans keep their lexeme because real files contain
lowercase `false`, and the 212 formulas still failing turn out to contain
curly quotes pasted from Word — broken in Excel too. Then the receipt drew
first blood on real data: a subset workbook whose dimension record claims
all 17 billion cells sent calamine's dense range reader after a 512 GiB
allocation. The fix — the streaming cell reader — is also the API that
hands us cached value and formula in one pass. Baseline receipt with nine
functions implemented: 73.90% of 921,095 verifiable cells bit-match Excel,
every miss classified, zero panics. The remaining gap is mostly a ranked
to-do list of functions, which is exactly the shape of work this build
was designed around.

## 2026-08-08 (evening) — Gates 0 and 1 fall, and the census kills Kill Risk 2

The corpus pipeline is real: 249,376 Common Crawl files filtered to 10,703
valid OOXML workbooks (the spec's red-team predicted ~10,500 — within 2%),
a deterministic 500-workbook regression subset committed behind a sha256
manifest, and zero errors or panics across the entire scan. Then the census,
16,167 workbooks and 7,003,444 formula cells later, answered the question
the whole business model hangs on: the projected refusal rate is 5.7%, not
the feared 50%. Four of five real workbooks compile fully under a 75-function
set; 14% compile partially. The function table is a power law with a long
tail — IF alone appears in 1.77M cells, and 75 functions cover 99% of all
function mentions, so the ~180-function budget has comfortable headroom.
Other surprises: VBA — the feature everyone assumes blocks everything —
appears in six workbooks out of sixteen thousand; meanwhile 240 workbooks
still carry the Mac 1904 epoch, and TEXTAFTER (shipped 2022) already outranks
ROUND. The corpus keeps correcting intuitions; that is exactly what it is for.

## 2026-08-08 (later) — The corpus fights back, productively

FUSE turned out to be a matryoshka: the 9.4 GB zip holds a 140-part split 7z,
undocumented anywhere on the Zenodo page, and this machine has no 7z binary
and no sudo. The fix is pure userland — stream each part out of the zip in
order, reassemble, extract with py7zr in a project-local venv — and the
Makefile now does each part idempotently, because the first naive attempt got
killed at a task timeout mid-append and would have restarted from byte zero
forever. While the LZMA stream unpacks, the Phase 1 census tool got built and
unit-tested against Excel's nastier formula-text corners: `_xlfn.` prefixes on
every modern function, string literals containing "SUM(", quoted sheet names,
and the difference between `Table1[Amount]` and a real external-workbook
reference like `[1]Sheet1!A1`. The census records, per workbook, each distinct
function-combination a formula uses with its cell count — which means the
refusal-rate question ("what fraction of real workbooks can we not compile at
all?") becomes an offline query over 16k JSONL rows for any candidate
function set, no corpus rescan required.

## 2026-08-08 — Bootstrap

Empty directory to building workspace in one sitting. Ten stub crates
(parse / graph / eval / IR / scenario / AD / lint / diff / CLI / wasm) compile
on stable Rust 1.97.1 with calamine pinned at exactly 0.36.1 — the crate this
whole project leans on, since it hands us each cell's formula *and* the value
Excel last computed for it, which is the build oracle. The gate dispatcher is
in place first (exit codes 0/1/2/3), and gate 0 was written before the corpus
tooling it checks, per the constitution. Two surprises already: the
SpreadsheetBench data file was renamed upstream (`all_data_912.tar.gz` →
`spreadsheetbench_912_v0.1.tar.gz` — the spec's own freshness check paying for
itself), and Zenodo pins FUSE with an md5, not a sha256, so we verify their
md5 once and then pin our own sha256 as the reproducibility anchor. The 9.4 GB
FUSE download streams in the background at ~16 MB/s while the scaffold goes up
around it.
