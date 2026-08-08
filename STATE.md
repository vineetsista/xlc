# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: TBD (set after Phase 1 census)   |   Week 1
Phase: 0 — Bootstrap and corpus          Gate: FAILING: subset not yet built (gate written first per Law 12)

## Done / In flight / Blocked
- Done: repo scaffold (10 crates, workspace builds on stable 1.97.1), git init,
  gate dispatcher (exit 0/1/2/3) + gate0.sh, corpus/Makefile with pinned sources,
  SpreadsheetBench fetched + sha256 pinned (9cf7228b…).
- In flight: FUSE 9.4 GB download from Zenodo (background, ~16 MB/s).
  sha256 pin PENDING until it lands; md5 pin from Zenodo API: 13e955c44f0b77d1c36088c0bbb3366d.
- In flight: `xlc corpus-filter` / `corpus-subset` / `corpus-verify` subcommands.
- Blocked: none.

## Next action
When fuse.zip lands: verify md5, pin sha256 in corpus/Makefile, extract, run
corpus-filter over FUSE, build the deterministic 500-workbook subset + manifest,
commit, run `make gate-0`.

## Numbers of Record
corpus workbooks: pending | parse rate: — | receipt pass rate: — | functions implemented: 0
detectors shipped: 0 | precision per detector: — | refusal rate: — | partial-compile rate: —
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
