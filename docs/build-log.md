# XLC build log

Dated entries written for a human reader; the Phase 9 engineering essay is
assembled from these.

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
