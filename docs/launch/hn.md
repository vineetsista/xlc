# HN launch draft (Law 11: the compiler story)

**Title:** Show HN: XLC – an optimizing compiler for Excel

**Body:**

Excel is the most widely used programming language on Earth and the only
one with no compiler, no linter, no tests, and no CI. I built the missing
toolchain in Rust: parse a .xlsx into a typed dataflow IR, prove it
re-derives the values Excel itself cached, find copy-paste defects with
machine-checkable proofs, and run 100k+ Monte-Carlo scenarios by
vectorizing across a scenario axis — all local, in the browser (817 KB
wasm) or CLI. Nothing uploads, ever.

The part I'd want to read about: every xlsx stores Excel's own computed
value next to each formula, which turns compiler verification into a data
problem. Against 16,167 Common-Crawl workbooks (7M formula cells): parser
round-trips 99.997% byte-exact; the evaluator re-derives 99.35% of
verifiable cells bit-exactly (exclusions printed, never hidden); the
corpus taught us that ROUND is decimal-faithful, that SUMIF coerces
"003607" to match both text and number while VLOOKUP won't, and that some
files' caches are simply stale — the oracle has a noise floor. The linter
gates itself on hand-audited precision and rejected its own first two
versions (27%, then 72%) before shipping at 90.8–100%. No JIT — a
pulp-vectorized interpreter over coarsened copied-formula families gets
AVX-512 that Cranelift can't emit; a single-input change on a 40k-formula
model re-simulates 100k scenarios in 12.6 ms.

Engineering essay with all the artifacts: https://claude.ai/code/artifact/dd596b1c-16a7-4fe6-91e8-e0516d4c72d0 (or your hosted copy). Every number is
reproducible with `make bench`.
