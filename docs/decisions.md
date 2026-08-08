# Decisions

Dated, append-only. Each entry: the decision, the alternative, why.

## 2026-08-08 — Receipt denominator: verifiable cells, exclusions always visible
The receipt pass rate is measured over VERIFIABLE cells: formula cells whose
oracle exists and which XLC compiles. Excluded and separately reported, per
Law 9: external-workbook refs (their cached values came from files we do not
have — including cells reaching external data through defined names),
nondeterministic volatiles (NOW/TODAY/RAND*), legacy CSE array formulas
(until the IR provides array semantics), unimplemented functions, and cells
Excel stored no cached value for. gate3 asserts the exclusions are present
in the artifact and total under half the subset, so the denominator cannot
be quietly gamed. Alternative — all-cells denominator — rejected: it makes
the metric measure corpus composition (12% of subset cells sit in
external-link workbooks) rather than semantic fidelity.

## 2026-08-08 — Oracle noise classes discovered by the receipt
Two corpus-reality classes cap the receipt below 100% and are NOT XLC bugs:
(1) stale caches — files saved after edits without recalculation
(1496fe18…: B19:B23 changed, SUM(IF(...)) caches kept; calcMode="auto", so
undetectable in general); (2) display-rounded caches — writers that store
the FORMATTED value as the cached value (26673f97…: D199/C199 cached as
1004.97 exactly). The LibreOffice-UNO secondary oracle adjudicates disputed
cells in a later pass. Recorded in methodology.md.

## 2026-08-08 — Criteria equality coerces numeric-looking text (corpus-verified)
COUNTIF/SUMIF-family equality criteria match numeric-looking text cells:
criteria "003607" matches text \'003607\', text \'3607\', and the number 3607
(27905c2f…: 4,400 cells flipped from fail to pass). Ordering comparisons
(>, <, >=, <=) remain strict-numeric until the corpus shows otherwise.
VLOOKUP/MATCH exact mode is the opposite: same-family equality only, text
never matches numbers (06a8920b…).

## 2026-08-08 — ROUND is decimal-faithful
Excel ROUND decides on the value\'s 15-significant-digit DECIMAL rendering,
not its binary value: the f64 nearest 1.275 sits just below it, yet
ROUND(x,2)=1.28. Implemented by string-rounding the {:.14e} rendering.
Discovered via ROUND(MEDIAN(...)) cells over a 3D 57-sheet span.

## 2026-08-08 — SpreadsheetBench data file renamed upstream
XLC.md Part IV points at `data/all_data_912.tar.gz`; that path 404s. The repo
(default branch `main`) now ships `data/spreadsheetbench_912_v0.1.tar.gz`
(95,752,357 bytes). Pinned that URL + sha256 `9cf7228b…` in corpus/Makefile.
Also available upstream: `sample_data_200.tar.gz`, `spreadsheetbench_verified_400.tar.gz`
(not fetched; the 912 set is the one the spec scopes).

## 2026-08-08 — FUSE fetched via Zenodo API URL
`https://zenodo.org/api/records/581678/files/fuse.zip/content` rather than the
HTML record page. Zenodo's API publishes only an md5 (`13e955c44f0b77d1c36088c0bbb3366d`);
we verify that md5 once after download, then pin our own sha256 in
corpus/Makefile as the reproducibility anchor (sha256 everywhere else in the
build, one hash discipline).

## 2026-08-08 — Deterministic subset selection rule
The committed 500-workbook regression subset = workbooks that (a) calamine
opens as OOXML, (b) contain ≥10 formula cells, ordered by content sha256
ascending, first 500. Content-hash ordering is deterministic, reproducible
from the raw corpus alone, and unbiased w.r.t. file name/origin/size.
Alternative considered: stratified sampling by formula count — rejected for
now as it adds a knob without a demonstrated need; revisit after Phase 3 if
the subset's function mix under-represents the corpus census.

## 2026-08-08 — fuse.zip wraps a 140-part split 7z; extract via py7zr venv
Undocumented on Zenodo: fuse.zip contains FUSE.7z.001..140 (64 MB parts) plus
a README pointing at the defunct tera-PROMISE repo. No 7z binary exists on
this machine and `sudo apt` needs a password (Law 13: no privileged installs),
so `make corpus` reassembles the parts by streaming them out of the zip in
order and extracts with py7zr inside a project-local venv
(corpus/work/venv, gitignored). Pure-userland, reproducible, no sudo.
Zenodo pins only an md5 for fuse.zip (verified: 13e955c4…); our own
sha256 (4f9126bd…) is the committed reproducibility anchor.

## 2026-08-08 — Corpus tooling lives in xlc-cli
`xlc corpus-filter` / `corpus-subset` / `corpus-verify` are subcommands of the
product binary rather than a separate tool: they exercise the same calamine
ingest path the compiler will use, so corpus filtering doubles as an early
integration test of §8.1.
