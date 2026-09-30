//! The document IR: the parse output and the contract shared with the
//! Python reference (`windvg.document`). See docs/language.md §6.

use std::fmt;

#[derive(Debug, Clone)]
pub struct Diag {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl Diag {
    pub fn new(msg: impl Into<String>, line: u32, col: u32) -> Diag {
        Diag {
            msg: msg.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(line: u32, col: u32) -> Span {
        Span { line, col }
    }
}

/// Appendix A of the language spec: identifiers must not be any of these.
pub const RESERVED: &[&str] = &[
    "about",
    "align",
    "along",
    "arc",
    "arc_between",
    "arc_circle",
    "arc_ellipse",
    "bar",
    "between",
    "black",
    "blue",
    "both",
    "c1",
    "c2",
    "ccw",
    "center",
    "center_color",
    "chord",
    "circle",
    "close",
    "color",
    "cols",
    "compound",
    "content",
    "ctrl",
    "cw",
    "cyan",
    "def",
    "deg",
    "direction",
    "dx",
    "dy",
    "edge",
    "ellipse",
    "end",
    "end_color",
    "fill",
    "font",
    "from",
    "gray",
    "green",
    "grid",
    "group",
    "guide",
    "hidden",
    "inner_radius",
    "intersects",
    "large",
    "let",
    "line",
    "linear",
    "magenta",
    "marker",
    "matrix",
    "middle",
    "mirror_x",
    "mirror_y",
    "motifs",
    "n",
    "none",
    "offset_pct",
    "origin",
    "outer_radius",
    "outline",
    "outline_fill",
    "p1",
    "p2",
    "paint",
    "path",
    "pie",
    "points",
    "polar",
    "polygon",
    "polyline",
    "quad",
    "radial",
    "radius",
    "rect",
    "red",
    "regular_polygon",
    "repeat",
    "rgb",
    "rgba",
    "rotate",
    "rotation_deg",
    "rounded",
    "rows",
    "rx",
    "ry",
    "sans",
    "scale",
    "scene",
    "seg",
    "shape",
    "shapes",
    "sides",
    "size",
    "star",
    "start",
    "start_angle_deg",
    "start_color",
    "start_deg",
    "stroke",
    "subpaths",
    "sweep_deg",
    "tangent",
    "text",
    "to",
    "track",
    "transform",
    "translate",
    "triangle",
    "use",
    "white",
    "width",
    "wvg",
    "yellow",
];
pub fn is_reserved(name: &str) -> bool {
    RESERVED.binary_search(&name).is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Cw,
    Ccw,
}

impl Orientation {
    pub fn mult(self) -> f64 {
        match self {
            Orientation::Cw => 1.0,
            Orientation::Ccw => -1.0,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Orientation::Cw => "cw",
            Orientation::Ccw => "ccw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub fn rgb(r: f64, g: f64, b: f64) -> Color {
        Color { r, g, b, a: 1.0 }
    }
}

/// The nine named colors of the spec (§6), all alpha 1.
pub fn named_color(name: &str) -> Option<Color> {
    let c = match name {
        "black" => (0.0, 0.0, 0.0),
        "white" => (1.0, 1.0, 1.0),
        "red" => (1.0, 0.0, 0.0),
        "green" => (0.0, 1.0, 0.0),
        "blue" => (0.0, 0.0, 1.0),
        "yellow" => (1.0, 1.0, 0.0),
        "cyan" => (0.0, 1.0, 1.0),
        "magenta" => (1.0, 0.0, 1.0),
        "gray" => (0.5, 0.5, 0.5),
        _ => return None,
    };
    Some(Color::rgb(c.0, c.1, c.2))
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa` with the digits normalized to uppercase.
pub fn color_from_hex(digits: &str) -> Color {
    let byte = |i: usize| -> f64 {
        let v = u8::from_str_radix(&digits[i..i + 2], 16).unwrap();
        v as f64 / 255.0
    };
    match digits.len() {
        6 => Color {
            r: byte(0),
            g: byte(2),
            b: byte(4),
            a: 1.0,
        },
        8 => Color {
            r: byte(0),
            g: byte(2),
            b: byte(4),
            a: byte(6),
        },
        _ => unreachable!("lexer normalizes hex to 6 or 8 digits"),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AlignMode {
    Tangent,
    Off,
}

#[derive(Debug, Clone)]
pub enum Paint {
    Color(Color),
    Linear {
        start: (f64, f64),
        end: (f64, f64),
        start_color: Color,
        end_color: Color,
    },
    Radial {
        center: (f64, f64),
        edge: (f64, f64),
        center_color: Color,
        edge_color: Color,
    },
}

/// A point expression: literal, anchor, segment, or grid-cell reference.
#[derive(Debug, Clone)]
pub enum PKind {
    Literal(f64, f64),
    Anchor {
        node: String,
        pct: f64,
        start: Option<(f64, f64)>,
        dir: Orientation,
        tangent: Option<(f64, f64)>,
        offset: Option<(f64, f64)>,
    },
    Segment {
        node: String,
        index: i64,
        pct: f64,
        tangent: Option<(f64, f64)>,
        offset: Option<(f64, f64)>,
    },
    Between {
        a: Box<PPoint>,
        b: Box<PPoint>,
        pct: f64,
        offset: Option<(f64, f64)>,
    },
    GridCell {
        node: String,
        col: i64,
        row: i64,
        offset: Option<(f64, f64)>,
    },
    Polar {
        center: Box<PPoint>,
        radius: f64,
        deg: f64,
    },
    Intersects {
        a: String,
        b: String,
        k: i64,
    },
}

#[derive(Debug, Clone)]
pub struct PPoint {
    pub kind: PKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum IKind {
    Line {
        to: PPoint,
    },
    Quad {
        ctrl: PPoint,
        to: PPoint,
    },
    Cubic {
        c1: PPoint,
        c2: PPoint,
        to: PPoint,
    },
    ArcCircle {
        radius: f64,
        large: bool,
        sweep_cw: bool,
        to: PPoint,
    },
    ArcEllipse {
        rx: f64,
        ry: f64,
        rotation_deg: f64,
        large: bool,
        sweep_cw: bool,
        to: PPoint,
    },
    Close,
}

#[derive(Debug, Clone)]
pub struct PInstr {
    pub kind: IKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PSubPath {
    pub start: PPoint,
    pub instructions: Vec<PInstr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum SKind {
    Circle {
        center: PPoint,
        radius: f64,
    },
    Ellipse {
        center: PPoint,
        rx: f64,
        ry: f64,
        rotation_deg: f64,
    },
    Arc {
        center: PPoint,
        radius: f64,
        start_deg: f64,
        sweep_deg: f64,
    },
    ArcBetween {
        p1: PPoint,
        p2: PPoint,
        deg: f64,
    },
    Polygon {
        points: Vec<PPoint>,
    },
    Polyline {
        points: Vec<PPoint>,
    },
    Path {
        subpaths: Vec<PSubPath>,
    },
    Compound {
        shapes: Vec<PShape>,
    },
    Along {
        track: Box<PShape>,
        motifs: Vec<PShape>,
        n: i64,
        offset_pct: f64,
        align: AlignMode,
        direction: Orientation,
    },
    Polar {
        center: PPoint,
        motifs: Vec<PShape>,
        n: i64,
        radius: f64,
        start_deg: f64,
        align: Option<AlignMode>,
    },
    Grid {
        motifs: Vec<PShape>,
        cols: i64,
        rows: i64,
        dx: f64,
        dy: f64,
        origin: PPoint,
    },
    GridGuide {
        origin: PPoint,
        cols: i64,
        rows: i64,
        dx: f64,
        dy: f64,
    },
    Rounded {
        shape: Box<PShape>,
        radius: f64,
    },
    Use {
        def_name: String,
    },
    Rect {
        center: PPoint,
        width: f64,
        height: f64,
    },
    Pie {
        center: PPoint,
        radius: f64,
        start_deg: f64,
        sweep_deg: f64,
        chord: bool,
    },
    Transform {
        t: [f64; 6],
        shape: Box<PShape>,
    },
    Text {
        at: PPoint,
        content: String,
        size: f64,
        font: String,
        anchor: String,
    },
}

#[derive(Debug, Clone)]
pub struct PShape {
    pub kind: SKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Fill,
    Stroke,
    OutlineFill,
    Text,
}

impl OpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OpKind::Fill => "fill",
            OpKind::Stroke => "stroke",
            OpKind::OutlineFill => "outline_fill",
            OpKind::Text => "text",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub op: OpKind,
    pub visible: bool,
    pub shape: PShape,
    pub paint: Paint,
    pub stroke_width: f64,
    pub outline_paint: Option<Paint>,
    pub markers: Vec<Marker>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub placement: String, // start | end | both
    pub kind: String,      // triangle | bar
    pub size: f64,
    pub paint: Option<Paint>,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<Node>,
    pub defs: Vec<(String, PShape)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appendix A is binary-searched: the list MUST stay sorted.
    #[test]
    fn reserved_is_sorted() {
        for pair in RESERVED.windows(2) {
            assert!(pair[0] < pair[1], "RESERVED out of order: {pair:?}");
        }
    }

    #[test]
    fn reserved_covers_the_grammar() {
        for w in [
            "arc_between",
            "bar",
            "both",
            "content",
            "def",
            "deg",
            "fill",
            "intersects",
            "let",
            "marker",
            "repeat",
            "rounded",
            "sans",
            "text",
            "triangle",
            "use",
        ] {
            assert!(is_reserved(w), "`{w}` must be reserved");
        }
    }
}
