//! Range-consuming builtins: the lookup family and the criteria family
//! (COUNTIF/SUMIF/...), plus order statistics and dispersion.

use crate::criteria::{has_wildcard, parse_criteria, wildcard_match};
use crate::interp::{Ctx, Interp, Operand, Rect};
use crate::value::{compare, ExcelError, Value};
use std::cmp::Ordering;
use xlc_parse::ast::CallArg;

impl<C: Ctx> Interp<'_, C> {
    // ---- lookup ----

    /// VLOOKUP / HLOOKUP. `horizontal` swaps the axes.
    pub(crate) fn fn_vlookup(&self, args: &[CallArg], horizontal: bool) -> Value {
        let key = self.arg_scalar(args, 0);
        if let Value::Err(e) = key {
            return Value::Err(e);
        }
        let table = match self.arg_rect(args, 1) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let idx = match self.arg_num(args, 2) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let approx = match self.arg_bool_or(args, 3, true) {
            Ok(b) => b,
            Err(e) => return Value::Err(e),
        };
        if idx < 1.0 {
            return Value::Err(ExcelError::Value);
        }
        let idx = idx.trunc() as u32 - 1;
        let (lanes, width) = if horizontal {
            (table.c1 - table.c0 + 1, table.r1 - table.r0 + 1)
        } else {
            (table.r1 - table.r0 + 1, table.c1 - table.c0 + 1)
        };
        if idx >= width {
            return Value::Err(ExcelError::Ref);
        }
        let get = |lane: u32, off: u32| -> Value {
            if horizontal {
                self.rect_get(&table, off, lane)
            } else {
                self.rect_get(&table, lane, off)
            }
        };
        let hit = if approx {
            // Largest key-column value <= key (ascending assumption):
            // remember the last lane satisfying <=, skipping type-mismatches.
            let mut best: Option<u32> = None;
            for lane in 0..lanes {
                let v = get(lane, 0);
                if same_family(&key, &v) {
                    match compare(&v, &key) {
                        Ok(Ordering::Less) | Ok(Ordering::Equal) => best = Some(lane),
                        _ => {}
                    }
                }
            }
            best
        } else {
            (0..lanes).find(|&lane| exact_key_match(&key, &get(lane, 0)))
        };
        match hit {
            Some(lane) => get(lane, idx),
            None => Value::Err(ExcelError::NA),
        }
    }

    pub(crate) fn fn_match(&self, args: &[CallArg]) -> Value {
        let key = self.arg_scalar(args, 0);
        if let Value::Err(e) = key {
            return Value::Err(e);
        }
        let rect = match self.arg_rect(args, 1) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let mode = match self.arg_num_or(args, 2, 1.0) {
            Ok(m) => m,
            Err(e) => return Value::Err(e),
        };
        let vertical = rect.c0 == rect.c1;
        let lanes = if vertical { rect.r1 - rect.r0 + 1 } else { rect.c1 - rect.c0 + 1 };
        let get = |lane: u32| -> Value {
            if vertical {
                self.rect_get(&rect, lane, 0)
            } else {
                self.rect_get(&rect, 0, lane)
            }
        };
        let hit: Option<u32> = if mode > 0.0 {
            let mut best = None;
            for lane in 0..lanes {
                let v = get(lane);
                if same_family(&key, &v)
                    && matches!(compare(&v, &key), Ok(Ordering::Less) | Ok(Ordering::Equal))
                {
                    best = Some(lane);
                }
            }
            best
        } else if mode < 0.0 {
            // Descending: smallest value >= key = last lane with >=.
            let mut best = None;
            for lane in 0..lanes {
                let v = get(lane);
                if same_family(&key, &v)
                    && matches!(compare(&v, &key), Ok(Ordering::Greater) | Ok(Ordering::Equal))
                {
                    best = Some(lane);
                } else if best.is_some() {
                    break;
                }
            }
            best
        } else {
            (0..lanes).find(|&lane| exact_key_match(&key, &get(lane)))
        };
        match hit {
            Some(lane) => Value::Num((lane + 1) as f64),
            None => Value::Err(ExcelError::NA),
        }
    }

    /// INDEX area form; returns a REFERENCE so `INDEX(..):B9` and scalar
    /// deref both work. row/col of 0 select the whole column/row.
    pub(crate) fn fn_index(&self, args: &[CallArg]) -> Operand {
        let rect = match self.arg_rect(args, 0) {
            Ok(r) => r,
            Err(e) => return Operand::Val(Value::Err(e)),
        };
        let height = rect.r1 - rect.r0 + 1;
        let width = rect.c1 - rect.c0 + 1;
        let row_arg = match self.arg_num_or(args, 1, 0.0) {
            Ok(x) => x.trunc() as i64,
            Err(e) => return Operand::Val(Value::Err(e)),
        };
        // Single-row vector with one index: the index means column.
        let (row_i, col_i) = if args.len() < 3 && height == 1 && width > 1 {
            (1, row_arg)
        } else {
            let c = match self.arg_num_or(args, 2, 0.0) {
                Ok(x) => x.trunc() as i64,
                Err(e) => return Operand::Val(Value::Err(e)),
            };
            (row_arg, c)
        };
        if row_i < 0 || col_i < 0 {
            return Operand::Val(Value::Err(ExcelError::Value));
        }
        if row_i as u32 > height || col_i as u32 > width {
            return Operand::Val(Value::Err(ExcelError::Ref));
        }
        let (r0, r1) = if row_i == 0 {
            (rect.r0, rect.r1)
        } else {
            (rect.r0 + row_i as u32 - 1, rect.r0 + row_i as u32 - 1)
        };
        let (c0, c1) = if col_i == 0 {
            (rect.c0, rect.c1)
        } else {
            (rect.c0 + col_i as u32 - 1, rect.c0 + col_i as u32 - 1)
        };
        Operand::Ref(vec![Rect { sheet: rect.sheet, r0, c0, r1, c1 }])
    }

    /// LOOKUP vector form (and array form reduced to its vectors).
    pub(crate) fn fn_lookup(&self, args: &[CallArg]) -> Value {
        let key = self.arg_scalar(args, 0);
        if let Value::Err(e) = key {
            return Value::Err(e);
        }
        let lookup = match self.arg_rect(args, 1) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let vertical = (lookup.r1 - lookup.r0) >= (lookup.c1 - lookup.c0);
        let lanes = if vertical { lookup.r1 - lookup.r0 + 1 } else { lookup.c1 - lookup.c0 + 1 };
        let get_l = |lane: u32| -> Value {
            if vertical {
                self.rect_get(&lookup, lane, 0)
            } else {
                self.rect_get(&lookup, 0, lane)
            }
        };
        let mut best: Option<u32> = None;
        for lane in 0..lanes {
            let v = get_l(lane);
            if same_family(&key, &v)
                && matches!(compare(&v, &key), Ok(Ordering::Less) | Ok(Ordering::Equal))
            {
                best = Some(lane);
            }
        }
        let Some(lane) = best else { return Value::Err(ExcelError::NA) };
        match self.arg(args, 2) {
            None => {
                // Array form: result from the last column/row of the range.
                if vertical {
                    self.rect_get(&lookup, lane, lookup.c1 - lookup.c0)
                } else {
                    self.rect_get(&lookup, lookup.r1 - lookup.r0, lane)
                }
            }
            Some(_) => match self.arg_rect(args, 2) {
                Ok(result) => {
                    let vertical_r = (result.r1 - result.r0) >= (result.c1 - result.c0);
                    if vertical_r {
                        self.rect_get(&result, lane, 0)
                    } else {
                        self.rect_get(&result, 0, lane)
                    }
                }
                Err(e) => Value::Err(e),
            },
        }
    }

    pub(crate) fn fn_choose(&self, args: &[CallArg]) -> Operand {
        let idx = match self.arg_num(args, 0) {
            Ok(x) => x.trunc() as i64,
            Err(e) => return Operand::Val(Value::Err(e)),
        };
        if idx < 1 || idx as usize >= args.len() {
            return Operand::Val(Value::Err(ExcelError::Value));
        }
        match self.arg(args, idx as usize) {
            Some(e) => self.eval(e),
            None => Operand::Val(Value::Num(0.0)),
        }
    }

    pub(crate) fn fn_row_col(&self, args: &[CallArg], want_row: bool) -> Value {
        match self.arg(args, 0) {
            None => Value::Num(if want_row {
                (self.origin.row + 1) as f64
            } else {
                (self.origin.col + 1) as f64
            }),
            Some(e) => match self.eval(e) {
                Operand::Ref(rects) if !rects.is_empty() => Value::Num(if want_row {
                    (rects[0].r0 + 1) as f64
                } else {
                    (rects[0].c0 + 1) as f64
                }),
                Operand::Val(Value::Err(err)) => Value::Err(err),
                _ => Value::Err(ExcelError::Value),
            },
        }
    }

    pub(crate) fn fn_dims(&self, args: &[CallArg], want_rows: bool) -> Value {
        match self.arg_rect(args, 0) {
            Ok(r) => Value::Num(if want_rows {
                (r.r1 - r.r0 + 1) as f64
            } else {
                (r.c1 - r.c0 + 1) as f64
            }),
            Err(e) => Value::Err(e),
        }
    }

    // ---- criteria family ----

    pub(crate) fn fn_countif(&self, args: &[CallArg]) -> Value {
        let rect = match self.arg_rect(args, 0) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let crit = parse_criteria(&self.arg_scalar(args, 1));
        let mut n = 0usize;
        for (r, c) in rect.cells() {
            if crit.matches(&self.ctx.cell(rect.sheet, r, c)) {
                n += 1;
            }
        }
        Value::Num(n as f64)
    }

    /// Shared plumbing for COUNTIFS / SUMIFS / AVERAGEIFS: pairs of
    /// (range, criteria) all congruent with the base rect.
    fn ifs_mask(
        &self,
        args: &[CallArg],
        first_pair: usize,
        base: &Rect,
    ) -> Result<Vec<bool>, ExcelError> {
        let cells = ((base.r1 - base.r0 + 1) * (base.c1 - base.c0 + 1)) as usize;
        let mut mask = vec![true; cells];
        let mut i = first_pair;
        while i < args.len() {
            let rect = self.arg_rect(args, i)?;
            if (rect.r1 - rect.r0) != (base.r1 - base.r0) || (rect.c1 - rect.c0) != (base.c1 - base.c0)
            {
                return Err(ExcelError::Value);
            }
            let crit = parse_criteria(&self.arg_scalar(args, i + 1));
            for (k, (r, c)) in rect.cells().enumerate() {
                if mask[k] && !crit.matches(&self.ctx.cell(rect.sheet, r, c)) {
                    mask[k] = false;
                }
            }
            i += 2;
        }
        Ok(mask)
    }

    pub(crate) fn fn_countifs(&self, args: &[CallArg]) -> Value {
        if args.len() < 2 || args.len() % 2 != 0 {
            return Value::Err(ExcelError::Value);
        }
        let base = match self.arg_rect(args, 0) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        match self.ifs_mask(args, 0, &base) {
            Ok(mask) => Value::Num(mask.iter().filter(|&&b| b).count() as f64),
            Err(e) => Value::Err(e),
        }
    }

    /// SUMIF / AVERAGEIF: criteria over one range, values from sum_range
    /// (or the criteria range itself).
    pub(crate) fn fn_sumif(&self, args: &[CallArg], average: bool) -> Value {
        let rect = match self.arg_rect(args, 0) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let crit = parse_criteria(&self.arg_scalar(args, 1));
        let sum_rect = match self.arg(args, 2) {
            None => rect,
            Some(_) => match self.arg_rect(args, 2) {
                // Excel resizes the sum range to the criteria range shape.
                Ok(s) => Rect {
                    sheet: s.sheet,
                    r0: s.r0,
                    c0: s.c0,
                    r1: s.r0 + (rect.r1 - rect.r0),
                    c1: s.c0 + (rect.c1 - rect.c0),
                },
                Err(e) => return Value::Err(e),
            },
        };
        let mut sum = 0.0;
        let mut n = 0usize;
        for ((r, c), (sr, sc)) in rect.cells().zip(sum_rect.cells()) {
            if crit.matches(&self.ctx.cell(rect.sheet, r, c)) {
                if let Value::Num(x) = self.ctx.cell(sum_rect.sheet, sr, sc) {
                    sum += x;
                    n += 1;
                }
            }
        }
        if average {
            if n == 0 {
                Value::Err(ExcelError::Div0)
            } else {
                Value::Num(sum / n as f64)
            }
        } else {
            Value::Num(sum)
        }
    }

    pub(crate) fn fn_sumifs(&self, args: &[CallArg], average: bool) -> Value {
        if args.len() < 3 || args.len() % 2 != 1 {
            return Value::Err(ExcelError::Value);
        }
        let sum_rect = match self.arg_rect(args, 0) {
            Ok(r) => r,
            Err(e) => return Value::Err(e),
        };
        let mask = match self.ifs_mask(args, 1, &sum_rect) {
            Ok(m) => m,
            Err(e) => return Value::Err(e),
        };
        let mut sum = 0.0;
        let mut n = 0usize;
        for (k, (r, c)) in sum_rect.cells().enumerate() {
            if mask[k] {
                if let Value::Num(x) = self.ctx.cell(sum_rect.sheet, r, c) {
                    sum += x;
                    n += 1;
                }
            }
        }
        if average {
            if n == 0 {
                Value::Err(ExcelError::Div0)
            } else {
                Value::Num(sum / n as f64)
            }
        } else {
            Value::Num(sum)
        }
    }

    // ---- aggregation ----

    pub(crate) fn fn_counta(&self, args: &[CallArg]) -> Value {
        let mut n = 0usize;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => {
                    if !matches!(v, Value::Blank) {
                        n += 1;
                    }
                }
                Operand::Ref(_) => self.for_each_value(&op, &mut |v| {
                    if !matches!(v, Value::Blank) {
                        n += 1;
                    }
                }),
            }
        }
        Value::Num(n as f64)
    }

    pub(crate) fn fn_countblank(&self, args: &[CallArg]) -> Value {
        match self.arg_rect(args, 0) {
            Ok(rect) => {
                let mut n = 0usize;
                for (r, c) in rect.cells() {
                    match self.ctx.cell(rect.sheet, r, c) {
                        Value::Blank => n += 1,
                        Value::Text(s) if s.is_empty() => n += 1,
                        _ => {}
                    }
                }
                Value::Num(n as f64)
            }
            Err(e) => Value::Err(e),
        }
    }

    /// SUMPRODUCT: congruent arrays multiplied pairwise; non-numeric cells
    /// contribute 0.
    pub(crate) fn fn_sumproduct(&self, args: &[CallArg]) -> Value {
        let mut rects: Vec<Rect> = Vec::new();
        for (i, _) in args.iter().enumerate() {
            match self.arg_rect(args, i) {
                Ok(r) => rects.push(r),
                Err(_) => {
                    // Scalar argument: multiply the running total instead.
                    match self.arg_num(args, i) {
                        Ok(x) => {
                            // Treat as 1x1 "array" broadcast is NOT Excel
                            // semantics; only allow when it's the sole arg.
                            if args.len() == 1 {
                                return Value::Num(x);
                            }
                            return Value::Err(ExcelError::Value);
                        }
                        Err(e) => return Value::Err(e),
                    }
                }
            }
        }
        let Some(first) = rects.first() else { return Value::Err(ExcelError::Value) };
        let shape = (first.r1 - first.r0, first.c1 - first.c0);
        if rects.iter().any(|r| (r.r1 - r.r0, r.c1 - r.c0) != shape) {
            return Value::Err(ExcelError::Value);
        }
        let mut sum = 0.0;
        for dr in 0..=shape.0 {
            for dc in 0..=shape.1 {
                let mut prod = 1.0;
                for r in &rects {
                    match self.rect_get(r, dr, dc) {
                        Value::Num(x) => prod *= x,
                        Value::Err(e) => return Value::Err(e),
                        _ => prod *= 0.0,
                    }
                }
                sum += prod;
            }
        }
        Value::Num(sum)
    }

    fn collect_numbers(&self, args: &[CallArg], skip_first: usize) -> Result<Vec<f64>, ExcelError> {
        let mut out = Vec::new();
        let mut err: Option<ExcelError> = None;
        for arg in args.iter().skip(skip_first).filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => out.push(v.to_number()?),
                Operand::Ref(_) => self.for_each_value(&op, &mut |v| match v {
                    Value::Num(x) if err.is_none() => out.push(x),
                    Value::Err(e) if err.is_none() => err = Some(e),
                    _ => {}
                }),
            }
        }
        match err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    pub(crate) fn fn_large_small(&self, args: &[CallArg], large: bool) -> Value {
        if args.len() != 2 {
            return Value::Err(ExcelError::Value);
        }
        let mut xs = match self.collect_numbers(&args[..1], 0) {
            Ok(v) => v,
            Err(e) => return Value::Err(e),
        };
        let k = match self.arg_num(args, 1) {
            Ok(x) => x.trunc() as i64,
            Err(e) => return Value::Err(e),
        };
        if k < 1 || k as usize > xs.len() {
            return Value::Err(ExcelError::Num);
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let idx = if large { xs.len() - k as usize } else { k as usize - 1 };
        Value::Num(xs[idx])
    }

    pub(crate) fn fn_median(&self, args: &[CallArg]) -> Value {
        let mut xs = match self.collect_numbers(args, 0) {
            Ok(v) => v,
            Err(e) => return Value::Err(e),
        };
        if xs.is_empty() {
            return Value::Err(ExcelError::Num);
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let n = xs.len();
        Value::Num(if n % 2 == 1 {
            xs[n / 2]
        } else {
            (xs[n / 2 - 1] + xs[n / 2]) / 2.0
        })
    }

    pub(crate) fn fn_stdev(&self, args: &[CallArg], sample: bool, sqrt: bool) -> Value {
        let xs = match self.collect_numbers(args, 0) {
            Ok(v) => v,
            Err(e) => return Value::Err(e),
        };
        let n = xs.len();
        if (sample && n < 2) || (!sample && n == 0) {
            return Value::Err(ExcelError::Div0);
        }
        let mean = xs.iter().sum::<f64>() / n as f64;
        // Corrected two-pass: subtracting the (nonzero only through
        // rounding) mean-residual term matches Excel's computation more
        // closely than the plain sum of squares.
        let ss: f64 = xs.iter().map(|x| (x - mean) * (x - mean)).sum();
        let comp: f64 = xs.iter().map(|x| x - mean).sum();
        let ss = ss - comp * comp / n as f64;
        let var = ss / (n as f64 - if sample { 1.0 } else { 0.0 });
        Value::Num(if sqrt { var.sqrt() } else { var })
    }

    pub(crate) fn fn_rank(&self, args: &[CallArg]) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let xs = match self.collect_numbers(&args[1..2], 0) {
            Ok(v) => v,
            Err(e) => return Value::Err(e),
        };
        let ascending = match self.arg_num_or(args, 2, 0.0) {
            Ok(o) => o != 0.0,
            Err(e) => return Value::Err(e),
        };
        if !xs.iter().any(|&v| v == x) {
            return Value::Err(ExcelError::NA);
        }
        let rank = 1 + xs
            .iter()
            .filter(|&&v| if ascending { v < x } else { v > x })
            .count();
        Value::Num(rank as f64)
    }

    /// SUBTOTAL: function codes 1-11 (and 101-111, treated identically —
    /// we cannot see hidden rows; the receipt prices that approximation).
    /// Nested SUBTOTALs are NOT excluded yet (same caveat).
    pub(crate) fn fn_subtotal(&self, args: &[CallArg]) -> Value {
        let code = match self.arg_num(args, 0) {
            Ok(x) => x.trunc() as i64,
            Err(e) => return Value::Err(e),
        };
        let rest = &args[1..];
        match code % 100 {
            1 => self.average(rest),
            2 => self.count(rest),
            3 => self.fn_counta(rest),
            4 => self.min_max(rest, false),
            5 => self.min_max(rest, true),
            6 => self.fold_numeric(rest, 1.0, |a, x| a * x),
            7 => self.fn_stdev(rest, true, true),
            8 => self.fn_stdev(rest, false, true),
            9 => self.fold_numeric(rest, 0.0, |a, x| a + x),
            10 => self.fn_stdev(rest, true, false),
            11 => self.fn_stdev(rest, false, false),
            _ => Value::Err(ExcelError::Value),
        }
    }
}

/// Exact-mode lookup key equality: same family only (text "00610" never
/// matches the number 610), case-insensitive for text, wildcards allowed
/// for text keys. This is NOT the COUNTIF criteria language.
fn exact_key_match(key: &Value, v: &Value) -> bool {
    match (key, v) {
        (Value::Text(pat), Value::Text(s)) if has_wildcard(pat) => {
            wildcard_match(&pat.to_lowercase(), s)
        }
        _ => same_family(key, v) && compare(key, v) == Ok(Ordering::Equal),
    }
}

/// Same comparison family (both numeric, both text, both bool) — lookup
/// scans skip cells of a different type rather than erroring.
fn same_family(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Num(_), Value::Num(_))
            | (Value::Text(_), Value::Text(_))
            | (Value::Bool(_), Value::Bool(_))
    )
}
