# XLC — STATE
Updated: 2026-08-08   |   LAUNCH TARGET: TBD (set after Phase 1 census)   |   Week 1
Phase: 0 — Bootstrap and corpus          Gate: FAILING: subset not yet built (blocked on FUSE extraction, in flight)

## Done / In flight / Blocked
- Done: scaffold (10 crates build, stable 1.97.1), gate dispatcher + gate0 + gate1
  (both written before their code, Law 12), corpus/Makefile with all sha256 pins
  (FUSE md5 verified vs Zenodo, our sha256 4f9126bd… pinned; SB 9cf7228b…),
  corpus-filter/subset/verify smoke-tested end-to-end, census subcommand +
  7 unit tests + projection.py ready for Phase 1, first tests/cases entry.
- In flight: fuse-cc-binaries.tar.gz extraction (detached tar, monitored) — the 7z held one more layer with the actual 249k files. Then: filter → subset → gate0 → census → gate1.

- Blocked: none (extraction is a wait, not a blocker; census tooling was
  built during the wait per Never-Stall rung 6).

## Next action
When extraction completes: run corpus-filter over work/fuse (detached +
monitored), then corpus-subset → commit subset + manifest → `make gate-0`.

## Numbers of Record
corpus workbooks: pending (FUSE extracting) | parse rate: — | receipt pass rate: — | functions implemented: 0
detectors shipped: 0 | precision per detector: — | refusal rate: — | partial-compile rate: —
scenario throughput native: — | browser: — | bytes moved per scenario: —
peak RSS: — | incremental recompute latency: —

## Outside conversations
last: never | count: 0
