# XLC build log

Dated entries written for a human reader; the Phase 9 engineering essay is
assembled from these.

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
