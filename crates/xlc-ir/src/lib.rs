//! Typed dataflow IR with range coarsening (§8.5).
//!
//! The key move: copied-formula families — one formula replicated along a
//! column or row with consistent relative offsets — become ONE vector node
//! of width W instead of W scalar nodes. Inside each family the exemplar
//! formula lowers to an instruction DAG with hash-consing CSE on pure
//! subtrees; function calls stay lazy black boxes (Excel's IF evaluates
//! only the taken branch — eager lowering would manufacture errors).
//!
//! Invariant (Gate 5): per-lane IR evaluation calls the SAME xlc-eval
//! primitives the scalar interpreter uses, so results are bit-identical
//! by construction — verified across the 500-workbook subset anyway.

use std::collections::HashMap;

use xlc_eval::interp::{Ctx, Interp, Operand, Origin, SheetId};
use xlc_eval::workbook::Workbook;
use xlc_eval::Value;
use xlc_parse::ast::{Area, BinOp, Coord, Expr, UnOp};
use xlc_parse::shape::shapes;

pub type InstId = u32;

/// One instruction. Operands are earlier InstIds (topological by
/// construction).
#[derive(Debug, Clone, PartialEq)]
pub enum Inst {
    Num(f64),
    Text(String),
    Bool(bool),
    /// Reference template: the exemplar's area with the axes that shift
    /// per lane (unanchored ones) rebased at evaluation time.
    RefT { sheets: Vec<SheetId>, area: Area },
    Binary { op: BinOp, a: InstId, b: InstId },
    Unary { op: UnOp, a: InstId },
    /// Lazy black box: names, calls, array literals, table refs, error
    /// literals, external refs — evaluated through the scalar interpreter
    /// at the lane's origin.
    Opaque { ast: Expr },
}

/// A coarsened node: W lanes sharing one instruction body.
pub struct Family {
    pub sheet: SheetId,
    /// (row, col) of each lane; lane 0 is the exemplar the body was
    /// lowered against.
    pub lanes: Vec<(u32, u32)>,
    pub insts: Vec<Inst>,
    pub result: InstId,
    /// Tree size before CSE (for the reduction metric).
    pub tree_insts: usize,
}

pub struct Module {
    pub families: Vec<Family>,
    pub formula_cells: usize,
}

impl Module {
    pub fn coarsening_nodes(&self) -> usize {
        self.families.len()
    }

    pub fn insts_before(&self) -> usize {
        self.families.iter().map(|f| f.tree_insts).sum()
    }

    pub fn insts_after(&self) -> usize {
        self.families.iter().map(|f| f.insts.len()).sum()
    }
}

/// Lower a workbook: group formula cells into contiguous same-shape runs
/// (vertical first, then horizontal, singletons last), lower each family's
/// exemplar into an instruction DAG.
pub fn lower(wb: &Workbook) -> Module {
    let mut families = Vec::new();
    let mut formula_cells = 0usize;

    for (sid, sheet) in wb.sheets.iter().enumerate() {
        let sid = sid as SheetId;
        let mut cells: HashMap<(u32, u32), (Expr, String)> = HashMap::new();
        for (&(row, col), src) in &sheet.formulas {
            formula_cells += 1;
            if let Ok(parsed) = xlc_parse::parse_formula(src) {
                let sh = shapes(&parsed.expr, row, col);
                cells.insert((row, col), (parsed.expr, sh.full));
            }
        }
        let mut claimed: HashMap<(u32, u32), bool> = HashMap::new();

        // Vertical runs: same column, contiguous rows, identical shape.
        let mut by_col: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(row, col) in cells.keys() {
            by_col.entry(col).or_default().push(row);
        }
        let mut cols: Vec<u32> = by_col.keys().copied().collect();
        cols.sort_unstable();
        for col in cols {
            let rows = by_col.get_mut(&col).unwrap();
            rows.sort_unstable();
            let mut i = 0;
            while i < rows.len() {
                let mut j = i;
                let shape_i = cells[&(rows[i], col)].1.clone();
                while j + 1 < rows.len()
                    && rows[j + 1] == rows[j] + 1
                    && cells[&(rows[j + 1], col)].1 == shape_i
                {
                    j += 1;
                }
                if j > i {
                    let lanes: Vec<(u32, u32)> = (i..=j).map(|k| (rows[k], col)).collect();
                    for &l in &lanes {
                        claimed.insert(l, true);
                    }
                    push_family(&mut families, wb, sid, lanes, &cells);
                }
                i = j + 1;
            }
        }

        // Horizontal runs over unclaimed cells (this also picks up
        // singletons as width-1 families).
        let mut by_row: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(row, col) in cells.keys() {
            if !claimed.contains_key(&(row, col)) {
                by_row.entry(row).or_default().push(col);
            }
        }
        let mut rows_keys: Vec<u32> = by_row.keys().copied().collect();
        rows_keys.sort_unstable();
        for row in rows_keys {
            let colv = by_row.get_mut(&row).unwrap();
            colv.sort_unstable();
            let mut i = 0;
            while i < colv.len() {
                let mut j = i;
                let shape_i = cells[&(row, colv[i])].1.clone();
                while j + 1 < colv.len()
                    && colv[j + 1] == colv[j] + 1
                    && cells[&(row, colv[j + 1])].1 == shape_i
                {
                    j += 1;
                }
                let lanes: Vec<(u32, u32)> = (i..=j).map(|k| (row, colv[k])).collect();
                for &l in &lanes {
                    claimed.insert(l, true);
                }
                push_family(&mut families, wb, sid, lanes, &cells);
                i = j + 1;
            }
        }
    }
    Module { families, formula_cells }
}

fn push_family(
    families: &mut Vec<Family>,
    wb: &Workbook,
    sheet: SheetId,
    lanes: Vec<(u32, u32)>,
    cells: &HashMap<(u32, u32), (Expr, String)>,
) {
    let exemplar = &cells[&lanes[0]].0;
    let mut lo = Lowerer { wb, sheet, insts: Vec::new(), cse: HashMap::new(), tree_insts: 0 };
    let result = lo.lower_expr(exemplar);
    families.push(Family { sheet, lanes, insts: lo.insts, result, tree_insts: lo.tree_insts });
}

struct Lowerer<'a> {
    wb: &'a Workbook,
    sheet: SheetId,
    insts: Vec<Inst>,
    /// Hash-consing over a structural key. Opaque insts never merge
    /// (laziness + volatility safety).
    cse: HashMap<String, InstId>,
    tree_insts: usize,
}

impl Lowerer<'_> {
    fn push(&mut self, inst: Inst, key: Option<String>) -> InstId {
        if let Some(k) = &key {
            if let Some(&id) = self.cse.get(k) {
                return id;
            }
        }
        let id = self.insts.len() as InstId;
        self.insts.push(inst);
        if let Some(k) = key {
            self.cse.insert(k, id);
        }
        id
    }

    fn lower_expr(&mut self, e: &Expr) -> InstId {
        self.tree_insts += 1;
        match e {
            Expr::Number { value, .. } => {
                self.push(Inst::Num(*value), Some(format!("n{:x}", value.to_bits())))
            }
            Expr::Text(s) => self.push(Inst::Text(s.clone()), Some(format!("t{s}"))),
            Expr::Bool { value, .. } => self.push(Inst::Bool(*value), Some(format!("b{value}"))),
            Expr::Paren { inner, .. } => {
                self.tree_insts -= 1; // parens are free
                self.lower_expr(inner)
            }
            Expr::Ref(xlc_parse::ast::RefExpr::Area { sheet: prefix, area })
                if !matches!(area, Area::RefError) =>
            {
                // Resolve the sheet span ONCE at lower time; anything odd
                // (external workbook, unknown sheet) becomes opaque.
                let sheets: Option<Vec<SheetId>> = match prefix {
                    None => Some(vec![self.sheet]),
                    Some(sp) if sp.workbook.is_some() => None,
                    Some(sp) => {
                        let first = self.wb.resolve_sheet_pub(&sp.first);
                        match (&sp.last, first) {
                            (None, Some(f)) => Some(vec![f]),
                            (Some(last), Some(f)) => self
                                .wb
                                .resolve_sheet_pub(last)
                                .map(|l| (f.min(l)..=f.max(l)).collect()),
                            _ => None,
                        }
                    }
                };
                match sheets {
                    Some(sheets) => {
                        let key = format!("r{sheets:?}|{area:?}");
                        self.push(Inst::RefT { sheets, area: area.clone() }, Some(key))
                    }
                    None => self.push(Inst::Opaque { ast: e.clone() }, None),
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let a = self.lower_expr(lhs);
                let b = self.lower_expr(rhs);
                self.push(Inst::Binary { op: *op, a, b }, Some(format!("B{op:?}|{a}|{b}")))
            }
            Expr::Unary { op, expr, .. } => {
                let a = self.lower_expr(expr);
                self.push(Inst::Unary { op: *op, a }, Some(format!("U{op:?}|{a}")))
            }
            // Calls, names, arrays, table refs, #REF! areas: lazy black box.
            _ => self.push(Inst::Opaque { ast: e.clone() }, None),
        }
    }
}

/// Rebase an entire expression tree by the lane delta: every relative
/// reference axis shifts; anchored axes, sheet prefixes, names, table
/// refs, and literals stay. This is the definition of a copied-formula
/// family — the exemplar AST plus an offset IS the lane's formula.
fn rebase_expr(e: &Expr, dr: i64, dc: i64) -> Expr {
    use xlc_parse::ast::{ArrayElem, CallArg, RefExpr};
    match e {
        Expr::Ref(RefExpr::Area { sheet, area }) => match rebase(area, dr, dc) {
            Some(a) => Expr::Ref(RefExpr::Area { sheet: sheet.clone(), area: a }),
            // Off-sheet shift: Excel shows #REF! for the dead axis.
            None => Expr::Ref(RefExpr::Area { sheet: sheet.clone(), area: Area::RefError }),
        },
        Expr::Binary { op, lhs, rhs, ws_l, ws_r } => Expr::Binary {
            op: *op,
            lhs: Box::new(rebase_expr(lhs, dr, dc)),
            rhs: Box::new(rebase_expr(rhs, dr, dc)),
            ws_l: ws_l.clone(),
            ws_r: ws_r.clone(),
        },
        Expr::Unary { op, expr, ws } => Expr::Unary {
            op: *op,
            expr: Box::new(rebase_expr(expr, dr, dc)),
            ws: ws.clone(),
        },
        Expr::Paren { ws_open, inner, ws_close } => Expr::Paren {
            ws_open: ws_open.clone(),
            inner: Box::new(rebase_expr(inner, dr, dc)),
            ws_close: ws_close.clone(),
        },
        Expr::Call { name, args } => Expr::Call {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| CallArg {
                    ws_before: a.ws_before.clone(),
                    expr: a.expr.as_ref().map(|e| rebase_expr(e, dr, dc)),
                    ws_after: a.ws_after.clone(),
                })
                .collect(),
        },
        Expr::ArrayLit(rows) => Expr::ArrayLit(
            rows.iter()
                .map(|row| {
                    row.iter()
                        .map(|el| ArrayElem {
                            ws_before: el.ws_before.clone(),
                            expr: rebase_expr(&el.expr, dr, dc),
                            ws_after: el.ws_after.clone(),
                        })
                        .collect()
                })
                .collect(),
        ),
        _ => e.clone(),
    }
}

/// Shift the unanchored axes of an area template by the lane delta.
fn rebase(area: &Area, dr: i64, dc: i64) -> Option<Area> {
    let shift_coord = |c: &Coord| -> Option<Coord> {
        let row = if c.row_anchored { c.row } else { u32::try_from(c.row as i64 + dr).ok()? };
        let col = if c.col_anchored { c.col } else { u32::try_from(c.col as i64 + dc).ok()? };
        Some(Coord { row, col, row_anchored: c.row_anchored, col_anchored: c.col_anchored })
    };
    Some(match area {
        Area::Cell(c) => Area::Cell(shift_coord(c)?),
        Area::CellRange(a, b) => Area::CellRange(shift_coord(a)?, shift_coord(b)?),
        Area::Cols { first, last, first_anchored, last_anchored } => {
            let f = if *first_anchored { *first } else { u32::try_from(*first as i64 + dc).ok()? };
            let l = if *last_anchored { *last } else { u32::try_from(*last as i64 + dc).ok()? };
            Area::Cols {
                first: f,
                last: l,
                first_anchored: *first_anchored,
                last_anchored: *last_anchored,
            }
        }
        Area::Rows { first, last, first_anchored, last_anchored } => {
            let f = if *first_anchored { *first } else { u32::try_from(*first as i64 + dr).ok()? };
            let l = if *last_anchored { *last } else { u32::try_from(*last as i64 + dr).ok()? };
            Area::Rows {
                first: f,
                last: l,
                first_anchored: *first_anchored,
                last_anchored: *last_anchored,
            }
        }
        Area::RefError => Area::RefError,
    })
}

impl Family {
    /// Evaluate one lane through the shared xlc-eval primitives.
    pub fn eval_lane<C: Ctx>(&self, ctx: &C, lane: usize) -> Value {
        let (row, col) = self.lanes[lane];
        let (er, ec) = self.lanes[0];
        let dr = row as i64 - er as i64;
        let dc = col as i64 - ec as i64;
        let interp = Interp::new(ctx, Origin { sheet: self.sheet, row, col });

        let mut vals: Vec<Operand> = Vec::with_capacity(self.insts.len());
        for inst in &self.insts {
            let v = match inst {
                Inst::Num(x) => Operand::Val(Value::Num(*x)),
                Inst::Text(s) => Operand::Val(Value::Text(s.clone())),
                Inst::Bool(b) => Operand::Val(Value::Bool(*b)),
                Inst::RefT { sheets, area } => match rebase(area, dr, dc) {
                    Some(a) => interp.resolve_area(sheets, &a),
                    None => Operand::Val(Value::Err(xlc_eval::ExcelError::Ref)),
                },
                Inst::Binary { op, a, b } => {
                    interp.apply_binary(*op, vals[*a as usize].clone(), vals[*b as usize].clone())
                }
                Inst::Unary { op, a } => interp.apply_unary(*op, vals[*a as usize].clone()),
                Inst::Opaque { ast } => {
                    if dr == 0 && dc == 0 {
                        interp.eval(ast)
                    } else {
                        interp.eval(&rebase_expr(ast, dr, dc))
                    }
                }
            };
            vals.push(v);
        }
        interp.finalize(vals[self.result as usize].clone())
    }
}

/// Bit-level value equality (NaN-safe: compares f64 bit patterns).
pub fn bit_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb_with(formulas: &[(u32, u32, &str)], values: &[(u32, u32, f64)]) -> Workbook {
        let mut wb = Workbook::default();
        let id = wb.add_sheet("S");
        for &(r, c, f) in formulas {
            wb.set_formula(id, r, c, f.to_string());
        }
        for &(r, c, v) in values {
            wb.set_value(id, r, c, Value::Num(v));
        }
        wb
    }

    fn check_drift(wb: &Workbook) -> (usize, usize) {
        let m = lower(wb);
        let mut compared = 0;
        let mut drift = 0;
        for fam in &m.families {
            for (lane, &(row, col)) in fam.lanes.iter().enumerate() {
                let src = &wb.sheets[fam.sheet as usize].formulas[&(row, col)];
                let parsed = xlc_parse::parse_formula(src).unwrap();
                let scalar = Interp::new(wb, Origin { sheet: fam.sheet, row, col })
                    .eval_formula(&parsed.expr);
                let ir = fam.eval_lane(wb, lane);
                compared += 1;
                if !bit_equal(&scalar, &ir) {
                    drift += 1;
                }
            }
        }
        (compared, drift)
    }

    #[test]
    fn column_family_coarsens_and_agrees() {
        let values: Vec<(u32, u32, f64)> = (0..50u32).map(|r| (r, 0u32, r as f64 + 1.0)).collect();
        let owned: Vec<(u32, u32, String)> =
            (0..50u32).map(|r| (r, 1u32, format!("A{}*2", r + 1))).collect();
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        let wb = wb_with(&refs, &values);
        let m = lower(&wb);
        assert_eq!(m.families.len(), 1, "one coarsened node");
        assert_eq!(m.families[0].lanes.len(), 50);
        let (compared, drift) = check_drift(&wb);
        assert_eq!((compared, drift), (50, 0));
    }

    #[test]
    fn cse_merges_repeated_subtrees() {
        // (A1*2)+(A1*2): the repeated subtree lowers once.
        let wb = wb_with(&[(0, 1, "(A1*2)+(A1*2)")], &[(0, 0, 21.0)]);
        let m = lower(&wb);
        let fam = &m.families[0];
        assert!(fam.insts.len() < fam.tree_insts, "{} < {}", fam.insts.len(), fam.tree_insts);
        assert_eq!(fam.eval_lane(&wb, 0), Value::Num(84.0));
    }

    #[test]
    fn mixed_shapes_split_families() {
        let owned: Vec<(u32, u32, String)> = (0..10u32)
            .map(|r| {
                let f = if r < 5 { format!("A{}*2", r + 1) } else { format!("A{}*3", r + 1) };
                (r, 1u32, f)
            })
            .collect();
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        let wb = wb_with(&refs, &[(0, 0, 1.0)]);
        let m = lower(&wb);
        assert_eq!(m.families.len(), 2);
        let (compared, drift) = check_drift(&wb);
        assert_eq!((compared, drift), (10, 0));
    }

    #[test]
    fn calls_and_anchors_agree() {
        let mut values = vec![(0u32, 0u32, 10.0)];
        for r in 0..20u32 {
            values.push((r, 2, (r as f64) * 1.5));
        }
        let owned: Vec<(u32, u32, String)> = (2..20u32)
            .map(|r| {
                (r, 3u32, format!("IF(C{}>$A$1,SUM(C1:C{}),ROUND(C{}%,2))", r + 1, r + 1, r + 1))
            })
            .collect();
        let refs: Vec<(u32, u32, &str)> =
            owned.iter().map(|(r, c, f)| (*r, *c, f.as_str())).collect();
        let wb = wb_with(&refs, &values);
        let (compared, drift) = check_drift(&wb);
        assert_eq!(compared, 18);
        assert_eq!(drift, 0);
    }
}
