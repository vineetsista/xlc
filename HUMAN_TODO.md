# HUMAN_TODO

Only what Law 13 forbids the agent: accounts, money, identity, public
posting. Everything else has been done — the demo is live, hosting is
one click, the secondary oracle is installed user-space (no sudo needed
after all) and its adjudication runs automatically.

## 1. Share or host the product (pick either; both are prepared)

- **Zero-effort demo (already live, private until you share):**
  https://claude.ai/code/artifact/02d27d90-befe-44db-8195-d2e8525e6595
  Open it, drop an .xlsx, use the page's share menu when satisfied.
- **Real hosting (one click, pick one):**
  - GitHub Pages: push the repo to GitHub → Settings → Pages → Source
    "GitHub Actions". `.github/workflows/deploy-pages.yml` does the rest.
  - Netlify/Cloudflare Pages: drag-and-drop `xlc-site.zip` — the built
    `web/dist` site zipped (already built;
    regenerate anytime with `make deploy-package`). COOP/COEP headers are
    preconfigured in `web/netlify.toml` and `web/public/_headers`.
  - Optionally buy a domain and point it at the host.

## 2. Post the launches (posting is yours alone)

Drafts are final in `docs/launch/hn.md` (compiler story) and
`docs/launch/finance.md` (found-bug story). Both already carry working
links (the private artifacts — share them from their page menus, or swap
in your hosted URLs) — then post. Essay artifact:
https://claude.ai/code/artifact/dd596b1c-16a7-4fe6-91e8-e0516d4c72d0
The YC application draft is `docs/application/yc.md`.

## 3. Payment checkout (only when you set pricing)

Third-party checkout (Paddle/Stripe) for the CLI/CI tier — an account
decision, deferred until pricing exists.
