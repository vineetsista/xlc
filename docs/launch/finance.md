# Finance-community launch draft (Law 11: the found-bug story)

**Where:** r/excel, r/FPandA, finance LinkedIn.
**Title:** I ran 16,167 real spreadsheets through a formula auditor. Here's what a broken copy-paste looks like at scale.

**Body:**

Copied formulas drift. One row of seventeen sums B21:B24 while its
sixteen siblings sum B21:B23 — and the cell it silently includes holds
59,384. A SUM includes its own header row ("Total Cost" — text, so
nobody notices until the header becomes a number). Two rows reference
each other's targets because they were transposed during a late-night
edit. We found all of these, with proof strings a reviewer can check in
seconds, by auditing 16,167 public spreadsheets.

The tool is a free page: drop your workbook, and on your machine — no
upload, no signup, no server — it recompiles every formula, verifies it
can reproduce the numbers Excel cached (so you can trust the rest of
what it says), and lists deviations from your own copied patterns as
compiler-style warnings, each with the evidence. Mark any finding
"intentional" and it stays suppressed for that file.

It will not tell you your discount rate is wrong. It will tell you that
K25 sums three rows where sixteen siblings sum four, and show you the
number that fell out. In our audits, nine out of ten findings the tool
ships are real drift — we measure that precision on public data and
publish the audit, including the detector versions we rejected.

[SITE-LINK]
