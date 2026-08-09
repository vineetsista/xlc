# Launch & monetization playbook

Market data live-verified 2026-08-09 (four research passes over vendor
pricing pages, community rules, and twenty open-core monetization
precedents). Prices are what the pages said that day; sources at the
bottom. Human-only actions stay in `HUMAN_TODO.md` (Law 13).

## The market XLC lands in

**Monte-Carlo-in-Excel incumbents** — all Windows desktop add-ins, none
in-browser, none with CI integration:

| product | price (verified Aug 2026) | note |
|---|---|---|
| @RISK (Lumivero) | ~€2,900–3,500/seat/yr, unpublished (contact-sales only) | market leader; pricing opacity is widely resented |
| DecisionTools Suite | ~$3,500–5,000/seat/yr, unpublished | the bundle above @RISK |
| Frontline Analytic Solver | $2,500/yr published | + $500 startup fee |
| Vose ModelRisk | €1,550/yr published | the "value" incumbent — the professional floor |
| Oracle Crystal Ball | ~$1,089–1,210 perpetual + $219/yr support | maintenance mode; a stranded install base |
| RiskAMP | $279 one-time / $19/mo | reviewed as minimal — the toy tier |

**Spreadsheet-audit incumbents** — per-seat add-ins and quote-gated
suites:

| product | price | note |
|---|---|---|
| PerfectXL | from ~€69/user/mo per tool; suites quote-only | closest feature competitor; processes files on their servers |
| Operis OAK Professional | £312/yr (£95 one-time Essentials) | tool is a lead-in to Operis's audit services |
| ExcelAnalyzer | ~€800/user/yr (via Capterra) | audit-firm oriented |
| Spreadsheet Detective | AU$70–225/user/yr published | Big-4 firms hold site licenses |
| Macabacus | ~$395/user/yr | productivity suite, partial overlap |
| CIMCON / Apparity / ClusterSeven | enterprise quote (5–6 figures) | EUC governance — proof compliance buyers pay heavily for *continuous* checking |

**What is unoccupied:** a genuinely free tier (only charity licenses and
timed trials exist anywhere), local-first processing (the one free web
entrant, excelriskcheck.com, uploads your file to its servers), and CI
integration (nobody has it). The free browser auditor sits on empty
ground; the paid CI tier has no direct comparable — the anchors are
$250–2,000/yr/seat add-ins below it and five-figure EUC platforms above.

## The line (decide before the first post, then never move it)

The precedents are unambiguous. Insomnia forced cloud accounts onto a
local tool and created its own competitor (Bruno). tldraw retroactively
fenced free production use behind $6k/yr keys and burned a year of
goodwill. Fig monetized nothing and ended as an acqui-hire-and-shutdown.
Earthly had real adoption but sold "developer productivity", which has
no budget line, and shut down July 2025. The rules that fall out:

1. The browser tool and the local CLI are **free forever, for everyone,
   commercial use included** — said in the README *before* launch. This
   is the distribution engine (Tailscale's stated model), and it is
   Law 1's natural pricing: the free tier costs nothing to serve.
2. Paid = **the CI surface, per organization** — `xlc check --ci` in a
   pipeline, baseline management, semantic diff on pull requests. That
   sells into the model-risk / audit / SOX budget line, which
   demonstrably pays (the EUC platforms), not the dev-productivity line
   that killed Earthly.
3. License keys verify **offline** (xlwings PRO proves the Excel niche
   accepts per-developer offline keys). A paid tier must never
   contradict "nothing leaves your machine".
4. Never move a feature from free to paid. New paid capabilities only.

## Recommended tiers

- **Free** — browser app + CLI, MIT, no signup, no telemetry. Forever.
- **Supporter** — $29 one-time (badge in the README, priority issue
  label). Ship within weeks of a good HN landing; Bruno's $19 Golden
  Edition and Obsidian's $25 Catalyst show HN-native audiences convert
  goodwill this way, and it validates willingness-to-pay with zero
  support burden.
- **XLC CI** — **$950/yr per organization**, flat, self-serve,
  published price, offline key, unlimited internal repos and users.
  Per-org flat is the Sidekiq model (solo dev, $3M→~$10M/yr on
  per-organization annual licenses): procurement-friendly and
  support-light for one person. $950 undercuts every per-seat incumbent
  while staying credible to buyers conditioned to four-figure
  spreadsheet-risk spend.
- **Later, if pulled** — a Firm tier (~$4,500/yr: support SLA, priority
  detector requests, audit-trail reports). Never quote-gated sales — a
  solo founder cannot afford the PerfectXL sales motion.

Expectation-setting from the precedents: freemium devtools convert
2–4% of free users; Plausible took four years to $1M ARR bootstrapped;
Sidekiq took a decade to $10M/yr. This is a durable-business plan, not
a fast-ARR plan — `docs/application/yc.md` is the fast-ARR fork if that
is ever wanted.

## Sequencing

| when | what |
|---|---|
| pre-launch | README gets the free-forever pricing promise (human decision, Step 1 of HUMAN_TODO); verify the one-click sample on the live site; the r/excel account answers a few questions; DM the FP&A creators (Barnhurst, Boucher, Martinez) |
| launch day (Tue–Thu, 9am–12pm ET) | Show HN per `hn.md` — maker first-comment immediately, 90 minutes of answers |
| +1–2 days | r/rust (native Rust framing, never the HN text); r/excel Show-and-Tell per `finance.md` |
| week 1–2 | LinkedIn findings carousel per `finance.md`; EuSpRIG mailing-list methodology post; podcast pitches (Financial Modeler's Corner, FP&A Today) framed as research |
| weeks 2–4 | if HN landed: Supporter SKU (merchant account is a human step; the agent wires checkout) |
| Oct 2026 | EuSpRIG 2027 CFP opens — the corpus/receipt work is a natural paper |
| when the first team asks | ship offline license-key verification + the published $950 CI page |

## Channel rules (live-verified 2026-08-09)

- **r/FPandA: never post the tool. Automatic permanent ban** — sidebar
  rule 3 covers every app/tool/blog/survey post. Genuine participation
  and their official Discord are the only routes in.
- **r/excel**: text posts only (link/image posts auto-removed), "Show
  and Tell" flair requires explaining *how* it works, account needs
  prior participation (site-wide ~10% self-promo norm), no paid
  mentions.
- **Show HN**: mechanism framing wins in this exact niche —
  compiler/Rust/WASM posts scored 79–225 points in 2024–26 while
  "Excel productivity tool" framings died at 2–5. Preloaded demo
  required; maker first-comment; never solicit votes (voting-ring
  detection); effectively one shot per year.
- **LinkedIn**: findings-shaped PDF carousel from a personal profile
  (~3× the reach of text for this audience); external link goes in the
  first comment — body links are reach-suppressed.
- **MrExcel / finance Slacks** (Off The Ledger is explicitly
  "sales-free"): no pitches, long-game participation only.
- Everywhere: never the same text twice; stagger channels by days.

## Sources

Vendor pages fetched 2026-08-09: shop.lumivero.com, solver.com/pricing,
vosesoftware.com/products/pricing.php, taradigm.com, riskamp.com,
perfectxl.com/pricing, operisanalysiskit.com/oak-price,
spreadsheetdetective.com, capterra.com (ExcelAnalyzer), vendr.com
(Macabacus). Community rules from the live sidebars/wikis of r/excel,
r/FPandA, r/rust, news.ycombinator.com/showhn.html, mrexcel.com; Show HN
timing from a 157k-post study plus an Algolia pull of 2024–26
Excel/WASM/compiler launches. Precedents: Sidekiq, Semgrep, Infracost,
Nx, Tailscale, Bruno, Obsidian, HTTP Toolkit, xlwings PRO, Plausible,
Excalidraw, Astral/uv, DuckDB/MotherDuck; failure postmortems: Earthly,
Insomnia 8.0, tldraw 4.0 licensing, Fig.
