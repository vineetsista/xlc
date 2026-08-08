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
