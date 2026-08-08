//! Lexer for Excel formula text (§8.2). Whitespace is a real token — a
//! single space between references is the intersection operator. Bracket
//! runs (`[...]`, with nesting and `'`-escapes) are lexed as one blob;
//! the parser decides whether a blob is a table spec, an external-workbook
//! prefix, or `[@Col]`.

use crate::ast::ErrorLit;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Numeric literal, verbatim lexeme (`1.50`, `.5`, `1E+5`).
    Number(String),
    /// String literal, unescaped content.
    Str(String),
    /// Identifier-ish run: function names, cell refs, defined names,
    /// unquoted sheet names, TRUE/FALSE. Disambiguated by the parser.
    Ident(String),
    /// `'quoted sheet'` content, unescaped (may include `[Book]` prefix text).
    Quoted(String),
    /// Balanced `[...]` run, verbatim including outer brackets.
    Bracket(String),
    Error(ErrorLit),
    /// One run of whitespace (spaces/tabs/newlines), verbatim.
    Ws(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    Bang,
    Percent,
    Amp,
    Caret,
    Star,
    Slash,
    Plus,
    Minus,
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    Ne,
    At,
    Hash,
    Dollar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub pos: usize,
    pub msg: String,
}

/// Longest-match table for error literals (checked in this order).
const ERROR_LITERALS: &[&str] = &[
    "#GETTING_DATA",
    "#DIV/0!",
    "#VALUE!",
    "#SPILL!",
    "#NULL!",
    "#CALC!",
    "#NAME?",
    "#NUM!",
    "#REF!",
    "#N/A",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '\\'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?')
}

pub fn lex(src: &str) -> Result<Vec<Tok>, LexError> {
    let mut toks = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                let start = i;
                while i < n && matches!(chars[i], ' ' | '\t' | '\n' | '\r') {
                    i += 1;
                }
                toks.push(Tok::Ws(chars[start..i].iter().collect()));
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= n {
                        return Err(LexError { pos: i, msg: "unterminated string".into() });
                    }
                    if chars[i] == '"' {
                        if i + 1 < n && chars[i + 1] == '"' {
                            s.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                toks.push(Tok::Str(s));
            }
            '\'' => {
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= n {
                        return Err(LexError { pos: i, msg: "unterminated quoted name".into() });
                    }
                    if chars[i] == '\'' {
                        if i + 1 < n && chars[i + 1] == '\'' {
                            s.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                toks.push(Tok::Quoted(s));
            }
            '[' => {
                // Balanced bracket run with '-escape: `['[x]` escapes a
                // literal bracket inside structured-ref column names.
                let start = i;
                let mut depth = 0usize;
                loop {
                    if i >= n {
                        return Err(LexError { pos: start, msg: "unterminated bracket".into() });
                    }
                    match chars[i] {
                        '\'' if i + 1 < n => i += 2, // escape next char
                        '[' => {
                            depth += 1;
                            i += 1;
                        }
                        ']' => {
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => i += 1,
                    }
                }
                toks.push(Tok::Bracket(chars[start..i].iter().collect()));
            }
            '#' => {
                let rest: String = chars[i..].iter().collect();
                if let Some(lit) = ERROR_LITERALS.iter().find(|l| rest.starts_with(**l)) {
                    toks.push(Tok::Error(ErrorLit::parse(lit).unwrap()));
                    i += lit.chars().count();
                } else {
                    toks.push(Tok::Hash);
                    i += 1;
                }
            }
            '0'..='9' => {
                i = lex_number(&chars, i, &mut toks);
            }
            '.' if i + 1 < n && chars[i + 1].is_ascii_digit() => {
                i = lex_number(&chars, i, &mut toks);
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '{' => {
                toks.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                toks.push(Tok::RBrace);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            ';' => {
                toks.push(Tok::Semi);
                i += 1;
            }
            ':' => {
                toks.push(Tok::Colon);
                i += 1;
            }
            '!' => {
                toks.push(Tok::Bang);
                i += 1;
            }
            '%' => {
                toks.push(Tok::Percent);
                i += 1;
            }
            '&' => {
                toks.push(Tok::Amp);
                i += 1;
            }
            '^' => {
                toks.push(Tok::Caret);
                i += 1;
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                toks.push(Tok::Minus);
                i += 1;
            }
            '=' => {
                toks.push(Tok::Eq);
                i += 1;
            }
            '@' => {
                toks.push(Tok::At);
                i += 1;
            }
            '$' => {
                toks.push(Tok::Dollar);
                i += 1;
            }
            '<' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    toks.push(Tok::Le);
                    i += 2;
                } else if i + 1 < n && chars[i + 1] == '>' {
                    toks.push(Tok::Ne);
                    i += 2;
                } else {
                    toks.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    toks.push(Tok::Ge);
                    i += 2;
                } else {
                    toks.push(Tok::Gt);
                    i += 1;
                }
            }
            c if is_ident_start(c) => {
                let start = i;
                while i < n && is_ident_continue(chars[i]) {
                    i += 1;
                }
                toks.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            _ => {
                return Err(LexError { pos: i, msg: format!("unexpected character {c:?}") });
            }
        }
    }
    Ok(toks)
}

/// Numbers: digits, optional fraction, optional exponent. The lexeme is
/// kept verbatim. `1E5` is a number; `E5` alone is an Ident (cell ref).
fn lex_number(chars: &[char], mut i: usize, toks: &mut Vec<Tok>) -> usize {
    let n = chars.len();
    let start = i;
    while i < n && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i < n && chars[i] == '.' {
        i += 1;
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < n && (chars[i] == 'E' || chars[i] == 'e') {
        let mut j = i + 1;
        if j < n && (chars[j] == '+' || chars[j] == '-') {
            j += 1;
        }
        if j < n && chars[j].is_ascii_digit() {
            i = j;
            while i < n && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    toks.push(Tok::Number(chars[start..i].iter().collect()));
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(
            lex("SUM(A1:B2)*2").unwrap(),
            vec![
                Tok::Ident("SUM".into()),
                Tok::LParen,
                Tok::Ident("A1".into()),
                Tok::Colon,
                Tok::Ident("B2".into()),
                Tok::RParen,
                Tok::Star,
                Tok::Number("2".into()),
            ]
        );
    }

    #[test]
    fn number_lexemes_verbatim() {
        assert_eq!(lex("1.50").unwrap(), vec![Tok::Number("1.50".into())]);
        assert_eq!(lex(".5").unwrap(), vec![Tok::Number(".5".into())]);
        assert_eq!(lex("1E+5").unwrap(), vec![Tok::Number("1E+5".into())]);
        // E5 alone is an ident (a cell ref), 1E5 is one number.
        assert_eq!(lex("1E5").unwrap(), vec![Tok::Number("1E5".into())]);
        assert_eq!(lex("E5").unwrap(), vec![Tok::Ident("E5".into())]);
    }

    #[test]
    fn string_escapes() {
        assert_eq!(lex("\"a\"\"b\"").unwrap(), vec![Tok::Str("a\"b".into())]);
    }

    #[test]
    fn quoted_sheet() {
        assert_eq!(
            lex("'My Sheet'!A1").unwrap(),
            vec![Tok::Quoted("My Sheet".into()), Tok::Bang, Tok::Ident("A1".into())]
        );
        assert_eq!(lex("'It''s'!B2").unwrap()[0], Tok::Quoted("It's".into()));
    }

    #[test]
    fn error_literals() {
        assert_eq!(lex("#DIV/0!").unwrap(), vec![Tok::Error(ErrorLit::Div0)]);
        assert_eq!(lex("#N/A").unwrap(), vec![Tok::Error(ErrorLit::NA)]);
        // Spill postfix: A1# lexes as ident + hash.
        assert_eq!(lex("A1#").unwrap(), vec![Tok::Ident("A1".into()), Tok::Hash]);
    }

    #[test]
    fn nested_table_brackets() {
        assert_eq!(
            lex("Table1[[#Headers],[Col]]").unwrap(),
            vec![Tok::Ident("Table1".into()), Tok::Bracket("[[#Headers],[Col]]".into())]
        );
    }

    #[test]
    fn intersection_space_is_a_token() {
        assert_eq!(
            lex("A1:A5 B2:B6").unwrap(),
            vec![
                Tok::Ident("A1".into()),
                Tok::Colon,
                Tok::Ident("A5".into()),
                Tok::Ws(" ".into()),
                Tok::Ident("B2".into()),
                Tok::Colon,
                Tok::Ident("B6".into()),
            ]
        );
    }

    #[test]
    fn comparison_ops() {
        assert_eq!(lex("A1<>B1").unwrap()[1], Tok::Ne);
        assert_eq!(lex("A1<=B1").unwrap()[1], Tok::Le);
        assert_eq!(lex("A1>=B1").unwrap()[1], Tok::Ge);
    }

    #[test]
    fn unterminated_string_errors_not_panics() {
        assert!(lex("\"abc").is_err());
        assert!(lex("'abc").is_err());
        assert!(lex("Table1[Col").is_err());
    }
}
