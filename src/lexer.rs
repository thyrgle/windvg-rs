//! Tokenizer for `.wvg` sources (spec §3).

use crate::ir::Diag;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(f64),
    /// Normalized to 6 or 8 uppercase hex digits.
    Hex(String),
    /// Double-quoted string with `\"` and `\\` escapes (spec v4 §3).
    Str(String),
    LP,
    RP,
    LB,
    RB,
    LC,
    RC,
    Comma,
    Eq,
    At,
    Percent,
    Plus,
    Minus,
    Star,
    Slash,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
}

impl Token {
    pub fn describe(&self) -> String {
        match &self.tok {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Num(v) => format!("number {v}"),
            Tok::Hex(h) => format!("`#{h}`"),
            Tok::Str(_) => "string".into(),
            Tok::LP => "`(`".into(),
            Tok::RP => "`)`".into(),
            Tok::LB => "`[`".into(),
            Tok::RB => "`]`".into(),
            Tok::LC => "`{`".into(),
            Tok::RC => "`}`".into(),
            Tok::Comma => "`,`".into(),
            Tok::Eq => "`=`".into(),
            Tok::At => "`@`".into(),
            Tok::Percent => "`%`".into(),
            Tok::Plus => "`+`".into(),
            Tok::Minus => "`-`".into(),
            Tok::Star => "`*`".into(),
            Tok::Slash => "`/`".into(),
            Tok::Eof => "end of file".into(),
        }
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, Diag> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;

    macro_rules! push {
        ($tok:expr) => {{
            out.push(Token {
                tok: $tok,
                line,
                col,
            });
        }};
    }

    while i < b.len() {
        let c = b[i];
        match c {
            ' ' | '\t' | '\r' => {
                i += 1;
                col += 1;
            }
            '\n' => {
                i += 1;
                line += 1;
                col = 1;
            }
            '/' if i + 1 < b.len() && b[i + 1] == '/' => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            '/' => {
                push!(Tok::Slash);
                i += 1;
                col += 1;
            }
            '*' => {
                push!(Tok::Star);
                i += 1;
                col += 1;
            }
            '(' => {
                push!(Tok::LP);
                i += 1;
                col += 1;
            }
            ')' => {
                push!(Tok::RP);
                i += 1;
                col += 1;
            }
            '[' => {
                push!(Tok::LB);
                i += 1;
                col += 1;
            }
            ']' => {
                push!(Tok::RB);
                i += 1;
                col += 1;
            }
            '{' => {
                push!(Tok::LC);
                i += 1;
                col += 1;
            }
            '}' => {
                push!(Tok::RC);
                i += 1;
                col += 1;
            }
            ',' => {
                push!(Tok::Comma);
                i += 1;
                col += 1;
            }
            '=' => {
                push!(Tok::Eq);
                i += 1;
                col += 1;
            }
            '@' => {
                push!(Tok::At);
                i += 1;
                col += 1;
            }
            '%' => {
                push!(Tok::Percent);
                i += 1;
                col += 1;
            }
            '+' => {
                push!(Tok::Plus);
                i += 1;
                col += 1;
            }
            '-' => {
                push!(Tok::Minus);
                i += 1;
                col += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                let start_col = col;
                let mut got_digit = false;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                    col += 1;
                    got_digit = true;
                }
                if i < b.len() && b[i] == '.' {
                    i += 1;
                    col += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                        col += 1;
                        got_digit = true;
                    }
                }
                if !got_digit {
                    return Err(Diag::new("invalid number", line, start_col));
                }
                if i < b.len() && (b[i] == 'e' || b[i] == 'E') {
                    let mut j = i + 1;
                    if j < b.len() && (b[j] == '+' || b[j] == '-') {
                        j += 1;
                    }
                    if j < b.len() && b[j].is_ascii_digit() {
                        while j < b.len() && b[j].is_ascii_digit() {
                            j += 1;
                        }
                        col += (j - i) as u32;
                        i = j;
                    }
                }
                let text: String = b[start..i].iter().collect();
                let v: f64 = text
                    .parse()
                    .map_err(|_| Diag::new("invalid number", line, start_col))?;
                if !v.is_finite() {
                    return Err(Diag::new("number out of range", line, start_col));
                }
                push!(Tok::Num(v));
            }
            '#' => {
                let start_col = col;
                i += 1;
                col += 1;
                let mut digits = String::new();
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    digits.push(b[i].to_ascii_uppercase());
                    i += 1;
                    col += 1;
                }
                match digits.len() {
                    3 => {
                        let e: String = digits.chars().flat_map(|ch| [ch, ch]).collect();
                        digits = e;
                    }
                    6 | 8 => {}
                    _ => {
                        return Err(Diag::new(
                            "invalid hex color: expected 3, 6, or 8 digits",
                            line,
                            start_col,
                        ));
                    }
                }
                push!(Tok::Hex(digits));
            }
            '"' => {
                let start_col = col;
                i += 1;
                col += 1;
                let mut text = String::new();
                loop {
                    let Some(c) = b.get(i) else {
                        return Err(Diag::new("unterminated string", line, start_col));
                    };
                    match c {
                        '"' => {
                            i += 1;
                            col += 1;
                            break;
                        }
                        '\\' => {
                            let Some(&esc) = b.get(i + 1) else {
                                return Err(Diag::new("unterminated string", line, start_col));
                            };
                            match esc {
                                '"' | '\\' => text.push(esc),
                                other => {
                                    return Err(Diag::new(
                                        format!("invalid string escape `\\{other}`"),
                                        line,
                                        col,
                                    ));
                                }
                            }
                            i += 2;
                            col += 2;
                        }
                        '\n' => {
                            return Err(Diag::new(
                                "unterminated string (newline in string literal)",
                                line,
                                col,
                            ));
                        }
                        other => {
                            text.push(*other);
                            i += 1;
                            col += 1;
                        }
                    }
                }
                push!(Tok::Str(text));
            }
            c if c == '_' || c == '~' || c.is_ascii_alphabetic() => {
                let start = i;
                while i < b.len()
                    && (b[i] == '_' || b[i] == '~' || b[i].is_ascii_alphanumeric())
                {
                    i += 1;
                    col += 1;
                }
                let name: String = b[start..i].iter().collect();
                push!(Tok::Ident(name));
            }
            other => {
                return Err(Diag::new(
                    format!("unexpected character `{other}`"),
                    line,
                    col,
                ));
            }
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}
