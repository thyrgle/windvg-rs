//! Recursive-descent parser (spec §4–§5). Produces the document IR
//! directly: defaults are applied, `line`/`regular_polygon`/`star` are
//! desugared, paint names are expanded, and names are bound.

use std::collections::{HashMap, HashSet};

use crate::geom::Pt;
use crate::ir::*;
use crate::lexer::{lex, Tok, Token};

pub fn parse(src: &str) -> Result<Document, Diag> {
    let toks = lex(src)?;
    Parser::new(toks).parse_file()
}

fn tok_describe(t: &Tok) -> String {
    Token {
        tok: t.clone(),
        line: 0,
        col: 0,
    }
    .describe()
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    paints: HashMap<String, Paint>,
    names: HashSet<String>,
    defs: HashMap<String, PShape>,
    constants: HashMap<String, f64>,
    in_repeat: bool,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Parser {
        Parser {
            toks,
            pos: 0,
            paints: HashMap::new(),
            names: HashSet::new(),
            defs: HashMap::new(),
            constants: HashMap::new(),
            in_repeat: false,
        }
    }

    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn err_here(&self, msg: impl Into<String>) -> Diag {
        let t = self.peek();
        Diag::new(msg, t.line, t.col)
    }

    fn expect(&mut self, msg: &str, pred: impl Fn(&Tok) -> bool) -> Result<Token, Diag> {
        let t = self.next();
        if pred(&t.tok) {
            Ok(t)
        } else {
            Err(Diag::new(
                format!("expected {msg}, found {}", t.describe()),
                t.line,
                t.col,
            ))
        }
    }

    fn keyword(&mut self, kw: &str) -> Result<(), Diag> {
        let t = self.next();
        match &t.tok {
            Tok::Ident(s) if s == kw => Ok(()),
            _ => Err(Diag::new(
                format!("expected `{kw}`, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn peek_kw(&self, kw: &str) -> bool {
        matches!(&self.peek().tok, Tok::Ident(s) if s == kw)
    }

    /// True where a NUMBER may begin: a literal or a unary sign (the sign
    /// is an operator, so `-50%` and `-3` must pass this gate).
    fn peek_number(&self) -> bool {
        matches!(
            self.peek().tok,
            Tok::Num(_) | Tok::Minus | Tok::Plus | Tok::LP
        )
    }

    /// Any NUMBER position: a closed-form expression over literals and
    /// constants (spec §7.21). Signs are unary operators.
    fn number(&mut self) -> Result<f64, Diag> {
        self.expr()
    }

    fn expr(&mut self) -> Result<f64, Diag> {
        let mut v = self.number_term()?;
        loop {
            match self.peek().tok {
                Tok::Plus => {
                    self.next();
                    v += self.number_term()?;
                }
                Tok::Minus => {
                    self.next();
                    v -= self.number_term()?;
                }
                _ => return Ok(v),
            }
        }
    }

    /// A POSITIONAL number — one separated from its neighbors by spaces or
    /// punctuation (`(x, y)`, `50%`, `tangent 12 deg 90`, `translate 5 -3`,
    /// `seg 0 -50%`). Binary `+`/`-` would be ambiguous there (an adjacent
    /// value or a point offset), so positional slots accept a single *term*:
    /// literals, constants, unary signs, `*`/`/`, and parentheses for
    /// arithmetic. Only `key = expr` slots parse full expressions.
    fn number_term(&mut self) -> Result<f64, Diag> {
        self.term()
    }

    /// Positional integer (segment index): a single term, integral.
    fn integer_term(&mut self) -> Result<i64, Diag> {
        let t = self.peek().clone();
        let v = self.term()?;
        if v.fract() == 0.0 && v.abs() <= 9.0e15 {
            Ok(v as i64)
        } else {
            Err(Diag::new(
                format!("expected an integer, found {}", t.describe()),
                t.line,
                t.col,
            ))
        }
    }

    fn term(&mut self) -> Result<f64, Diag> {
        let mut v = self.factor()?;
        loop {
            match self.peek().tok {
                Tok::Star => {
                    self.next();
                    v *= self.factor()?;
                }
                Tok::Slash => {
                    self.next();
                    let d = self.factor()?;
                    if d == 0.0 {
                        let t = self.peek().clone();
                        return Err(Diag::new("division by zero", t.line, t.col));
                    }
                    v /= d;
                }
                _ => return Ok(v),
            }
        }
    }

    fn factor(&mut self) -> Result<f64, Diag> {
        let t = self.next();
        match t.tok {
            Tok::Num(v) => {
                if v.is_finite() {
                    Ok(v)
                } else {
                    Err(Diag::new("number out of range", t.line, t.col))
                }
            }
            Tok::Ident(name) => match self.constants.get(&name) {
                Some(v) => Ok(*v),
                None => Err(Diag::new(
                    format!(
                        "unknown constant `{name}` (constants must be declared with `let` before use)"
                    ),
                    t.line,
                    t.col,
                )),
            },
            Tok::LP => {
                let v = self.expr()?;
                self.expect("`)`", |t| *t == Tok::RP)?;
                Ok(v)
            }
            Tok::Minus => Ok(-self.factor()?),
            Tok::Plus => self.factor(),
            other => Err(Diag::new(
                format!("expected a number, found {}", Token { tok: other, line: t.line, col: t.col }.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn integer(&mut self) -> Result<i64, Diag> {
        let t = self.peek().clone();
        let v = self.number()?;
        if v.fract() == 0.0 && v.abs() <= 9.0e15 {
            Ok(v as i64)
        } else {
            Err(Diag::new(
                format!("expected an integer, found {}", t.describe()),
                t.line,
                t.col,
            ))
        }
    }

    /// `NUMBER "%"`.
    fn percent(&mut self) -> Result<f64, Diag> {
        let v = self.number_term()?;
        let t = self.next();
        if t.tok == Tok::Percent {
            Ok(v)
        } else {
            Err(Diag::new(
                format!("expected `%`, found {}", t.describe()),
                t.line,
                t.col,
            ))
        }
    }

    fn literal(&mut self) -> Result<(PPoint, (f64, f64)), Diag> {
        let t = self.expect("`(`", |t| *t == Tok::LP)?;
        let x = self.number_term()?;
        self.expect("`,`", |t| *t == Tok::Comma)?;
        let y = self.number_term()?;
        self.expect("`)`", |t| *t == Tok::RP)?;
        let pt = PPoint {
            kind: PKind::Literal(x, y),
            span: Span::new(t.line, t.col),
        };
        Ok((pt, (x, y)))
    }

    fn bind_name(&mut self, what: &str) -> Result<(String, Span), Diag> {
        let t = self.next();
        let name = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    format!("expected a name, found {}", t.describe()),
                    t.line,
                    t.col,
                ))
            }
        };
        if is_reserved(&name) {
            return Err(Diag::new(
                format!("`{name}` is a reserved word and cannot be a {what} name"),
                t.line,
                t.col,
            ));
        }
        if name.contains('~') && !self.in_repeat {
            return Err(Diag::new(
                "`~` in a name is only valid inside a repeat body".to_string(),
                t.line,
                t.col,
            ));
        }
        if self.names.contains(&name) {
            return Err(Diag::new(format!("duplicate name `{name}`"), t.line, t.col));
        }
        self.names.insert(name.clone());
        Ok((name, Span::new(t.line, t.col)))
    }

    // ---- top level ------------------------------------------------------

    /// One top-level statement. Returns Ok(false) at the end of input.
    fn parse_top_into(&mut self, nodes: &mut Vec<Node>) -> Result<bool, Diag> {
        match &self.peek().tok {
            Tok::Eof => Ok(false),
            Tok::Ident(s) => match s.as_str() {
                "paint" => {
                    self.parse_paint_decl()?;
                    Ok(true)
                }
                "let" => {
                    self.parse_let()?;
                    Ok(true)
                }
                "repeat" => {
                    self.parse_repeat(nodes)?;
                    Ok(true)
                }
                "fill" | "stroke" | "outline_fill" => {
                    nodes.push(self.parse_node()?);
                    Ok(true)
                }
                "text" => {
                    nodes.push(self.parse_text_node()?);
                    Ok(true)
                }
                "guide" => {
                    nodes.push(self.parse_guide()?);
                    Ok(true)
                }
                "group" => {
                    self.parse_group(nodes)?;
                    Ok(true)
                }
                "def" => {
                    self.parse_def()?;
                    Ok(true)
                }
                "scene" => Err(self.err_here("duplicate scene declaration")),
                _ => Err(self.err_here("expected a statement")),
            },
            _ => Err(self.err_here("expected a statement")),
        }
    }

    /// `let name = expr` — a named numeric constant (spec §7.21).
    fn parse_let(&mut self) -> Result<(), Diag> {
        self.keyword("let")?;
        let t = self.next();
        let name = match &t.tok {
            Tok::Ident(s) => s.clone(),
            other => {
                return Err(Diag::new(
                    format!("expected a name, found {}", tok_describe(other)),
                    t.line,
                    t.col,
                ));
            }
        };
        if is_reserved(&name) {
            return Err(Diag::new(
                format!("`{name}` is a reserved word and cannot be a constant name"),
                t.line,
                t.col,
            ));
        }
        if name.contains('~') {
            return Err(Diag::new(
                "`~` in a name is only valid inside a repeat body".to_string(),
                t.line,
                t.col,
            ));
        }
        if self.names.contains(&name) && !self.in_repeat {
            return Err(Diag::new(format!("duplicate name `{name}`"), t.line, t.col));
        }
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let value = self.expr()?;
        self.constants.insert(name.clone(), value);
        self.names.insert(name);
        Ok(())
    }

    /// `repeat i = n { top* }` — parse-time expansion, one body parse per
    /// iteration with the index substituted into numbers and `~` names.
    fn parse_repeat(&mut self, nodes: &mut Vec<Node>) -> Result<(), Diag> {
        if self.in_repeat {
            return Err(self.err_here("nested repeat is not supported"));
        }
        self.keyword("repeat")?;
        let t = self.next();
        let index = match &t.tok {
            Tok::Ident(s) if !s.contains('~') => s.clone(),
            Tok::Ident(_) => {
                return Err(Diag::new(
                    "`~` is not allowed in the repeat index name".to_string(),
                    t.line,
                    t.col,
                ));
            }
            other => {
                return Err(Diag::new(
                    format!("expected a name, found {}", tok_describe(other)),
                    t.line,
                    t.col,
                ));
            }
        };
        if is_reserved(&index) {
            return Err(Diag::new(
                format!("`{index}` is a reserved word and cannot be the repeat index"),
                t.line,
                t.col,
            ));
        }
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let count = self.expr()?;
        if count.fract() != 0.0 {
            return Err(self.err_here("repeat count must be an integer"));
        }
        if !(1.0..=1000.0).contains(&count) {
            return Err(self.err_here("repeat count must be between 1 and 1000"));
        }
        self.expect("`{`", |t| *t == Tok::LC)?;
        let start = self.pos;
        let mut depth = 1usize;
        let mut end = start;
        while end < self.toks.len() {
            match self.toks[end].tok {
                Tok::LC => depth += 1,
                Tok::RC => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Tok::Eof => break,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            return Err(self.err_here("unterminated repeat"));
        }
        let body: Vec<Token> = self.toks[start..end].to_vec();
        self.pos = end + 1;

        let saved_toks = std::mem::take(&mut self.toks);
        let saved_pos = self.pos;
        self.in_repeat = true;
        for k in 1..=(count as usize) {
            let ks = k.to_string();
            let mut sub: Vec<Token> = Vec::with_capacity(body.len() + 1);
            for tok in &body {
                match &tok.tok {
                    Tok::Ident(name) => {
                        if name.contains('~') {
                            sub.push(Token {
                                tok: Tok::Ident(name.replace('~', &ks)),
                                line: tok.line,
                                col: tok.col,
                            });
                        } else if *name == index {
                            sub.push(Token {
                                tok: Tok::Num(k as f64),
                                line: tok.line,
                                col: tok.col,
                            });
                        } else {
                            sub.push(tok.clone());
                        }
                    }
                    _ => sub.push(tok.clone()),
                }
            }
            sub.push(Token {
                tok: Tok::Eof,
                line: 0,
                col: 0,
            });
            self.toks = sub;
            self.pos = 0;
            while self.parse_top_into(nodes)? {}
        }
        self.in_repeat = false;
        self.toks = saved_toks;
        self.pos = saved_pos;
        Ok(())
    }

    fn parse_file(&mut self) -> Result<Document, Diag> {
        self.keyword("wvg")?;
        let version = self.integer()?;
        if !(1..=6).contains(&version) {
            let t = self.toks[self.pos - 1].clone();
            return Err(Diag::new(
                format!("unsupported format version {version}"),
                t.line,
                t.col,
            ));
        }
        self.keyword("scene")?;
        let width = self.number()?;
        let height = self.number()?;
        if width <= 0.0 || height <= 0.0 {
            let t = self.toks[self.pos - 2].clone();
            return Err(Diag::new(
                "scene dimensions must be positive",
                t.line,
                t.col,
            ));
        }

        let mut nodes: Vec<Node> = Vec::new();
        while self.parse_top_into(&mut nodes)? {}
        Ok(Document {
            width,
            height,
            nodes,
            defs: self.defs.drain().collect(),
        })
    }

    /// `text name = at=<point> content="s" size=n [font=f]
    ///  [anchor=start|middle|end] [color=paint] [hidden]` (spec §7.19).
    fn parse_text_node(&mut self) -> Result<Node, Diag> {
        let kw = self.next();
        let (name, name_span) = self.bind_name("node")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        self.keyword("at")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let at = self.parse_point()?;
        self.keyword("content")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let content = match self.next().tok {
            Tok::Str(s) => s,
            ref other => {
                return Err(Diag::new(
                    format!("expected a string, found {other:?}"),
                    name_span.line,
                    name_span.col,
                ));
            }
        };
        self.keyword("size")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let size = self.number()?;
        if size <= 0.0 {
            return Err(Diag::new(
                "text size must be positive",
                name_span.line,
                name_span.col,
            ));
        }
        let mut font = "sans".to_string();
        if self.peek_kw("font") {
            self.keyword("font")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            font = match self.next().tok {
                Tok::Ident(s) => s,
                ref other => {
                    return Err(Diag::new(
                        format!("expected a font name, found {other:?}"),
                        name_span.line,
                        name_span.col,
                    ));
                }
            };
        }
        let mut anchor = "start".to_string();
        if self.peek_kw("anchor") {
            self.keyword("anchor")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            anchor = match self.next().tok {
                Tok::Ident(ref s) if s == "start" || s == "middle" || s == "end" => s.clone(),
                ref other => {
                    return Err(Diag::new(
                        format!("expected anchor start|middle|end, found {other:?}"),
                        name_span.line,
                        name_span.col,
                    ));
                }
            };
        }
        let mut paint = Paint::Color(Color::rgb(0.0, 0.0, 0.0));
        if self.peek_kw("color") {
            self.keyword("color")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            paint = self.parse_paint_value()?;
        }
        let mut visible = true;
        if self.peek_kw("hidden") {
            self.keyword("hidden")?;
            visible = false;
        }
        Ok(Node {
            id: String::new(), // assigned after parsing completes
            name,
            op: OpKind::Text,
            visible,
            shape: PShape {
                kind: SKind::Text {
                    at,
                    content,
                    size,
                    font,
                    anchor,
                },
                span: Span::new(kw.line, kw.col),
            },
            paint,
            stroke_width: 1.0,
            outline_paint: None,
            markers: Vec::new(),
            span: Span::new(kw.line, kw.col),
        })
    }

    fn parse_paint_decl(&mut self) -> Result<(), Diag> {
        self.keyword("paint")?;
        let (name, span) = self.bind_name("paint")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let paint = self.parse_paint_value()?;
        self.paints.insert(name.clone(), paint);
        let _ = span;
        Ok(())
    }

    fn parse_paint_value(&mut self) -> Result<Paint, Diag> {
        let t = self.next();
        match &t.tok {
            Tok::Hex(h) => Ok(Paint::Color(color_from_hex(h))),
            Tok::Ident(s) => match s.as_str() {
                "rgb" | "rgba" => Ok(Paint::Color(self.parse_rgb_call(s == "rgba", &t)?)),
                "linear" | "radial" => {
                    let linear = s == "linear";
                    let (pt1_kw, pt2_kw) = if linear {
                        ("start", "end")
                    } else {
                        ("center", "edge")
                    };
                    self.keyword(pt1_kw)?;
                    self.expect("`=`", |t| *t == Tok::Eq)?;
                    let (_, start_v) = self.literal()?;
                    self.keyword(pt2_kw)?;
                    self.expect("`=`", |t| *t == Tok::Eq)?;
                    let (_, end_v) = self.literal()?;
                    let (first_kw, second_kw) = if linear {
                        ("start_color", "end_color")
                    } else {
                        ("center_color", "edge_color")
                    };
                    self.keyword(first_kw)?;
                    self.expect("`=`", |t| *t == Tok::Eq)?;
                    let c0 = self.parse_color_value()?;
                    self.keyword(second_kw)?;
                    self.expect("`=`", |t| *t == Tok::Eq)?;
                    let c1 = self.parse_color_value()?;
                    if linear {
                        Ok(Paint::Linear {
                            start: start_v,
                            end: end_v,
                            start_color: c0,
                            end_color: c1,
                        })
                    } else {
                        Ok(Paint::Radial {
                            center: start_v,
                            edge: end_v,
                            center_color: c0,
                            edge_color: c1,
                        })
                    }
                }
                other => {
                    if let Some(c) = named_color(other) {
                        return Ok(Paint::Color(c));
                    }
                    match self.paints.get(other) {
                        Some(p) => Ok(p.clone()),
                        None => Err(Diag::new(
                            format!("unknown paint `{other}` (paints must be declared before use)"),
                            t.line,
                            t.col,
                        )),
                    }
                }
            },
            _ => Err(Diag::new(
                format!("expected a paint, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn parse_color_value(&mut self) -> Result<Color, Diag> {
        let t = self.next();
        match &t.tok {
            Tok::Hex(h) => Ok(color_from_hex(h)),
            Tok::Ident(s) => {
                if s == "rgb" || s == "rgba" {
                    self.parse_rgb_call(s == "rgba", &t)
                } else if let Some(c) = named_color(s) {
                    Ok(c)
                } else {
                    Err(Diag::new(
                        format!("expected a color, found `{s}`"),
                        t.line,
                        t.col,
                    ))
                }
            }
            _ => Err(Diag::new(
                format!("expected a color, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn parse_rgb_call(&mut self, alpha: bool, kw: &Token) -> Result<Color, Diag> {
        self.expect("`(`", |t| *t == Tok::LP)?;
        let r = self.number()?;
        self.expect("`,`", |t| *t == Tok::Comma)?;
        let g = self.number()?;
        self.expect("`,`", |t| *t == Tok::Comma)?;
        let b = self.number()?;
        let a = if alpha {
            self.expect("`,`", |t| *t == Tok::Comma)?;
            self.number()?
        } else {
            1.0
        };
        self.expect("`)`", |t| *t == Tok::RP)?;
        for (i, v) in [r, g, b, a].into_iter().enumerate() {
            if !(0.0..=1.0).contains(&v) {
                return Err(Diag::new(
                    format!("color channel {} out of the 0..1 range", i + 1),
                    kw.line,
                    kw.col,
                ));
            }
        }
        Ok(Color { r, g, b, a })
    }

    // ---- nodes -----------------------------------------------------------

    fn parse_node(&mut self) -> Result<Node, Diag> {
        let kw = self.next();
        let op = match kw.tok {
            Tok::Ident(ref s) if s == "fill" => OpKind::Fill,
            Tok::Ident(ref s) if s == "stroke" => OpKind::Stroke,
            Tok::Ident(ref s) if s == "outline_fill" => OpKind::OutlineFill,
            _ => unreachable!(),
        };
        let (name, name_span) = self.bind_name("node")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let mut shape = self.parse_shape()?;
        if self.peek_kw("transform") {
            self.keyword("transform")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            let t = self.parse_transform_expr()?;
            let span = shape.span;
            shape = PShape {
                kind: SKind::Transform {
                    t,
                    shape: Box::new(shape),
                },
                span,
            };
        }
        self.keyword("color")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let paint = self.parse_paint_value()?;
        let mut stroke_width = 1.0;
        let mut outline_paint = None;
        if op == OpKind::OutlineFill {
            self.keyword("outline")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            outline_paint = Some(self.parse_paint_value()?);
        }
        if op != OpKind::Fill && self.peek_kw("width") {
            self.keyword("width")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            stroke_width = self.number()?;
            if stroke_width < 0.0 {
                return Err(Diag::new(
                    "stroke width must be non-negative",
                    name_span.line,
                    name_span.col,
                ));
            }
        }
        let mut markers: Vec<Marker> = Vec::new();
        if op != OpKind::Fill && self.peek_kw("marker") {
            self.keyword("marker")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            let placement = match self.next().tok {
                Tok::Ident(ref s) if s == "start" || s == "end" || s == "both" => s.clone(),
                ref other => {
                    return Err(Diag::new(
                        format!("expected marker placement, found {other:?}"),
                        name_span.line,
                        name_span.col,
                    ))
                }
            };
            let kind = match self.next().tok {
                Tok::Ident(ref s) if s == "triangle" || s == "bar" => s.clone(),
                ref other => {
                    return Err(Diag::new(
                        format!("expected marker kind, found {other:?}"),
                        name_span.line,
                        name_span.col,
                    ))
                }
            };
            let size = self.number()?;
            if size <= 0.0 {
                return Err(Diag::new(
                    "marker size must be positive",
                    name_span.line,
                    name_span.col,
                ));
            }
            let marker_paint = if matches!(self.peek().tok, Tok::Hex(_))
                || self.peek_kw("rgb")
                || self.peek_kw("rgba")
                || matches!(self.peek().tok, Tok::Ident(ref s) if named_color(s).is_some())
            {
                Some(self.parse_paint_value()?)
            } else {
                None
            };
            markers.push(Marker {
                placement,
                kind,
                size,
                paint: marker_paint,
            });
        }
        let mut visible = true;
        if self.peek_kw("hidden") {
            self.keyword("hidden")?;
            visible = false;
        }
        let markers_opt = if markers.is_empty() {
            None
        } else {
            Some(markers)
        };
        Ok(Node {
            id: String::new(), // assigned after parsing completes
            name,
            op,
            visible,
            shape,
            paint,
            stroke_width,
            outline_paint,
            markers: markers_opt.unwrap_or_default(),
            span: Span::new(kw.line, kw.col),
        })
    }

    /// `group [transform_expr] { top* }` — sugar: the group transform is
    /// composed onto each contained node's own transform (§7.14).
    fn parse_group(&mut self, nodes: &mut Vec<Node>) -> Result<(), Diag> {
        self.keyword("group")?;
        let group_t = if self.peek_kw("transform") {
            self.keyword("transform")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            self.parse_transform_expr()?
        } else {
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        };
        self.expect("`{`", |t| *t == Tok::LC)?;
        let identity = group_t == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut inner: Vec<Node> = Vec::new();
        loop {
            if matches!(self.peek().tok, Tok::RC) {
                break;
            }
            if !self.parse_top_into(&mut inner)? {
                return Err(self.err_here("unterminated group"));
            }
        }
        self.expect("`}`", |t| *t == Tok::RC)?;
        for mut node in inner {
            if node.op == OpKind::Text {
                if !identity {
                    return Err(Diag::new(
                        "text nodes cannot sit in a transformed group",
                        node.span.line,
                        node.span.col,
                    ));
                }
                nodes.push(node);
                continue;
            }
            node.shape = compose_group_transform(group_t, node.shape);
            nodes.push(node);
        }
        Ok(())
    }

    /// `def name = shape` — document-scope named shape (spec §7.17).
    fn parse_def(&mut self) -> Result<(), Diag> {
        self.keyword("def")?;
        let (name, _) = self.bind_name("def")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let shape = self.parse_shape()?;
        self.defs.insert(name, shape);
        Ok(())
    }

    /// Parses a named or matrix transform expression into six coefficients.
    fn parse_transform_expr(&mut self) -> Result<[f64; 6], Diag> {
        let t = self.next();
        let kw = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    format!("expected a transform, found {}", t.describe()),
                    t.line,
                    t.col,
                ))
            }
        };
        // positional slots: single terms (see number_term)
        let num = |p: &mut Self| p.number_term();
        let maybe_about = |p: &mut Self| -> Result<(f64, f64), Diag> {
            if p.peek_kw("about") {
                p.keyword("about")?;
                Ok(p.literal()?.1)
            } else {
                Ok((0.0, 0.0))
            }
        };
        let rad = |deg: f64| deg.to_radians();
        match kw.as_str() {
            "translate" => {
                let tx = num(self)?;
                let ty = num(self)?;
                Ok([1.0, 0.0, 0.0, 1.0, tx, ty])
            }
            "rotate" => {
                let deg = num(self)?;
                let (cx, cy) = maybe_about(self)?;
                let (c, s_) = (rad(deg).cos(), rad(deg).sin());
                // T(c) · R · T(-c)
                Ok([
                    c,
                    s_,
                    -s_,
                    c,
                    cx - (c * cx - s_ * cy),
                    cy - (s_ * cx + c * cy),
                ])
            }
            "scale" => {
                let sx = num(self)?;
                let sy = if self.peek_number() { num(self)? } else { sx };
                let (cx, cy) = maybe_about(self)?;
                Ok([sx, 0.0, 0.0, sy, cx - sx * cx, cy - sy * cy])
            }
            "mirror_x" => {
                let axis = num(self)?;
                Ok([-1.0, 0.0, 0.0, 1.0, 2.0 * axis, 0.0])
            }
            "mirror_y" => {
                let axis = num(self)?;
                Ok([1.0, 0.0, 0.0, -1.0, 0.0, 2.0 * axis])
            }
            "matrix" => {
                let mut out = [0.0f64; 6];
                for v in out.iter_mut() {
                    *v = num(self)?;
                }
                Ok(out)
            }
            _ => Err(Diag::new(
                format!("expected a transform, found `{kw}`"),
                t.line,
                t.col,
            )),
        }
    }

    fn parse_guide(&mut self) -> Result<Node, Diag> {
        let kw = self.next();
        let (name, _) = self.bind_name("node")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        // `grid` followed by `motifs` is a hidden grid *generator* node;
        // `grid` with lattice props is a grid guide.
        let is_grid_generator = self.peek_kw("grid")
            && matches!(&self.toks[self.pos + 1].tok, Tok::Ident(s) if s == "motifs");
        if !self.peek_kw("grid") || is_grid_generator {
            let shape = self.parse_shape()?;
            return Ok(Node {
                id: String::new(),
                name,
                op: OpKind::Fill,
                visible: false,
                shape,
                paint: Paint::Color(Color::rgb(0.0, 0.0, 0.0)),
                stroke_width: 1.0,
                outline_paint: None,
                markers: Vec::new(),
                span: Span::new(kw.line, kw.col),
            });
        }
        self.keyword("grid")?;
        let mut origin = PPoint {
            kind: PKind::Literal(0.0, 0.0),
            span: Span::new(kw.line, kw.col),
        };
        let (mut cols, mut rows) = (8i64, 6i64);
        let (mut dx, mut dy) = (40.0f64, 40.0f64);
        if self.peek_kw("origin") {
            self.keyword("origin")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            origin = self.parse_point()?;
        }
        if self.peek_kw("cols") {
            self.keyword("cols")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            cols = self.integer()?;
        }
        if self.peek_kw("rows") {
            self.keyword("rows")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            rows = self.integer()?;
        }
        if self.peek_kw("dx") {
            self.keyword("dx")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            dx = self.number()?;
        }
        if self.peek_kw("dy") {
            self.keyword("dy")?;
            self.expect("`=`", |t| *t == Tok::Eq)?;
            dy = self.number()?;
        }
        Ok(Node {
            id: String::new(),
            name,
            op: OpKind::Fill,
            visible: false,
            shape: PShape {
                kind: SKind::GridGuide {
                    origin,
                    cols,
                    rows,
                    dx,
                    dy,
                },
                span: Span::new(kw.line, kw.col),
            },
            paint: Paint::Color(Color::rgb(0.0, 0.0, 0.0)),
            stroke_width: 1.0,
            outline_paint: None,
            markers: Vec::new(),
            span: Span::new(kw.line, kw.col),
        })
    }

    // ---- points ----------------------------------------------------------

    /// `tangent NUMBER [deg NUMBER]` — local-frame offset (spec §7.20).
    fn parse_tangent_off(&mut self) -> Result<Option<(f64, f64)>, Diag> {
        if !self.peek_kw("tangent") {
            return Ok(None);
        }
        self.keyword("tangent")?;
        let len = self.number_term()?;
        let deg = if self.peek_kw("deg") {
            self.keyword("deg")?;
            self.number_term()?
        } else {
            0.0
        };
        Ok(Some((len, deg)))
    }

    fn parse_point(&mut self) -> Result<PPoint, Diag> {
        let t = self.next();
        let span = Span::new(t.line, t.col);
        match t.tok {
            Tok::LP => {
                let x = self.number_term()?;
                self.expect("`,`", |t| *t == Tok::Comma)?;
                let y = self.number_term()?;
                self.expect("`)`", |t| *t == Tok::RP)?;
                Ok(PPoint {
                    kind: PKind::Literal(x, y),
                    span,
                })
            }
            Tok::At => {
                let nt = self.next();
                let node = match nt.tok {
                    Tok::Ident(s) => s,
                    _ => {
                        return Err(Diag::new(
                            format!("expected a node name after `@`, found {}", nt.describe()),
                            nt.line,
                            nt.col,
                        ))
                    }
                };
                if self.peek_kw("seg") {
                    self.keyword("seg")?;
                    let index = self.integer_term()?;
                    let pct = if self.peek_number() {
                        self.percent()?
                    } else {
                        0.0
                    };
                    let tangent = self.parse_tangent_off()?;
                    let offset = if self.peek().tok == Tok::Plus {
                        self.next();
                        Some(self.literal()?.1)
                    } else {
                        None
                    };
                    return Ok(PPoint {
                        kind: PKind::Segment {
                            node,
                            index,
                            pct,
                            tangent,
                            offset,
                        },
                        span,
                    });
                }
                if self.peek().tok == Tok::LB {
                    self.next();
                    let col = self.integer()?;
                    self.expect("`,`", |t| *t == Tok::Comma)?;
                    let row = self.integer()?;
                    self.expect("`]`", |t| *t == Tok::RB)?;
                    let offset = if self.peek().tok == Tok::Plus {
                        self.next();
                        Some(self.literal()?.1)
                    } else {
                        None
                    };
                    return Ok(PPoint {
                        kind: PKind::GridCell {
                            node,
                            col,
                            row,
                            offset,
                        },
                        span,
                    });
                }
                let mut dir = Orientation::Cw;
                if self.peek_kw("cw") {
                    self.keyword("cw")?;
                } else if self.peek_kw("ccw") {
                    self.keyword("ccw")?;
                    dir = Orientation::Ccw;
                }
                let mut pct = 0.0;
                if self.peek_number() {
                    pct = self.percent()?;
                }
                let mut start = None;
                if self.peek_kw("from") {
                    self.keyword("from")?;
                    start = Some(self.literal()?.1);
                }
                let tangent = self.parse_tangent_off()?;
                let offset = if self.peek().tok == Tok::Plus {
                    self.next();
                    Some(self.literal()?.1)
                } else {
                    None
                };
                Ok(PPoint {
                    kind: PKind::Anchor {
                        node,
                        pct,
                        start,
                        dir,
                        tangent,
                        offset,
                    },
                    span,
                })
            }
            Tok::Ident(ref s) if s == "polar" => {
                self.keyword("center")?;
                self.expect("`=`", |t| *t == Tok::Eq)?;
                let center = Box::new(self.parse_point()?);
                self.keyword("radius")?;
                self.expect("`=`", |t| *t == Tok::Eq)?;
                let radius = self.number()?;
                self.keyword("deg")?;
                self.expect("`=`", |t| *t == Tok::Eq)?;
                let deg = self.number()?;
                Ok(PPoint {
                    kind: PKind::Polar {
                        center,
                        radius,
                        deg,
                    },
                    span,
                })
            }
            Tok::Ident(ref s) if s == "between" => {
                let a = Box::new(self.parse_point()?);
                let b = Box::new(self.parse_point()?);
                let pct = self.percent()?;
                let offset = if self.peek().tok == Tok::Plus {
                    self.next();
                    Some(self.literal()?.1)
                } else {
                    None
                };
                Ok(PPoint {
                    kind: PKind::Between { a, b, pct, offset },
                    span,
                })
            }
            _ => Err(Diag::new(
                format!("expected a point, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    // ---- shapes ----------------------------------------------------------

    fn parse_shape(&mut self) -> Result<PShape, Diag> {
        let t = self.next();
        let span = Span::new(t.line, t.col);
        let kw = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    format!("expected a shape, found {}", t.describe()),
                    t.line,
                    t.col,
                ))
            }
        };
        let eq = |p: &mut Self| p.expect("`=`", |t| *t == Tok::Eq);

        let kind = match kw.as_str() {
            "circle" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                if radius <= 0.0 {
                    return Err(Diag::new(
                        "circle radius must be positive",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Circle { center, radius }
            }
            "ellipse" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("rx")?;
                eq(self)?;
                let rx = self.number()?;
                self.keyword("ry")?;
                eq(self)?;
                let ry = self.number()?;
                if rx <= 0.0 || ry <= 0.0 {
                    return Err(Diag::new(
                        "ellipse radii must be positive",
                        span.line,
                        span.col,
                    ));
                }
                let mut rotation_deg = 0.0;
                if self.peek_kw("rotation_deg") {
                    self.keyword("rotation_deg")?;
                    eq(self)?;
                    rotation_deg = self.number()?;
                }
                SKind::Ellipse {
                    center,
                    rx,
                    ry,
                    rotation_deg,
                }
            }
            "arc" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                if radius <= 0.0 {
                    return Err(Diag::new(
                        "arc radius must be positive",
                        span.line,
                        span.col,
                    ));
                }
                self.keyword("start_deg")?;
                eq(self)?;
                let start_deg = self.number()?;
                self.keyword("sweep_deg")?;
                eq(self)?;
                let sweep_deg = self.number()?;
                if !(0.0..360.0).contains(&sweep_deg.abs()) || sweep_deg == 0.0 {
                    return Err(Diag::new(
                        "arc sweep must be strictly within ±360 degrees; use circle",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Arc {
                    center,
                    radius,
                    start_deg,
                    sweep_deg,
                }
            }
            "polygon" => {
                self.keyword("points")?;
                eq(self)?;
                let points = self.point_list()?;
                if points.len() < 3 {
                    return Err(Diag::new(
                        "a polygon needs at least 3 points",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Polygon { points }
            }
            "polyline" => {
                self.keyword("points")?;
                eq(self)?;
                let points = self.point_list()?;
                if points.len() < 2 {
                    return Err(Diag::new(
                        "a polyline needs at least 2 points",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Polyline { points }
            }
            "line" => {
                self.keyword("p1")?;
                eq(self)?;
                let p1 = self.parse_point()?;
                self.keyword("p2")?;
                eq(self)?;
                let p2 = self.parse_point()?;
                SKind::Polyline {
                    points: vec![p1, p2],
                }
            }
            "path" => {
                self.keyword("subpaths")?;
                eq(self)?;
                let subpaths = self.subpath_list()?;
                SKind::Path { subpaths }
            }
            "compound" => {
                self.keyword("shapes")?;
                eq(self)?;
                let shapes = self.shape_list()?;
                SKind::Compound { shapes }
            }
            "along" => {
                self.keyword("track")?;
                eq(self)?;
                let track = Box::new(self.parse_shape()?);
                self.keyword("motifs")?;
                eq(self)?;
                let motifs = self.shape_list()?;
                self.keyword("n")?;
                eq(self)?;
                let n = self.integer()?;
                let mut offset_pct = 0.0;
                if self.peek_kw("offset_pct") {
                    self.keyword("offset_pct")?;
                    eq(self)?;
                    offset_pct = self.percent()?;
                }
                let mut align = AlignMode::Tangent;
                if self.peek_kw("align") {
                    self.keyword("align")?;
                    eq(self)?;
                    align = self.align_mode()?;
                }
                let mut direction = Orientation::Cw;
                if self.peek_kw("direction") {
                    self.keyword("direction")?;
                    eq(self)?;
                    direction = self.orientation()?;
                }
                SKind::Along {
                    track,
                    motifs,
                    n,
                    offset_pct,
                    align,
                    direction,
                }
            }
            "polar" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("motifs")?;
                eq(self)?;
                let motifs = self.shape_list()?;
                self.keyword("n")?;
                eq(self)?;
                let n = self.integer()?;
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                if radius <= 0.0 {
                    return Err(Diag::new(
                        "polar radius must be positive",
                        span.line,
                        span.col,
                    ));
                }
                let mut start_deg = 0.0;
                if self.peek_kw("start_deg") {
                    self.keyword("start_deg")?;
                    eq(self)?;
                    start_deg = self.number()?;
                }
                let mut align = None;
                if self.peek_kw("align") {
                    self.keyword("align")?;
                    eq(self)?;
                    align = Some(self.align_mode()?);
                }
                SKind::Polar {
                    center,
                    motifs,
                    n,
                    radius,
                    start_deg,
                    align,
                }
            }
            "grid" => {
                self.keyword("motifs")?;
                eq(self)?;
                let motifs = self.shape_list()?;
                self.keyword("cols")?;
                eq(self)?;
                let cols = self.integer()?;
                self.keyword("rows")?;
                eq(self)?;
                let rows = self.integer()?;
                if cols < 1 || rows < 1 {
                    return Err(Diag::new(
                        "grid needs at least 1 column and 1 row",
                        span.line,
                        span.col,
                    ));
                }
                self.keyword("dx")?;
                eq(self)?;
                let dx = self.number()?;
                self.keyword("dy")?;
                eq(self)?;
                let dy = self.number()?;
                let mut origin = PPoint {
                    kind: PKind::Literal(0.0, 0.0),
                    span: Span::new(t.line, t.col),
                };
                if self.peek_kw("origin") {
                    self.keyword("origin")?;
                    eq(self)?;
                    origin = self.parse_point()?;
                }
                SKind::Grid {
                    motifs,
                    cols,
                    rows,
                    dx,
                    dy,
                    origin,
                }
            }
            "regular_polygon" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                let c = match center.kind {
                    PKind::Literal(x, y) => (x, y),
                    _ => return Err(Diag::new(
                        "regular_polygon center must be a literal point (it desugars to a polygon)",
                        span.line,
                        span.col,
                    )),
                };
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                self.keyword("sides")?;
                eq(self)?;
                let sides = self.integer()?;
                if sides < 3 {
                    return Err(Diag::new(
                        "a regular polygon needs at least 3 sides",
                        span.line,
                        span.col,
                    ));
                }
                let mut start_angle_deg = 0.0;
                if self.peek_kw("start_angle_deg") {
                    self.keyword("start_angle_deg")?;
                    eq(self)?;
                    start_angle_deg = self.number()?;
                }
                let pts = regular_polygon_points(c, radius, sides, start_angle_deg);
                SKind::Polygon {
                    points: literal_points(pts, span),
                }
            }
            "star" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                let c = match center.kind {
                    PKind::Literal(x, y) => (x, y),
                    _ => {
                        return Err(Diag::new(
                            "star center must be a literal point (it desugars to a polygon)",
                            span.line,
                            span.col,
                        ))
                    }
                };
                self.keyword("outer_radius")?;
                eq(self)?;
                let outer_radius = self.number()?;
                self.keyword("inner_radius")?;
                eq(self)?;
                let inner_radius = self.number()?;
                let mut points = 5i64;
                if self.peek_kw("points") {
                    self.keyword("points")?;
                    eq(self)?;
                    points = self.integer()?;
                }
                if points < 2 {
                    return Err(Diag::new(
                        "a star needs at least 2 points",
                        span.line,
                        span.col,
                    ));
                }
                let mut start_angle_deg = 0.0;
                if self.peek_kw("start_angle_deg") {
                    self.keyword("start_angle_deg")?;
                    eq(self)?;
                    start_angle_deg = self.number()?;
                }
                let pts = star_points(c, outer_radius, inner_radius, points, start_angle_deg);
                SKind::Polygon {
                    points: literal_points(pts, span),
                }
            }
            "rect" => {
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("size")?;
                eq(self)?;
                let size = self.literal()?.1;
                if size.0 <= 0.0 || size.1 <= 0.0 {
                    return Err(Diag::new(
                        "rect size components must be positive",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Rect {
                    center,
                    width: size.0,
                    height: size.1,
                }
            }
            "pie" | "chord" => {
                let chord = kw == "chord";
                self.keyword("center")?;
                eq(self)?;
                let center = self.parse_point()?;
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                if radius <= 0.0 {
                    return Err(Diag::new(
                        format!("{kw} radius must be positive"),
                        span.line,
                        span.col,
                    ));
                }
                self.keyword("start_deg")?;
                eq(self)?;
                let start_deg = self.number()?;
                self.keyword("sweep_deg")?;
                eq(self)?;
                let sweep_deg = self.number()?;
                if !(0.0..360.0).contains(&sweep_deg.abs()) || sweep_deg == 0.0 {
                    return Err(Diag::new(
                        format!("{kw} sweep must be strictly within ±360 degrees"),
                        span.line,
                        span.col,
                    ));
                }
                SKind::Pie {
                    center,
                    radius,
                    start_deg,
                    sweep_deg,
                    chord,
                }
            }
            "use" => {
                let nt = self.next();
                let def_name = match nt.tok {
                    Tok::Ident(ref s) => s.clone(),
                    _ => {
                        return Err(Diag::new(
                            format!("expected a def name, found {}", nt.describe()),
                            nt.line,
                            nt.col,
                        ))
                    }
                };
                SKind::Use { def_name }
            }
            "rounded" => {
                self.keyword("shape")?;
                eq(self)?;
                let shape = Box::new(self.parse_shape()?);
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                if radius <= 0.0 {
                    return Err(Diag::new(
                        "fillet radius must be positive",
                        span.line,
                        span.col,
                    ));
                }
                SKind::Rounded { shape, radius }
            }
            _ => {
                return Err(Diag::new(
                    format!("expected a shape, found `{kw}`"),
                    t.line,
                    t.col,
                ))
            }
        };
        Ok(PShape { kind, span })
    }

    fn align_mode(&mut self) -> Result<AlignMode, Diag> {
        let t = self.next();
        match &t.tok {
            Tok::Ident(s) if s == "tangent" => Ok(AlignMode::Tangent),
            Tok::Ident(s) if s == "none" => Ok(AlignMode::Off),
            _ => Err(Diag::new(
                format!("expected `tangent` or `none`, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn orientation(&mut self) -> Result<Orientation, Diag> {
        let t = self.next();
        match &t.tok {
            Tok::Ident(s) if s == "cw" => Ok(Orientation::Cw),
            Tok::Ident(s) if s == "ccw" => Ok(Orientation::Ccw),
            _ => Err(Diag::new(
                format!("expected `cw` or `ccw`, found {}", t.describe()),
                t.line,
                t.col,
            )),
        }
    }

    fn point_list(&mut self) -> Result<Vec<PPoint>, Diag> {
        self.expect("`[`", |t| *t == Tok::LB)?;
        let mut pts = vec![self.parse_point()?];
        while self.peek().tok == Tok::Comma {
            self.next();
            pts.push(self.parse_point()?);
        }
        self.expect("`]`", |t| *t == Tok::RB)?;
        Ok(pts)
    }

    fn shape_list(&mut self) -> Result<Vec<PShape>, Diag> {
        self.expect("`[`", |t| *t == Tok::LB)?;
        let mut shapes = vec![self.parse_shape()?];
        while self.peek().tok == Tok::Comma {
            self.next();
            shapes.push(self.parse_shape()?);
        }
        self.expect("`]`", |t| *t == Tok::RB)?;
        Ok(shapes)
    }

    fn subpath_list(&mut self) -> Result<Vec<PSubPath>, Diag> {
        self.expect("`[`", |t| *t == Tok::LB)?;
        let mut subs = vec![self.parse_subpath()?];
        while self.peek().tok == Tok::Comma {
            self.next();
            subs.push(self.parse_subpath()?);
        }
        self.expect("`]`", |t| *t == Tok::RB)?;
        Ok(subs)
    }

    fn parse_subpath(&mut self) -> Result<PSubPath, Diag> {
        let t = self.expect("`{`", |t| *t == Tok::LC)?;
        let span = Span::new(t.line, t.col);
        self.keyword("start")?;
        self.expect("`=`", |t| *t == Tok::Eq)?;
        let start = self.parse_point()?;
        while self.peek().tok == Tok::Comma {
            self.next();
        }
        let mut instructions = vec![self.parse_instruction()?];
        while self.peek().tok == Tok::Comma {
            self.next();
            instructions.push(self.parse_instruction()?);
        }
        self.expect("`}`", |t| *t == Tok::RC)?;
        Ok(PSubPath {
            start,
            instructions,
            span,
        })
    }

    fn parse_instruction(&mut self) -> Result<PInstr, Diag> {
        let t = self.next();
        let span = Span::new(t.line, t.col);
        let kw = match &t.tok {
            Tok::Ident(s) => s.clone(),
            _ => {
                return Err(Diag::new(
                    format!("expected a path instruction, found {}", t.describe()),
                    t.line,
                    t.col,
                ))
            }
        };
        let eq = |p: &mut Self| p.expect("`=`", |t| *t == Tok::Eq);
        let point_prop = |p: &mut Self, name: &str| -> Result<PPoint, Diag> {
            p.keyword(name)?;
            p.expect("`=`", |t| *t == Tok::Eq)?;
            p.parse_point()
        };

        let kind = match kw.as_str() {
            "line" => {
                let to = point_prop(self, "to")?;
                IKind::Line { to }
            }
            "quad" => {
                let ctrl = point_prop(self, "ctrl")?;
                let to = point_prop(self, "to")?;
                IKind::Quad { ctrl, to }
            }
            "cubic" => {
                let c1 = point_prop(self, "c1")?;
                let c2 = point_prop(self, "c2")?;
                let to = point_prop(self, "to")?;
                IKind::Cubic { c1, c2, to }
            }
            "arc_circle" => {
                self.keyword("radius")?;
                eq(self)?;
                let radius = self.number()?;
                let (large, sweep_cw) = self.arc_flags()?;
                let to = point_prop(self, "to")?;
                IKind::ArcCircle {
                    radius,
                    large,
                    sweep_cw,
                    to,
                }
            }
            "arc_ellipse" => {
                self.keyword("rx")?;
                eq(self)?;
                let rx = self.number()?;
                self.keyword("ry")?;
                eq(self)?;
                let ry = self.number()?;
                self.keyword("rotation_deg")?;
                eq(self)?;
                let rotation_deg = self.number()?;
                let (large, sweep_cw) = self.arc_flags()?;
                let to = point_prop(self, "to")?;
                IKind::ArcEllipse {
                    rx,
                    ry,
                    rotation_deg,
                    large,
                    sweep_cw,
                    to,
                }
            }
            "close" => IKind::Close,
            _ => {
                return Err(Diag::new(
                    format!("expected a path instruction, found `{kw}`"),
                    t.line,
                    t.col,
                ))
            }
        };
        Ok(PInstr { kind, span })
    }

    /// Optional `[ large ] [ cw | ccw ]` flags in fixed order.
    fn arc_flags(&mut self) -> Result<(bool, bool), Diag> {
        let mut large = false;
        if self.peek_kw("large") {
            self.keyword("large")?;
            large = true;
        }
        let mut sweep_cw = true;
        if self.peek_kw("cw") {
            self.keyword("cw")?;
        } else if self.peek_kw("ccw") {
            self.keyword("ccw")?;
            sweep_cw = false;
        }
        Ok((large, sweep_cw))
    }
}

fn literal_points(pts: Vec<Pt>, span: Span) -> Vec<PPoint> {
    pts.into_iter()
        .map(|p| PPoint {
            kind: PKind::Literal(p.x, p.y),
            span,
        })
        .collect()
}

/// Desugaring for `regular_polygon` (spec §7.7).
fn regular_polygon_points(center: (f64, f64), radius: f64, sides: i64, start_deg: f64) -> Vec<Pt> {
    let (cx, cy) = center;
    let step = 2.0 * std::f64::consts::PI / sides as f64;
    let a0 = start_deg.to_radians();
    (0..sides)
        .map(|i| {
            let t = a0 + i as f64 * step;
            Pt::new(cx + radius * t.cos(), cy + radius * t.sin())
        })
        .collect()
}

/// Desugaring for `star` (spec §7.7).
#[allow(clippy::too_many_arguments)]
fn star_points(center: (f64, f64), outer: f64, inner: f64, points: i64, start_deg: f64) -> Vec<Pt> {
    let (cx, cy) = center;
    let step = std::f64::consts::PI / points as f64;
    let a0 = start_deg.to_radians();
    (0..2 * points)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let t = a0 + i as f64 * step;
            Pt::new(cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

// Assign ids after a successful parse (spec §5.4).
pub fn assign_ids(doc: &mut Document) {
    for (i, node) in doc.nodes.iter_mut().enumerate() {
        node.id = format!("n{}", i + 1);
    }
}

/// `group [t] { … }` sugar: compose the group transform onto the node's own
/// (own applied first, then the group's) and collapse into one transform.
fn compose_group_transform(group_t: [f64; 6], shape: PShape) -> PShape {
    let node_t = match &shape.kind {
        SKind::Transform { t, .. } => Some(*t),
        _ => None,
    };
    let combined = match node_t {
        Some(n) => mul6(group_t, n),
        None => group_t,
    };
    match shape.kind {
        SKind::Transform { shape: inner, .. } => PShape {
            kind: SKind::Transform {
                t: combined,
                shape: inner,
            },
            span: shape.span,
        },
        other => PShape {
            kind: SKind::Transform {
                t: combined,
                shape: Box::new(PShape {
                    kind: other,
                    span: shape.span,
                }),
            },
            span: shape.span,
        },
    }
}

/// Six-coefficient matrix product `(g ∘ n)`: apply `n` first, then `g`.
fn mul6(g: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    [
        g[0] * n[0] + g[2] * n[1],
        g[1] * n[0] + g[3] * n[1],
        g[0] * n[2] + g[2] * n[3],
        g[1] * n[2] + g[3] * n[3],
        g[0] * n[4] + g[2] * n[5] + g[4],
        g[1] * n[4] + g[3] * n[5] + g[5],
    ]
}
