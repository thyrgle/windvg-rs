//! JSON emitters for the two conformance artifacts: the document IR
//! (`Document.to_dict`) and the resolved ops (`resolve_to_json`), matching
//! the Python reference field-for-field.

use crate::geom::Pt;
use crate::ir::*;
use crate::resolve::{RInstr, ROp, RPaint, RShape, RSubPath};

fn f(v: f64) -> String {
    format!("{v:?}")
}

fn num_list(p: (f64, f64)) -> String {
    format!("[{}, {}]", f(p.0), f(p.1))
}

fn pt(p: Pt) -> String {
    format!("[{}, {}]", f(p.x), f(p.y))
}

fn color_json(c: Color) -> String {
    format!(
        "{{\"kind\": \"color\", \"rgba\": [{}, {}, {}, {}]}}",
        f(c.r),
        f(c.g),
        f(c.b),
        f(c.a)
    )
}

fn color_list(c: Color) -> String {
    format!("[{}, {}, {}, {}]", f(c.r), f(c.g), f(c.b), f(c.a))
}

fn paint_json(p: &Paint) -> String {
    match p {
        Paint::Color(c) => color_json(*c),
        Paint::Linear { start, end, start_color, end_color } => format!(
            "{{\"kind\": \"linear\", \"start\": {}, \"end\": {}, \"start_color\": {}, \"end_color\": {}}}",
            num_list(*start),
            num_list(*end),
            color_list(*start_color),
            color_list(*end_color)
        ),
        Paint::Radial { center, edge, center_color, edge_color } => format!(
            "{{\"kind\": \"radial\", \"center\": {}, \"edge\": {}, \"center_color\": {}, \"edge_color\": {}}}",
            num_list(*center),
            num_list(*edge),
            color_list(*center_color),
            color_list(*edge_color)
        ),
    }
}

fn rpaint_json(p: &RPaint) -> String {
    match p {
        RPaint::Color(c) => color_json(*c),
        RPaint::Linear { start, end, start_color, end_color } => format!(
            "{{\"kind\": \"linear\", \"start\": {}, \"end\": {}, \"start_color\": {}, \"end_color\": {}}}",
            pt(*start),
            pt(*end),
            color_list(*start_color),
            color_list(*end_color)
        ),
        RPaint::Radial { center, edge, center_color, edge_color } => format!(
            "{{\"kind\": \"radial\", \"center\": {}, \"edge\": {}, \"center_color\": {}, \"edge_color\": {}}}",
            pt(*center),
            pt(*edge),
            color_list(*center_color),
            color_list(*edge_color)
        ),
    }
}

fn point_json(p: &PPoint) -> String {
    match &p.kind {
        PKind::Literal(x, y) => num_list((*x, *y)),
        PKind::Anchor {
            node,
            pct,
            start,
            dir,
        } => {
            let mut s = format!("{{\"anchor\": {{\"node\": \"{node}\", \"pct\": {}", f(*pct));
            if let Some((x, y)) = start {
                s.push_str(&format!(", \"start\": {}", num_list((*x, *y))));
            }
            if *dir != Orientation::Cw {
                s.push_str(&format!(", \"direction\": \"{}\"", dir.as_str()));
            }
            s.push_str("}}");
            s
        }
        PKind::Segment { node, index, pct } => format!(
            "{{\"segment\": {{\"node\": \"{node}\", \"index\": {index}, \"pct\": {}}}}}",
            f(*pct)
        ),
        PKind::GridCell {
            node,
            col,
            row,
            offset,
        } => {
            let mut s =
                format!("{{\"grid_cell\": {{\"node\": \"{node}\", \"col\": {col}, \"row\": {row}");
            if let Some((x, y)) = offset {
                s.push_str(&format!(", \"offset\": {}", num_list((*x, *y))));
            }
            s.push_str("}}");
            s
        }
    }
}

fn align_json(a: AlignMode) -> &'static str {
    match a {
        AlignMode::Tangent => "\"tangent\"",
        AlignMode::Off => "null",
    }
}

fn polar_align_json(a: Option<AlignMode>) -> &'static str {
    match a {
        Some(AlignMode::Tangent) => "\"tangent\"",
        _ => "null",
    }
}

fn shape_json(s: &PShape) -> String {
    match &s.kind {
        SKind::Circle { center, radius } => format!(
            "{{\"kind\": \"circle\", \"center\": {}, \"radius\": {}}}",
            point_json(center),
            f(*radius)
        ),
        SKind::Ellipse { center, rx, ry, rotation_deg } => format!(
            "{{\"kind\": \"ellipse\", \"center\": {}, \"rx\": {}, \"ry\": {}, \"rotation_deg\": {}}}",
            point_json(center),
            f(*rx),
            f(*ry),
            f(*rotation_deg)
        ),
        SKind::Arc { center, radius, start_deg, sweep_deg } => format!(
            "{{\"kind\": \"arc\", \"center\": {}, \"radius\": {}, \"start_deg\": {}, \"sweep_deg\": {}}}",
            point_json(center),
            f(*radius),
            f(*start_deg),
            f(*sweep_deg)
        ),
        SKind::Polygon { points } => points_list_json("polygon", points),
        SKind::Polyline { points } => points_list_json("polyline", points),
        SKind::Path { subpaths } => {
            let subs: Vec<String> = subpaths
                .iter()
                .map(|sub| {
                    let instrs: Vec<String> =
                        sub.instructions.iter().map(instr_json).collect();
                    format!(
                        "{{\"start\": {}, \"instructions\": [{}]}}",
                        point_json(&sub.start),
                        instrs.join(", ")
                    )
                })
                .collect();
            format!("{{\"kind\": \"path\", \"subpaths\": [{}]}}", subs.join(", "))
        }
        SKind::Compound { shapes } => {
            let inner: Vec<String> = shapes.iter().map(shape_json).collect();
            format!("{{\"kind\": \"compound\", \"shapes\": [{}]}}", inner.join(", "))
        }
        SKind::Along { track, motifs, n, offset_pct, align, direction } => format!(
            "{{\"kind\": \"along\", \"track\": {}, \"motifs\": [{}], \"n\": {}, \"offset_pct\": {}, \"align\": {}, \"direction\": \"{}\"}}",
            shape_json(track),
            motif_list_json(motifs),
            n,
            f(*offset_pct),
            align_json(*align),
            direction.as_str()
        ),
        SKind::Polar { center, motifs, n, radius, start_deg, align } => format!(
            "{{\"kind\": \"polar\", \"center\": {}, \"motifs\": [{}], \"n\": {}, \"radius\": {}, \"start_deg\": {}, \"align\": {}}}",
            point_json(center),
            motif_list_json(motifs),
            n,
            f(*radius),
            f(*start_deg),
            polar_align_json(*align)
        ),
        SKind::Grid { motifs, cols, rows, dx, dy, origin } => format!(
            "{{\"kind\": \"grid\", \"motifs\": [{}], \"cols\": {}, \"rows\": {}, \"dx\": {}, \"dy\": {}, \"origin\": {}}}",
            motif_list_json(motifs),
            cols,
            rows,
            f(*dx),
            f(*dy),
            point_json(origin)
        ),
        SKind::GridGuide { origin, cols, rows, dx, dy } => format!(
            "{{\"kind\": \"grid_guide\", \"origin\": {}, \"cols\": {}, \"rows\": {}, \"dx\": {}, \"dy\": {}}}",
            point_json(origin),
            cols,
            rows,
            f(*dx),
            f(*dy)
        ),
        SKind::Rounded { shape, radius } => format!(
            "{{\"kind\": \"rounded\", \"shape\": {}, \"radius\": {}}}",
            shape_json(shape),
            f(*radius)
        ),
    }
}

fn points_list_json(kind: &str, points: &[PPoint]) -> String {
    let pts: Vec<String> = points.iter().map(point_json).collect();
    format!("{{\"kind\": \"{kind}\", \"points\": [{}]}}", pts.join(", "))
}

fn motif_list_json(motifs: &[PShape]) -> String {
    motifs.iter().map(shape_json).collect::<Vec<_>>().join(", ")
}

fn instr_json(i: &PInstr) -> String {
    match &i.kind {
        IKind::Line { to } => format!("{{\"cmd\": \"line\", \"to\": {}}}", point_json(to)),
        IKind::Quad { ctrl, to } => format!(
            "{{\"cmd\": \"quad\", \"ctrl\": {}, \"to\": {}}}",
            point_json(ctrl),
            point_json(to)
        ),
        IKind::Cubic { c1, c2, to } => format!(
            "{{\"cmd\": \"cubic\", \"c1\": {}, \"c2\": {}, \"to\": {}}}",
            point_json(c1),
            point_json(c2),
            point_json(to)
        ),
        IKind::ArcCircle { radius, large, sweep_cw, to } => format!(
            "{{\"cmd\": \"arc_circle\", \"radius\": {}, \"large\": {}, \"sweep_cw\": {}, \"to\": {}}}",
            f(*radius),
            large,
            sweep_cw,
            point_json(to)
        ),
        IKind::ArcEllipse { rx, ry, rotation_deg, large, sweep_cw, to } => format!(
            "{{\"cmd\": \"arc_ellipse\", \"rx\": {}, \"ry\": {}, \"rotation_deg\": {}, \"large\": {}, \"sweep_cw\": {}, \"to\": {}}}",
            f(*rx),
            f(*ry),
            f(*rotation_deg),
            large,
            sweep_cw,
            point_json(to)
        ),
        IKind::Close => "{\"cmd\": \"close\"}".into(),
    }
}

pub fn document_json(doc: &Document) -> String {
    let nodes: Vec<String> = doc
        .nodes
        .iter()
        .map(|n| {
            let mut s = format!(
                "{{\"id\": \"{}\", \"name\": \"{}\", \"op\": \"{}\", \"visible\": {}, \"shape\": {}, \"paint\": {}, \"stroke_width\": {}",
                n.id,
                n.name,
                n.op.as_str(),
                n.visible,
                shape_json(&n.shape),
                paint_json(&n.paint),
                f(n.stroke_width)
            );
            if let Some(o) = &n.outline_paint {
                s.push_str(&format!(", \"outline_paint\": {}", paint_json(o)));
            }
            s.push('}');
            s
        })
        .collect();
    format!(
        "{{\"version\": 1, \"canvas\": [{}, {}], \"nodes\": [{}]}}",
        f(doc.width),
        f(doc.height),
        nodes.join(", ")
    )
}

// ---- resolved ops -------------------------------------------------------------

fn rpoint_list(pts: &[Pt]) -> String {
    let items: Vec<String> = pts.iter().map(|p| pt(*p)).collect();
    format!("[{}]", items.join(", "))
}

fn rinstr_json(i: &RInstr) -> String {
    match i {
        RInstr::Line { to } => format!("{{\"cmd\": \"line\", \"to\": {}}}", pt(*to)),
        RInstr::Quad { ctrl, to } => format!(
            "{{\"cmd\": \"quad\", \"ctrl\": {}, \"to\": {}}}",
            pt(*ctrl),
            pt(*to)
        ),
        RInstr::Cubic { c1, c2, to } => format!(
            "{{\"cmd\": \"cubic\", \"c1\": {}, \"c2\": {}, \"to\": {}}}",
            pt(*c1),
            pt(*c2),
            pt(*to)
        ),
        RInstr::ArcCircle { radius, large, sweep_cw, to } => format!(
            "{{\"cmd\": \"arc_circle\", \"radius\": {}, \"large\": {}, \"sweep_cw\": {}, \"to\": {}}}",
            f(*radius),
            large,
            sweep_cw,
            pt(*to)
        ),
        RInstr::ArcEllipse { rx, ry, rotation_deg, large, sweep_cw, to } => format!(
            "{{\"cmd\": \"arc_ellipse\", \"rx\": {}, \"ry\": {}, \"rotation_deg\": {}, \"large\": {}, \"sweep_cw\": {}, \"to\": {}}}",
            f(*rx),
            f(*ry),
            f(*rotation_deg),
            large,
            sweep_cw,
            pt(*to)
        ),
        RInstr::Close => "{\"cmd\": \"close\"}".into(),
    }
}

fn rsub_json(sub: &RSubPath) -> String {
    let instrs: Vec<String> = sub.instructions.iter().map(rinstr_json).collect();
    format!(
        "{{\"start\": {}, \"instructions\": [{}]}}",
        pt(sub.start),
        instrs.join(", ")
    )
}

/// Resolved shape dict. Compound fills have no Python-reference spelling
/// (resolve_to_json cannot represent them); we emit a compound dict.
fn rshape_json(s: &RShape) -> String {
    match s {
        RShape::Polygon(pts) => format!("{{\"kind\": \"polygon\", \"points\": {}}}", rpoint_list(pts)),
        RShape::Polyline(pts) => {
            format!("{{\"kind\": \"polyline\", \"points\": {}}}", rpoint_list(pts))
        }
        RShape::Circle { c, r } => format!(
            "{{\"kind\": \"circle\", \"center\": {}, \"radius\": {}}}",
            pt(*c),
            f(*r)
        ),
        RShape::Ellipse { c, rx, ry, rotation_deg, .. } => format!(
            "{{\"kind\": \"ellipse\", \"center\": {}, \"rx\": {}, \"ry\": {}, \"rotation_deg\": {}}}",
            pt(*c),
            f(*rx),
            f(*ry),
            f(*rotation_deg)
        ),
        RShape::Arc { c, r, start_deg, sweep_deg } => format!(
            "{{\"kind\": \"arc\", \"center\": {}, \"radius\": {}, \"start_deg\": {}, \"sweep_deg\": {}}}",
            pt(*c),
            f(*r),
            f(*start_deg),
            f(*sweep_deg)
        ),
        RShape::Path { subpaths, .. } => {
            let subs: Vec<String> = subpaths.iter().map(rsub_json).collect();
            format!("{{\"kind\": \"path\", \"subpaths\": [{}]}}", subs.join(", "))
        }
        RShape::Compound(subs) => {
            let inner: Vec<String> = subs.iter().map(rshape_json).collect();
            format!("{{\"kind\": \"compound\", \"shapes\": [{}]}}", inner.join(", "))
        }
    }
}

pub fn ops_json(ops: &[ROp]) -> String {
    let items: Vec<String> = ops
        .iter()
        .map(|op| {
            let (lo, hi) = op.shape.bbox();
            let outline = match &op.outline_paint {
                Some(p) => rpaint_json(p),
                None => "null".into(),
            };
            format!(
                "{{\"id\": \"{}\", \"op\": \"{}\", \"shape\": {}, \"paint\": {}, \"stroke_width\": {}, \"outline_paint\": {}, \"bbox\": [{}, {}, {}, {}]}}",
                op.id,
                op.kind.as_str(),
                rshape_json(&op.shape),
                rpaint_json(&op.paint),
                f(op.width),
                outline,
                f(lo.x),
                f(lo.y),
                f(hi.x),
                f(hi.y)
            )
        })
        .collect();
    format!("[{}]", items.join(", "))
}
