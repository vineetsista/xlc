//! AST for Excel formulas (§8.2). Built for byte-exact round-trip printing:
//! numeric literals keep their lexeme ("1.50" ≠ "1.5"), explicit parens are
//! preserved as nodes, and reference syntax keeps its original shape
//! (A1 vs R1C1, anchor positions, sheet-name quoting).

/// The seven-plus-two Excel error literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLit {
    Div0,    // #DIV/0!
    NA,      // #N/A
    Value,   // #VALUE!
    Ref,     // #REF!
    Name,    // #NAME?
    Num,     // #NUM!
    Null,    // #NULL!
    Spill,   // #SPILL!
    Calc,    // #CALC!
    GettingData, // #GETTING_DATA — appears in cached files
}

impl ErrorLit {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorLit::Div0 => "#DIV/0!",
            ErrorLit::NA => "#N/A",
            ErrorLit::Value => "#VALUE!",
            ErrorLit::Ref => "#REF!",
            ErrorLit::Name => "#NAME?",
            ErrorLit::Num => "#NUM!",
            ErrorLit::Null => "#NULL!",
            ErrorLit::Spill => "#SPILL!",
            ErrorLit::Calc => "#CALC!",
            ErrorLit::GettingData => "#GETTING_DATA",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "#DIV/0!" => ErrorLit::Div0,
            "#N/A" => ErrorLit::NA,
            "#VALUE!" => ErrorLit::Value,
            "#REF!" => ErrorLit::Ref,
            "#NAME?" => ErrorLit::Name,
            "#NUM!" => ErrorLit::Num,
            "#NULL!" => ErrorLit::Null,
            "#SPILL!" => ErrorLit::Spill,
            "#CALC!" => ErrorLit::Calc,
            "#GETTING_DATA" => ErrorLit::GettingData,
            _ => return None,
        })
    }
}

/// One axis of a cell coordinate: `$C$5` → both anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    /// 0-based row.
    pub row: u32,
    /// 0-based column.
    pub col: u32,
    pub row_anchored: bool,
    pub col_anchored: bool,
}

/// A sheet reference prefix: `Sheet1!`, `'My Sheet'!`, `Sheet1:Sheet3!`,
/// `[1]Sheet1!`, `[Book.xlsx]Sheet1!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetPrefix {
    /// External workbook: `[1]` index or `[Book.xlsx]` name, verbatim
    /// inner text without brackets.
    pub workbook: Option<String>,
    /// First (or only) sheet name, unquoted form.
    pub first: String,
    /// Second sheet for 3D refs (`Sheet1:Sheet3!`).
    pub last: Option<String>,
    /// Whether the original used single-quote quoting (preserved for
    /// round-trip; quoting is required iff the name demands it, but real
    /// files sometimes quote unnecessarily).
    pub quoted: bool,
}

/// A single area reference following an optional sheet prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Area {
    /// `C5`, `$C$5`
    Cell(Coord),
    /// `A1:B2` (both corners kept verbatim)
    CellRange(Coord, Coord),
    /// `A:C` whole columns; 0-based indices + anchors
    Cols { first: u32, last: u32, first_anchored: bool, last_anchored: bool },
    /// `5:9` whole rows
    Rows { first: u32, last: u32, first_anchored: bool, last_anchored: bool },
    /// `#REF!` after deletions — a ref-shaped error
    RefError,
}

/// Structured Table reference: `Table1[Amount]`,
/// `Table1[[#Headers],[Col]]`, `[@Col]` (bare, inside the table's own
/// column). Kept near-verbatim; semantic resolution happens at lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRef {
    /// External workbook prefix (`[1]!Table1[..]` form), inner text.
    pub workbook: Option<String>,
    /// Table name; empty for bare `[@Col]` form.
    pub table: String,
    /// The bracketed spec verbatim, including outer brackets.
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefExpr {
    Area { sheet: Option<SheetPrefix>, area: Area },
    Table(TableRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Reference operators (bind tightest)
    Range,      // :   (general form, e.g. INDEX(..):C5)
    Intersect,  // ' ' (single space)
    Union,      // ,   (only within parens)
    // Arithmetic
    Pow,        // ^
    Mul,        // *
    Div,        // /
    Add,        // +
    Sub,        // -
    // Text
    Concat,     // &
    // Comparison (bind loosest)
    Eq,         // =
    Ne,         // <>
    Lt,         // <
    Le,         // <=
    Gt,         // >
    Ge,         // >=
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Range => ":",
            BinOp::Intersect => " ",
            BinOp::Union => ",",
            BinOp::Pow => "^",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Concat => "&",
            BinOp::Eq => "=",
            BinOp::Ne => "<>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,      // -x
    Pos,      // +x
    Percent,  // x%  (postfix)
    ImplicitIntersect, // @x (postfix-prefix in modern dynamic-array formulas)
    SpillRange,        // x# (postfix: A1# spill reference)
}

/// An array literal element is a constant (number/text/bool/error, with
/// optional leading minus on numbers).
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal with its original lexeme for exact round-trip.
    Number { value: f64, lexeme: String },
    /// String literal, unescaped content ("" → ").
    Text(String),
    Bool { value: bool, lexeme: String },
    Error(ErrorLit),
    Ref(RefExpr),
    /// Defined name (workbook- or sheet-scoped resolved later), possibly
    /// sheet-qualified: `Sheet1!MyName`.
    Name { sheet: Option<SheetPrefix>, name: String },
    Call { name: String, args: Vec<CallArg> },
    /// ws_l sits before the operator, ws_r after. For Intersect the
    /// whitespace run IS the operator and lives in ws_l (ws_r empty).
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, ws_l: String, ws_r: String },
    /// Prefix ops: ws between op and operand. Postfix: ws before the op.
    Unary { op: UnOp, expr: Box<Expr>, ws: String },
    /// `{1,2;3,4}` — rows of constants.
    ArrayLit(Vec<Vec<ArrayElem>>),
    /// Explicit parentheses, preserved for round-trip.
    Paren { ws_open: String, inner: Box<Expr>, ws_close: String },
}

/// One call argument with surrounding whitespace: `IF(a , b)` keeps the
/// space before the comma as ws_after of the first argument.
#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub ws_before: String,
    pub expr: Option<Expr>,
    pub ws_after: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayElem {
    pub ws_before: String,
    pub expr: Expr,
    pub ws_after: String,
}

/// A parsed formula: the expression plus root-level whitespace trivia.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    pub ws_lead: String,
    pub expr: Expr,
    pub ws_trail: String,
}

impl Formula {
    pub fn to_formula_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.ws_lead);
        self.expr.print(&mut s);
        s.push_str(&self.ws_trail);
        s
    }
}

// ---- printing (the round-trip oracle's other half) ----

/// Convert a 0-based column index to letters (0 → A, 25 → Z, 26 → AA).
pub fn col_letters(mut col: u32) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// Parse column letters to a 0-based index. Returns None on overflow past
/// XFD is allowed lexically (validated at lowering, not here).
pub fn letters_col(s: &str) -> Option<u32> {
    let mut col: u32 = 0;
    for c in s.bytes() {
        if !c.is_ascii_uppercase() {
            return None;
        }
        col = col.checked_mul(26)?.checked_add((c - b'A') as u32 + 1)?;
    }
    col.checked_sub(1)
}

impl Coord {
    pub fn print(&self, out: &mut String) {
        if self.col_anchored {
            out.push('$');
        }
        out.push_str(&col_letters(self.col));
        if self.row_anchored {
            out.push('$');
        }
        out.push_str(&(self.row + 1).to_string());
    }
}

/// A sheet name needs quoting when it contains anything beyond
/// alphanumerics/underscore/dot (or starts with a digit, or looks like a
/// cell reference). We preserve the original `quoted` flag instead of
/// recomputing, for byte-exact round-trip.
impl SheetPrefix {
    pub fn print(&self, out: &mut String) {
        let quote = self.quoted;
        if quote {
            out.push('\'');
        }
        if let Some(wb) = &self.workbook {
            out.push('[');
            out.push_str(wb);
            out.push(']');
        }
        push_sheet_name(out, &self.first, quote);
        if let Some(last) = &self.last {
            out.push(':');
            push_sheet_name(out, last, quote);
        }
        if quote {
            out.push('\'');
        }
        out.push('!');
    }
}

fn push_sheet_name(out: &mut String, name: &str, quoted: bool) {
    if quoted {
        // Inside quotes, a literal quote doubles.
        for c in name.chars() {
            if c == '\'' {
                out.push('\'');
            }
            out.push(c);
        }
    } else {
        out.push_str(name);
    }
}

impl Area {
    pub fn print(&self, out: &mut String) {
        match self {
            Area::Cell(c) => c.print(out),
            Area::CellRange(a, b) => {
                a.print(out);
                out.push(':');
                b.print(out);
            }
            Area::Cols { first, last, first_anchored, last_anchored } => {
                if *first_anchored {
                    out.push('$');
                }
                out.push_str(&col_letters(*first));
                out.push(':');
                if *last_anchored {
                    out.push('$');
                }
                out.push_str(&col_letters(*last));
            }
            Area::Rows { first, last, first_anchored, last_anchored } => {
                if *first_anchored {
                    out.push('$');
                }
                out.push_str(&(first + 1).to_string());
                out.push(':');
                if *last_anchored {
                    out.push('$');
                }
                out.push_str(&(last + 1).to_string());
            }
            Area::RefError => out.push_str("#REF!"),
        }
    }
}

impl Expr {
    pub fn print(&self, out: &mut String) {
        match self {
            Expr::Number { lexeme, .. } => out.push_str(lexeme),
            Expr::Text(s) => {
                out.push('"');
                for c in s.chars() {
                    if c == '"' {
                        out.push('"');
                    }
                    out.push(c);
                }
                out.push('"');
            }
            Expr::Bool { lexeme, .. } => out.push_str(lexeme),
            Expr::Error(e) => out.push_str(e.as_str()),
            Expr::Ref(RefExpr::Area { sheet, area }) => {
                if let Some(s) = sheet {
                    s.print(out);
                }
                area.print(out);
            }
            Expr::Ref(RefExpr::Table(t)) => {
                if let Some(wb) = &t.workbook {
                    out.push('[');
                    out.push_str(wb);
                    out.push_str("]!");
                }
                out.push_str(&t.table);
                out.push_str(&t.spec);
            }
            Expr::Name { sheet, name } => {
                if let Some(s) = sheet {
                    s.print(out);
                }
                out.push_str(name);
            }
            Expr::Call { name, args } => {
                out.push_str(name);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&a.ws_before);
                    if let Some(e) = &a.expr {
                        e.print(out);
                    }
                    out.push_str(&a.ws_after);
                }
                out.push(')');
            }
            Expr::Binary { op, lhs, rhs, ws_l, ws_r } => {
                lhs.print(out);
                out.push_str(ws_l);
                if *op != BinOp::Intersect {
                    out.push_str(op.as_str());
                }
                out.push_str(ws_r);
                rhs.print(out);
            }
            Expr::Unary { op, expr, ws } => match op {
                UnOp::Neg => {
                    out.push('-');
                    out.push_str(ws);
                    expr.print(out);
                }
                UnOp::Pos => {
                    out.push('+');
                    out.push_str(ws);
                    expr.print(out);
                }
                UnOp::Percent => {
                    expr.print(out);
                    out.push_str(ws);
                    out.push('%');
                }
                UnOp::ImplicitIntersect => {
                    out.push('@');
                    out.push_str(ws);
                    expr.print(out);
                }
                UnOp::SpillRange => {
                    expr.print(out);
                    out.push_str(ws);
                    out.push('#');
                }
            },
            Expr::ArrayLit(rows) => {
                out.push('{');
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        out.push(';');
                    }
                    for (j, e) in row.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        out.push_str(&e.ws_before);
                        e.expr.print(out);
                        out.push_str(&e.ws_after);
                    }
                }
                out.push('}');
            }
            Expr::Paren { ws_open, inner, ws_close } => {
                out.push('(');
                out.push_str(ws_open);
                inner.print(out);
                out.push_str(ws_close);
                out.push(')');
            }
        }
    }

    pub fn to_formula_string(&self) -> String {
        let mut s = String::new();
        self.print(&mut s);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_letter_roundtrip() {
        for col in [0u32, 1, 25, 26, 27, 51, 52, 701, 702, 703, 16383] {
            assert_eq!(letters_col(&col_letters(col)), Some(col), "col {col}");
        }
        assert_eq!(col_letters(0), "A");
        assert_eq!(col_letters(25), "Z");
        assert_eq!(col_letters(26), "AA");
        assert_eq!(col_letters(701), "ZZ");
        assert_eq!(col_letters(702), "AAA");
        assert_eq!(col_letters(16383), "XFD");
    }

    #[test]
    fn print_anchors() {
        let mut s = String::new();
        Coord { row: 4, col: 2, row_anchored: true, col_anchored: true }.print(&mut s);
        assert_eq!(s, "$C$5");
    }

    #[test]
    fn print_text_escapes() {
        let e = Expr::Text("say \"hi\"".into());
        assert_eq!(e.to_formula_string(), "\"say \"\"hi\"\"\"");
    }
}
