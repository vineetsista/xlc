# HUMAN_TODO — your exact checklist

Everything else is done: the code (ten gates green), the public repo, the
live site, the benchmarks, the drafts. This file is the complete list of
what only you can do, written so you can come back cold in a month and
finish in one sitting.

---

## Step 0 — Sanity check (2 minutes)

Open **https://vineetsista.github.io/xlc/** and drop any `.xlsx` you have.
You should see three lines: *compiled N formulas*, a green *receipt*, and
any findings as warnings with proofs. Nothing uploads — you can do this
with a real work file.

If anything looks wrong, open Claude Code in this directory and say:
`Read XLC.md and STATE.md, then fix <what you saw>.`

## Step 1 — Post the two launches (the only real task)

Two audiences, two messages, posted separately (Law 11 — HN upvotes
compilers; finance people buy bug-finders):

1. **Hacker News** — open `docs/launch/hn.md`, copy the title and body,
   submit at https://news.ycombinator.com/submit as a **Show HN**.
   Links inside already point at the live site and repo.
   Tip: post on a weekday morning US-Pacific; stay around for the first
   hour to answer comments (the essay at `docs/essay.md` has the answers
   to the likely technical questions).
2. **Finance surfaces** — open `docs/launch/finance.md`, post to
   r/excel and/or r/FPandA and/or LinkedIn. It reads standalone.

## Step 2 — Optional, whenever you feel like it

- **Custom domain**: buy one, then in GitHub → xlc → Settings → Pages →
  Custom domain. (The site also works fine at the github.io URL, which
  serves the built `web/dist`.)
- **YC application**: `docs/application/yc.md` is drafted with the real
  numbers. Paste into the form at apply.ycombinator.com if you want to.
- **Payment / pricing**: when you decide to charge for the CLI/CI tier,
  create a Paddle or Stripe account and tell the agent — it will wire
  the checkout into the site. Nothing to do until you set a price.

## Reference — where everything lives

| thing | where |
|---|---|
| live site | https://vineetsista.github.io/xlc/ (auto-deploys from `main`) |
| repo | https://github.com/vineetsista/xlc |
| backup demo (private until you share) | https://claude.ai/code/artifact/02d27d90-befe-44db-8195-d2e8525e6595 |
| launch drafts | `docs/launch/hn.md`, `docs/launch/finance.md` |
| essay / application | `docs/essay.md`, `docs/application/yc.md` |
| every published number | `docs/benchmarks/` (re-derive: `make bench`) |
| what the tool can't tell you | `docs/methodology.md` |
| project state / backlog | `STATE.md` |

To resume building (backlog: `^`-precision class, TEXT function, wasm
threads, more detectors): open Claude Code here and say
`Read XLC.md in full. Follow its Session Protocol. Continue from STATE.md.`
