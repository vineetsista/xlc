# HUMAN_TODO

Items only a human may do (Law 13: no money, no accounts, no identity, no
public posts). Each carries exact steps. Nothing here blocks the build unless
flagged at the top of STATE.md.

## Install LibreOffice + UNO for the secondary oracle (Phase 3, non-blocking)

The receipt found oracle-noise classes (stale caches, display-rounded
caches — docs/methodology.md "The oracle's noise floor"). The planned
adjudicator is LibreOffice driven via UNO (XLC.md §8.4 — never
`soffice --convert-to`, it silently emits cached values). Installing needs
sudo, which the agent does not use:

```
sudo apt-get install -y libreoffice-calc python3-uno
```

After install, tell the agent; it will build the UNO adjudication harness
(`scripts/lo_oracle.py`) and re-classify the disputed-cell list. Until then
the receipt's mismatch classes simply retain the suspected-oracle-noise
cells — measured, documented, non-blocking.

## Phase 4 first-launch actions (agent-prepared, human-posted — Law 13)

The free, no-signup, in-browser Excel bug finder is built and verified
(web/dist after `npm run build`; 13/13 headless-Chromium checks). Human
steps when ready to launch:

1. Choose + register a domain; host `web/dist/` on any static CDN
   (the only server the product ever has, per Law 1).
2. Review the page copy in `web/index.html`.
3. The launch posts themselves are drafted in Phase 9 (two messages, two
   surfaces, Law 11); early soft-launch is possible sooner if desired —
   tell the agent and it will draft the copy for review.
