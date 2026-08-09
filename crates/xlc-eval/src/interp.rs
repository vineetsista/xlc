//! The scalar interpreter (§8.4): plain, obviously correct, cell-at-a-time.
//! Speed is Phase 5/6's job; this is the receipt's reference semantics.
//!
//! Expressions evaluate to an `Operand`: either a concrete `Value` or a
//! still-lazy reference (rectangular area / union of areas). Functions
//! decide how to consume references (SUM iterates; scalar context derefs
//! via Excel's implicit-intersection rules).

use crate::funcs_scalar::IsKind;
use crate::value::{self, ExcelError, Value};
use xlc_parse::ast::{Area, BinOp, CallArg, Coord, Expr, RefExpr, UnOp};

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
    /// Whether the workbook uses the 1904 date system.
    fn epoch_1904(&self) -> bool {
        false
    }
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
    /// A formula result is never Blank: `=A1` with A1 empty yields 0.
    pub fn eval_formula(&self, e: &Expr) -> Value {
        let op = self.eval(e);
        self.finalize(op)
    }

    /// Final formula-result rule shared with the IR interpreter.
    pub fn finalize(&self, op: Operand) -> Value {
        match self.deref_scalar(op) {
            Value::Blank => Value::Num(0.0),
            v => v,
        }
    }

    /// Operand-level binary application (the IR's entry point — same
    /// semantics as AST evaluation because it IS the same code).
    pub fn apply_binary(&self, op: BinOp, a: Operand, b: Operand) -> Operand {
        match op {
            BinOp::Range | BinOp::Union | BinOp::Intersect => self.ref_op_operands(op, a, b),
            _ => {
                let x = self.deref_scalar(a);
                let y = self.deref_scalar(b);
                Operand::Val(apply_scalar_binop(op, &x, &y))
            }
        }
    }

    /// Operand-level unary application.
    pub fn apply_unary(&self, op: UnOp, a: Operand) -> Operand {
        match op {
            UnOp::ImplicitIntersect => Operand::Val(self.deref_scalar(a)),
            UnOp::SpillRange => Operand::Val(Value::Err(ExcelError::Name)),
            _ => {
                let v = self.deref_scalar(a);
                Operand::Val(match op {
                    UnOp::Neg => value::neg(&v),
                    UnOp::Pos => value::pos(&v),
                    UnOp::Percent => value::percent(&v),
                    _ => unreachable!(),
                })
            }
        }
    }

    /// Resolve an area on explicit sheets into a reference operand.
    pub fn resolve_area(&self, sheets: &[SheetId], area: &Area) -> Operand {
        let mut rects = Vec::with_capacity(sheets.len());
        for &s in sheets {
            match area_to_rect(area, s, self.ctx) {
                Some(rect) => rects.push(rect),
                None => return Operand::Val(Value::Err(ExcelError::Ref)),
            }
        }
        Operand::Ref(rects)
    }

    pub fn eval(&self, e: &Expr) -> Operand {
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
    pub fn deref_scalar(&self, op: Operand) -> Value {
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
    pub(crate) fn for_each_value(&self, op: &Operand, f: &mut dyn FnMut(Value)) {
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
        self.ref_op_operands(op, a, b)
    }

    fn ref_op_operands(&self, op: BinOp, a: Operand, b: Operand) -> Operand {
        let (Operand::Ref(ra), Operand::Ref(rb)) = (&a, &b) else {
            // Range op over non-refs (e.g. INDEX(..):A1) needs ref-valued
            // function returns — not yet modeled; error out clearly.
            let e = first_error(&a)
                .or_else(|| first_error(&b))
                .unwrap_or(ExcelError::Value);
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
                            out.push(Rect {
                                sheet: x.sheet,
                                r0,
                                c0,
                                r1,
                                c1,
                            });
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

    // ---- array-context evaluation (§8.5 preview) ----
    //
    // Modern Excel evaluates `SUM(IF(range,a,b))` elementwise without CSE;
    // the corpus is full of cached values produced that way. Aggregates
    // therefore evaluate their arguments in ARRAY mode: ranges become
    // element vectors, scalar ops broadcast, and a bounded set of scalar
    // functions maps elementwise. Everything else falls back to scalar
    // evaluation. Full array semantics arrive with the IR.

    /// Materialize an already-evaluated operand for array context: single
    /// or multiple rects concatenate into one element vector (unions in
    /// aggregate position iterate every cell).
    fn arr_from_operand(&self, op: Operand) -> ArrOrScalar {
        match op {
            Operand::Val(v) => ArrOrScalar::Scalar(v),
            Operand::Ref(rects) => {
                let total: u64 = rects
                    .iter()
                    .map(|r| (r.r1 - r.r0 + 1) as u64 * (r.c1 - r.c0 + 1) as u64)
                    .sum();
                if total > 4_000_000 {
                    return ArrOrScalar::Scalar(Value::Err(ExcelError::Value));
                }
                match rects.as_slice() {
                    [r] => {
                        let h = r.r1 - r.r0 + 1;
                        let w = r.c1 - r.c0 + 1;
                        let vals = r
                            .cells()
                            .map(|(row, col)| self.ctx.cell(r.sheet, row, col))
                            .collect();
                        ArrOrScalar::Arr { h, w, vals }
                    }
                    _ => {
                        let mut vals = Vec::with_capacity(total as usize);
                        for r in &rects {
                            for (row, col) in r.cells() {
                                vals.push(self.ctx.cell(r.sheet, row, col));
                            }
                        }
                        let n = vals.len() as u32;
                        ArrOrScalar::Arr { h: n, w: 1, vals }
                    }
                }
            }
        }
    }

    pub(crate) fn eval_array(&self, e: &Expr) -> ArrOrScalar {
        use ArrOrScalar::*;
        match e {
            Expr::Paren { inner, .. } => self.eval_array(inner),
            Expr::Ref(_) | Expr::Name { .. } => self.arr_from_operand(self.eval(e)),
            Expr::Binary { op, lhs, rhs, .. }
                if !matches!(op, BinOp::Range | BinOp::Union | BinOp::Intersect) =>
            {
                let a = self.eval_array(lhs);
                let b = self.eval_array(rhs);
                broadcast2(&a, &b, |x, y| apply_scalar_binop(*op, x, y))
            }
            Expr::Unary { op, expr, .. } if matches!(op, UnOp::Neg | UnOp::Pos | UnOp::Percent) => {
                let a = self.eval_array(expr);
                map1(&a, |v| match op {
                    UnOp::Neg => value::neg(v),
                    UnOp::Pos => value::pos(v),
                    _ => value::percent(v),
                })
            }
            Expr::Call { name, args } => {
                let canon = canonical_fn_name(name);
                match canon.as_str() {
                    "IF" if (2..=3).contains(&args.len()) => {
                        let cond = match self.arg(args, 0) {
                            Some(c) => self.eval_array(c),
                            None => Scalar(Value::Blank),
                        };
                        if matches!(cond, Scalar(_)) {
                            // Scalar condition: normal IF (its result may
                            // itself be a range — SUM(IF(x,A1:A5,B1:B5))).
                            return self.arr_from_operand(self.eval(e));
                        }
                        let t = match self.arg(args, 1) {
                            Some(x) => self.eval_array(x),
                            None => Scalar(Value::Num(0.0)),
                        };
                        let f = match self.arg(args, 2) {
                            Some(x) => self.eval_array(x),
                            None => Scalar(Value::Bool(false)),
                        };
                        broadcast3(&cond, &t, &f, |c, tv, fv| match c.to_bool() {
                            Ok(true) => tv.clone(),
                            Ok(false) => fv.clone(),
                            Err(err) => Value::Err(err),
                        })
                    }
                    // Elementwise-safe one-argument functions.
                    "ISBLANK" | "ISTEXT" | "ISNUMBER" | "ISLOGICAL" | "ISNA" | "ISERR"
                    | "ISERROR" | "NOT" | "ABS" | "N" | "T" | "VALUE" | "TRIM" | "LEN"
                    | "UPPER" | "LOWER"
                        if args.len() == 1 =>
                    {
                        let a = match self.arg(args, 0) {
                            Some(x) => self.eval_array(x),
                            None => Scalar(Value::Blank),
                        };
                        if matches!(a, Scalar(_)) {
                            return Scalar(self.deref_scalar(self.eval(e)));
                        }
                        map1(&a, |v| self.apply_scalar_fn1(&canon, v))
                    }
                    _ => self.arr_from_operand(self.eval(e)),
                }
            }
            _ => self.arr_from_operand(self.eval(e)),
        }
    }

    /// Syntactically a plain reference chain? Those stay on the lazy
    /// no-allocation path in aggregates (whole-column SUMs must not
    /// materialize a million-element vector per formula).
    pub(crate) fn is_ref_shaped(e: &Expr) -> bool
    where
        Self: Sized,
    {
        match e {
            Expr::Ref(_) | Expr::Name { .. } => true,
            Expr::Paren { inner, .. } => Self::is_ref_shaped(inner),
            Expr::Binary { op, lhs, rhs, .. } => {
                matches!(op, BinOp::Range | BinOp::Union | BinOp::Intersect)
                    && Self::is_ref_shaped(lhs)
                    && Self::is_ref_shaped(rhs)
            }
            _ => false,
        }
    }

    /// Apply a 1-argument scalar builtin to an already-computed value.
    fn apply_scalar_fn1(&self, canon: &str, v: &Value) -> Value {
        match canon {
            "ISBLANK" => Value::Bool(matches!(v, Value::Blank)),
            "ISTEXT" => Value::Bool(matches!(v, Value::Text(_))),
            "ISNUMBER" => Value::Bool(matches!(v, Value::Num(_))),
            "ISLOGICAL" => Value::Bool(matches!(v, Value::Bool(_))),
            "ISNA" => Value::Bool(matches!(v, Value::Err(ExcelError::NA))),
            "ISERR" => Value::Bool(matches!(v, Value::Err(e) if *e != ExcelError::NA)),
            "ISERROR" => Value::Bool(v.is_err()),
            "NOT" => match v.to_bool() {
                Ok(b) => Value::Bool(!b),
                Err(e) => Value::Err(e),
            },
            "ABS" => match v.to_number() {
                Ok(x) => Value::Num(x.abs()),
                Err(e) => Value::Err(e),
            },
            "N" => match v {
                Value::Num(x) => Value::Num(*x),
                Value::Bool(b) => Value::Num(if *b { 1.0 } else { 0.0 }),
                Value::Err(e) => Value::Err(*e),
                _ => Value::Num(0.0),
            },
            "T" => match v {
                Value::Text(s) => Value::Text(s.clone()),
                Value::Err(e) => Value::Err(*e),
                _ => Value::Text(String::new()),
            },
            "VALUE" => match v {
                Value::Num(x) => Value::Num(*x),
                Value::Text(s) => match crate::value::parse_excel_number(s) {
                    Some(x) => Value::Num(x),
                    None => Value::Err(ExcelError::Value),
                },
                Value::Blank => Value::Num(0.0),
                Value::Err(e) => Value::Err(*e),
                Value::Bool(_) => Value::Err(ExcelError::Value),
            },
            "TRIM" | "UPPER" | "LOWER" | "LEN" => match v.to_text() {
                Err(e) => Value::Err(e),
                Ok(t) => match canon {
                    "LEN" => Value::Num(t.chars().count() as f64),
                    "UPPER" => Value::Text(t.to_uppercase()),
                    "LOWER" => Value::Text(t.to_lowercase()),
                    _ => {
                        let mut out = String::with_capacity(t.len());
                        let mut pending = false;
                        for c in t.chars() {
                            if c == ' ' {
                                pending = !out.is_empty();
                            } else {
                                if pending {
                                    out.push(' ');
                                    pending = false;
                                }
                                out.push(c);
                            }
                        }
                        Value::Text(out)
                    }
                },
            },
            _ => Value::Err(ExcelError::Name),
        }
    }

    // ---- shared argument helpers ----

    pub(crate) fn arg<'b>(&self, args: &'b [CallArg], i: usize) -> Option<&'b Expr> {
        args.get(i).and_then(|a| a.expr.as_ref())
    }

    /// Scalar value of argument i; Blank when omitted or absent.
    pub(crate) fn arg_scalar(&self, args: &[CallArg], i: usize) -> Value {
        match self.arg(args, i) {
            Some(e) => self.deref_scalar(self.eval(e)),
            None => Value::Blank,
        }
    }

    pub(crate) fn arg_num(&self, args: &[CallArg], i: usize) -> Result<f64, ExcelError> {
        self.arg_scalar(args, i).to_number()
    }

    pub(crate) fn arg_num_or(
        &self,
        args: &[CallArg],
        i: usize,
        default: f64,
    ) -> Result<f64, ExcelError> {
        match self.arg(args, i) {
            Some(e) => self.deref_scalar(self.eval(e)).to_number(),
            None => Ok(default),
        }
    }

    pub(crate) fn arg_text(&self, args: &[CallArg], i: usize) -> Result<String, ExcelError> {
        self.arg_scalar(args, i).to_text()
    }

    pub(crate) fn arg_bool_or(
        &self,
        args: &[CallArg],
        i: usize,
        default: bool,
    ) -> Result<bool, ExcelError> {
        match self.arg(args, i) {
            Some(e) => self.deref_scalar(self.eval(e)).to_bool(),
            None => Ok(default),
        }
    }

    /// Argument i as a single rectangular area (lookup tables, criteria
    /// ranges). Errors with #VALUE! if it is not exactly one rect.
    pub(crate) fn arg_rect(&self, args: &[CallArg], i: usize) -> Result<Rect, ExcelError> {
        let Some(e) = self.arg(args, i) else {
            return Err(ExcelError::Value);
        };
        match self.eval(e) {
            Operand::Ref(rects) if rects.len() == 1 => Ok(rects[0]),
            Operand::Val(Value::Err(err)) => Err(err),
            _ => Err(ExcelError::Value),
        }
    }

    pub(crate) fn rect_get(&self, r: &Rect, dr: u32, dc: u32) -> Value {
        self.ctx.cell(r.sheet, r.r0 + dr, r.c0 + dc)
    }

    // ---- functions (the census-determined set grows here) ----

    fn eval_call(&self, name: &str, args: &[CallArg]) -> Operand {
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
            // Logic / info
            "IFERROR" => return self.fn_iferror(args, false),
            "IFNA" => return self.fn_iferror(args, true),
            "AND" => self.fn_and_or(args, true),
            "OR" => self.fn_and_or(args, false),
            "XOR" => self.fn_xor(args),
            "NOT" => self.fn_not(args),
            "ISBLANK" => self.fn_is(args, IsKind::Blank),
            "ISTEXT" => self.fn_is(args, IsKind::Text),
            "ISNONTEXT" => self.fn_is(args, IsKind::NonText),
            "ISNUMBER" => self.fn_is(args, IsKind::Number),
            "ISLOGICAL" => self.fn_is(args, IsKind::Logical),
            "ISNA" => self.fn_is(args, IsKind::Na),
            "ISERR" => self.fn_is(args, IsKind::ErrNotNa),
            "ISERROR" => self.fn_is(args, IsKind::AnyErr),
            "ISEVEN" => self.fn_parity(args, true),
            "ISODD" => self.fn_parity(args, false),
            "N" => self.fn_n(args),
            "T" => self.fn_t(args),
            // Lookup / reference
            "VLOOKUP" => self.fn_vlookup(args, false),
            "HLOOKUP" => self.fn_vlookup(args, true),
            "INDEX" => return self.fn_index(args),
            "MATCH" => self.fn_match(args),
            "LOOKUP" => self.fn_lookup(args),
            "CHOOSE" => return self.fn_choose(args),
            "ROW" => self.fn_row_col(args, true),
            "COLUMN" => self.fn_row_col(args, false),
            "ROWS" => self.fn_dims(args, true),
            "COLUMNS" => self.fn_dims(args, false),
            "HYPERLINK" => self.fn_hyperlink(args),
            // Criteria family + aggregation
            "COUNTIF" => self.fn_countif(args),
            "COUNTIFS" => self.fn_countifs(args),
            "SUMIF" => self.fn_sumif(args, false),
            "AVERAGEIF" => self.fn_sumif(args, true),
            "SUMIFS" => self.fn_sumifs(args, false),
            "AVERAGEIFS" => self.fn_sumifs(args, true),
            "COUNTA" => self.fn_counta(args),
            "COUNTBLANK" => self.fn_countblank(args),
            "SUMPRODUCT" => self.fn_sumproduct(args),
            "PRODUCT" => self.fold_numeric(args, 1.0, |a, x| a * x),
            "LARGE" => self.fn_large_small(args, true),
            "SMALL" => self.fn_large_small(args, false),
            "MEDIAN" => self.fn_median(args),
            "STDEV" | "STDEV.S" => self.fn_stdev(args, true, true),
            "STDEVP" | "STDEV.P" => self.fn_stdev(args, false, true),
            "VAR" | "VAR.S" => self.fn_stdev(args, true, false),
            "VARP" | "VAR.P" => self.fn_stdev(args, false, false),
            "RANK" | "RANK.EQ" => self.fn_rank(args),
            "SUBTOTAL" => self.fn_subtotal(args),
            // Text
            "LEFT" => self.fn_left_right(args, false),
            "RIGHT" => self.fn_left_right(args, true),
            "MID" => self.fn_mid(args),
            "LEN" => self.fn_len(args),
            "LOWER" => self.fn_case(args, false),
            "UPPER" => self.fn_case(args, true),
            "PROPER" => self.fn_proper(args),
            "TRIM" => self.fn_trim(args),
            "SUBSTITUTE" => self.fn_substitute(args),
            "REPLACE" => self.fn_replace(args),
            "CONCATENATE" | "CONCAT" => self.fn_concatenate(args),
            "TEXTJOIN" => self.fn_textjoin(args),
            "TEXTAFTER" => self.fn_text_after_before(args, true),
            "TEXTBEFORE" => self.fn_text_after_before(args, false),
            "VALUE" => self.fn_value(args),
            "EXACT" => self.fn_exact(args),
            "FIND" => self.fn_find_search(args, true),
            "SEARCH" => self.fn_find_search(args, false),
            "REPT" => self.fn_rept(args),
            "CHAR" => self.fn_char(args),
            "CODE" => self.fn_code(args),
            // Date / time
            "DATE" => self.fn_date(args),
            "TIME" => self.fn_time(args),
            "YEAR" => self.fn_ymd(args, 0),
            "MONTH" => self.fn_ymd(args, 1),
            "DAY" => self.fn_ymd(args, 2),
            "HOUR" => self.fn_hms(args, 0),
            "MINUTE" => self.fn_hms(args, 1),
            "SECOND" => self.fn_hms(args, 2),
            "DAYS" => self.fn_days(args),
            "WEEKDAY" => self.fn_weekday(args),
            // Math
            "INT" => self.unary_math(args, f64::floor),
            "PI" if args.is_empty() => Value::Num(std::f64::consts::PI),
            "EXP" => self.unary_math(args, f64::exp),
            "LN" => self.unary_math_checked(args, |x| {
                if x <= 0.0 {
                    Err(ExcelError::Num)
                } else {
                    Ok(x.ln())
                }
            }),
            "LOG10" => self.unary_math_checked(args, |x| {
                if x <= 0.0 {
                    Err(ExcelError::Num)
                } else {
                    Ok(x.log10())
                }
            }),
            "LOG" => self.fn_log(args),
            "POWER" => self.fn_power(args),
            "MOD" => self.fn_mod(args),
            "SIGN" => self.unary_math(args, f64::signum),
            "TRUNC" => self.fn_trunc(args),
            "ROUNDUP" => self.fn_round_dir(args, true),
            "ROUNDDOWN" => self.fn_round_dir(args, false),
            "FLOOR" | "FLOOR.MATH" => self.fn_floor_ceiling(args, true),
            "CEILING" | "CEILING.MATH" => self.fn_floor_ceiling(args, false),
            "EVEN" => self.fn_even_odd(args, true),
            "ODD" => self.fn_even_odd(args, false),
            "RADIANS" => self.unary_math(args, f64::to_radians),
            "DEGREES" => self.unary_math(args, f64::to_degrees),
            "SIN" => self.unary_math(args, f64::sin),
            "COS" => self.unary_math(args, f64::cos),
            "TAN" => self.unary_math(args, f64::tan),
            "ASIN" => self.unary_math_checked(args, |x| {
                if !(-1.0..=1.0).contains(&x) {
                    Err(ExcelError::Num)
                } else {
                    Ok(x.asin())
                }
            }),
            "ACOS" => self.unary_math_checked(args, |x| {
                if !(-1.0..=1.0).contains(&x) {
                    Err(ExcelError::Num)
                } else {
                    Ok(x.acos())
                }
            }),
            "ATAN" => self.unary_math(args, f64::atan),
            "ATAN2" => self.fn_atan2(args),
            // Stats / engineering
            "NORMDIST" | "NORM.DIST" => self.fn_normdist(args),
            "NORMSDIST" => self.fn_normsdist(args),
            "NORM.S.DIST" => self.fn_norm_s_dist(args),
            "ERF" => self.fn_erf(args),
            "ERFC" | "ERFC.PRECISE" => self.fn_erfc(args),
            _ => Value::Err(ExcelError::Name), // unimplemented → excluded (Law 9)
        };
        Operand::Val(v)
    }

    /// SUM-style folds: range cells contribute numbers only (text and
    /// bools in ranges are IGNORED); scalar args coerce (SUM("2",1)=3);
    /// errors propagate.
    pub(crate) fn fold_numeric(
        &self,
        args: &[CallArg],
        init: f64,
        f: impl Fn(f64, f64) -> f64,
    ) -> Value {
        let mut acc = init;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            if Self::is_ref_shaped(arg) {
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
            } else {
                // Array context: IF over ranges, range arithmetic, and
                // ref-returning calls; elements follow RANGE rules
                // (text ignored, errors propagate), scalars coerce.
                match self.eval_array(arg) {
                    ArrOrScalar::Scalar(v) => match v.to_number() {
                        Ok(x) => acc = f(acc, x),
                        Err(e) => return Value::Err(e),
                    },
                    ArrOrScalar::Arr { vals, .. } => {
                        for v in vals {
                            match v {
                                Value::Num(x) => acc = f(acc, x),
                                Value::Err(e) => return Value::Err(e),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        Value::Num(acc)
    }

    pub(crate) fn count(&self, args: &[CallArg]) -> Value {
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

    pub(crate) fn average(&self, args: &[CallArg]) -> Value {
        let mut sum = 0.0;
        let mut n = 0usize;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            if Self::is_ref_shaped(arg) {
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
            } else {
                match self.eval_array(arg) {
                    ArrOrScalar::Scalar(v) => match v.to_number() {
                        Ok(x) => {
                            sum += x;
                            n += 1;
                        }
                        Err(e) => return Value::Err(e),
                    },
                    ArrOrScalar::Arr { vals, .. } => {
                        for v in vals {
                            match v {
                                Value::Num(x) => {
                                    sum += x;
                                    n += 1;
                                }
                                Value::Err(e) => return Value::Err(e),
                                _ => {}
                            }
                        }
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

    pub(crate) fn min_max(&self, args: &[CallArg], want_min: bool) -> Value {
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

    fn fn_if(&self, args: &[CallArg]) -> Operand {
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
            None => Operand::Val(if b {
                Value::Num(0.0)
            } else {
                Value::Bool(false)
            }),
        }
    }

    /// ROUND: half-AWAY-FROM-ZERO, never banker's rounding (§8.4).
    fn fn_round(&self, args: &[CallArg]) -> Value {
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

    pub(crate) fn unary_math(&self, args: &[CallArg], f: impl Fn(f64) -> f64) -> Value {
        self.unary_math_checked(args, |x| Ok(f(x)))
    }

    pub(crate) fn unary_math_checked(
        &self,
        args: &[CallArg],
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

    pub(crate) fn scalar_number(&self, arg: &CallArg) -> Result<f64, ExcelError> {
        match &arg.expr {
            Some(e) => self.deref_scalar(self.eval(e)).to_number(),
            None => Ok(0.0),
        }
    }
}

/// Excel ROUND: half away from zero at the given digit position — decided
/// on the value's 15-significant-digit DECIMAL rendering, not its binary
/// value (§8.4). The f64 nearest to 1.275 sits just below it, yet Excel's
/// ROUND(x,2) yields 1.28 because the displayed decimal is "1.275".
pub fn round_half_away(x: f64, digits: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let neg = x < 0.0;
    // 15-significant-digit scientific rendering: "d.dddddddddddddde±EE".
    let s = format!("{:.14e}", x.abs());
    let (mant, exp) = s.split_once('e').expect("scientific format");
    let exp: i32 = exp.parse().expect("exponent");
    let digits15: Vec<u8> = mant
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    debug_assert_eq!(digits15.len(), 15);
    // Value = 0.digits15 * 10^(exp+1). Keep k leading digits where
    // k = exp + 1 + digits: rounding at 10^-digits.
    let k = exp + 1 + digits;
    if k < 0 {
        return if neg { -0.0 } else { 0.0 };
    }
    if k == 0 {
        // Rounding position sits at the first significant digit:
        // ROUND(0.005, 2) = 0.01 — the leading digit alone decides.
        return if digits15[0] >= 5 {
            let v = 10f64.powi(-digits);
            if neg {
                -v
            } else {
                v
            }
        } else if neg {
            -0.0
        } else {
            0.0
        };
    }
    let k = k as usize;
    if k >= 15 {
        return x; // rounding position beyond stored precision: unchanged
    }
    let mut kept: Vec<u8> = digits15[..k].to_vec();
    if digits15[k] >= 5 {
        // Propagate the carry.
        let mut i = k;
        loop {
            if i == 0 {
                kept.insert(0, 1);
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let carried = kept.len() > k;
    let mant_str: String = kept.iter().map(|d| (d + b'0') as char).collect();
    let new_exp = exp + if carried { 1 } else { 0 };
    let rebuilt = format!("0.{mant_str}e{}", new_exp + 1);
    let v: f64 = rebuilt.parse().expect("rebuilt decimal");
    if neg {
        -v
    } else {
        v
    }
}

/// Result of array-context evaluation.
pub(crate) enum ArrOrScalar {
    Scalar(Value),
    Arr { h: u32, w: u32, vals: Vec<Value> },
}

impl ArrOrScalar {
    fn get(&self, i: usize) -> &Value {
        match self {
            ArrOrScalar::Scalar(v) => v,
            ArrOrScalar::Arr { vals, .. } => &vals[i],
        }
    }

    fn shape(&self) -> Option<(u32, u32)> {
        match self {
            ArrOrScalar::Scalar(_) => None,
            ArrOrScalar::Arr { h, w, .. } => Some((*h, *w)),
        }
    }
}

fn common_shape(shapes: &[Option<(u32, u32)>]) -> Result<Option<(u32, u32)>, ExcelError> {
    let mut out = None;
    for s in shapes.iter().flatten() {
        match out {
            None => out = Some(*s),
            Some(prev) if prev == *s => {}
            // Mismatched shapes: Excel pads with #N/A; we reject for now
            // (the receipt prices this simplification).
            Some(_) => return Err(ExcelError::Value),
        }
    }
    Ok(out)
}

fn broadcast2(
    a: &ArrOrScalar,
    b: &ArrOrScalar,
    f: impl Fn(&Value, &Value) -> Value,
) -> ArrOrScalar {
    match common_shape(&[a.shape(), b.shape()]) {
        Err(e) => ArrOrScalar::Scalar(Value::Err(e)),
        Ok(None) => ArrOrScalar::Scalar(f(a.get(0), b.get(0))),
        Ok(Some((h, w))) => {
            let n = (h * w) as usize;
            let vals = (0..n)
                .map(|i| {
                    f(
                        if a.shape().is_some() {
                            a.get(i)
                        } else {
                            a.get(0)
                        },
                        if b.shape().is_some() {
                            b.get(i)
                        } else {
                            b.get(0)
                        },
                    )
                })
                .collect();
            ArrOrScalar::Arr { h, w, vals }
        }
    }
}

fn broadcast3(
    a: &ArrOrScalar,
    b: &ArrOrScalar,
    c: &ArrOrScalar,
    f: impl Fn(&Value, &Value, &Value) -> Value,
) -> ArrOrScalar {
    match common_shape(&[a.shape(), b.shape(), c.shape()]) {
        Err(e) => ArrOrScalar::Scalar(Value::Err(e)),
        Ok(None) => ArrOrScalar::Scalar(f(a.get(0), b.get(0), c.get(0))),
        Ok(Some((h, w))) => {
            let n = (h * w) as usize;
            let pick = |x: &ArrOrScalar, i: usize| {
                if x.shape().is_some() {
                    x.get(i).clone()
                } else {
                    x.get(0).clone()
                }
            };
            let vals = (0..n)
                .map(|i| f(&pick(a, i), &pick(b, i), &pick(c, i)))
                .collect();
            ArrOrScalar::Arr { h, w, vals }
        }
    }
}

fn map1(a: &ArrOrScalar, f: impl Fn(&Value) -> Value) -> ArrOrScalar {
    match a {
        ArrOrScalar::Scalar(v) => ArrOrScalar::Scalar(f(v)),
        ArrOrScalar::Arr { h, w, vals } => ArrOrScalar::Arr {
            h: *h,
            w: *w,
            vals: vals.iter().map(f).collect(),
        },
    }
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
    while let Some(rest) = n
        .strip_prefix("_XLFN.")
        .or_else(|| n.strip_prefix("_XLWS."))
    {
        n = rest.to_string();
    }
    n
}

pub fn area_to_rect(area: &Area, sheet: SheetId, ctx: &dyn Ctx) -> Option<Rect> {
    Some(match area {
        Area::Cell(c) => Rect {
            sheet,
            r0: c.row,
            c0: c.col,
            r1: c.row,
            c1: c.col,
        },
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
            self.cells
                .get(&(sheet, row, col))
                .cloned()
                .unwrap_or(Value::Blank)
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
        Interp::new(
            wb,
            Origin {
                sheet: 0,
                row: origin.0,
                col: origin.1,
            },
        )
        .eval_formula(&e.expr)
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
        assert_eq!(
            eval(&wb, "IF(A1>0,\"yes\",\"no\")"),
            Value::Text("yes".into())
        );
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
        // Decimal-faithful boundary: f64 nearest 1.275 is below it, but
        // Excel rounds the displayed decimal.
        assert_eq!(eval(&wb, "ROUND(1.275,2)"), Value::Num(1.28));
        assert_eq!(eval(&wb, "ROUND(2.675,2)"), Value::Num(2.68));
        assert_eq!(eval(&wb, "ROUND(-1.275,2)"), Value::Num(-1.28));
        assert_eq!(eval(&wb, "ROUND(0.001,4)"), Value::Num(0.001));
        assert_eq!(eval(&wb, "ROUND(9.999,2)"), Value::Num(10.0));
        assert_eq!(eval(&wb, "ROUND(0.005,2)"), Value::Num(0.01));
        assert_eq!(eval(&wb, "ROUND(0.004,2)"), Value::Num(0.0));
        assert_eq!(eval(&wb, "ROUND(-0.005,2)"), Value::Num(-0.01));
        assert_eq!(eval(&wb, "ROUND(0.0004,2)"), Value::Num(0.0));
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
        assert_eq!(
            eval_at(&wb, (9, 5), "A1:A5*2"),
            Value::Err(ExcelError::Value)
        );
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
        assert_eq!(
            eval(&wb, "_xlfn.XLOOKUP(1,A1:A2,B1:B2)"),
            Value::Err(ExcelError::Name)
        );
    }

    #[test]
    fn array_context_sum_if() {
        // B1=1, B2 blank, B3=1; C1=0.1, C2=99, C3=0.34. Origin far away.
        let mut wb = MockWb::new(&[((0, 1), 1.0), ((2, 1), 1.0)]);
        wb.cells.insert((0, 0, 2), Value::Num(0.1));
        wb.cells.insert((0, 1, 2), Value::Num(99.0));
        wb.cells.insert((0, 2, 2), Value::Num(0.34));
        wb.extent = (2, 2);
        assert_eq!(
            eval(&wb, "SUM(IF(NOT(ISBLANK(B1:B3)),C1:C3,\"\"))"),
            Value::Num(0.1 + 0.34)
        );
        assert_eq!(
            eval(&wb, "SUM(IF(ISBLANK(B1:B3),C1:C3,0))"),
            Value::Num(99.0)
        );
        // Range arithmetic in array context.
        assert_eq!(eval(&wb, "SUM(C1:C3*2)"), Value::Num(198.88));
    }

    #[test]
    fn errors_flow_through() {
        let wb = MockWb::new(&[]);
        assert_eq!(eval(&wb, "SUM(1/0,5)"), Value::Err(ExcelError::Div0));
        assert_eq!(eval(&wb, "IF(1/0,1,2)"), Value::Err(ExcelError::Div0));
        assert_eq!(eval(&wb, "#N/A*2"), Value::Err(ExcelError::NA));
    }
}
