# XLC — an optimizing compiler for Excel

The complete specification is `XLC.md`. Read it in full at the start of every
session and follow its Session Protocol exactly. Mutable state lives in
`STATE.md`; human-only items in `HUMAN_TODO.md`.

## Commands

- `make gate` — run the current phase's gate (phase number in `.phase`).
  Exit codes: 0 pass · 1 fail · 2 unwritten · 3 unmeasurable here.
- `make gate-N` / `make gate-all` — specific gate / all written gates.
- `make corpus` — fetch, verify, extract, filter the corpus (reproducible;
  raw data in `corpus/raw/`, gitignored).
- `make build` / `make test` / `make clippy` — cargo across the workspace.
- `make receipt` — the bit-diff against Excel's cached values (Phase 3+).

## Unchanging conventions

- Rust stable only, no nightly. SIMD via `pulp` (pinned =0.22.3),
  xlsx via `calamine` (pinned =0.36.1). No JIT in v1.
- Gates are committed as `gate(N): ...` BEFORE the code they check (Law 12).
  Gate scripts live in `scripts/gates/gateN.sh`.
- Corpus runs never stop on a bad file: skip, log, count (Never-Stall rung 2).
- LibreOffice oracle: UNO only, never `soffice --convert-to` (it silently
  emits cached values — XLC.md §8.4).
- SpreadsheetBench is CC-BY-SA: testing only, never vendored.
- Nothing leaves the user's machine (Law 1). The agent never spends money,
  makes accounts, or posts publicly (Law 13) — those go to `HUMAN_TODO.md`.
- Every finding ships with a machine-checkable proof (Law 8). Detectors ship
  only above ~90% hand-audited precision (Law 7).
- Update `STATE.md` + append to `docs/build-log.md` + commit every ~90 min
  and at session end.
