# HUMAN_TODO

Only what Law 13 forbids the agent: accounts, money, identity, public
posting. Everything else has been done — the demo is live, hosting is
one click, the secondary oracle is installed user-space (no sudo needed
after all) and its adjudication runs automatically.

## 1. Hosting — DONE (verify, optionally add a domain)

- Public repo: https://github.com/vineetsista/xlc (created + pushed).
- GitHub Pages enabled via API; the site deploys from `main` to
  **https://vineetsista.github.io/xlc/** on every push. Verify it loads
  and drop an .xlsx.
- Also live as a private-until-shared artifact:
  https://claude.ai/code/artifact/02d27d90-befe-44db-8195-d2e8525e6595
- Optional: buy a custom domain and point it at either host (alternate
  one-click hosts preconfigured: `xlc-site.zip` for Netlify/CF drop,
  regenerate with `make deploy-package`; the `web/dist` build output is
  what every host serves).

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
