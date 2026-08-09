# YC application draft

**Company:** XLC — the compiler for Excel.

**What do you make?** A toolchain that treats Excel as the programming
language it is: a compiler that proves it reproduces Excel's own
computed values (99.35% of verifiable cells bit-exact across 16,167 real
workbooks), an auditor that finds copy-paste defects with hand-audited
90.8–100% precision, a Monte-Carlo engine that re-simulates 100,000
scenarios in 12.6 ms after an input change, and a semantic diff that
shows the exact input vector where two versions of a model disagree.
Everything runs locally — browser (817 KB wasm) or CLI — nothing is
uploaded, and the only server is a CDN.

**Why now / why us?** The technical moat is a corpus-verified semantics
of Excel nobody else has bothered to build: byte-exact parsing of 7M
real formulas (99.997%), the documented hostile corners (decimal-
faithful ROUND, criteria coercion asymmetries, dynamic-array inference,
stale-cache oracle noise), and an IR that coarsens copied-formula
families 11.2× — the substrate for vectorized simulation, AD, and diff
that cell-at-a-time recalculation can never offer. Every claim is a
committed artifact reproducible with one command.

**Traction plan:** the free in-browser auditor is the trial funnel
(census-measured refusal rate: 5.7% of real workbooks); the CLI's CI
mode (`xlc check --ci` fails builds on new findings) is the sticky
revenue tier; scenario/diff/AD are the expansion products. Finance never
has to leave Excel — we make the file better, we do not replace it.

**Progress:** ten crates of stable Rust, 96 tests, nine numeric release
gates all green, product verified end-to-end in headless Chromium.
Detailed build log and methodology (including "What XLC cannot tell
you") in the repo.
