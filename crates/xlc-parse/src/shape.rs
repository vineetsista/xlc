//! Formula shape normalization (§8.8 foundation).
//!
//! A formula's SHAPE is its AST printed with every relative reference
//! rewritten as an offset from the host cell (R1C1-style) and trivia
//! dropped. Cells produced by copying one formula across a region have
//! identical shapes; a deviating cell is exactly a shape mismatch.
//!
//! Two granularities:
//!   - `full_shape`: ranges concrete (offsets included) — equality means
//!     "the same formula, faithfully copied".
//!   - `struct_shape`: ranges replaced by ® placeholders, with the range
//!     descriptors returned separately — lets the off-by-one detector
//!     compare families whose only difference is one range boundary.

use crate::ast::{Area, Coord, Expr, RefExpr, SheetPrefix, UnOp};

/// One axis of a normalized range boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Relative: offset from host.
    Rel(i64),
    /// Anchored: absolute 0-based index.
    Abs(u32),
}

impl Axis {
    fn print(self, letter: char, out: &mut String) {
        match self {
            Axis::Rel(0) => out.push(letter),
            Axis::Rel(d) => {
                out.push(letter);
                out.push('[');
                out.push_str(&d.to_string());
                out.push(']');
            }
            Axis::Abs(i) => {
                out.push(letter);
                out.push_str(&(i + 1).to_string());
            }
        }
    }

    /// Distance in cells between two boundaries of the same kind.
    pub fn diff(self, other: Axis) -> Option<i64> {
        match (self, other) {
            (Axis::Rel(a), Axis::Rel(b)) => Some(a - b),
            (Axis::Abs(a), Axis::Abs(b)) => Some(a as i64 - b as i64),
            _ => None,
        }
    }
}

/// A normalized rectangular range (or single cell when degenerate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeDesc {
    pub sheet: Option<String>,
    pub r0: Axis,
    pub c0: Axis,
    pub r1: Axis,
    pub c1: Axis,
    /// True when the source was a single cell reference.
    pub single: bool,
}

impl RangeDesc {
    pub fn print(&self, out: &mut String) {
        if let Some(s) = &self.sheet {
            out.push_str(s);
            out.push('!');
        }
        self.r0.print('R', out);
        self.c0.print('C', out);
        if !self.single {
            out.push(':');
            self.r1.print('R', out);
            self.c1.print('C', out);
        }
    }
}

pub struct Shapes {
    pub full: String,
    pub structural: String,
    pub ranges: Vec<RangeDesc>,
    /// Formula references a #REF! (deleted cell or sheet).
    pub has_ref_error: bool,
}

pub fn shapes(e: &Expr, host_row: u32, host_col: u32) -> Shapes {
    let mut s = Shaper {
        host_row,
        host_col,
        full: String::new(),
        structural: String::new(),
        ranges: Vec::new(),
        has_ref_error: false,
    };
    s.walk(e);
    Shapes { full: s.full, structural: s.structural, ranges: s.ranges, has_ref_error: s.has_ref_error }
}

struct Shaper {
    host_row: u32,
    host_col: u32,
    full: String,
    structural: String,
    ranges: Vec<RangeDesc>,
    has_ref_error: bool,
}

impl Shaper {
    fn push(&mut self, s: &str) {
        self.full.push_str(s);
        self.structural.push_str(s);
    }

    fn axis_row(&self, c: &Coord) -> Axis {
        if c.row_anchored {
            Axis::Abs(c.row)
        } else {
            Axis::Rel(c.row as i64 - self.host_row as i64)
        }
    }

    fn axis_col(&self, c: &Coord) -> Axis {
        if c.col_anchored {
            Axis::Abs(c.col)
        } else {
            Axis::Rel(c.col as i64 - self.host_col as i64)
        }
    }

    fn push_range(&mut self, d: RangeDesc) {
        let mut printed = String::new();
        d.print(&mut printed);
        self.full.push_str(&printed);
        self.structural.push('®');
        self.structural.push_str(&self.ranges.len().to_string());
        self.ranges.push(d);
    }

    fn sheet_str(sheet: &Option<SheetPrefix>) -> Option<String> {
        sheet.as_ref().map(|sp| {
            let mut s = String::new();
            if let Some(wb) = &sp.workbook {
                s.push('[');
                s.push_str(wb);
                s.push(']');
            }
            s.push_str(&sp.first.to_uppercase());
            if let Some(last) = &sp.last {
                s.push(':');
                s.push_str(&last.to_uppercase());
            }
            s
        })
    }

    fn walk(&mut self, e: &Expr) {
        match e {
            Expr::Number { lexeme, .. } => self.push(lexeme),
            Expr::Text(t) => {
                self.push("\"");
                self.push(t);
                self.push("\"");
            }
            Expr::Bool { value, .. } => self.push(if *value { "TRUE" } else { "FALSE" }),
            Expr::Error(err) => {
                if matches!(err, crate::ast::ErrorLit::Ref) {
                    self.has_ref_error = true;
                }
                self.push(err.as_str());
            }
            Expr::Ref(RefExpr::Area { sheet, area }) => {
                if sheet.as_ref().is_some_and(|sp| sp.first == "#REF") {
                    self.has_ref_error = true;
                }
                let sheet_s = Self::sheet_str(sheet);
                match area {
                    Area::Cell(c) => {
                        let d = RangeDesc {
                            sheet: sheet_s,
                            r0: self.axis_row(c),
                            c0: self.axis_col(c),
                            r1: self.axis_row(c),
                            c1: self.axis_col(c),
                            single: true,
                        };
                        self.push_range(d);
                    }
                    Area::CellRange(a, b) => {
                        let d = RangeDesc {
                            sheet: sheet_s,
                            r0: self.axis_row(a),
                            c0: self.axis_col(a),
                            r1: self.axis_row(b),
                            c1: self.axis_col(b),
                            single: false,
                        };
                        self.push_range(d);
                    }
                    Area::Cols { first, last, first_anchored, last_anchored } => {
                        let c0 = if *first_anchored {
                            Axis::Abs(*first)
                        } else {
                            Axis::Rel(*first as i64 - self.host_col as i64)
                        };
                        let c1 = if *last_anchored {
                            Axis::Abs(*last)
                        } else {
                            Axis::Rel(*last as i64 - self.host_col as i64)
                        };
                        let d = RangeDesc {
                            sheet: sheet_s,
                            r0: Axis::Abs(u32::MAX),
                            c0,
                            r1: Axis::Abs(u32::MAX),
                            c1,
                            single: false,
                        };
                        self.push_range(d);
                    }
                    Area::Rows { first, last, first_anchored, last_anchored } => {
                        let r0 = if *first_anchored {
                            Axis::Abs(*first)
                        } else {
                            Axis::Rel(*first as i64 - self.host_row as i64)
                        };
                        let r1 = if *last_anchored {
                            Axis::Abs(*last)
                        } else {
                            Axis::Rel(*last as i64 - self.host_row as i64)
                        };
                        let d = RangeDesc {
                            sheet: sheet_s,
                            r0,
                            c0: Axis::Abs(u32::MAX),
                            r1,
                            c1: Axis::Abs(u32::MAX),
                            single: false,
                        };
                        self.push_range(d);
                    }
                    Area::RefError => {
                        self.has_ref_error = true;
                        self.push("#REF!");
                    }
                }
            }
            Expr::Ref(RefExpr::Table(t)) => {
                self.push(&t.table.to_uppercase());
                self.push(&t.spec);
            }
            Expr::Name { sheet, name } => {
                if let Some(s) = Self::sheet_str(sheet) {
                    self.push(&s);
                    self.push("!");
                }
                self.push(&name.to_uppercase());
            }
            Expr::Call { name, args } => {
                self.push(&name.to_uppercase());
                self.push("(");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.push(",");
                    }
                    if let Some(e) = &a.expr {
                        self.walk(e);
                    }
                }
                self.push(")");
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                self.walk(lhs);
                self.push(op.as_str());
                self.walk(rhs);
            }
            Expr::Unary { op, expr, .. } => match op {
                UnOp::Neg => {
                    self.push("-");
                    self.walk(expr);
                }
                UnOp::Pos => {
                    self.push("+");
                    self.walk(expr);
                }
                UnOp::Percent => {
                    self.walk(expr);
                    self.push("%");
                }
                UnOp::ImplicitIntersect => {
                    self.push("@");
                    self.walk(expr);
                }
                UnOp::SpillRange => {
                    self.walk(expr);
                    self.push("#");
                }
            },
            Expr::ArrayLit(rows) => {
                self.push("{");
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        self.push(";");
                    }
                    for (j, el) in row.iter().enumerate() {
                        if j > 0 {
                            self.push(",");
                        }
                        self.walk(&el.expr);
                    }
                }
                self.push("}");
            }
            Expr::Paren { inner, .. } => {
                self.push("(");
                self.walk(inner);
                self.push(")");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;

    fn full(f: &str, row: u32, col: u32) -> String {
        shapes(&parse_formula(f).unwrap().expr, row, col).full
    }

    #[test]
    fn copies_share_shape() {
        // =A1*B1 in C1 has the same shape as =A2*B2 in C2.
        assert_eq!(full("A1*B1", 0, 2), full("A2*B2", 1, 2));
        // ...and a different shape when a ref drifts.
        assert_ne!(full("A1*B1", 0, 2), full("A1*B2", 1, 2));
    }

    #[test]
    fn anchors_are_absolute() {
        // $A$1 is the same shape from anywhere; A1 is not.
        assert_eq!(full("$A$1*2", 5, 5), full("$A$1*2", 9, 9));
        assert_ne!(full("A1*2", 5, 5), full("A1*2", 9, 9));
    }

    #[test]
    fn range_family_shape() {
        // SUM(D5:D20) in D21 == SUM(E5:E20) in E21.
        assert_eq!(full("SUM(D5:D20)", 20, 3), full("SUM(E5:E20)", 20, 4));
        // Off-by-one range differs in full shape but not structural shape.
        let a = shapes(&parse_formula("SUM(D5:D20)").unwrap().expr, 20, 3);
        let b = shapes(&parse_formula("SUM(D5:D19)").unwrap().expr, 20, 3);
        assert_ne!(a.full, b.full);
        assert_eq!(a.structural, b.structural);
        assert_eq!(a.ranges.len(), 1);
        let d = a.ranges[0].r1.diff(b.ranges[0].r1);
        assert_eq!(d, Some(1));
    }

    #[test]
    fn ref_error_flagged() {
        let s = shapes(&parse_formula("SUM(#REF!)").unwrap().expr, 0, 0);
        assert!(s.has_ref_error);
        let s = shapes(&parse_formula("#REF!A1+1").unwrap().expr, 0, 0);
        assert!(s.has_ref_error);
        let s = shapes(&parse_formula("A1+1").unwrap().expr, 0, 0);
        assert!(!s.has_ref_error);
    }

    #[test]
    fn case_insensitive_function_names() {
        assert_eq!(full("sum(A1:A5)", 9, 0), full("SUM(A1:A5)", 9, 0));
    }
}
