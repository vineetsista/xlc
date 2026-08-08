# tests/cases — minimal workbooks with hand-verified expected values

Each `.xlsx` here is tiny, hand-constructed (see `scripts/make_fixture.py`
for the generation pattern), and carries its expected values in this table.
Phase 3's receipt must reproduce every cached value bit-for-bit.

| file | cell | formula | cached value | notes |
|---|---|---|---|---|
| basic-sum.xlsx | A3 | `SUM(A1:A2)` | 5 | A1=2, A2=3 |
| basic-sum.xlsx | B1 | `A3*2` | 10 | depends on a formula cell |

Convention: when a Never-Stall rung-3 event shrinks a corpus failure to a
minimal workbook, it lands here with its row in this table and the function
family in the filename (e.g. `round-half-away.xlsx`, `date-1900-bug.xlsx`).
