//! Text-level formula scanning utilities shared by the census, the
//! receipt's volatile check, the capability report, and the wasm surface.
//! These operate on raw formula TEXT (no parse) — cheap and total.

use std::collections::BTreeSet;

/// Marker token for a formula referencing an external workbook.
pub const EXTREF: &str = "[EXTREF]";

/// Nondeterministic volatile functions: their cached values are snapshots
/// (save-time clock or randomness) and can never be re-derived.
pub const VOLATILE_NONDETERMINISTIC: [&str; 5] =
    ["NOW", "TODAY", "RAND", "RANDBETWEEN", "RANDARRAY"];

/// Strip string literals ("..", with "" escape) and quoted sheet names
/// ('..', with '' escape) so their contents never look like function calls.
pub fn strip_quoted(formula: &str) -> String {
    let mut out = String::with_capacity(formula.len());
    let mut chars = formula.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(c2) = chars.next() {
                    if c2 == '"' {
                        if chars.peek() == Some(&'"') {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                out.push_str("\"\"");
            }
            '\'' => {
                while let Some(c2) = chars.next() {
                    if c2 == '\'' {
                        if chars.peek() == Some(&'\'') {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                out.push_str("''");
            }
            _ => out.push(c),
        }
    }
    out
}

/// Extract the set of function names called in one formula.
/// A function call is an identifier immediately followed by `(` (with
/// optional whitespace). Modern functions are stored with `_xlfn.` /
/// `_xlws.` prefixes in the file format; those are stripped.
pub fn extract_functions(formula: &str, out: &mut BTreeSet<String>) {
    let cleaned = strip_quoted(formula);
    let b = cleaned.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() {
                let c2 = b[i] as char;
                if c2.is_ascii_alphanumeric() || c2 == '_' || c2 == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let mut j = i;
            while j < b.len() && (b[j] as char).is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'(' {
                let mut name = cleaned[start..i].to_ascii_uppercase();
                while let Some(rest) = name
                    .strip_prefix("_XLFN.")
                    .or_else(|| name.strip_prefix("_XLWS."))
                {
                    name = rest.to_string();
                }
                if !name.is_empty() {
                    out.insert(name);
                }
            }
        } else {
            i += 1;
        }
    }
    // External-workbook reference heuristic (exact in Phase 2 when the real
    // parser lands): `[N]Sheet!` index form, or a bracketed *.xls* path.
    if external_ref_heuristic(&cleaned) {
        out.insert(EXTREF.to_string());
    }
}

pub fn external_ref_heuristic(cleaned: &str) -> bool {
    let b = cleaned.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'[' {
            let mut j = i + 1;
            while j < b.len() && (b[j] as char).is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < b.len() && b[j] == b']' {
                return true;
            }
        }
        i += 1;
    }
    false
}
