# Decisions

Dated, append-only. Each entry: the decision, the alternative, why.

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
