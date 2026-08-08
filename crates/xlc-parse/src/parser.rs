//! Pratt parser for Excel formulas (§8.2).
//!
//! Excel precedence, loosest → tightest:
//!   comparisons (= <> < <= > >=) · & · + - · * / · ^ · % (postfix) ·
//!   unary - + (tighter than ^: -2^2 = 4) · reference ops (union `,`
//!   inside parens < intersection ` ` < range `:`) · @ · # (postfix)
//!
//! Whitespace is an operator (intersection) when it sits between two
//! expressions; decorative whitespace elsewhere is skipped (and therefore
//! not reproduced — such formulas count against Gate 2's 0.5% budget).

use crate::ast::*;
use crate::lexer::{lex, LexError, Tok};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub msg: String,
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError { msg: format!("lex error at {}: {}", e.pos, e.msg) }
    }
}

pub fn parse_formula(src: &str) -> Result<Expr, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0, paren_depth: 0 };
    p.skip_ws();
    let e = p.expr(0)?;
    p.skip_ws();
    if p.pos != p.toks.len() {
        return Err(ParseError { msg: format!("trailing tokens at {}", p.pos) });
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Depth of plain (non-call) parens — union commas are legal inside.
    paren_depth: u32,
}

// Binding powers.
const BP_CMP: (u8, u8) = (10, 11);
const BP_CONCAT: (u8, u8) = (20, 21);
const BP_ADD: (u8, u8) = (30, 31);
const BP_MUL: (u8, u8) = (40, 41);
const BP_POW: (u8, u8) = (50, 51);
const BP_PERCENT: u8 = 60; // postfix
const BP_UNARY: u8 = 70; // prefix - +
const BP_UNION: (u8, u8) = (84, 85);
const BP_ISECT: (u8, u8) = (90, 91);
const BP_RANGE: (u8, u8) = (100, 101);
const BP_AT: u8 = 95; // prefix @
const BP_SPILL: u8 = 110; // postfix #

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.pos + k)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(Tok::Ws(_))) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, t: &Tok, what: &str) -> Result<(), ParseError> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError { msg: format!("expected {what} at token {}", self.pos) })
        }
    }

    // ---- Pratt core ----

    fn expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.prefix()?;
        loop {
            // Whitespace in operator position: intersection if what follows
            // begins an expression, decorative otherwise.
            let mut la = 0usize;
            let mut saw_ws = false;
            while let Some(Tok::Ws(_)) = self.peek_at(la) {
                la += 1;
                saw_ws = true;
            }
            let Some(next) = self.peek_at(la) else {
                break;
            };
            if saw_ws && starts_expr(next) {
                if BP_ISECT.0 < min_bp {
                    break;
                }
                self.pos += la; // consume the whitespace run
                let rhs = self.expr(BP_ISECT.1)?;
                lhs = Expr::Binary { op: BinOp::Intersect, lhs: Box::new(lhs), rhs: Box::new(rhs) };
                continue;
            }
            let next = next.clone();
            match next {
                Tok::Percent => {
                    if BP_PERCENT < min_bp {
                        break;
                    }
                    self.pos += la + 1;
                    lhs = Expr::Unary { op: UnOp::Percent, expr: Box::new(lhs) };
                }
                Tok::Hash => {
                    if BP_SPILL < min_bp {
                        break;
                    }
                    self.pos += la + 1;
                    lhs = Expr::Unary { op: UnOp::SpillRange, expr: Box::new(lhs) };
                }
                Tok::Colon => {
                    if BP_RANGE.0 < min_bp {
                        break;
                    }
                    self.pos += la + 1;
                    self.skip_ws();
                    let rhs = self.expr(BP_RANGE.1)?;
                    lhs = Expr::Binary { op: BinOp::Range, lhs: Box::new(lhs), rhs: Box::new(rhs) };
                }
                Tok::Comma if self.paren_depth > 0 => {
                    // Union — only meaningful inside plain parens; in call
                    // args the Comma is consumed by the call loop first
                    // because call args reset paren_depth.
                    if BP_UNION.0 < min_bp {
                        break;
                    }
                    self.pos += la + 1;
                    self.skip_ws();
                    let rhs = self.expr(BP_UNION.1)?;
                    lhs = Expr::Binary { op: BinOp::Union, lhs: Box::new(lhs), rhs: Box::new(rhs) };
                }
                t => {
                    let Some((op, (lbp, rbp))) = infix_op(&t) else {
                        break;
                    };
                    if lbp < min_bp {
                        break;
                    }
                    self.pos += la + 1;
                    self.skip_ws();
                    let rhs = self.expr(rbp)?;
                    lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
                }
            }
        }
        Ok(lhs)
    }

    // ---- prefix position ----

    fn prefix(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        let Some(t) = self.peek().cloned() else {
            return Err(ParseError { msg: "unexpected end of formula".into() });
        };
        match t {
            Tok::Number(_) | Tok::Dollar => self.reference_or_number(),
            Tok::Str(s) => {
                self.pos += 1;
                Ok(Expr::Text(s))
            }
            Tok::Error(e) => {
                self.pos += 1;
                if e == ErrorLit::Ref {
                    // A deleted *sheet* prints as `#REF!A1` — the literal's
                    // own `!` doubles as the sheet separator. If an area
                    // follows immediately, `#REF!` is a dead sheet prefix
                    // (stored as first="#REF" so printing re-adds the `!`).
                    if self.peek().is_some_and(starts_expr) {
                        let sheet = SheetPrefix {
                            workbook: None,
                            first: "#REF".into(),
                            last: None,
                            quoted: false,
                        };
                        if let Ok(expr) = self.sheet_suffix(Some(sheet)) {
                            return Ok(expr);
                        }
                    }
                    // Otherwise: a standalone deleted-cell area.
                    Ok(Expr::Ref(RefExpr::Area { sheet: None, area: Area::RefError }))
                } else {
                    Ok(Expr::Error(e))
                }
            }
            Tok::Minus => {
                self.pos += 1;
                let e = self.expr(BP_UNARY)?;
                Ok(Expr::Unary { op: UnOp::Neg, expr: Box::new(e) })
            }
            Tok::Plus => {
                self.pos += 1;
                let e = self.expr(BP_UNARY)?;
                Ok(Expr::Unary { op: UnOp::Pos, expr: Box::new(e) })
            }
            Tok::At => {
                self.pos += 1;
                let e = self.expr(BP_AT)?;
                Ok(Expr::Unary { op: UnOp::ImplicitIntersect, expr: Box::new(e) })
            }
            Tok::LParen => {
                self.pos += 1;
                self.paren_depth += 1;
                self.skip_ws();
                let inner = self.expr(0)?;
                self.skip_ws();
                self.expect(&Tok::RParen, ")")?;
                self.paren_depth -= 1;
                Ok(Expr::Paren(Box::new(inner)))
            }
            Tok::LBrace => self.array_literal(),
            Tok::Quoted(q) => {
                self.pos += 1;
                self.expect(&Tok::Bang, "! after quoted sheet name")?;
                let sheet = quoted_to_prefix(&q);
                self.sheet_suffix(Some(sheet))
            }
            Tok::Bracket(b) => {
                self.pos += 1;
                // `[1]Sheet1!A1` external prefix, or bare `[@Col]` /
                // `[Col]` structured ref inside a table's own column.
                if let Some(Tok::Ident(name)) = self.peek().cloned() {
                    if matches!(self.peek_at(1), Some(Tok::Bang)) {
                        self.pos += 2;
                        let sheet = SheetPrefix {
                            workbook: Some(b[1..b.len() - 1].to_string()),
                            first: name,
                            last: None,
                            quoted: false,
                        };
                        return self.sheet_suffix(Some(sheet));
                    }
                }
                Ok(Expr::Ref(RefExpr::Table(TableRef { table: String::new(), spec: b })))
            }
            Tok::Ident(_) => self.ident_prefix(),
            other => Err(ParseError { msg: format!("unexpected token {other:?}") }),
        }
    }

    /// Prefix position starting at an Ident: function call, sheet prefix,
    /// cell/col reference, TRUE/FALSE, defined name, or table ref.
    fn ident_prefix(&mut self) -> Result<Expr, ParseError> {
        let Some(Tok::Ident(name)) = self.peek().cloned() else { unreachable!() };

        // Sheet prefix: Ident! or Ident:Ident! (3D). Sheet names that look
        // like cell refs must be quoted in real formulas, so this wins.
        if matches!(self.peek_at(1), Some(Tok::Bang)) {
            self.pos += 2;
            let sheet = SheetPrefix { workbook: None, first: name, last: None, quoted: false };
            return self.sheet_suffix(Some(sheet));
        }
        if matches!(self.peek_at(1), Some(Tok::Colon)) {
            if let Some(Tok::Ident(second)) = self.peek_at(2).cloned() {
                if matches!(self.peek_at(3), Some(Tok::Bang)) {
                    self.pos += 4;
                    let sheet = SheetPrefix {
                        workbook: None,
                        first: name,
                        last: Some(second),
                        quoted: false,
                    };
                    return self.sheet_suffix(Some(sheet));
                }
            }
        }

        // Function call: Ident immediately followed by `(`.
        if matches!(self.peek_at(1), Some(Tok::LParen)) {
            self.pos += 2;
            return self.call_args(name);
        }

        // Structured table ref: Ident immediately followed by a bracket run.
        if let Some(Tok::Bracket(spec)) = self.peek_at(1).cloned() {
            self.pos += 2;
            return Ok(Expr::Ref(RefExpr::Table(TableRef { table: name, spec })));
        }

        // TRUE/FALSE literals (not followed by `(` — TRUE() handled above).
        if name.eq_ignore_ascii_case("TRUE") {
            self.pos += 1;
            return Ok(Expr::Bool(true));
        }
        if name.eq_ignore_ascii_case("FALSE") {
            self.pos += 1;
            return Ok(Expr::Bool(false));
        }

        self.sheet_suffix(None)
    }

    /// Parse the reference/name that follows an (optional) sheet prefix.
    fn sheet_suffix(&mut self, sheet: Option<SheetPrefix>) -> Result<Expr, ParseError> {
        if let Some(area) = self.try_area()? {
            return Ok(Expr::Ref(RefExpr::Area { sheet, area }));
        }
        // Defined name (possibly sheet-qualified).
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            self.pos += 1;
            return Ok(Expr::Name { sheet, name });
        }
        if sheet.is_some() {
            if let Some(Tok::Error(ErrorLit::Ref)) = self.peek() {
                self.pos += 1;
                return Ok(Expr::Ref(RefExpr::Area { sheet, area: Area::RefError }));
            }
            return Err(ParseError { msg: "expected reference after sheet prefix".into() });
        }
        Err(ParseError { msg: "expected reference or name".into() })
    }

    /// Try to consume an area reference at the current position:
    /// `A1`, `$A$1`, `A1:B2`, `A:C`, `1:3`, `$1:$3` — including all anchor
    /// placements. Returns Ok(None) if the tokens don't look like an area
    /// (position unchanged).
    fn try_area(&mut self) -> Result<Option<Area>, ParseError> {
        let save = self.pos;

        if let Some(first) = self.try_coord() {
            match first {
                CoordLike::Cell(a) => {
                    // A1:B2 — only bind the colon if the far side is
                    // cell-shaped; otherwise leave it for the Pratt loop.
                    if matches!(self.peek(), Some(Tok::Colon)) {
                        let save2 = self.pos;
                        self.pos += 1;
                        if let Some(CoordLike::Cell(b)) = self.try_coord() {
                            return Ok(Some(Area::CellRange(a, b)));
                        }
                        self.pos = save2;
                    }
                    return Ok(Some(Area::Cell(a)));
                }
                CoordLike::Col { idx, anchored } => {
                    // Whole-column: REQUIRES `:` + column.
                    if matches!(self.peek(), Some(Tok::Colon)) {
                        let save2 = self.pos;
                        self.pos += 1;
                        if let Some(CoordLike::Col { idx: last, anchored: la }) = self.try_coord() {
                            return Ok(Some(Area::Cols {
                                first: idx,
                                last,
                                first_anchored: anchored,
                                last_anchored: la,
                            }));
                        }
                        self.pos = save2;
                    }
                    self.pos = save;
                    return Ok(None);
                }
                CoordLike::Row { idx, anchored } => {
                    if matches!(self.peek(), Some(Tok::Colon)) {
                        let save2 = self.pos;
                        self.pos += 1;
                        if let Some(CoordLike::Row { idx: last, anchored: la }) = self.try_coord() {
                            return Ok(Some(Area::Rows {
                                first: idx,
                                last,
                                first_anchored: anchored,
                                last_anchored: la,
                            }));
                        }
                        self.pos = save2;
                    }
                    self.pos = save;
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    /// One coordinate-ish token group: `A1`, `$A$1`, `A`, `$A`, `1`, `$1`.
    fn try_coord(&mut self) -> Option<CoordLike> {
        let save = self.pos;
        let col_anchored = if matches!(self.peek(), Some(Tok::Dollar)) {
            self.pos += 1;
            true
        } else {
            false
        };
        match self.peek().cloned() {
            Some(Tok::Ident(s)) => {
                self.pos += 1;
                if let Some((letters, digits)) = split_cell_ident(&s) {
                    // `A1` or `$A1` in one ident.
                    let col = crate::ast::letters_col(letters)?;
                    let row: u32 = digits.parse().ok()?;
                    if !valid_cell(col, row) || digits.starts_with('0') {
                        self.pos = save;
                        return None;
                    }
                    return Some(CoordLike::Cell(Coord {
                        row: row - 1,
                        col,
                        row_anchored: false,
                        col_anchored,
                    }));
                }
                if s.bytes().all(|b| b.is_ascii_uppercase()) {
                    if let Some(col) = crate::ast::letters_col(&s) {
                        if col < 16_384 {
                            // `A$1` / `$A$1`: letters then $ then row digits.
                            if matches!(self.peek(), Some(Tok::Dollar)) {
                                if let Some(Tok::Number(d)) = self.peek_at(1).cloned() {
                                    if let Ok(row) = d.parse::<u32>() {
                                        if valid_cell(col, row) && !d.starts_with('0') {
                                            self.pos += 2;
                                            return Some(CoordLike::Cell(Coord {
                                                row: row - 1,
                                                col,
                                                row_anchored: true,
                                                col_anchored,
                                            }));
                                        }
                                    }
                                }
                                self.pos = save;
                                return None;
                            }
                            return Some(CoordLike::Col { idx: col, anchored: col_anchored });
                        }
                    }
                }
                self.pos = save;
                None
            }
            Some(Tok::Number(d)) => {
                if let Ok(row) = d.parse::<u32>() {
                    if (1..=1_048_576).contains(&row) && !d.starts_with('0') && !d.contains('.') {
                        self.pos += 1;
                        return Some(CoordLike::Row { idx: row - 1, anchored: col_anchored });
                    }
                }
                self.pos = save;
                None
            }
            _ => {
                self.pos = save;
                None
            }
        }
    }

    fn call_args(&mut self, name: String) -> Result<Expr, ParseError> {
        // Inside call args, plain-paren union rules don't apply until a
        // nested `(` — stash and reset depth.
        let saved_depth = self.paren_depth;
        self.paren_depth = 0;
        let mut args: Vec<Option<Expr>> = Vec::new();
        self.skip_ws();
        if matches!(self.peek(), Some(Tok::RParen)) {
            self.pos += 1;
            self.paren_depth = saved_depth;
            return Ok(Expr::Call { name, args });
        }
        loop {
            self.skip_ws();
            if matches!(self.peek(), Some(Tok::Comma)) {
                args.push(None); // omitted argument: IF(A1,,2)
                self.pos += 1;
                continue;
            }
            if matches!(self.peek(), Some(Tok::RParen)) {
                args.push(None);
                self.pos += 1;
                break;
            }
            let e = self.expr(0)?;
            args.push(Some(e));
            self.skip_ws();
            match self.peek() {
                Some(Tok::Comma) => {
                    self.pos += 1;
                }
                Some(Tok::RParen) => {
                    self.pos += 1;
                    break;
                }
                _ => {
                    self.paren_depth = saved_depth;
                    return Err(ParseError {
                        msg: format!("expected , or ) in args of {name} at token {}", self.pos),
                    });
                }
            }
        }
        self.paren_depth = saved_depth;
        Ok(Expr::Call { name, args })
    }

    fn array_literal(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Tok::LBrace, "{")?;
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();
        loop {
            self.skip_ws();
            let elem = self.array_element()?;
            row.push(elem);
            self.skip_ws();
            match self.bump() {
                Some(Tok::Comma) => {}
                Some(Tok::Semi) => {
                    rows.push(std::mem::take(&mut row));
                }
                Some(Tok::RBrace) => {
                    rows.push(row);
                    break;
                }
                other => {
                    return Err(ParseError { msg: format!("bad array literal near {other:?}") });
                }
            }
        }
        Ok(Expr::ArrayLit(rows))
    }

    /// Array elements are constants only: numbers (with optional sign),
    /// strings, booleans, errors.
    fn array_element(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
            Some(Tok::Number(x)) => Ok(number_expr(x)),
            Some(Tok::Minus) => match self.bump() {
                Some(Tok::Number(x)) => Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(number_expr(x)),
                }),
                other => Err(ParseError { msg: format!("bad array element after -: {other:?}") }),
            },
            Some(Tok::Str(s)) => Ok(Expr::Text(s)),
            Some(Tok::Error(e)) => Ok(Expr::Error(e)),
            Some(Tok::Ident(s)) if s.eq_ignore_ascii_case("TRUE") => Ok(Expr::Bool(true)),
            Some(Tok::Ident(s)) if s.eq_ignore_ascii_case("FALSE") => Ok(Expr::Bool(false)),
            other => Err(ParseError { msg: format!("bad array element: {other:?}") }),
        }
    }

    fn reference_or_number(&mut self) -> Result<Expr, ParseError> {
        // `$`-anchored coords, whole-row ranges (`1:3`), or a plain number.
        if let Some(area) = self.try_area()? {
            return Ok(Expr::Ref(RefExpr::Area { sheet: None, area }));
        }
        match self.bump() {
            Some(Tok::Number(x)) => Ok(number_expr(x)),
            other => Err(ParseError { msg: format!("expected number or reference, got {other:?}") }),
        }
    }
}

enum CoordLike {
    Cell(Coord),
    Col { idx: u32, anchored: bool },
    Row { idx: u32, anchored: bool },
}

fn number_expr(lexeme: String) -> Expr {
    let value: f64 = lexeme.parse().unwrap_or(f64::NAN);
    Expr::Number { value, lexeme }
}

/// Split `A1`-shaped identifiers into (letters, digits); both non-empty,
/// letters all uppercase A-Z, digits all ascii.
fn split_cell_ident(s: &str) -> Option<(&str, &str)> {
    let letters_end = s.bytes().take_while(|b| b.is_ascii_uppercase()).count();
    if letters_end == 0 || letters_end > 3 || letters_end == s.len() {
        return None;
    }
    let (letters, digits) = s.split_at(letters_end);
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        Some((letters, digits))
    } else {
        None
    }
}

fn valid_cell(col: u32, row_1based: u32) -> bool {
    col < 16_384 && (1..=1_048_576).contains(&row_1based)
}

fn infix_op(t: &Tok) -> Option<(BinOp, (u8, u8))> {
    Some(match t {
        Tok::Eq => (BinOp::Eq, BP_CMP),
        Tok::Ne => (BinOp::Ne, BP_CMP),
        Tok::Lt => (BinOp::Lt, BP_CMP),
        Tok::Le => (BinOp::Le, BP_CMP),
        Tok::Gt => (BinOp::Gt, BP_CMP),
        Tok::Ge => (BinOp::Ge, BP_CMP),
        Tok::Amp => (BinOp::Concat, BP_CONCAT),
        Tok::Plus => (BinOp::Add, BP_ADD),
        Tok::Minus => (BinOp::Sub, BP_ADD),
        Tok::Star => (BinOp::Mul, BP_MUL),
        Tok::Slash => (BinOp::Div, BP_MUL),
        Tok::Caret => (BinOp::Pow, BP_POW),
        _ => return None,
    })
}

/// Does this token begin an expression? (Used to distinguish intersection
/// whitespace from decorative whitespace.)
fn starts_expr(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Number(_)
            | Tok::Str(_)
            | Tok::Ident(_)
            | Tok::Quoted(_)
            | Tok::Bracket(_)
            | Tok::Error(_)
            | Tok::LParen
            | Tok::LBrace
            | Tok::Dollar
            | Tok::At
    )
}

/// Quoted sheet content: may embed a workbook prefix (`[Book.xlsx]Sheet`)
/// and a 3D span (`Sheet1:Sheet3` — colons are illegal in sheet names, so
/// splitting is safe).
fn quoted_to_prefix(q: &str) -> SheetPrefix {
    let (workbook, rest) = if let Some(stripped) = q.strip_prefix('[') {
        match stripped.split_once(']') {
            Some((wb, rest)) => (Some(wb.to_string()), rest),
            None => (None, q),
        }
    } else {
        (None, q)
    };
    let (first, last) = match rest.split_once(':') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (rest.to_string(), None),
    };
    SheetPrefix { workbook, first, last, quoted: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(src: &str) {
        let e = parse_formula(src).unwrap_or_else(|err| panic!("parse {src:?}: {err:?}"));
        assert_eq!(e.to_formula_string(), src, "round-trip of {src:?}");
    }

    #[test]
    fn round_trips() {
        for f in [
            "SUM(A1:A10)",
            "SUM($A$1:$A$10)*2",
            "IF(A1>0,B1,-C1)",
            "IF(A1,,2)",
            "A1+B2*C3^D4",
            "-2^2",
            "1.50%",
            "\"say \"\"hi\"\"\"&A1",
            "{1,2;3,4}",
            "{-1,\"x\";TRUE,#N/A}",
            "Sheet1!A1",
            "'My Sheet'!$B$2:C3",
            "Sheet1:Sheet3!A1",
            "'S1:S3 x'!A1",
            "SUM(Sheet1:Sheet3!A1:A9)",
            "[1]Names!X9",
            "Table1[Amount]",
            "Table1[[#Headers],[Col]]",
            "SUM(Table1[Amount])",
            "A:C",
            "$A:A",
            "3:9",
            "SUM(A:A)",
            "A1:A5 B2:B6",
            "(A1:A5,B2:B6)",
            "SUM((A1:A5,B2:B6))",
            "INDEX(A1:C9,1,1):C9",
            "@A1:A10",
            "A1#",
            "SUM(A1#)",
            "XLOOKUP(A1,B:B,C:C)",
            "_xlfn.XLOOKUP(A1,B:B,C:C)",
            "MyName*2",
            "Sheet1!MyName",
            "TRUE",
            "FALSE()",
            "#DIV/0!",
            "A1=#REF!",
            "#REF!A1",
            "#REF!#REF!",
            "SUM(#REF!A1:B2)",
            "1E+5*A1",
            ".5+A1",
            "SUM(A1,)",
        ] {
            rt(f);
        }
    }

    #[test]
    fn precedence_shapes() {
        // -2^2 = (-2)^2: unary binds tighter than ^.
        let e = parse_formula("-2^2").unwrap();
        match e {
            Expr::Binary { op: BinOp::Pow, lhs, .. } => {
                assert!(matches!(*lhs, Expr::Unary { op: UnOp::Neg, .. }));
            }
            other => panic!("bad shape: {other:?}"),
        }
        // 1+2*3: mul under add.
        let e = parse_formula("1+2*3").unwrap();
        match e {
            Expr::Binary { op: BinOp::Add, rhs, .. } => {
                assert!(matches!(*rhs, Expr::Binary { op: BinOp::Mul, .. }));
            }
            other => panic!("bad shape: {other:?}"),
        }
        // Concat looser than +: "a"&1+2 is "a"&(1+2).
        let e = parse_formula("\"a\"&1+2").unwrap();
        assert!(matches!(e, Expr::Binary { op: BinOp::Concat, .. }));
        // Comparison loosest.
        let e = parse_formula("A1+1>B1*2").unwrap();
        assert!(matches!(e, Expr::Binary { op: BinOp::Gt, .. }));
    }

    #[test]
    fn intersection_vs_decorative_ws() {
        // Between two refs: intersection.
        let e = parse_formula("A1:A5 B2:B6").unwrap();
        assert!(matches!(e, Expr::Binary { op: BinOp::Intersect, .. }));
        // Around an infix operator: decorative (drops on print).
        let e = parse_formula("A1 + 1").unwrap();
        assert_eq!(e.to_formula_string(), "A1+1");
    }

    #[test]
    fn union_only_in_parens() {
        assert!(parse_formula("(A1,B1)").is_ok());
        // At top level a bare comma is trailing garbage.
        assert!(parse_formula("A1,B1").is_err());
    }

    #[test]
    fn errors_not_panics() {
        for bad in ["", "SUM(", ")", "A1+", "{1,2", "'x!A1", "[Book", "1..2", "@"] {
            assert!(parse_formula(bad).is_err(), "should fail: {bad:?}");
        }
    }

    #[test]
    fn call_vs_name() {
        assert!(matches!(parse_formula("TRUE()").unwrap(), Expr::Call { .. }));
        assert!(matches!(parse_formula("TRUE").unwrap(), Expr::Bool(true)));
        // Space before ( is NOT a call (Excel normalizes; we stay strict).
        let e = parse_formula("SUM (A1)").unwrap();
        assert!(matches!(e, Expr::Binary { op: BinOp::Intersect, .. }));
    }
}
