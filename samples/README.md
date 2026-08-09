# samples

`acme-budget-fy2027.xlsx` — a small quarterly budget model for trying the
demo at https://vineetsista.github.io/xlc/. Its cached values are
internally consistent (the receipt verifies 29/29 cells bit-exact), and
it contains two planted, findable defects:

- **Budget!F9** — the "Marketing & events" total sums `B9:D9`, one
  quarter short of its eleven siblings' `B:E` (range-off-by-one; the
  missing Q4 cell holds 66,500).
- **Budget!G7** — the "Travel" share-of-total reads `F8` (the row below)
  instead of `F7` (slipped reference).

Direct download:
https://github.com/vineetsista/xlc/raw/main/samples/acme-budget-fy2027.xlsx
