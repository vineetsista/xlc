# XLC build log

Dated entries written for a human reader; the Phase 9 engineering essay is
assembled from these.

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
