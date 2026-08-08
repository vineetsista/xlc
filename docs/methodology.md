# XLC methodology

Written continuously per Law 14. This document explains how every claim XLC
makes is verified, and — just as important — what XLC cannot tell you.

## The oracle

Every `.xlsx` stores, for each formula cell, both the formula text and the
value Excel last computed for it. XLC's primary correctness claim ("the
receipt") is produced by re-deriving every formula cell from raw inputs and
bit-comparing against Excel's own cached values, under an explicit, published
ULP policy (Phase 3). The corpus is ~16,000 real-world workbooks from Common
Crawl (FUSE, CC-BY-4.0), not synthetic tests.

A secondary oracle cross-checks disagreements: LibreOffice Calc driven via
UNO with an explicit `calculateAll()` — never `soffice --convert-to`, which
silently emits the file's cached values without recomputing (its OOXML
recalc-on-load default is "Never recalculate" and there is no CLI flag).

## Corpus provenance

- FUSE (Zenodo 581678, CC-BY-4.0): 249,376 spreadsheets extracted from
  Common Crawl; filtered here to valid OOXML workbooks containing formulas.
- SpreadsheetBench (CC-BY-SA-4.0): used for testing only, never vendored.
- All sources are pinned by URL + sha256 in `corpus/Makefile`; the committed
  500-workbook regression subset is pinned per-file in `corpus/manifest.json`.
- Corpus runs skip unreadable files and report the skip rate; a corpus
  drawn from the web contains garbage and the skip rate is itself a metric.

## What XLC cannot tell you

- **Whether your model is *right*.** XLC proves it recomputes what Excel
  computed and flags structural anomalies with evidence. It cannot know that
  your discount rate should have been 12% instead of 8%.
- **Anything about cells it excluded.** Partial compilation reports exactly
  which cells were excluded and why (VBA, external links, unimplemented
  functions, volatile functions under a fixed-seed policy); claims never
  extend to excluded cells.
- **Whether a flagged irregularity is intentional.** Detectors are calibrated
  to ≥90% measured precision on real workbooks, which still means up to 1 in
  10 findings is deliberate modeling. Every finding carries its evidence so a
  human can decide in seconds, and suppressions persist.
- **Distributional truth.** Monte Carlo results are exactly as good as the
  input distributions you chose. XLC's five distributions and independence
  assumption are stated on every scenario report.
- **Anything about `.xls` (BIFF), Power Query refresh, RTD/DDE feeds, or
  add-in functions.** Out of scope in v1 and reported as exclusions.

## Reproducibility

- Deterministic RNG: counter-based (scenario k derivable from (seed, k));
  results are identical across machines and thread counts.
- Every published benchmark records machine, ISA (AVX-512/NEON/WASM-SIMD128),
  and the one command that reproduces it. Native and browser numbers are
  published separately, never blended.
