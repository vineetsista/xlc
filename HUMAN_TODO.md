# HUMAN_TODO — your exact checklist

Everything else is done: the code (ten gates green), the public repo, the
live site, the benchmarks, the drafts. This file is the complete list of
what only you can do, written so you can come back cold in a month and
finish in one sitting.

Channel rules and incumbent pricing were live-verified 2026-08-09; the
reasoning behind every step below is in `docs/launch/playbook.md`.

---

## Step 0 — Sanity check (2 minutes)

Open **https://vineetsista.github.io/xlc/** and click the sample
workbook (then, if you like, drop a real `.xlsx` — nothing uploads).
You should see three lines: *compiled N formulas*, a green *receipt*,
and findings as warnings with proofs. The one-click sample matters:
Show HN readers must see the tool work without uploading anything.

If anything looks wrong, open Claude Code in this directory and say:
`Read XLC.md and STATE.md, then fix <what you saw>.`

## Step 1 — Decide the free/paid line (1 minute, BEFORE any posting)

Recommended (playbook, "The line"): browser tool + CLI free forever;
a paid per-organization CI tier later ($950/yr, offline license keys).
If you agree, tell the agent "add the pricing promise to the README" —
the line must be public before the launch posts (tldraw's retroactive
fencing backlash is the cautionary tale). If you'd rather never charge,
that's also a decision — say so and skip Step 4.

## Step 2 — Post the launches, in this order

⚠️ **Never post the tool to r/FPandA — their rule 3 auto-permabans any
app/tool post.** Participate there as a person only.

1. **Hacker News** — Tue/Wed/Thu, 9am–12pm ET. Open `docs/launch/hn.md`,
   submit as a Show HN, immediately add the prepared first comment,
   stay 90 minutes and answer everything. Never ask anyone to upvote.
2. **r/rust** — 1–2 days later, reframed for Rust readers (the essay's
   architecture sections are the base). Never the same text as HN.
3. **r/excel** — "Show and Tell" flair, TEXT post with screenshots
   embedded, from an account that has answered a few questions there
   first. Copy from `docs/launch/finance.md` (r/excel section). No paid
   mentions.
4. **LinkedIn** — findings carousel per `docs/launch/finance.md`
   (LinkedIn section); the link goes in the first comment, not the body.
5. **EuSpRIG mailing list** (groups.io/g/eusprig) — the methodology
   post; a small audience with maximal trust-per-reader.

Amplifiers, before or at launch: DM Paul Barnhurst (The FP&A Guy),
Nicolas Boucher, Christian Martinez offering the tool for review; pitch
Financial Modeler's Corner / FP&A Today as spreadsheet-error research,
not a product.

## Step 3 — Optional, whenever you feel like it

- **Custom domain**: buy one, then GitHub → xlc → Settings → Pages →
  Custom domain. (The github.io URL works fine meanwhile.)
- **YC application**: `docs/application/yc.md` is drafted with the real
  numbers. Paste into apply.ycombinator.com if you want the fast-ARR
  fork; the playbook's default is the durable solo path.
- **EuSpRIG 2027 paper**: CFP ~Oct 2026; the corpus/receipt work is a
  natural submission and buys durable credibility with the people who
  advise firms on spreadsheet risk.

## Step 4 — Money (after Step 1's decision, only when you want it)

- **Supporter SKU** ($29 one-time) within weeks of a good HN landing:
  create a Paddle or Stripe account, then tell the agent — it wires the
  checkout into the site (Law 13: it cannot make the account for you).
- **XLC CI tier** when the first team asks: same flow — you create the
  merchant account; the agent builds offline license-key verification
  and the published pricing page ($950/yr per organization recommended;
  anchors and reasoning in the playbook).

## Reference — where everything lives

| thing | where |
|---|---|
| live site | https://vineetsista.github.io/xlc/ (CI builds `web/dist` and deploys it on every push to `main`) |
| repo | https://github.com/vineetsista/xlc |
| backup demo (private until you share) | https://claude.ai/code/artifact/02d27d90-befe-44db-8195-d2e8525e6595 |
| launch playbook (why every step above) | `docs/launch/playbook.md` |
| launch drafts | `docs/launch/hn.md`, `docs/launch/finance.md` |
| essay / application | `docs/essay.md`, `docs/application/yc.md` |
| every published number | `docs/benchmarks/` (re-derive: `make bench`) |
| what the tool can't tell you | `docs/methodology.md` |
| project state / backlog | `STATE.md` |

To resume building (backlog: `^`-precision class, TEXT function, wasm
threads, more detectors): open Claude Code here and say
`Read XLC.md in full. Follow its Session Protocol. Continue from STATE.md.`
