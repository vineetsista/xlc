//! Scalar-shaped builtins: logic/info, text, date/time, math, stats.
//! Range-consuming builtins (lookup + criteria family) live in
//! funcs_range.rs. Every function's quirks cite Excel behavior; the
//! corpus receipt is the arbiter.

use crate::dates;
use crate::interp::{Ctx, Interp, Operand};
use crate::value::{parse_excel_number, ExcelError, Value};
use xlc_parse::ast::CallArg;

pub(crate) enum IsKind {
    Blank,
    Text,
    NonText,
    Number,
    Logical,
    Na,
    ErrNotNa,
    AnyErr,
}

impl<C: Ctx> Interp<'_, C> {
    // ---- logic / info ----

    /// IFERROR / IFNA: errors in the first argument's own evaluation are
    /// caught, not propagated.
    pub(crate) fn fn_iferror(&self, args: &[CallArg], na_only: bool) -> Operand {
        if args.is_empty() || args.len() > 2 {
            return Operand::Val(Value::Err(ExcelError::Value));
        }
        let v = self.arg_scalar(args, 0);
        let caught = match &v {
            Value::Err(ExcelError::NA) => true,
            Value::Err(_) => !na_only,
            _ => false,
        };
        if caught {
            match self.arg(args, 1) {
                Some(e) => self.eval(e),
                None => Operand::Val(Value::Text(String::new())),
            }
        } else {
            Operand::Val(v)
        }
    }

    /// AND / OR over scalars and ranges. In ranges only logical and
    /// numeric cells participate; text and blanks are ignored. Scalar
    /// text arguments must coerce ("TRUE") or the result is #VALUE!.
    pub(crate) fn fn_and_or(&self, args: &[CallArg], is_and: bool) -> Value {
        let mut acc: Option<bool> = None;
        let mut err: Option<ExcelError> = None;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => match v.to_bool() {
                    Ok(b) => acc = Some(combine(acc, b, is_and)),
                    Err(e) => return Value::Err(e),
                },
                Operand::Ref(_) => self.for_each_value(&op, &mut |v| match v {
                    Value::Bool(b) if err.is_none() => acc = Some(combine(acc, b, is_and)),
                    Value::Num(x) if err.is_none() => acc = Some(combine(acc, x != 0.0, is_and)),
                    Value::Err(e) if err.is_none() => err = Some(e),
                    _ => {}
                }),
            }
        }
        if let Some(e) = err {
            return Value::Err(e);
        }
        match acc {
            Some(b) => Value::Bool(b),
            None => Value::Err(ExcelError::Value),
        }
    }

    pub(crate) fn fn_xor(&self, args: &[CallArg]) -> Value {
        let mut acc = false;
        let mut any = false;
        let mut err: Option<ExcelError> = None;
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            match &op {
                Operand::Val(v) => match v.to_bool() {
                    Ok(b) => {
                        acc ^= b;
                        any = true;
                    }
                    Err(e) => return Value::Err(e),
                },
                Operand::Ref(_) => self.for_each_value(&op, &mut |v| match v {
                    Value::Bool(b) if err.is_none() => {
                        acc ^= b;
                        any = true;
                    }
                    Value::Num(x) if err.is_none() => {
                        acc ^= x != 0.0;
                        any = true;
                    }
                    Value::Err(e) if err.is_none() => err = Some(e),
                    _ => {}
                }),
            }
        }
        if let Some(e) = err {
            return Value::Err(e);
        }
        if any {
            Value::Bool(acc)
        } else {
            Value::Err(ExcelError::Value)
        }
    }

    pub(crate) fn fn_not(&self, args: &[CallArg]) -> Value {
        if args.len() != 1 {
            return Value::Err(ExcelError::Value);
        }
        match self.arg_scalar(args, 0).to_bool() {
            Ok(b) => Value::Bool(!b),
            Err(e) => Value::Err(e),
        }
    }

    /// The IS* family. ISBLANK is reference-aware: ISBLANK(A1) asks about
    /// the cell, not a coerced value.
    pub(crate) fn fn_is(&self, args: &[CallArg], kind: IsKind) -> Value {
        if args.len() != 1 {
            return Value::Err(ExcelError::Value);
        }
        let v = self.arg_scalar(args, 0);
        let b = match kind {
            IsKind::Blank => matches!(v, Value::Blank),
            IsKind::Text => matches!(v, Value::Text(_)),
            IsKind::NonText => !matches!(v, Value::Text(_)),
            IsKind::Number => matches!(v, Value::Num(_)),
            IsKind::Logical => matches!(v, Value::Bool(_)),
            IsKind::Na => matches!(v, Value::Err(ExcelError::NA)),
            IsKind::ErrNotNa => matches!(v, Value::Err(e) if e != ExcelError::NA),
            IsKind::AnyErr => matches!(v, Value::Err(_)),
        };
        Value::Bool(b)
    }

    pub(crate) fn fn_parity(&self, args: &[CallArg], want_even: bool) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) => {
                let n = x.trunc() as i64;
                Value::Bool((n % 2 == 0) == want_even)
            }
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_n(&self, args: &[CallArg]) -> Value {
        match self.arg_scalar(args, 0) {
            Value::Num(x) => Value::Num(x),
            Value::Bool(b) => Value::Num(if b { 1.0 } else { 0.0 }),
            Value::Err(e) => Value::Err(e),
            _ => Value::Num(0.0),
        }
    }

    pub(crate) fn fn_t(&self, args: &[CallArg]) -> Value {
        match self.arg_scalar(args, 0) {
            Value::Text(s) => Value::Text(s),
            Value::Err(e) => Value::Err(e),
            _ => Value::Text(String::new()),
        }
    }

    pub(crate) fn fn_hyperlink(&self, args: &[CallArg]) -> Value {
        // Cell value is the friendly name when given, else the link.
        let i = if args.len() >= 2 { 1 } else { 0 };
        self.arg_scalar(args, i)
    }

    // ---- text ----

    pub(crate) fn fn_left_right(&self, args: &[CallArg], right: bool) -> Value {
        let t = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let n = match self.arg_num_or(args, 1, 1.0) {
            Ok(n) => n,
            Err(e) => return Value::Err(e),
        };
        if n < 0.0 {
            return Value::Err(ExcelError::Value);
        }
        let n = n.trunc() as usize;
        let chars: Vec<char> = t.chars().collect();
        let n = n.min(chars.len());
        let s: String = if right {
            chars[chars.len() - n..].iter().collect()
        } else {
            chars[..n].iter().collect()
        };
        Value::Text(s)
    }

    pub(crate) fn fn_mid(&self, args: &[CallArg]) -> Value {
        let t = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let start = match self.arg_num(args, 1) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let len = match self.arg_num(args, 2) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        if start < 1.0 || len < 0.0 {
            return Value::Err(ExcelError::Value);
        }
        let chars: Vec<char> = t.chars().collect();
        let start = (start.trunc() as usize - 1).min(chars.len());
        let end = (start + len.trunc() as usize).min(chars.len());
        Value::Text(chars[start..end].iter().collect())
    }

    pub(crate) fn fn_len(&self, args: &[CallArg]) -> Value {
        match self.arg_text(args, 0) {
            Ok(t) => Value::Num(t.chars().count() as f64),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_case(&self, args: &[CallArg], upper: bool) -> Value {
        match self.arg_text(args, 0) {
            Ok(t) => Value::Text(if upper {
                t.to_uppercase()
            } else {
                t.to_lowercase()
            }),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_proper(&self, args: &[CallArg]) -> Value {
        match self.arg_text(args, 0) {
            Ok(t) => {
                let mut out = String::with_capacity(t.len());
                let mut cap = true;
                for c in t.chars() {
                    if c.is_alphabetic() {
                        out.extend(if cap {
                            c.to_uppercase().collect::<Vec<_>>()
                        } else {
                            c.to_lowercase().collect()
                        });
                        cap = false;
                    } else {
                        out.push(c);
                        cap = true;
                    }
                }
                Value::Text(out)
            }
            Err(e) => Value::Err(e),
        }
    }

    /// Excel TRIM: strips leading/trailing spaces AND collapses internal
    /// runs to a single space (ASCII space only, not tabs/NBSP).
    pub(crate) fn fn_trim(&self, args: &[CallArg]) -> Value {
        match self.arg_text(args, 0) {
            Ok(t) => {
                let mut out = String::with_capacity(t.len());
                let mut pending_space = false;
                for c in t.chars() {
                    if c == ' ' {
                        pending_space = !out.is_empty();
                    } else {
                        if pending_space {
                            out.push(' ');
                            pending_space = false;
                        }
                        out.push(c);
                    }
                }
                Value::Text(out)
            }
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_substitute(&self, args: &[CallArg]) -> Value {
        let (t, old, new) = match (
            self.arg_text(args, 0),
            self.arg_text(args, 1),
            self.arg_text(args, 2),
        ) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return Value::Err(e),
        };
        if old.is_empty() {
            return Value::Text(t);
        }
        match self.arg(args, 3) {
            None => Value::Text(t.replace(&old, &new)),
            Some(_) => {
                let inst = match self.arg_num(args, 3) {
                    Ok(x) if x >= 1.0 => x.trunc() as usize,
                    Ok(_) => return Value::Err(ExcelError::Value),
                    Err(e) => return Value::Err(e),
                };
                let mut count = 0usize;
                let mut out = String::with_capacity(t.len());
                let mut rest = t.as_str();
                while let Some(pos) = rest.find(&old) {
                    count += 1;
                    out.push_str(&rest[..pos]);
                    if count == inst {
                        out.push_str(&new);
                    } else {
                        out.push_str(&old);
                    }
                    rest = &rest[pos + old.len()..];
                }
                out.push_str(rest);
                Value::Text(out)
            }
        }
    }

    pub(crate) fn fn_replace(&self, args: &[CallArg]) -> Value {
        let t = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let (start, n) = match (self.arg_num(args, 1), self.arg_num(args, 2)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Value::Err(e),
        };
        let new = match self.arg_text(args, 3) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        if start < 1.0 || n < 0.0 {
            return Value::Err(ExcelError::Value);
        }
        let chars: Vec<char> = t.chars().collect();
        let start = (start.trunc() as usize - 1).min(chars.len());
        let end = (start + n.trunc() as usize).min(chars.len());
        let mut out: String = chars[..start].iter().collect();
        out.push_str(&new);
        out.extend(chars[end..].iter());
        Value::Text(out)
    }

    pub(crate) fn fn_concatenate(&self, args: &[CallArg]) -> Value {
        let mut out = String::new();
        for arg in args.iter().filter_map(|a| a.expr.as_ref()) {
            match self.deref_scalar(self.eval(arg)).to_text() {
                Ok(t) => out.push_str(&t),
                Err(e) => return Value::Err(e),
            }
        }
        Value::Text(out)
    }

    pub(crate) fn fn_textjoin(&self, args: &[CallArg]) -> Value {
        let delim = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let skip_empty = match self.arg_bool_or(args, 1, true) {
            Ok(b) => b,
            Err(e) => return Value::Err(e),
        };
        let mut parts: Vec<String> = Vec::new();
        let mut err: Option<ExcelError> = None;
        for arg in args.iter().skip(2).filter_map(|a| a.expr.as_ref()) {
            let op = self.eval(arg);
            self.for_each_value(&op, &mut |v| {
                if err.is_some() {
                    return;
                }
                match v.to_text() {
                    Ok(t) => {
                        if !(skip_empty && t.is_empty()) {
                            parts.push(t);
                        }
                    }
                    Err(e) => err = Some(e),
                }
            });
        }
        if let Some(e) = err {
            return Value::Err(e);
        }
        Value::Text(parts.join(&delim))
    }

    /// TEXTAFTER / TEXTBEFORE with instance support (negative counts from
    /// the end). match_mode/match_end/pad args beyond instance are not yet
    /// honored (corpus will price them).
    pub(crate) fn fn_text_after_before(&self, args: &[CallArg], after: bool) -> Value {
        let t = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let delim = match self.arg_text(args, 1) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let inst = match self.arg_num_or(args, 2, 1.0) {
            Ok(x) => x.trunc() as i64,
            Err(e) => return Value::Err(e),
        };
        if delim.is_empty() || inst == 0 {
            return Value::Err(ExcelError::Value);
        }
        let hits: Vec<usize> = t.match_indices(&delim).map(|(i, _)| i).collect();
        if inst.unsigned_abs() as usize > hits.len() {
            return Value::Err(ExcelError::NA);
        }
        let idx = if inst > 0 {
            inst - 1
        } else {
            hits.len() as i64 + inst
        } as usize;
        let pos = hits[idx];
        let s = if after {
            &t[pos + delim.len()..]
        } else {
            &t[..pos]
        };
        Value::Text(s.to_string())
    }

    pub(crate) fn fn_value(&self, args: &[CallArg]) -> Value {
        match self.arg_scalar(args, 0) {
            Value::Num(x) => Value::Num(x),
            Value::Text(s) => match parse_excel_number(&s) {
                Some(x) => Value::Num(x),
                None => Value::Err(ExcelError::Value),
            },
            Value::Blank => Value::Num(0.0),
            Value::Err(e) => Value::Err(e),
            Value::Bool(_) => Value::Err(ExcelError::Value),
        }
    }

    pub(crate) fn fn_exact(&self, args: &[CallArg]) -> Value {
        match (self.arg_text(args, 0), self.arg_text(args, 1)) {
            (Ok(a), Ok(b)) => Value::Bool(a == b), // case-SENSITIVE
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        }
    }

    /// FIND (case-sensitive, no wildcards) / SEARCH (case-insensitive,
    /// wildcards). 1-based position or #VALUE!.
    pub(crate) fn fn_find_search(&self, args: &[CallArg], case_sensitive: bool) -> Value {
        let needle = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let hay = match self.arg_text(args, 1) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        let start = match self.arg_num_or(args, 2, 1.0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        if start < 1.0 {
            return Value::Err(ExcelError::Value);
        }
        let hay_chars: Vec<char> = hay.chars().collect();
        let from = start.trunc() as usize - 1;
        if from > hay_chars.len() {
            return Value::Err(ExcelError::Value);
        }
        if case_sensitive {
            let hs: String = hay_chars[from..].iter().collect();
            match hs.find(&needle) {
                Some(byte_pos) => {
                    let char_pos = hs[..byte_pos].chars().count();
                    Value::Num((from + char_pos + 1) as f64)
                }
                None => Value::Err(ExcelError::Value),
            }
        } else {
            // SEARCH: wildcard prefix-match sliding over the haystack.
            let pat = needle.to_lowercase();
            for i in from..=hay_chars.len() {
                let window: String = hay_chars[i..].iter().collect();
                if crate::criteria::wildcard_prefix_match(&pat, &window) {
                    return Value::Num((i + 1) as f64);
                }
            }
            Value::Err(ExcelError::Value)
        }
    }

    pub(crate) fn fn_rept(&self, args: &[CallArg]) -> Value {
        let t = match self.arg_text(args, 0) {
            Ok(t) => t,
            Err(e) => return Value::Err(e),
        };
        match self.arg_num(args, 1) {
            Ok(n) if n >= 0.0 => {
                let n = n.trunc() as usize;
                if t.len().saturating_mul(n) > 32_767 * 4 {
                    return Value::Err(ExcelError::Value);
                }
                Value::Text(t.repeat(n))
            }
            Ok(_) => Value::Err(ExcelError::Value),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_char(&self, args: &[CallArg]) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) => {
                let n = x.trunc() as i64;
                if !(1..=255).contains(&n) {
                    return Value::Err(ExcelError::Value);
                }
                // ANSI (Windows-1252) range treated as Unicode scalar for
                // the common ASCII span; the corpus prices the rest.
                match char::from_u32(n as u32) {
                    Some(c) => Value::Text(c.to_string()),
                    None => Value::Err(ExcelError::Value),
                }
            }
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_code(&self, args: &[CallArg]) -> Value {
        match self.arg_text(args, 0) {
            Ok(t) => match t.chars().next() {
                Some(c) => Value::Num((c as u32) as f64),
                None => Value::Err(ExcelError::Value),
            },
            Err(e) => Value::Err(e),
        }
    }

    // ---- date / time ----

    pub(crate) fn fn_date(&self, args: &[CallArg]) -> Value {
        let (y, m, d) = match (
            self.arg_num(args, 0),
            self.arg_num(args, 1),
            self.arg_num(args, 2),
        ) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return Value::Err(e),
        };
        let mut y = y.trunc() as i32;
        // Excel: years 0..1900 mean 1900+y.
        if (0..1900).contains(&y) {
            y += 1900;
        }
        let serial = dates::ymd_to_serial_1900(y, m.trunc() as i32, d.trunc() as i32);
        let serial = if self.ctx.epoch_1904() {
            serial - 1462
        } else {
            serial
        };
        if serial < 0 {
            return Value::Err(ExcelError::Num);
        }
        Value::Num(serial as f64)
    }

    pub(crate) fn fn_time(&self, args: &[CallArg]) -> Value {
        let (h, m, s) = match (
            self.arg_num(args, 0),
            self.arg_num(args, 1),
            self.arg_num(args, 2),
        ) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return Value::Err(e),
        };
        if h < 0.0 || m < 0.0 || s < 0.0 {
            return Value::Err(ExcelError::Num);
        }
        let secs = h.trunc() * 3600.0 + m.trunc() * 60.0 + s.trunc();
        Value::Num((secs % 86_400.0) / 86_400.0)
    }

    fn serial_1900(&self, x: f64) -> i64 {
        let s = x.floor() as i64;
        if self.ctx.epoch_1904() {
            dates::serial_1904_to_1900(s)
        } else {
            s
        }
    }

    pub(crate) fn fn_ymd(&self, args: &[CallArg], part: usize) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) if x >= 0.0 => {
                let (y, m, d) = dates::serial_to_ymd_1900(self.serial_1900(x));
                Value::Num([y as f64, m as f64, d as f64][part])
            }
            Ok(_) => Value::Err(ExcelError::Num),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_hms(&self, args: &[CallArg], part: usize) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) if x >= 0.0 => {
                let frac = x.fract();
                let total = (frac * 86_400.0).round() as i64 % 86_400;
                let v = match part {
                    0 => total / 3600,
                    1 => (total % 3600) / 60,
                    _ => total % 60,
                };
                Value::Num(v as f64)
            }
            Ok(_) => Value::Err(ExcelError::Num),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_days(&self, args: &[CallArg]) -> Value {
        match (self.arg_num(args, 0), self.arg_num(args, 1)) {
            (Ok(end), Ok(start)) => Value::Num(end.trunc() - start.trunc()),
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        }
    }

    pub(crate) fn fn_weekday(&self, args: &[CallArg]) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let mode = match self.arg_num_or(args, 1, 1.0) {
            Ok(m) => m.trunc() as i64,
            Err(e) => return Value::Err(e),
        };
        let serial = self.serial_1900(x);
        // Serial 1 (1900-01-01) was a Sunday in Excel's (bug-shifted) world.
        let dow0 = (serial % 7 + 7) % 7; // 1 => Sunday=1 ... pattern below
        let sunday1 = ((dow0 + 6) % 7) + 1; // 1=Sunday..7=Saturday
        let v = match mode {
            1 | 17 => sunday1,
            2 | 11 => ((sunday1 + 5) % 7) + 1, // 1=Monday..7=Sunday
            3 => (sunday1 + 5) % 7,            // 0=Monday..6=Sunday
            _ => return Value::Err(ExcelError::Num),
        };
        Value::Num(v as f64)
    }

    // ---- math ----

    pub(crate) fn fn_log(&self, args: &[CallArg]) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let base = match self.arg_num_or(args, 1, 10.0) {
            Ok(b) => b,
            Err(e) => return Value::Err(e),
        };
        if x <= 0.0 || base <= 0.0 || base == 1.0 {
            return Value::Err(ExcelError::Num);
        }
        Value::Num(x.log(base))
    }

    pub(crate) fn fn_power(&self, args: &[CallArg]) -> Value {
        let a = self.arg_scalar(args, 0);
        let b = self.arg_scalar(args, 1);
        crate::value::pow(&a, &b)
    }

    /// Excel MOD: result has the sign of the divisor.
    pub(crate) fn fn_mod(&self, args: &[CallArg]) -> Value {
        match (self.arg_num(args, 0), self.arg_num(args, 1)) {
            (Ok(n), Ok(d)) => {
                if d == 0.0 {
                    Value::Err(ExcelError::Div0)
                } else {
                    Value::Num(n - d * (n / d).floor())
                }
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        }
    }

    pub(crate) fn fn_trunc(&self, args: &[CallArg]) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let d = match self.arg_num_or(args, 1, 0.0) {
            Ok(d) => d.trunc() as i32,
            Err(e) => return Value::Err(e),
        };
        let f = 10f64.powi(d);
        Value::Num((x * f).trunc() / f)
    }

    /// ROUNDUP (away from zero) / ROUNDDOWN (toward zero).
    pub(crate) fn fn_round_dir(&self, args: &[CallArg], up: bool) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let d = match self.arg_num(args, 1) {
            Ok(d) => d.trunc() as i32,
            Err(e) => return Value::Err(e),
        };
        let f = 10f64.powi(d);
        let scaled = x * f;
        let r = if up {
            scaled.abs().ceil() * scaled.signum()
        } else {
            scaled.trunc()
        };
        Value::Num(r / f)
    }

    /// Legacy FLOOR / CEILING with a significance argument.
    pub(crate) fn fn_floor_ceiling(&self, args: &[CallArg], floor: bool) -> Value {
        let x = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        let sig = match self.arg_num_or(args, 1, 1.0) {
            Ok(s) => s,
            Err(e) => return Value::Err(e),
        };
        if sig == 0.0 {
            return if floor {
                Value::Err(ExcelError::Div0)
            } else {
                Value::Num(0.0)
            };
        }
        if x > 0.0 && sig < 0.0 {
            return Value::Err(ExcelError::Num);
        }
        let q = x / sig;
        let r = if floor { q.floor() } else { q.ceil() };
        Value::Num(r * sig)
    }

    /// EVEN / ODD: round away from zero to the next even/odd integer.
    pub(crate) fn fn_even_odd(&self, args: &[CallArg], even: bool) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) => {
                let a = x.abs();
                let r = if even {
                    (a / 2.0).ceil() * 2.0
                } else {
                    ((a + 1.0) / 2.0).ceil() * 2.0 - 1.0
                };
                Value::Num(r * if x < 0.0 { -1.0 } else { 1.0 })
            }
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_atan2(&self, args: &[CallArg]) -> Value {
        match (self.arg_num(args, 0), self.arg_num(args, 1)) {
            // Excel argument order is (x, y); Rust's is y.atan2(x).
            (Ok(x), Ok(y)) => {
                if x == 0.0 && y == 0.0 {
                    Value::Err(ExcelError::Div0)
                } else {
                    Value::Num(y.atan2(x))
                }
            }
            (Err(e), _) | (_, Err(e)) => Value::Err(e),
        }
    }

    // ---- stats / engineering ----

    pub(crate) fn fn_normdist(&self, args: &[CallArg]) -> Value {
        let (x, mean, sd) = match (
            self.arg_num(args, 0),
            self.arg_num(args, 1),
            self.arg_num(args, 2),
        ) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return Value::Err(e),
        };
        let cumulative = match self.arg_bool_or(args, 3, true) {
            Ok(b) => b,
            Err(e) => return Value::Err(e),
        };
        if sd <= 0.0 {
            return Value::Err(ExcelError::Num);
        }
        let z = (x - mean) / sd;
        let v = if cumulative {
            phi(z)
        } else {
            (-0.5 * z * z).exp() / (sd * (2.0 * std::f64::consts::PI).sqrt())
        };
        Value::Num(v)
    }

    pub(crate) fn fn_normsdist(&self, args: &[CallArg]) -> Value {
        match self.arg_num(args, 0) {
            Ok(z) => Value::Num(phi(z)),
            Err(e) => Value::Err(e),
        }
    }

    pub(crate) fn fn_norm_s_dist(&self, args: &[CallArg]) -> Value {
        let z = match self.arg_num(args, 0) {
            Ok(z) => z,
            Err(e) => return Value::Err(e),
        };
        match self.arg_bool_or(args, 1, true) {
            Ok(true) => Value::Num(phi(z)),
            Ok(false) => Value::Num((-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt()),
            Err(e) => Value::Err(e),
        }
    }

    /// ERF(lower) or ERF(lower, upper) = erf(upper) - erf(lower).
    pub(crate) fn fn_erf(&self, args: &[CallArg]) -> Value {
        let lo = match self.arg_num(args, 0) {
            Ok(x) => x,
            Err(e) => return Value::Err(e),
        };
        match self.arg(args, 1) {
            None => Value::Num(libm::erf(lo)),
            Some(_) => match self.arg_num(args, 1) {
                Ok(hi) => Value::Num(libm::erf(hi) - libm::erf(lo)),
                Err(e) => Value::Err(e),
            },
        }
    }

    pub(crate) fn fn_erfc(&self, args: &[CallArg]) -> Value {
        match self.arg_num(args, 0) {
            Ok(x) => Value::Num(libm::erfc(x)),
            Err(e) => Value::Err(e),
        }
    }
}

fn combine(acc: Option<bool>, b: bool, is_and: bool) -> bool {
    match acc {
        None => b,
        Some(a) => {
            if is_and {
                a && b
            } else {
                a || b
            }
        }
    }
}

/// Standard normal CDF via erfc for full-tail precision.
fn phi(z: f64) -> f64 {
    0.5 * libm::erfc(-z / std::f64::consts::SQRT_2)
}
