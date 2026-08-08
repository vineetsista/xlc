//! The criteria mini-language shared by COUNTIF / SUMIF / COUNTIFS /
//! SUMIFS / AVERAGEIF(S) (§8.4).
//!
//! A criterion is a value. If it is text beginning with a comparison
//! operator (`>=`, `<=`, `<>`, `>`, `<`, `=`), the rest is the RHS
//! (parsed as number if possible, else compared as text). Otherwise it is
//! an equality test — where text containing `*` / `?` is a wildcard
//! pattern (case-insensitive, `~` escapes). Numbers match numeric cells;
//! text matches text case-insensitively; empty criteria match blank cells.

use crate::value::{parse_excel_number, Value};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
pub enum Criteria {
    /// Comparison against a number.
    CmpNum(CmpOp, f64),
    /// Comparison against text (case-insensitive).
    CmpText(CmpOp, String),
    /// Equality with a wildcard pattern (already lowercased).
    Wildcard(String),
    /// Plain equality with a value.
    EqValue(Value),
    /// Matches blank cells (empty criteria).
    Blank,
    /// `<>` with empty RHS: matches non-blank cells.
    NonBlank,
}

fn split_op(s: &str) -> Option<(CmpOp, &str)> {
    if let Some(r) = s.strip_prefix(">=") {
        Some((CmpOp::Ge, r))
    } else if let Some(r) = s.strip_prefix("<=") {
        Some((CmpOp::Le, r))
    } else if let Some(r) = s.strip_prefix("<>") {
        Some((CmpOp::Ne, r))
    } else if let Some(r) = s.strip_prefix('>') {
        Some((CmpOp::Gt, r))
    } else if let Some(r) = s.strip_prefix('<') {
        Some((CmpOp::Lt, r))
    } else if let Some(r) = s.strip_prefix('=') {
        Some((CmpOp::Eq, r))
    } else {
        None
    }
}

/// Any character that sends a pattern through the wildcard engine — `~`
/// counts because Excel processes escapes even when nothing is wildcarded
/// ("2~*3" matches the literal text "2*3").
pub fn has_wildcard(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('~')
}

pub fn parse_criteria(v: &Value) -> Criteria {
    match v {
        Value::Text(s) => {
            if let Some((op, rest)) = split_op(s) {
                if rest.is_empty() {
                    return match op {
                        CmpOp::Ne => Criteria::NonBlank,
                        CmpOp::Eq => Criteria::Blank,
                        // ">" with nothing compares against empty text.
                        _ => Criteria::CmpText(op, String::new()),
                    };
                }
                if let Some(n) = parse_excel_number(rest) {
                    return Criteria::CmpNum(op, n);
                }
                if (op == CmpOp::Eq || op == CmpOp::Ne) && has_wildcard(rest) {
                    // `=pat*` / `<>pat*` — wildcard (in)equality.
                    let w = Criteria::Wildcard(rest.to_lowercase());
                    return if op == CmpOp::Eq {
                        w
                    } else {
                        // Encode <>wildcard as CmpText(Ne, pattern) and let
                        // matches() route it through the wildcard engine.
                        Criteria::CmpText(CmpOp::Ne, rest.to_lowercase())
                    };
                }
                return Criteria::CmpText(op, rest.to_lowercase());
            }
            if s.is_empty() {
                return Criteria::Blank;
            }
            if has_wildcard(s) {
                return Criteria::Wildcard(s.to_lowercase());
            }
            if let Some(n) = parse_excel_number(s) {
                return Criteria::CmpNum(CmpOp::Eq, n);
            }
            Criteria::EqValue(Value::Text(s.to_lowercase()))
        }
        Value::Num(n) => Criteria::CmpNum(CmpOp::Eq, *n),
        Value::Bool(b) => Criteria::EqValue(Value::Bool(*b)),
        Value::Blank => Criteria::Blank,
        Value::Err(_) => Criteria::EqValue(v.clone()),
    }
}

/// Wildcard match, pattern already lowercased; `~` escapes `*`/`?`.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();

    // Iterative glob with backtracking on `*`.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_p, mut star_t) = (usize::MAX, 0usize);
    while ti < t.len() {
        let (lit, adv) = if pi < p.len() {
            match p[pi] {
                '~' if pi + 1 < p.len() => (Some(p[pi + 1]), 2),
                '*' => {
                    star_p = pi + 1;
                    star_t = ti;
                    pi += 1;
                    continue;
                }
                '?' => (None, 1),
                c => (Some(c), 1),
            }
        } else {
            (Some('\u{0}'), 0)
        };
        let matched = pi < p.len() && (lit.is_none() || lit == Some(t[ti]));
        if matched {
            pi += adv;
            ti += 1;
        } else if star_p != usize::MAX {
            pi = star_p;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    // Consume trailing stars.
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Does the wildcard pattern match some PREFIX of `text`? (SEARCH slides
/// this along the haystack.) Pattern must be lowercased by the caller;
/// text is lowercased here.
pub fn wildcard_prefix_match(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        if p.is_empty() {
            return true; // pattern exhausted — prefix matched
        }
        match p[0] {
            '~' if p.len() >= 2 => !t.is_empty() && t[0] == p[1] && go(&p[2..], &t[1..]),
            '*' => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            '?' => !t.is_empty() && go(&p[1..], &t[1..]),
            c => !t.is_empty() && t[0] == c && go(&p[1..], &t[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    go(&p, &t)
}

impl Criteria {
    pub fn matches(&self, v: &Value) -> bool {
        match self {
            Criteria::Blank => matches!(v, Value::Blank) || matches!(v, Value::Text(s) if s.is_empty()),
            Criteria::NonBlank => !matches!(v, Value::Blank),
            Criteria::CmpNum(op, rhs) => match v {
                Value::Num(x) => cmp_holds(x.partial_cmp(rhs), *op),
                // Equality criteria coerce numeric-looking text on the
                // range side: COUNTIF(range,"003607") matches text
                // '003607' (corpus-verified). Ordering comparisons stay
                // strict until the corpus says otherwise.
                Value::Text(s) if *op == CmpOp::Eq => {
                    parse_excel_number(s).is_some_and(|x| x == *rhs)
                }
                Value::Text(s) if *op == CmpOp::Ne => {
                    parse_excel_number(s).is_none_or(|x| x != *rhs)
                }
                _ => false,
            },
            Criteria::CmpText(op, rhs) => match v {
                Value::Text(s) => {
                    let lhs = s.to_lowercase();
                    if has_wildcard(rhs) && *op == CmpOp::Ne {
                        return !wildcard_match(rhs, s);
                    }
                    cmp_holds(Some(lhs.cmp(rhs)), *op)
                }
                // `<>text` also matches any non-text cell.
                _ => *op == CmpOp::Ne,
            },
            Criteria::Wildcard(pat) => match v {
                Value::Text(s) => wildcard_match(pat, s),
                _ => false,
            },
            Criteria::EqValue(rhs) => match (v, rhs) {
                (Value::Text(a), Value::Text(b)) => a.to_lowercase() == *b,
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::Err(a), Value::Err(b)) => a == b,
                _ => false,
            },
        }
    }
}

fn cmp_holds(ord: Option<Ordering>, op: CmpOp) -> bool {
    let Some(ord) = ord else { return false };
    match op {
        CmpOp::Eq => ord == Ordering::Equal,
        CmpOp::Ne => ord != Ordering::Equal,
        CmpOp::Lt => ord == Ordering::Less,
        CmpOp::Le => ord != Ordering::Greater,
        CmpOp::Gt => ord == Ordering::Greater,
        CmpOp::Ge => ord != Ordering::Less,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Criteria {
        parse_criteria(&Value::Text(s.into()))
    }

    #[test]
    fn numeric_comparisons() {
        assert!(c(">5").matches(&Value::Num(6.0)));
        assert!(!c(">5").matches(&Value::Num(5.0)));
        assert!(c(">=5").matches(&Value::Num(5.0)));
        assert!(c("<>3").matches(&Value::Num(4.0)));
        assert!(!c("<>3").matches(&Value::Num(3.0)));
        // Numeric criteria never match text cells.
        assert!(!c(">5").matches(&Value::Text("6".into())));
    }

    #[test]
    fn plain_number_equality() {
        let crit = parse_criteria(&Value::Num(5.0));
        assert!(crit.matches(&Value::Num(5.0)));
        assert!(!crit.matches(&Value::Num(5.5)));
        // "5" as criteria text matches number 5.
        assert!(c("5").matches(&Value::Num(5.0)));
        // Leading-zero text criteria match same-valued text cells.
        assert!(c("003607").matches(&Value::Text("003607".into())));
        assert!(c("003607").matches(&Value::Text("3607".into())));
        assert!(c("003607").matches(&Value::Num(3607.0)));
        assert!(!c("003607").matches(&Value::Text("36070".into())));
        assert!(c("<>3607").matches(&Value::Text("abc".into())));
        assert!(!c("<>3607").matches(&Value::Text("003607".into())));
    }

    #[test]
    fn text_equality_case_insensitive() {
        assert!(c("apple").matches(&Value::Text("APPLE".into())));
        assert!(!c("apple").matches(&Value::Text("apples".into())));
    }

    #[test]
    fn wildcards() {
        assert!(c("a*e").matches(&Value::Text("Apple".into())));
        assert!(c("a?c").matches(&Value::Text("abc".into())));
        assert!(!c("a?c").matches(&Value::Text("abbc".into())));
        assert!(c("*").matches(&Value::Text("anything".into())));
        assert!(!c("*").matches(&Value::Num(5.0)));
        // ~* is a literal asterisk.
        assert!(c("2~*3").matches(&Value::Text("2*3".into())));
        assert!(!c("2~*3").matches(&Value::Text("213".into())));
        assert!(c("<>a*").matches(&Value::Text("bob".into())));
        assert!(!c("<>a*").matches(&Value::Text("alice".into())));
    }

    #[test]
    fn blank_and_nonblank() {
        assert!(c("").matches(&Value::Blank));
        assert!(c("=").matches(&Value::Blank));
        assert!(c("<>").matches(&Value::Num(1.0)));
        assert!(!c("<>").matches(&Value::Blank));
    }
}
