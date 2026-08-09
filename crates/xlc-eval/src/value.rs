//! The Excel value model and its coercion rules (§8.4). Every rule here is
//! paid for explicitly and covered by a test; the corpus receipt is the
//! final judge.
//!
//! Core distinctions Excel actually makes:
//!   - Blank (an empty cell) is not 0 and not "" — it coerces to either
//!     depending on the operator, and `A1=0` AND `A1=""` are both TRUE
//!     when A1 is blank.
//!   - Text→number coercion applies in arithmetic ("2"+1 = 3) but NOT in
//!     comparisons ("2">1 is TRUE because Text > Number, always).
//!   - Text comparison is case-insensitive ("a"="A" is TRUE).
//!   - Cross-type ordering: Number < Text < Bool(FALSE) < Bool(TRUE).
//!   - Errors propagate left-to-right through operands.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExcelError {
    Div0,
    NA,
    Value,
    Ref,
    Name,
    Num,
    Null,
    Spill,
    Calc,
    GettingData,
}

impl ExcelError {
    pub fn as_str(self) -> &'static str {
        match self {
            ExcelError::Div0 => "#DIV/0!",
            ExcelError::NA => "#N/A",
            ExcelError::Value => "#VALUE!",
            ExcelError::Ref => "#REF!",
            ExcelError::Name => "#NAME?",
            ExcelError::Num => "#NUM!",
            ExcelError::Null => "#NULL!",
            ExcelError::Spill => "#SPILL!",
            ExcelError::Calc => "#CALC!",
            ExcelError::GettingData => "#GETTING_DATA",
        }
    }
}

impl fmt::Display for ExcelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Text(String),
    Bool(bool),
    Err(ExcelError),
    /// An empty cell. Distinct from Num(0.0) and Text("").
    Blank,
}

impl Value {
    pub fn err(e: ExcelError) -> Value {
        Value::Err(e)
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Value::Err(_))
    }

    /// Coerce to a number for arithmetic context.
    /// Blank→0, Bool→0/1, Text→Excel text-to-number parse, Err passes through.
    pub fn to_number(&self) -> Result<f64, ExcelError> {
        match self {
            Value::Num(x) => Ok(*x),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Blank => Ok(0.0),
            Value::Text(s) => parse_excel_number(s).ok_or(ExcelError::Value),
            Value::Err(e) => Err(*e),
        }
    }

    /// Coerce to text for concatenation context. Blank→"".
    pub fn to_text(&self) -> Result<String, ExcelError> {
        match self {
            Value::Text(s) => Ok(s.clone()),
            Value::Num(x) => Ok(format_general(*x)),
            Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
            Value::Blank => Ok(String::new()),
            Value::Err(e) => Err(*e),
        }
    }

    /// Coerce to boolean for logical context (IF's condition).
    /// Text "TRUE"/"FALSE" (any case) coerce; other text is #VALUE!.
    pub fn to_bool(&self) -> Result<bool, ExcelError> {
        match self {
            Value::Bool(b) => Ok(*b),
            Value::Num(x) => Ok(*x != 0.0),
            Value::Blank => Ok(false),
            Value::Text(s) => {
                if s.eq_ignore_ascii_case("TRUE") {
                    Ok(true)
                } else if s.eq_ignore_ascii_case("FALSE") {
                    Ok(false)
                } else {
                    Err(ExcelError::Value)
                }
            }
            Value::Err(e) => Err(*e),
        }
    }
}

/// Excel's text→number parse for arithmetic coercion. Accepts leading and
/// trailing whitespace, optional sign, decimals, scientific notation,
/// a trailing `%` (divides by 100), thousands separators in the integer
/// part, and leading `$`. Returns None if unparseable (→ #VALUE!).
///
/// Deliberately NOT yet handled (corpus will price them): date/time text
/// ("1/2/2020", "10:30"), parenthesized negatives ("(5)"), currency
/// symbols other than `$`, locale variants. Logged as a known gap.
pub fn parse_excel_number(s: &str) -> Option<f64> {
    let mut t = s.trim();
    if t.is_empty() {
        return None;
    }
    let mut pct = false;
    if let Some(stripped) = t.strip_suffix('%') {
        pct = true;
        t = stripped.trim_end();
    }
    // Optional sign before an optional `$`.
    let mut neg = false;
    if let Some(stripped) = t.strip_prefix('-') {
        neg = true;
        t = stripped.trim_start();
    } else if let Some(stripped) = t.strip_prefix('+') {
        t = stripped.trim_start();
    }
    if let Some(stripped) = t.strip_prefix('$') {
        t = stripped.trim_start();
    }
    // Strip thousands separators, but only in the integer part and only in
    // valid 3-digit groupings; be permissive: remove commas then parse,
    // rejecting pathological forms like ",1" or "1,,2".
    let cleaned: String;
    if t.contains(',') {
        if t.starts_with(',') || t.contains(",,") || t.contains(",.") {
            return None;
        }
        cleaned = t.replace(',', "");
        t = &cleaned;
    }
    let x: f64 = t.parse().ok()?;
    let x = if neg { -x } else { x };
    Some(if pct { x / 100.0 } else { x })
}

/// Excel "General" formatting of a number to text — the 15-significant-
/// digit display rule. First cut: shortest Rust f64 formatting truncated
/// to 15 significant digits; the receipt's TEXT/concat mismatches will
/// calibrate this against reality.
pub fn format_general(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        // Integers print without a decimal point.
        return format!("{}", x as i64);
    }
    let s = format!("{x}");
    if significant_digits(&s) <= 15 {
        return s;
    }
    let r = format!("{x:.*}", 15usize.saturating_sub(int_digits(x)));
    trim_trailing_zeros(&r)
}

fn int_digits(x: f64) -> usize {
    let a = x.abs();
    if a < 1.0 {
        0
    } else {
        (a.log10().floor() as usize) + 1
    }
}

fn significant_digits(s: &str) -> usize {
    s.chars().filter(|c| c.is_ascii_digit()).count()
}

fn trim_trailing_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

// ---- binary operator semantics ----

pub fn add(a: &Value, b: &Value) -> Value {
    numeric_binop(a, b, |x, y| Ok(x + y))
}

pub fn sub(a: &Value, b: &Value) -> Value {
    numeric_binop(a, b, |x, y| Ok(x - y))
}

pub fn mul(a: &Value, b: &Value) -> Value {
    numeric_binop(a, b, |x, y| Ok(x * y))
}

pub fn div(a: &Value, b: &Value) -> Value {
    numeric_binop(a, b, |x, y| {
        if y == 0.0 {
            Err(ExcelError::Div0)
        } else {
            Ok(x / y)
        }
    })
}

/// Excel `^`: 0^0 is #NUM!, negative^non-integer is #NUM!.
pub fn pow(a: &Value, b: &Value) -> Value {
    numeric_binop(a, b, |x, y| {
        if x == 0.0 && y == 0.0 {
            return Err(ExcelError::Num);
        }
        if x < 0.0 && y.fract() != 0.0 {
            return Err(ExcelError::Num);
        }
        let r = x.powf(y);
        if r.is_finite() {
            Ok(r)
        } else if x == 0.0 && y < 0.0 {
            Err(ExcelError::Div0)
        } else {
            Err(ExcelError::Num)
        }
    })
}

pub fn concat(a: &Value, b: &Value) -> Value {
    match (a.to_text(), b.to_text()) {
        (Err(e), _) => Value::Err(e),
        (_, Err(e)) => Value::Err(e),
        (Ok(x), Ok(y)) => Value::Text(x + &y),
    }
}

pub fn percent(a: &Value) -> Value {
    numeric_unop(a, |x| Ok(x / 100.0))
}

pub fn neg(a: &Value) -> Value {
    numeric_unop(a, |x| Ok(-x))
}

/// Unary plus is an identity that still coerces errors through but leaves
/// text untouched (Excel: +"abc" stays "abc").
pub fn pos(a: &Value) -> Value {
    a.clone()
}

fn numeric_binop(a: &Value, b: &Value, f: impl Fn(f64, f64) -> Result<f64, ExcelError>) -> Value {
    let x = match a.to_number() {
        Ok(x) => x,
        Err(e) => return Value::Err(e),
    };
    let y = match b.to_number() {
        Ok(y) => y,
        Err(e) => return Value::Err(e),
    };
    match f(x, y) {
        Ok(r) if r.is_finite() => Value::Num(r),
        Ok(_) => Value::Err(ExcelError::Num),
        Err(e) => Value::Err(e),
    }
}

fn numeric_unop(a: &Value, f: impl Fn(f64) -> Result<f64, ExcelError>) -> Value {
    match a.to_number() {
        Ok(x) => match f(x) {
            Ok(r) if r.is_finite() => Value::Num(r),
            Ok(_) => Value::Err(ExcelError::Num),
            Err(e) => Value::Err(e),
        },
        Err(e) => Value::Err(e),
    }
}

/// Comparison: returns Ordering-like -1/0/1 wrapped in the Excel rules,
/// or an error. Used by = <> < <= > >=.
///
/// Rules: errors propagate; Blank equals 0 AND equals "" (it adopts the
/// other side's domain, FALSE for bools); text compares case-insensitively;
/// mixed types order Number < Text < Bool; no text→number coercion here.
pub fn compare(a: &Value, b: &Value) -> Result<std::cmp::Ordering, ExcelError> {
    use std::cmp::Ordering;
    use Value::*;

    fn rank(v: &Value) -> u8 {
        match v {
            Num(_) => 0,
            Text(_) => 1,
            Bool(_) => 2,
            _ => unreachable!(),
        }
    }

    match (a, b) {
        (Err(e), _) => Result::Err(*e),
        (_, Err(e)) => Result::Err(*e),
        (Blank, Blank) => Ok(Ordering::Equal),
        (Blank, other) => compare(&blank_as(other), other),
        (other, Blank) => compare(other, &blank_as(other)),
        (Num(x), Num(y)) => Ok(x.partial_cmp(y).unwrap_or(Ordering::Equal)),
        (Text(x), Text(y)) => {
            let lx = x.to_lowercase();
            let ly = y.to_lowercase();
            Ok(lx.cmp(&ly))
        }
        (Bool(x), Bool(y)) => Ok(x.cmp(y)),
        (x, y) => Ok(rank(x).cmp(&rank(y))),
    }
}

/// What Blank pretends to be when compared against `other`.
fn blank_as(other: &Value) -> Value {
    match other {
        Value::Num(_) => Value::Num(0.0),
        Value::Text(_) => Value::Text(String::new()),
        Value::Bool(_) => Value::Bool(false),
        _ => Value::Num(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn arithmetic_coercion() {
        // "2"+1 = 3: text coerces in arithmetic.
        assert_eq!(
            add(&Value::Text("2".into()), &Value::Num(1.0)),
            Value::Num(3.0)
        );
        // TRUE+1 = 2.
        assert_eq!(add(&Value::Bool(true), &Value::Num(1.0)), Value::Num(2.0));
        // Blank+1 = 1.
        assert_eq!(add(&Value::Blank, &Value::Num(1.0)), Value::Num(1.0));
        // "abc"+1 = #VALUE!.
        assert_eq!(
            add(&Value::Text("abc".into()), &Value::Num(1.0)),
            Value::Err(ExcelError::Value)
        );
        // " 2.5 "+0 parses with whitespace; "50%"+0 = 0.5; "$3"+0 = 3;
        // "1,234"+0 = 1234.
        assert_eq!(
            add(&Value::Text(" 2.5 ".into()), &Value::Num(0.0)),
            Value::Num(2.5)
        );
        assert_eq!(
            add(&Value::Text("50%".into()), &Value::Num(0.0)),
            Value::Num(0.5)
        );
        assert_eq!(
            add(&Value::Text("$3".into()), &Value::Num(0.0)),
            Value::Num(3.0)
        );
        assert_eq!(
            add(&Value::Text("1,234".into()), &Value::Num(0.0)),
            Value::Num(1234.0)
        );
    }

    #[test]
    fn division_and_pow_errors() {
        assert_eq!(
            div(&Value::Num(1.0), &Value::Num(0.0)),
            Value::Err(ExcelError::Div0)
        );
        assert_eq!(
            div(&Value::Num(1.0), &Value::Blank),
            Value::Err(ExcelError::Div0)
        );
        assert_eq!(
            pow(&Value::Num(0.0), &Value::Num(0.0)),
            Value::Err(ExcelError::Num)
        );
        assert_eq!(
            pow(&Value::Num(-8.0), &Value::Num(0.5)),
            Value::Err(ExcelError::Num)
        );
        assert_eq!(pow(&Value::Num(-8.0), &Value::Num(2.0)), Value::Num(64.0));
    }

    #[test]
    fn error_propagation_left_first() {
        let na = Value::Err(ExcelError::NA);
        let dv = Value::Err(ExcelError::Div0);
        assert_eq!(add(&na, &dv), Value::Err(ExcelError::NA));
        assert_eq!(add(&dv, &na), Value::Err(ExcelError::Div0));
    }

    #[test]
    fn comparison_no_text_coercion() {
        // "2" > 1000 is TRUE: Text always outranks Number.
        assert_eq!(
            compare(&Value::Text("2".into()), &Value::Num(1000.0)).unwrap(),
            Ordering::Greater
        );
        // TRUE > "zzz": Bool outranks Text.
        assert_eq!(
            compare(&Value::Bool(true), &Value::Text("zzz".into())).unwrap(),
            Ordering::Greater
        );
    }

    #[test]
    fn comparison_case_insensitive() {
        assert_eq!(
            compare(&Value::Text("Apple".into()), &Value::Text("aPPLE".into())).unwrap(),
            Ordering::Equal
        );
    }

    #[test]
    fn blank_equals_zero_and_empty_string() {
        assert_eq!(
            compare(&Value::Blank, &Value::Num(0.0)).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            compare(&Value::Blank, &Value::Text(String::new())).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            compare(&Value::Blank, &Value::Bool(false)).unwrap(),
            Ordering::Equal
        );
        // But Blank is NOT equal to "0"-as-text.
        assert_eq!(
            compare(&Value::Blank, &Value::Text("0".into())).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn concat_semantics() {
        assert_eq!(
            concat(&Value::Num(1.0), &Value::Text("x".into())),
            Value::Text("1x".into())
        );
        assert_eq!(
            concat(&Value::Blank, &Value::Text("x".into())),
            Value::Text("x".into())
        );
        assert_eq!(
            concat(&Value::Bool(true), &Value::Blank),
            Value::Text("TRUE".into())
        );
        // 0.1 concats as "0.1", not "0.1000000000000000055…".
        assert_eq!(
            concat(&Value::Num(0.1), &Value::Blank),
            Value::Text("0.1".into())
        );
    }

    #[test]
    fn percent_and_neg() {
        assert_eq!(percent(&Value::Num(50.0)), Value::Num(0.5));
        assert_eq!(neg(&Value::Text("2".into())), Value::Num(-2.0));
        // Unary plus leaves text alone.
        assert_eq!(pos(&Value::Text("abc".into())), Value::Text("abc".into()));
    }

    #[test]
    fn general_format() {
        assert_eq!(format_general(3.0), "3");
        assert_eq!(format_general(-3.0), "-3");
        assert_eq!(format_general(0.5), "0.5");
        assert_eq!(format_general(1234567890.0), "1234567890");
        // 15-significant-digit cap.
        assert_eq!(format_general(1.0 / 3.0), "0.333333333333333");
    }
}
