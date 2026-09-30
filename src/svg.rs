//! SVG renderer: resolved ops → an SVG document, for preview and testing.

use crate::geom::Pt;
use crate::ir::OpKind;
use crate::resolve::{RInstr, ROp, RPaint, RShape, RSubPath};

/// Python-parity number formatting: 3 decimals, trailing zeros trimmed.
fn fmt3(v: f64) -> String {
    let t = format!("{v:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn font_family(font: &str) -> &str {
    match font {
        "sans" => "Noto Sans",
        other => other,
    }
}

fn text_element(op: &ROp, meta: &crate::resolve::TextMeta, gradient_id: Option<&str>) -> String {
    let shift = match meta.anchor.as_str() {
        "middle" => 0.5,
        "end" => 1.0,
        _ => 0.0,
    };
    let x = meta.at.x - meta.width * shift;
    let fill = match gradient_id {
        Some(id) => format!("fill=\"url(#{id})\""),
        None => match &op.paint {
            RPaint::Color(c) => {
                let mut s = format!("fill=\"{}\"", css_color(*c));
                if c.a < 1.0 {
                    s.push_str(&format!(" fill-opacity=\"{:.3}\"", c.a));
                }
                s
            }
            _ => "fill=\"none\"".to_string(),
        },
    };
    format!(
        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" text-anchor=\"{}\" {}>{}</text>",
        fmt3(x),
        fmt3(meta.at.y),
        font_family(&meta.font),
        fmt3(meta.size),
        meta.anchor,
        fill,
        escape_text(&meta.content)
    )
}

fn fmt_f(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        let s = format!("{v:?}");
        s
    }
}

fn css_color(c: crate::ir::Color) -> String {
    let hex = |v: f64| -> String { format!("{:02X}", (v * 255.0).round().clamp(0.0, 255.0) as u8) };
    if c.a >= 1.0 {
        format!("#{}{}{}", hex(c.r), hex(c.g), hex(c.b))
    } else {
        format!(
            "#{}{}{}{:02X}",
            hex(c.r),
            hex(c.g),
            hex(c.b),
            (c.a * 255.0).round() as u8
        )
    }
}

pub fn render(ops: &[ROp], width: f64, height: f64) -> String {
    let mut out = String::new();
    let mut defs = String::new();
    let mut body = String::new();

    for op in ops {
        let gradient_fill = match (&op.paint, op.kind) {
            (RPaint::Linear { .. }, OpKind::Fill | OpKind::Text)
            | (RPaint::Radial { .. }, OpKind::Fill | OpKind::Text) => {
                let id = format!("g{}", defs_matches(&defs));
                push_gradient_def(&mut defs, &id, &op.paint);
                Some(id)
            }
            _ => None,
        };
        let gradient_stroke = match (&op.paint, op.kind) {
            (RPaint::Linear { .. }, OpKind::Stroke) | (RPaint::Radial { .. }, OpKind::Stroke) => {
                let id = format!("g{}", defs_matches(&defs));
                push_gradient_def(&mut defs, &id, &op.paint);
                Some(id)
            }
            _ => None,
        };
        let outline_gradient = match (&op.outline_paint, op.kind) {
            (Some(RPaint::Linear { .. }), OpKind::OutlineFill)
            | (Some(RPaint::Radial { .. }), OpKind::OutlineFill) => {
                let id = format!("g{}", defs_matches(&defs));
                push_gradient_def(&mut defs, &id, op.outline_paint.as_ref().unwrap());
                Some(id)
            }
            _ => None,
        };

        let fill = if op.kind == OpKind::Stroke {
            "none".to_string()
        } else if let Some(id) = &gradient_fill {
            format!("url(#{id})")
        } else {
            match &op.paint {
                RPaint::Color(c) => css_color(*c),
                _ => "none".into(),
            }
        };

        let stroke = if op.kind == OpKind::Fill {
            None
        } else {
            let p = if op.kind == OpKind::OutlineFill {
                op.outline_paint.as_ref().unwrap()
            } else {
                &op.paint
            };
            let color = if let Some(id) = if op.kind == OpKind::OutlineFill {
                &outline_gradient
            } else {
                &gradient_stroke
            } {
                format!("url(#{id})")
            } else if let RPaint::Color(c) = p {
                css_color(*c)
            } else {
                "none".into()
            };
            Some(color)
        };

        let fill_rule = if matches!(op.shape, RShape::Compound(_)) {
            " fill-rule=\"evenodd\""
        } else {
            ""
        };

        let elements: Vec<String> = if let Some(meta) = &op.text {
            vec![text_element(op, meta, gradient_fill.as_deref())]
        } else {
            match (&op.shape, op.kind) {
                (RShape::Polygon(pts), _) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    vec![format!(
                        "<polygon{} points=\"{}\"/>",
                        attrs,
                        points_attr(pts)
                    )]
                }
                (RShape::Polyline(pts), _) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    vec![format!(
                        "<polyline{} points=\"{}\"/>",
                        attrs,
                        points_attr(pts)
                    )]
                }
                (RShape::Circle { c, r }, _) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    vec![format!(
                        "<circle{} cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                        attrs,
                        fmt_f(c.x),
                        fmt_f(c.y),
                        fmt_f(*r)
                    )]
                }
                (
                    RShape::Ellipse {
                        c,
                        rx,
                        ry,
                        rotation_deg,
                        ..
                    },
                    _,
                ) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    let rot = if *rotation_deg != 0.0 {
                        format!(
                            " transform=\"rotate({} {} {})\"",
                            fmt_f(*rotation_deg),
                            fmt_f(c.x),
                            fmt_f(c.y)
                        )
                    } else {
                        String::new()
                    };
                    vec![format!(
                        "<ellipse{} cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}/>",
                        attrs,
                        fmt_f(c.x),
                        fmt_f(c.y),
                        fmt_f(*rx),
                        fmt_f(*ry),
                        rot
                    )]
                }
                (
                    RShape::Arc {
                        c,
                        r,
                        start_deg,
                        sweep_deg,
                    },
                    _,
                ) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    let rad0 = start_deg.to_radians();
                    let rad1 = (start_deg + sweep_deg).to_radians();
                    let s = Pt::new(c.x + r * rad0.cos(), c.y + r * rad0.sin());
                    let e = Pt::new(c.x + r * rad1.cos(), c.y + r * rad1.sin());
                    let large = if sweep_deg.abs() > 180.0 { 1 } else { 0 };
                    let sweep = if *sweep_deg > 0.0 { 1 } else { 0 };
                    vec![format!(
                        "<path{} d=\"M {} {} A {} {} 0 {} {} {} {}\"/>",
                        attrs,
                        fmt_f(s.x),
                        fmt_f(s.y),
                        fmt_f(*r),
                        fmt_f(*r),
                        large,
                        sweep,
                        fmt_f(e.x),
                        fmt_f(e.y)
                    )]
                }
                (RShape::Path { subpaths, .. }, _) => {
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    vec![format!("<path{} d=\"{}\"/>", attrs, path_attr(subpaths))]
                }
                (RShape::Compound(subs), OpKind::Stroke) => subs
                    .iter()
                    .map(|s| match s {
                        RShape::Polygon(pts) => {
                            let attrs = common_attrs(&fill, &stroke, op.width, "");
                            format!("<polygon{} points=\"{}\"/>", attrs, points_attr(pts))
                        }
                        RShape::Polyline(pts) => {
                            let attrs = common_attrs(&fill, &stroke, op.width, "");
                            format!("<polyline{} points=\"{}\"/>", attrs, points_attr(pts))
                        }
                        RShape::Circle { c, r } => {
                            let attrs = common_attrs(&fill, &stroke, op.width, "");
                            format!(
                                "<circle{} cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                                attrs,
                                fmt_f(c.x),
                                fmt_f(c.y),
                                fmt_f(*r)
                            )
                        }
                        RShape::Ellipse {
                            c,
                            rx,
                            ry,
                            rotation_deg,
                            ..
                        } => {
                            let attrs = common_attrs(&fill, &stroke, op.width, "");
                            let rot = if *rotation_deg != 0.0 {
                                format!(
                                    " transform=\"rotate({} {} {})\"",
                                    fmt_f(*rotation_deg),
                                    fmt_f(c.x),
                                    fmt_f(c.y)
                                )
                            } else {
                                String::new()
                            };
                            format!(
                                "<ellipse{} cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}/>",
                                attrs,
                                fmt_f(c.x),
                                fmt_f(c.y),
                                fmt_f(*rx),
                                fmt_f(*ry),
                                rot
                            )
                        }
                        RShape::Path { subpaths, .. } => {
                            let attrs = common_attrs(&fill, &stroke, op.width, "");
                            format!("<path{} d=\"{}\"/>", attrs, path_attr(subpaths))
                        }
                        _ => String::new(),
                    })
                    .collect(),
                (RShape::Compound(subs), _) => {
                    // even-odd compound fill: flatten all subpaths into one path
                    let all: Vec<RSubPath> = subs
                        .iter()
                        .flat_map(|s| match s {
                            RShape::Path { subpaths, .. } => subpaths.clone(),
                            RShape::Circle { c, r } => vec![RSubPath {
                                start: Pt::new(c.x + r, c.y),
                                instructions: vec![
                                    RInstr::ArcCircle {
                                        radius: *r,
                                        large: false,
                                        sweep_cw: true,
                                        to: Pt::new(c.x - r, c.y),
                                    },
                                    RInstr::ArcCircle {
                                        radius: *r,
                                        large: false,
                                        sweep_cw: true,
                                        to: Pt::new(c.x + r, c.y),
                                    },
                                    RInstr::Close,
                                ],
                            }],
                            RShape::Ellipse {
                                c,
                                rx,
                                ry,
                                rotation_deg,
                                ..
                            } => {
                                let p0 =
                                    crate::resolve::ellipse_point(*c, *rx, *ry, *rotation_deg, 0.0);
                                let p1 = crate::resolve::ellipse_point(
                                    *c,
                                    *rx,
                                    *ry,
                                    *rotation_deg,
                                    std::f64::consts::PI,
                                );
                                let arc = |to: Pt| RInstr::ArcEllipse {
                                    rx: *rx,
                                    ry: *ry,
                                    rotation_deg: *rotation_deg,
                                    large: false,
                                    sweep_cw: true,
                                    to,
                                };
                                vec![RSubPath {
                                    start: p0,
                                    instructions: vec![arc(p1), arc(p0), RInstr::Close],
                                }]
                            }
                            RShape::Polygon(pts) => {
                                let mut instructions: Vec<RInstr> =
                                    pts[1..].iter().map(|p| RInstr::Line { to: *p }).collect();
                                instructions.push(RInstr::Close);
                                vec![RSubPath {
                                    start: pts[0],
                                    instructions,
                                }]
                            }
                            _ => vec![],
                        })
                        .collect();
                    let attrs = common_attrs(&fill, &stroke, op.width, fill_rule);
                    vec![format!("<path{} d=\"{}\"/>", attrs, path_attr(&all))]
                }
            }
        };

        for e in elements {
            body.push_str("  ");
            body.push_str(&e);
            body.push('\n');
        }
    }

    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        fmt3(width),
        fmt3(height),
        fmt3(width),
        fmt3(height)
    ));
    if !defs.is_empty() {
        out.push_str(&format!("  <defs>\n{defs}  </defs>\n"));
    }
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

fn defs_matches(defs: &str) -> usize {
    defs.matches("<linearGradient").count() + defs.matches("<radialGradient").count()
}

fn push_gradient_def(defs: &mut String, id: &str, paint: &RPaint) {
    match paint {
        RPaint::Linear {
            start,
            end,
            start_color,
            end_color,
        } => {
            defs.push_str(&format!(
                "    <linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">\n",
                fmt_f(start.x),
                fmt_f(start.y),
                fmt_f(end.x),
                fmt_f(end.y)
            ));
            defs.push_str(&format!(
                "      <stop offset=\"0\" stop-color=\"{}\"/>\n",
                css_color(*start_color)
            ));
            defs.push_str(&format!(
                "      <stop offset=\"1\" stop-color=\"{}\"/>\n",
                css_color(*end_color)
            ));
            defs.push_str("    </linearGradient>\n");
        }
        RPaint::Radial {
            center,
            edge,
            center_color,
            edge_color,
        } => {
            let r = center.dist(*edge);
            defs.push_str(&format!(
                "    <radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">\n",
                fmt_f(center.x),
                fmt_f(center.y),
                fmt_f(r)
            ));
            defs.push_str(&format!(
                "      <stop offset=\"0\" stop-color=\"{}\"/>\n",
                css_color(*center_color)
            ));
            defs.push_str(&format!(
                "      <stop offset=\"1\" stop-color=\"{}\"/>\n",
                css_color(*edge_color)
            ));
            defs.push_str("    </radialGradient>\n");
        }
        RPaint::Color(_) => {}
    }
}

fn common_attrs(fill: &str, stroke: &Option<String>, width: f64, fill_rule: &str) -> String {
    let mut s = String::new();
    if !fill_rule.is_empty() {
        s.push_str(&format!(" {fill_rule}"));
    }
    s.push_str(&format!(" fill=\"{fill}\""));
    if let Some(st) = stroke {
        s.push_str(&format!(
            " stroke=\"{st}\" stroke-width=\"{}\"",
            fmt_f(width)
        ));
    }
    s
}

fn points_attr(pts: &[Pt]) -> String {
    pts.iter()
        .map(|p| format!("{},{}", fmt_f(p.x), fmt_f(p.y)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_attr(subpaths: &[RSubPath]) -> String {
    let mut d = String::new();
    for sub in subpaths {
        d.push_str(&format!("M {} {} ", fmt_f(sub.start.x), fmt_f(sub.start.y)));
        for instr in &sub.instructions {
            match instr {
                RInstr::Line { to } => d.push_str(&format!("L {} {} ", fmt_f(to.x), fmt_f(to.y))),
                RInstr::Quad { ctrl, to } => d.push_str(&format!(
                    "Q {} {} {} {} ",
                    fmt_f(ctrl.x),
                    fmt_f(ctrl.y),
                    fmt_f(to.x),
                    fmt_f(to.y)
                )),
                RInstr::Cubic { c1, c2, to } => d.push_str(&format!(
                    "C {} {} {} {} {} {} ",
                    fmt_f(c1.x),
                    fmt_f(c1.y),
                    fmt_f(c2.x),
                    fmt_f(c2.y),
                    fmt_f(to.x),
                    fmt_f(to.y)
                )),
                RInstr::ArcCircle {
                    radius,
                    large,
                    sweep_cw,
                    to,
                } => d.push_str(&format!(
                    "A {} {} 0 {} {} {} {} ",
                    fmt_f(*radius),
                    fmt_f(*radius),
                    *large as u8,
                    *sweep_cw as u8,
                    fmt_f(to.x),
                    fmt_f(to.y)
                )),
                RInstr::ArcEllipse {
                    rx,
                    ry,
                    rotation_deg,
                    large,
                    sweep_cw,
                    to,
                } => d.push_str(&format!(
                    "A {} {} {} {} {} {} {} ",
                    fmt_f(*rx),
                    fmt_f(*ry),
                    fmt_f(*rotation_deg),
                    *large as u8,
                    *sweep_cw as u8,
                    fmt_f(to.x),
                    fmt_f(to.y)
                )),
                RInstr::Close => d.push_str("Z "),
            }
        }
    }
    d.trim_end().to_string()
}
