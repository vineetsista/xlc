# Finance-community launch drafts (Law 11: the found-bug story)

Channel rules live-verified 2026-08-09; full detail in `playbook.md`.
The landmine first: **r/FPandA bans every tool/app post — sidebar
rule 3, automatic permanent ban. Do not post the tool there.**
Participate there as a person, or use their official Discord; the tool
comes up only if someone asks what you use.

---

## r/excel — flair "Show and Tell" (890k members)

Rules that matter: TEXT post only (link and image posts are
auto-removed — screenshots go inside the text post), the flair's wiki
demands "show what you did, and tell how you did it", body ≥10 words,
title ≤150 chars, no [square tags], and Reddit's ~10% self-promo norm —
answer a few questions from the account before posting. Mention nothing
paid.

**Title:** I ran 16,167 public spreadsheets through a formula auditor I
built. Here's what broken copy-paste looks like at scale — and how the
tool proves each finding.

**Body:**

Copied formulas drift. One row of seventeen sums B21:B24 while its
sixteen siblings sum B21:B23 — and the cell it silently includes holds
59,384. A SUM includes its own header row ("Total Cost" — text, so
nobody notices until the header becomes a number). Two rows reference
each other's targets because they were transposed during a late-night
edit. I found all of these, with proof strings a reviewer can check in
seconds, by auditing 16,167 public spreadsheets.

**How it works** (the Tell part): the page parses your .xlsx in
WebAssembly, right in the browser tab — nothing uploads, there is no
server. It rebuilds every formula's dependency graph, recomputes the
whole workbook, and first verifies *itself*: it bit-compares its own
results against the values Excel cached inside your file. That green
"receipt" is why you can trust the warnings that follow. Then it looks
for one specific thing: cells that deviate from their own copied
pattern — a range one row short of its siblings, a hardcoded constant
in a formula column, a reference that slipped during a paste. Every
warning shows its evidence (which siblings disagree, which cell fell
out of the sum), and one click marks it "intentional" and keeps it
suppressed for that file.

*(embed the two ready screenshots here — `docs/launch/assets/receipt.png`
and `docs/launch/assets/finding-caret.png`, both captured from the live
sample audit)*

It will not tell you your discount rate is wrong. It will tell you that
K25 sums three rows where sixteen siblings sum four, and show you the
number that fell out. Nine out of ten findings it ships are real drift —
I measure that precision on public data and publish the audit, including
the two detector versions I rejected for being too noisy.

Free, no signup, and safe to try on a real work file because nothing
leaves your machine: https://vineetsista.github.io/xlc/
(source, MIT: https://github.com/vineetsista/xlc)

---

## LinkedIn — findings carousel (personal profile, not a page)

Format notes (2026): document/PDF carousels get ~3× the reach of text
for this audience; external links in the body are reach-suppressed —
the link goes in the FIRST COMMENT; end on a genuine question. Post
9–11am in the audience's timezone, midweek.

**Carousel: "Seven ways real spreadsheet models silently break"**
(one corpus finding per slide)

1. The copied row that sums one cell fewer than its 16 siblings — and
   the 59,384 that fell out
2. The SUM that includes its own header row
3. The transposed pair — two rows referencing each other's targets
4. The hardcoded constant hiding in a formula column
5. The #REF! a later formula still reads
6. The stale cache — a file whose stored answers disagree with its own
   stored inputs
7. How to catch all six in ten seconds, on your machine, with nothing
   uploaded — the free tool, revealed on the last slide

Closing question: "What's the worst formula bug that ever made it into
a board pack?"

First comment: the link + "runs locally — your model never leaves your
laptop."

---

## Amplifiers (before or at launch)

FP&A reach flows through a small set of creators — Paul Barnhurst (The
FP&A Guy; Financial Modeler's Corner podcast), Nicolas Boucher,
Christian Martinez. A short DM offering the tool for review beats any
algorithm; one share from that tier outperforms the post itself.
Podcast pitches (Financial Modeler's Corner; FP&A Today) should lead
with the research — "we bit-diffed 16,167 real workbooks against
Excel's own cached values" — never the product. EuSpRIG's mailing list
(groups.io/g/eusprig) welcomes the methodology post as-is; their 2027
conference CFP (~Oct 2026) is worth a paper.
