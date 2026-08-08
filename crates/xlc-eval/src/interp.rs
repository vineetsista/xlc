//! The scalar interpreter (§8.4): plain, obviously correct, cell-at-a-time.
//! Speed is Phase 5/6's job; this is the receipt's reference semantics.
//!
//! Expressions evaluate to an `Operand`: either a concrete `Value` or a
//! still-lazy reference (rectangular area / union of areas). Functions
//! decide how to consume references (SUM iterates; scalar context derefs
//! via Excel's implicit-intersection rules).

use crate::value::{self, ExcelError, Value};
use xlc_parse::ast::{Area, BinOp, Coord, Expr, RefExpr, UnOp};

/// Sheet index within the workbook model.
pub type SheetId = u32;

/// What the interpreter needs from the workbook. The real model lands with
/// the ingest pipeline; tests use a mock.
pub trait Ctx {
    /// Value of a cell (already-computed; scheduling guarantees deps first).
    fn cell(&self, sheet: SheetId, row: u32, col: u32) -> Value;
    fn resolve_sheet(&self, name: &str) -> Option<SheetId>;
    /// Highest used (row, col) per sheet, for clipping whole-col/row refs.
    fn used_extent(&self, sheet: SheetId) -> (u32, u32);
    /// Body of a defined name, if known.
    fn defined_name(&self, name: &str) -> Option<&Expr>;
}

/// A rectangular area, resolved to concrete bounds (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub sheet: SheetId,
    pub r0: u32,
    pub c0: u32,
    pub r1: u32,
    pub c1: u32,
}

impl Rect {
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (self.r0..=self.r1).flat_map(move |r| (self.c0..=self.c1).map(move |c| (r, c)))
    }

    pub fn contains(&self, r: u32, c: u32) -> bool {
        (self.r0..=self.r1).contains(&r) && (self.c0..=self.c1).contains(&c)
    }
}

/// Evaluation result of a sub-expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Val(Value),
    /// One or more rectangles (unions produce >1). 3D refs contribute one
    /// rect per sheet in the span.
    Ref(Vec<Rect>),
}

/// Where the formula being evaluated lives — needed for implicit
/// intersection and (later) R1C1.
#[derive(Debug, Clone, Copy)]
pub struct Origin {
    pub sheet: SheetId,
    pub row: u32,
    pub col: u32,
}

pub struct Interp<'a, C: Ctx> {
    pub ctx: &'a C,
    pub origin: Origin,
}

impl<'a, C: Ctx> Interp<'a, C> {
    pub fn new(ctx: &'a C, origin: Origin) -> Self {
        Interp { ctx, origin }
    }

    /// Evaluate a formula to its final scalar value (the cell's value).
    pub fn eval_formula(&self, e: &Expr) -> Value {
        let op = self.eval(e);
        self.deref_scalar(op)
    }

    fn eval(&self, e: &Expr) -> Operand {
        match e {
            Expr::Number { value, .. } => Operand::Val(Value::Num(*value)),
            Expr::Text(s) => Operand::Val(Value::Text(s.clone())),
            Expr::Bool { value, .. } => Operand::Val(Value::Bool(*value)),
            Expr::Error(err) => Operand::Val(Value::Err(lower_error(*err))),
            Expr::Paren { inner, .. } => self.eval(inner),
            Expr::Ref(r) => self.eval_ref(r),
            Expr::Name { sheet: _, name } => match self.ctx.defined_name(name) {
                Some(body) => self.eval(body),
                None => Operand::Val(Value::Err(ExcelError::Name)),
            },
            Expr::Unary { op, expr, .. } => self.eval_unary(*op, expr),
            Expr::Binary { op, lhs, rhs, .. } => self.eval_binary(*op, lhs, rhs),
            Expr::Call { name, args } => self.eval_call(name, args),
            Expr::ArrayLit(_rows) => {
                // Array semantics arrive with the IR (§8.5); scalar receipt
                // treats a bare array literal as its top-left element.
                match _rows.first().and_then(|r| r.first()) {
                    Some(e) => self.eval(&e.expr),
                    None => Operand::Val(Value::Err(ExcelError::Value)),
                }
            }
        }
    }

    // ---- references ----

    fn eval_ref(&self, r: &RefExpr) -> Operand {
        match r {
            RefExpr::Area { sheet, area } => {
                let sheets: Vec<SheetId> = match sheet {
                    None => vec![self.origin.sheet],
                    Some(sp) => {
                        if sp.workbook.is_some() {
                            // External workbook: excluded from compilation
                            // (Law 9) — scalar receipt yields #REF!.
                            return Operand::Val(Value::Err(ExcelError::Ref));
                        }
                        let Some(first) = self.ctx.resolve_sheet(&sp.first) else {
                            return Operand::Val(Value::Err(ExcelError::Ref));
                        };
                        match &sp.last {
                            None => vec![first],
                            Some(last_name) => {
                                let Some(last) = self.ctx.resolve_sheet(last_name) else {
                                    return Operand::Val(Value::Err(ExcelError::Ref));
                                };
                                let (a, b) = (first.min(last), first.max(last));
                                (a..=b).collect()
                            }
                        }
                    }
                };
                let mut rects = Vec::with_capacity(sheets.len());
                for s in sheets {
                    match area_to_rect(area, s, self.ctx) {
                        Some(rect) => rects.push(rect),
                        None => return Operand::Val(Value::Err(ExcelError::Ref)),
                    }
                }
                Operand::Ref(rects)
            }
            RefExpr::Table(_) => {
                // Structured refs resolve at lowering (needs Table ranges
                // from ingest). Until then: excluded cell (#NAME? mirrors
                // Excel's behavior for unknown structured refs).
                Operand::Val(Value::Err(ExcelError::Name))
            }
        }
    }

    /// Scalar deref: single cell → value; multi-cell → implicit
    /// intersection with the formula's own row/column (legacy semantics);
    /// no intersection → #VALUE!.
    fn deref_scalar(&self, op: Operand) -> Value {
        match op {
            Operand::Val(v) => v,
            Operand::Ref(rects) => match rects.as_slice() {
                [r] if r.r0 == r.r1 && r.c0 == r.c1 => self.ctx.cell(r.sheet, r.r0, r.c0),
                [r] => {
                    // Implicit intersection: a 1-column range intersects the
                    // formula's row; a 1-row range intersects its column.
                    if r.c0 == r.c1 && (r.r0..=r.r1).contains(&self.origin.row) {
                        self.ctx.cell(r.sheet, self.origin.row, r.c0)
                    } else if r.r0 == r.r1 && (r.c0..=r.c1).contains(&self.origin.col) {
                        self.ctx.cell(r.sheet, r.r0, self.origin.col)
                    } else {
                        Value::Err(ExcelError::Value)
                    }
                }
                _ => Value::Err(ExcelError::Value),
            },
        }
    }

    /// Iterate every cell value in an operand (for range-consuming
    /// functions like SUM). Scalars yield themselves once.
    fn for_each_value(&self, op: &Operand, f: &mut dyn FnMut(Value)) {
        match op {
            Operand::Val(v) => f(v.clone()),
            Operand::Ref(rects) => {
                for r in rects {
                    for (row, col) in r.cells() {
                        f(self.ctx.cell(r.sheet, row, col));
                    }
                }
            }
        }
    }

    // ---- operators ----

    fn eval_unary(&self, op: UnOp, expr: &Expr) -> Operand {
        match op {
            UnOp::ImplicitIntersect => {
                let inner = self.eval(expr);
                Operand::Val(self.deref_scalar(inner))
            }
            UnOp::SpillRange => {
                // Dynamic-array spill ranges land with the IR; excluded for
                // the scalar receipt.
                Operand::Val(Value::Err(ExcelError::Name))
            }
            _ => {
                let v = self.deref_scalar(self.eval(expr));
                Operand::Val(match op {
                    UnOp::Neg => value::neg(&v),
                    UnOp::Pos => value::pos(&v),
                    UnOp::Percent => value::percent(&v),
                    _ => unreachable!(),
                })
            }
        }
    }

    fn eval_binary(&self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Operand {
        match op {
            BinOp::Range | BinOp::Union | BinOp::Intersect => self.eval_ref_op(op, lhs, rhs),
            _ => {
                let a = self.deref_scalar(self.eval(lhs));
                let b = self.deref_scalar(self.eval(rhs));
                Operand::Val(apply_scalar_binop(op, &a, &b))
            }
        }
    }

    fn eval_ref_op(&self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Operand {
        let a = self.eval(lhs);
        let b = self.eval(rhs);
        let (Operand::Ref(ra), Operand::Ref(rb)) = (&a, &b) else {
            // Range op over non-refs (e.g. INDEX(..):A1) needs ref-valued
            // function returns — not yet modeled; error out clearly.
            let e = first_error(&a).or_else(|| first_error(&b)).unwrap_or(ExcelError::Value);
            return Operand::Val(Value::Err(e));
        };
        match op {
            BinOp::Union => {
                let mut all = ra.clone();
                all.extend(rb.iter().copied());
                Operand::Ref(all)
            }
            BinOp::Range => match (ra.as_slice(), rb.as_slice()) {
                ([x], [y]) if x.sheet == y.sheet => Operand::Ref(vec![Rect {
                    sheet: x.sheet,
                    r0: x.r0.min(y.r0),
                    c0: x.c0.min(y.c0),
                    r1: x.r1.max(y.r1),
                    c1: x.c1.max(y.c1),
                }]),
                _ => Operand::Val(Value::Err(ExcelError::Value)),
            },
            BinOp::Intersect => {
                let mut out = Vec::new();
                for x in ra {
                    for y in rb {
                        if x.sheet != y.sheet {
                            continue;
                        }
                        let r0 = x.r0.max(y.r0);
                        let r1 = x.r1.min(y.r1);
                        let c0 = x.c0.max(y.c0);
                        let c1 = x.c1.min(y.c1);
                        if r0 <= r1 && c0 <= c1 {
                            out.push(Rect { sheet: x.sheet, r0, c0, r1, c1 });
                        }
                    }
                }
                if out.is_empty() {
                    Operand::Val(Value::Err(ExcelError::Null))
                } else {
                    Operand::Ref(out)
                }
            }
            _ => unreachable!(),
        }
    }

    // ---- functions (the census-determined set grows here) ----

    fn eval_call(&self, name: &str, args: &[xlc_parse::ast::CallArg]) -> Operand {
        let canon = canonical_fn_name(name);
        let v = match canon.as_str() {
            "SUM" => self.fold_numeric(args, 0.0, |acc, x| acc + x),
            "COUNT" => self.count(args),
            "AVERAGE" => self.average(args),
            "MIN" => self.min_max(args, true),
            "MAX" => self.min_max(args, false),
            "IF" => return self.fn_if(args),
            "ROUND" => self.fn_round(args),
            "ABS" => self.unary_math(args, f64::abs),
            "SQRT" => self.unary_math_checked(args, |x| {
                if x < 0.0 {
                    Err(ExcelError::Num)
                } else {
                    Ok(x.sqrt())
                }
            }),
            "TRUE" if args.is_empty() => Value::Bool(true),
            "FALSE" if args.is_empty() => Value::Bool(false),
            _ => Value::Err(ExcelError::Name), // unimplemented → excluded (Law 9)
        };
        Operand::Val(v)
    }

    /// SUM-style folds: range cells contribute numbers only (text and
    /// bools in ranges are IGNORED); scalar args coerce (SUM("2",1)=3);
    /// errors propagate.
    fn fold_numeric(
        &self,
        args: &[xlc_parse::ast::CallArg],
        init: f64,
        f: impl Fn(f64, f64) -> f64,
    ) -> Value {
        let mut acc = init;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => match v.to_number() {
                    Ok(x) => acc = f(acc, x),
                    Err(e) => return Value::Err(e),
                },
                Operand::Ref(_) => {
                    let mut err = None;
                    self.for_each_value(&op, &mut |v| match v {
                        Value::Num(x) if err.is_none() => acc = f(acc, x),
                        Value::Err(e) if err.is_none() => err = Some(e),
                        _ => {}
                    });
                    if let Some(e) = err {
                        return Value::Err(e);
                    }
                }
            }
        }
        Value::Num(acc)
    }

    fn count(&self, args: &[xlc_parse::ast::CallArg]) -> Value {
        // COUNT: numbers only. In ranges, only numeric cells count; as
        // direct args, numbers and number-coercible text count.
        let mut n = 0usize;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => {
                    if v.to_number().is_ok() && !matches!(v, Value::Err(_) | Value::Blank) {
                        n += 1;
                    }
                }
                Operand::Ref(_) => {
                    self.for_each_value(&op, &mut |v| {
                        if matches!(v, Value::Num(_)) {
                            n += 1;
                        }
                    });
                }
            }
        }
        Value::Num(n as f64)
    }

    fn average(&self, args: &[xlc_parse::ast::CallArg]) -> Value {
        let mut sum = 0.0;
        let mut n = 0usize;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => match v.to_number() {
                    Ok(x) => {
                        sum += x;
                        n += 1;
                    }
                    Err(e) => return Value::Err(e),
                },
                Operand::Ref(_) => {
                    let mut err = None;
                    self.for_each_value(&op, &mut |v| match v {
                        Value::Num(x) if err.is_none() => {
                            sum += x;
                            n += 1;
                        }
                        Value::Err(e) if err.is_none() => err = Some(e),
                        _ => {}
                    });
                    if let Some(e) = err {
                        return Value::Err(e);
                    }
                }
            }
        }
        if n == 0 {
            Value::Err(ExcelError::Div0)
        } else {
            Value::Num(sum / n as f64)
        }
    }

    fn min_max(&self, args: &[xlc_parse::ast::CallArg], want_min: bool) -> Value {
        let mut best: Option<f64> = None;
        let mut err = None;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => match v.to_number() {
                    Ok(x) => {
                        best = Some(match best {
                            None => x,
                            Some(b) => {
                                if want_min {
                                    b.min(x)
                                } else {
                                    b.max(x)
                                }
                            }
                        })
                    }
                    Err(e) => return Value::Err(e),
                },
                Operand::Ref(_) => {
                    self.for_each_value(&op, &mut |v| match v {
                        Value::Num(x) if err.is_none() => {
                            best = Some(match best {
                                None => x,
                                Some(b) => {
                                    if want_min {
                                        b.min(x)
                                    } else {
                                        b.max(x)
                                    }
                                }
                            })
                        }
                        Value::Err(e) if err.is_none() => err = Some(e),
                        _ => {}
                    });
                }
            }
        }
        if let Some(e) = err {
            return Value::Err(e);
        }
        Value::Num(best.unwrap_or(0.0))
    }

    fn fn_if(&self, args: &[xlc_parse::ast::CallArg]) -> Operand {
        if args.is_empty() || args.len() > 3 {
            return Operand::Val(Value::Err(ExcelError::Value));
        }
        let cond = match args[0].expr.as_ref() {
            Some(e) => self.deref_scalar(self.eval(e)),
            None => Value::Blank,
        };
        let b = match cond.to_bool() {
            Ok(b) => b,
            Err(e) => return Operand::Val(Value::Err(e)),
        };
        let branch = if b { args.get(1) } else { args.get(2) };
        match branch.map(|a| a.expr.as_ref()) {
            Some(Some(e)) => self.eval(e),
            // Omitted branch: IF(TRUE,,5) → 0; IF(FALSE,1) → FALSE.
            Some(None) => Operand::Val(Value::Num(0.0)),
            None => Operand::Val(if b { Value::Num(0.0) } else { Value::Bool(false) }),
        }
    }

    /// ROUND: half-AWAY-FROM-ZERO, never banker's rounding (§8.4).
    fn fn_round(&self, args: &[xlc_parse::ast::CallArg]) -> Value {
        if args.len() != 2 {
            return Value::Err(ExcelError::Value);
        }
        let x = match self.scalar_number(&args[0]) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let digits = match self.scalar_number(&args[1]) {
            Ok(d) => d.trunc() as i32,
            Err(e) => return Value::Err(e),
        };
        Value::Num(round_half_away(x, digits))
    }

    fn unary_math(&self, args: &[xlc_parse::ast::CallArg], f: impl Fn(f64) -> f64) -> Value {
        self.unary_math_checked(args, |x| Ok(f(x)))
    }

    fn unary_math_checked(
        &self,
        args: &[xlc_parse::ast::CallArg],
        f: impl Fn(f64) -> Result<f64, ExcelError>,
    ) -> Value {
        if args.len() != 1 {
            return Value::Err(ExcelError::Value);
        }
        match self.scalar_number(&args[0]).and_then(&f) {
            Ok(r) if r.is_finite() => Value::Num(r),
            Ok(_) => Value::Err(ExcelError::Num),
            Err(e) => Value::Err(e),
        }
    }

    fn scalar_number(&self, arg: &xlc_parse::ast::CallArg) -> Result<f64, ExcelError> {
        match &arg.expr {
            Some(e) => self.deref_scalar(self.eval(e)).to_number(),
            None => Ok(0.0),
        }
    }
}

/// Excel ROUND: half away from zero at the given digit position.
pub fn round_half_away(x: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    let scaled = x * factor;
    // f64::round is already half-away-from-zero.
    scaled.round() / factor
}

fn apply_scalar_binop(op: BinOp, a: &Value, b: &Value) -> Value {
    use std::cmp::Ordering;
    match op {
        BinOp::Add => value::add(a, b),
        BinOp::Sub => value::sub(a, b),
        BinOp::Mul => value::mul(a, b),
        BinOp::Div => value::div(a, b),
        BinOp::Pow => value::pow(a, b),
        BinOp::Concat => value::concat(a, b),
        _cmp => match value::compare(a, b) {
            Err(e) => Value::Err(e),
            Ok(ord) => Value::Bool(match _cmp {
                BinOp::Eq => ord == Ordering::Equal,
                BinOp::Ne => ord != Ordering::Equal,
                BinOp::Lt => ord == Ordering::Less,
                BinOp::Le => ord != Ordering::Greater,
                BinOp::Gt => ord == Ordering::Greater,
                BinOp::Ge => ord != Ordering::Less,
                _ => unreachable!(),
            }),
        },
    }
}

fn first_error(op: &Operand) -> Option<ExcelError> {
    match op {
        Operand::Val(Value::Err(e)) => Some(*e),
        _ => None,
    }
}

/// `_xlfn.`-prefixed names canonicalize for dispatch (the printer keeps
/// the original spelling; only dispatch normalizes).
fn canonical_fn_name(name: &str) -> String {
    let mut n = name.to_ascii_uppercase();
    while let Some(rest) = n.strip_prefix("_XLFN.").or_else(|| n.strip_prefix("_XLWS.")) {
        n = rest.to_string();
    }
    n
}

fn area_to_rect(area: &Area, sheet: SheetId, ctx: &dyn Ctx) -> Option<Rect> {
    Some(match area {
        Area::Cell(c) => Rect { sheet, r0: c.row, c0: c.col, r1: c.row, c1: c.col },
        Area::CellRange(a, b) => rect_from_coords(sheet, a, b),
        Area::Cols { first, last, .. } => {
            let (max_row, _) = ctx.used_extent(sheet);
            Rect {
                sheet,
                r0: 0,
                c0: *first.min(last),
                r1: max_row,
                c1: *first.max(last),
            }
        }
        Area::Rows { first, last, .. } => {
            let (_, max_col) = ctx.used_extent(sheet);
            Rect {
                sheet,
                r0: *first.min(last),
                c0: 0,
                r1: *first.max(last),
                c1: max_col,
            }
        }
        Area::RefError => return None,
    })
}

fn rect_from_coords(sheet: SheetId, a: &Coord, b: &Coord) -> Rect {
    Rect {
        sheet,
        r0: a.row.min(b.row),
        c0: a.col.min(b.col),
        r1: a.row.max(b.row),
        c1: a.col.max(b.col),
    }
}

fn lower_error(e: xlc_parse::ast::ErrorLit) -> ExcelError {
    use xlc_parse::ast::ErrorLit as L;
    match e {
        L::Div0 => ExcelError::Div0,
        L::NA => ExcelError::NA,
        L::Value => ExcelError::Value,
        L::Ref => ExcelError::Ref,
        L::Name => ExcelError::Name,
        L::Num => ExcelError::Num,
        L::Null => ExcelError::Null,
        L::Spill => ExcelError::Spill,
        L::Calc => ExcelError::Calc,
        L::GettingData => ExcelError::GettingData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockWb {
        cells: HashMap<(SheetId, u32, u32), Value>,
        extent: (u32, u32),
    }

    impl MockWb {
        fn new(cells: &[((u32, u32), f64)]) -> Self {
            let mut m = HashMap::new();
            let mut extent = (0, 0);
            for ((r, c), v) in cells {
                m.insert((0u32, *r, *c), Value::Num(*v));
                extent = (extent.0.max(*r), extent.1.max(*c));
            }
            MockWb { cells: m, extent }
        }
    }

    impl Ctx for MockWb {
        fn cell(&self, sheet: SheetId, row: u32, col: u32) -> Value {
            self.cells.get(&(sheet, row, col)).cloned().unwrap_or(Value::Blank)
        }
        fn resolve_sheet(&self, name: &str) -> Option<SheetId> {
            (name == "Sheet1").then_some(0)
        }
        fn used_extent(&self, _sheet: SheetId) -> (u32, u32) {
            self.extent
        }
        fn defined_name(&self, _name: &str) -> Option<&Expr> {
            None
        }
    }

    fn eval_at(wb: &MockWb, origin: (u32, u32), formula: &str) -> Value {
        let e = xlc_parse::parse_formula(formula).unwrap();
        Interp::new(wb, Origin { sheet: 0, row: origin.0, col: origin.1 }).eval_formula(&e.expr)
    }

    fn eval(wb: &MockWb, formula: &str) -> Value {
        eval_at(wb, (99, 99), formula)
    }

    #[test]
    fn sum_over_range_ignores_blanks() {
        // A1=2, A2=3, A4=5 (A3 blank): SUM(A1:A5)=10.
        let wb = MockWb::new(&[((0, 0), 2.0), ((1, 0), 3.0), ((3, 0), 5.0)]);
        assert_eq!(eval(&wb, "SUM(A1:A5)"), Value::Num(10.0));
        assert_eq!(eval(&wb, "SUM(A:A)"), Value::Num(10.0));
        assert_eq!(eval(&wb, "SUM(A1:A5,10)"), Value::Num(20.0));
        assert_eq!(eval(&wb, "SUM((A1:A2,A4))"), Value::Num(10.0));
    }

    #[test]
    fn sum_range_ignores_text_but_scalar_coerces() {
        let mut wb = MockWb::new(&[((0, 0), 2.0)]);
        wb.cells.insert((0, 1, 0), Value::Text("7".into()));
        // In a range, numeric-looking text is ignored.
        assert_eq!(eval(&wb, "SUM(A1:A2)"), Value::Num(2.0));
        // As a direct scalar arg, it coerces.
        assert_eq!(eval(&wb, "SUM(\"7\",A1)"), Value::Num(9.0));
    }

    #[test]
    fn average_count_min_max() {
        let wb = MockWb::new(&[((0, 0), 2.0), ((1, 0), 4.0), ((3, 0), 6.0)]);
        assert_eq!(eval(&wb, "AVERAGE(A1:A5)"), Value::Num(4.0));
        assert_eq!(eval(&wb, "COUNT(A1:A5)"), Value::Num(3.0));
        assert_eq!(eval(&wb, "MIN(A1:A5)"), Value::Num(2.0));
        assert_eq!(eval(&wb, "MAX(A1:A5)"), Value::Num(6.0));
        assert_eq!(eval(&wb, "AVERAGE(B1:B5)"), Value::Err(ExcelError::Div0));
    }

    #[test]
    fn if_with_omitted_args() {
        let wb = MockWb::new(&[((0, 0), 1.0)]);
        assert_eq!(eval(&wb, "IF(A1>0,\"yes\",\"no\")"), Value::Text("yes".into()));
        assert_eq!(eval(&wb, "IF(A1>0,,5)"), Value::Num(0.0));
        assert_eq!(eval(&wb, "IF(A1<0,5)"), Value::Bool(false));
    }

    #[test]
    fn round_half_away_from_zero() {
        let wb = MockWb::new(&[]);
        assert_eq!(eval(&wb, "ROUND(2.5,0)"), Value::Num(3.0));
        assert_eq!(eval(&wb, "ROUND(-2.5,0)"), Value::Num(-3.0));
        assert_eq!(eval(&wb, "ROUND(1.45,1)"), Value::Num(1.5));
        assert_eq!(eval(&wb, "ROUND(1234.5678,-2)"), Value::Num(1200.0));
    }

    #[test]
    fn implicit_intersection() {
        // A1..A5 = 10,20,30,40,50. In a formula at row 3 (0-based 2),
        // =A1:A5*2 intersects at A3 → 60.
        let wb = MockWb::new(&[
            ((0, 0), 10.0),
            ((1, 0), 20.0),
            ((2, 0), 30.0),
            ((3, 0), 40.0),
            ((4, 0), 50.0),
        ]);
        assert_eq!(eval_at(&wb, (2, 5), "A1:A5*2"), Value::Num(60.0));
        assert_eq!(eval_at(&wb, (2, 5), "@A1:A5"), Value::Num(30.0));
        // Outside the range's rows → #VALUE!.
        assert_eq!(eval_at(&wb, (9, 5), "A1:A5*2"), Value::Err(ExcelError::Value));
    }

    #[test]
    fn intersection_operator() {
        // A1:B2 ∩ B2:C3 = B2.
        let wb = MockWb::new(&[((1, 1), 42.0)]);
        assert_eq!(eval(&wb, "SUM(A1:B2 B2:C3)"), Value::Num(42.0));
        // Disjoint → #NULL!.
        assert_eq!(eval(&wb, "SUM(A1:A2 C1:C2)"), Value::Err(ExcelError::Null));
    }

    #[test]
    fn unknown_function_is_name_error() {
        let wb = MockWb::new(&[]);
        assert_eq!(eval(&wb, "NOTAREALFN(1)"), Value::Err(ExcelError::Name));
        assert_eq!(eval(&wb, "_xlfn.XLOOKUP(1,A1:A2,B1:B2)"), Value::Err(ExcelError::Name));
    }

    #[test]
    fn errors_flow_through() {
        let wb = MockWb::new(&[]);
        assert_eq!(eval(&wb, "SUM(1/0,5)"), Value::Err(ExcelError::Div0));
        assert_eq!(eval(&wb, "IF(1/0,1,2)"), Value::Err(ExcelError::Div0));
        assert_eq!(eval(&wb, "#N/A*2"), Value::Err(ExcelError::NA));
    }
}
