//! TinyVG 1.0 binary writer — a faithful port of `windvg.tinyvg` (spec §8).
//! Quantization uses round-half-to-even; ellipse rotation is negated on
//! output; RGBA8888 colors; 16-bit units with automatic 32-bit upgrade.

use crate::geom::round_half_even;
use crate::ir::{Color, OpKind};
use crate::resolve::{RInstr, ROp, RPaint, RShape};

const END_OF_DOCUMENT: u8 = 0x00;
const FILL_POLYGON: u8 = 1;
const FILL_PATH: u8 = 3;
const DRAW_LINE_LOOP: u8 = 5;
const DRAW_LINE_STRIP: u8 = 6;
const DRAW_LINE_PATH: u8 = 7;
const OUTLINE_FILL_POLYGON: u8 = 8;
const OUTLINE_FILL_PATH: u8 = 10;

const STYLE_FLAT: u8 = 0;
const STYLE_LINEAR: u8 = 1;
const STYLE_RADIAL: u8 = 2;

const INSTR_LINE: u8 = 0;
const INSTR_ARC_CIRCLE: u8 = 4;
const INSTR_ARC_ELLIPSE: u8 = 5;
const INSTR_CLOSE_PATH: u8 = 6;
const INSTR_CUBIC: u8 = 3;
const INSTR_QUAD: u8 = 7;

const MAX_OUTLINE_SEGMENTS: usize = 64;

pub fn encode(ops: &[ROp], width: f64, height: f64, scale: u32) -> Result<Vec<u8>, String> {
    if scale > 15 {
        return Err("scale must fit in 4 bits (0..15)".into());
    }
    let coord_range = choose_coord_range(ops, scale)?;
    let bits: u32 = match coord_range {
        0 => 16,
        2 => 32,
        _ => unreachable!(),
    };

    let mut w = Writer {
        buf: Vec::new(),
        scale,
        bits,
    };

    // header
    w.buf.extend_from_slice(&[0x72, 0x56]);
    w.byte(1); // version
    w.byte((scale as u8) | (coord_range << 6)); // color_encoding 0 (RGBA8888) << 4
    let dim_bytes = (bits / 8) as usize;
    for dim in [width, height] {
        let value = round_half_even(dim);
        if !(0.0 < value && value < (1i64 << (bits - 1)) as f64) {
            return Err(format!(
                "scene dimension {dim} out of range for {bits}-bit headers"
            ));
        }
        let v = value as i64 as u64;
        for i in 0..dim_bytes {
            w.buf.push(((v >> (8 * i)) & 0xff) as u8);
        }
    }

    // color table
    let colors = collect_colors(ops);
    w.varuint(colors.len() as u64);
    for c in &colors {
        for ch in rgba8(*c) {
            w.buf.push(ch);
        }
    }

    // commands
    for op in ops {
        emit_op(&mut w, op, &colors)?;
    }
    w.byte(END_OF_DOCUMENT);
    Ok(w.buf)
}

// ---- command emission ---------------------------------------------------------

struct Writer {
    buf: Vec<u8>,
    scale: u32,
    bits: u32,
}

impl Writer {
    fn byte(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn varuint(&mut self, v: u64) {
        let mut value = v;
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                self.buf.push(byte | 0x80);
            } else {
                self.buf.push(byte);
                return;
            }
        }
    }

    fn unit(&mut self, value: f64) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!("non-finite coordinate {value}"));
        }
        let q = round_half_even(value * (2f64).powi(self.scale as i32));
        let limit = (1i64 << (self.bits - 1)) - 1;
        if q.abs() > limit as f64 {
            return Err(format!(
                "coordinate {value} does not fit in {}-bit units",
                self.bits
            ));
        }
        let q = q as i64;
        for i in 0..(self.bits / 8) as usize {
            self.buf.push((q.wrapping_shr(8 * i as u32) & 0xff) as u8);
        }
        Ok(())
    }

    fn point(&mut self, p: crate::geom::Pt) -> Result<(), String> {
        self.unit(p.x)?;
        self.unit(p.y)
    }

    fn command(&mut self, index: u8, style_kind: u8) {
        self.byte(index | (style_kind << 6));
    }

    fn paint_style(&mut self, paint: &RPaint, colors: &[Color]) -> Result<u8, String> {
        match paint {
            RPaint::Color(c) => {
                self.varuint(palette_index(colors, c) as u64);
                Ok(STYLE_FLAT)
            }
            RPaint::Linear {
                start,
                end,
                start_color,
                end_color,
            } => {
                self.point(*start)?;
                self.point(*end)?;
                self.varuint(palette_index(colors, start_color) as u64);
                self.varuint(palette_index(colors, end_color) as u64);
                Ok(STYLE_LINEAR)
            }
            RPaint::Radial {
                center,
                edge,
                center_color,
                edge_color,
            } => {
                self.point(*center)?;
                self.point(*edge)?;
                self.varuint(palette_index(colors, center_color) as u64);
                self.varuint(palette_index(colors, edge_color) as u64);
                Ok(STYLE_RADIAL)
            }
        }
    }

    fn paint_line_style(
        &mut self,
        paint: &RPaint,
        colors: &[Color],
        width: f64,
    ) -> Result<u8, String> {
        let kind = self.paint_style(paint, colors)?;
        self.unit(width)?;
        Ok(kind)
    }
}

fn palette_index(colors: &[Color], c: &Color) -> usize {
    colors
        .iter()
        .position(|x| x == c)
        .expect("color missing from table")
}

fn rgba8(c: Color) -> [u8; 4] {
    let q = |v: f64| -> u8 { (round_half_even(v * 255.0) as i64).clamp(0, 255) as u8 };
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

fn paint_colors(p: &RPaint) -> Vec<Color> {
    match p {
        RPaint::Color(c) => vec![*c],
        RPaint::Linear {
            start_color,
            end_color,
            ..
        } => vec![*start_color, *end_color],
        RPaint::Radial {
            center_color,
            edge_color,
            ..
        } => vec![*center_color, *edge_color],
    }
}

fn collect_colors(ops: &[ROp]) -> Vec<Color> {
    let mut colors: Vec<Color> = Vec::new();
    for op in ops {
        let mut paints: Vec<&RPaint> = vec![&op.paint];
        if let Some(o) = &op.outline_paint {
            paints.push(o);
        }
        for p in paints {
            for c in paint_colors(p) {
                if !colors.contains(&c) {
                    colors.push(c);
                }
            }
        }
    }
    colors
}

// ---- coordinate range selection ---------------------------------------------

fn choose_coord_range(ops: &[ROp], scale: u32) -> Result<u8, String> {
    let mut units: Vec<f64> = Vec::new();
    for op in ops {
        collect_shape_units(&op.shape, &mut units);
        for p in [Some(&op.paint), op.outline_paint.as_ref()]
            .into_iter()
            .flatten()
        {
            match p {
                RPaint::Linear { start, end, .. } => units.extend([start.x, start.y, end.x, end.y]),
                RPaint::Radial { center, edge, .. } => {
                    units.extend([center.x, center.y, edge.x, edge.y])
                }
                RPaint::Color(_) => {}
            }
        }
        if op.kind != OpKind::Fill {
            units.push(op.width);
        }
    }
    let (lo, hi) = match (
        units.iter().cloned().min_by(|a, b| a.total_cmp(b)),
        units.iter().cloned().max_by(|a, b| a.total_cmp(b)),
    ) {
        (Some(l), Some(h)) => (l, h),
        _ => (0.0, 0.0),
    };
    let factor = (2f64).powi(scale as i32);
    let lo_q = round_half_even(lo * factor);
    let hi_q = round_half_even(hi * factor);
    if lo_q >= -(2f64).powi(15) && hi_q <= (2f64).powi(15) - 1.0 {
        return Ok(0);
    }
    if lo_q >= -(2f64).powi(31) && hi_q <= (2f64).powi(31) - 1.0 {
        return Ok(2);
    }
    Err("coordinates do not fit even in 32-bit units; reduce the scale".into())
}

fn collect_shape_units(shape: &RShape, units: &mut Vec<f64>) {
    match shape {
        RShape::Compound(subs) => {
            for s in subs {
                collect_shape_units(s, units);
            }
        }
        RShape::Path { geoms, tol, .. } => {
            let mut lo = (f64::INFINITY, f64::INFINITY);
            let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in geoms.iter().flat_map(|g| g.chain.iter()) {
                lo.0 = lo.0.min(p.x);
                lo.1 = lo.1.min(p.y);
                hi.0 = hi.0.max(p.x);
                hi.1 = hi.1.max(p.y);
            }
            units.extend([lo.0 - tol, lo.1 - tol, hi.0 + tol, hi.1 + tol]);
        }
        RShape::Circle { c, r } => {
            units.extend([c.x - r, c.y - r, c.x + r, c.y + r]);
        }
        RShape::Ellipse {
            c,
            rx,
            ry,
            rotation_deg,
            ..
        } => {
            let phi = rotation_deg.to_radians();
            let (cp, sp) = (phi.cos(), phi.sin());
            let ex = ((rx * cp).powi(2) + (ry * sp).powi(2)).sqrt();
            let ey = ((rx * sp).powi(2) + (ry * cp).powi(2)).sqrt();
            units.extend([c.x - ex, c.y - ey, c.x + ex, c.y + ey]);
        }
        RShape::Arc {
            c,
            r,
            start_deg,
            sweep_deg,
        } => {
            for i in 0..65 {
                let deg = start_deg + sweep_deg * i as f64 / 64.0;
                let rad = deg.to_radians();
                units.push(c.x + r * rad.cos());
                units.push(c.y + r * rad.sin());
            }
        }
        RShape::Polygon(pts) | RShape::Polyline(pts) => {
            for p in pts {
                units.push(p.x);
                units.push(p.y);
            }
        }
    }
}

// ---- command emission ---------------------------------------------------------

fn emit_op(w: &mut Writer, op: &ROp, colors: &[Color]) -> Result<(), String> {
    let kind = op.kind;
    let fill_kind = style_kind_of(&op.paint);

    match &op.shape {
        RShape::Compound(subs) => {
            if kind == OpKind::OutlineFill {
                return Err("compound shapes cannot be outline-filled; fill each part".into());
            }
            if kind == OpKind::Stroke {
                for sub in subs {
                    let sub_op = ROp {
                        id: op.id.clone(),
                        kind: OpKind::Stroke,
                        shape: sub.clone(),
                        paint: op.paint,
                        outline_paint: None,
                        width: op.width,
                    };
                    emit_op(w, &sub_op, colors)?;
                }
            } else {
                w.command(FILL_PATH, fill_kind);
                w.varuint(segment_count(&op.shape) as u64 - 1);
                w.paint_style(&op.paint, colors)?;
                emit_shape_segments(w, &op.shape)?;
            }
            return Ok(());
        }
        RShape::Path { subpaths, .. } => {
            let segments = subpaths.len();
            match kind {
                OpKind::Stroke => {
                    w.command(DRAW_LINE_PATH, fill_kind);
                    w.varuint(segments as u64 - 1);
                    w.paint_line_style(&op.paint, colors, op.width)?;
                    emit_shape_segments(w, &op.shape)?;
                }
                OpKind::OutlineFill if segments <= MAX_OUTLINE_SEGMENTS => {
                    let outline_kind = style_kind_of(op.outline_paint.as_ref().unwrap());
                    w.command(OUTLINE_FILL_PATH, fill_kind);
                    w.byte((segments as u8 - 1) | (outline_kind << 6));
                    w.paint_style(&op.paint, colors)?;
                    w.paint_style(op.outline_paint.as_ref().unwrap(), colors)?;
                    w.unit(op.width)?;
                    emit_shape_segments(w, &op.shape)?;
                }
                OpKind::OutlineFill => {
                    let outline_kind = style_kind_of(op.outline_paint.as_ref().unwrap());
                    w.command(FILL_PATH, fill_kind);
                    w.varuint(segments as u64 - 1);
                    w.paint_style(&op.paint, colors)?;
                    emit_shape_segments(w, &op.shape)?;
                    w.command(DRAW_LINE_PATH, outline_kind);
                    w.varuint(segments as u64 - 1);
                    w.paint_line_style(op.outline_paint.as_ref().unwrap(), colors, op.width)?;
                    emit_shape_segments(w, &op.shape)?;
                }
                OpKind::Fill => {
                    w.command(FILL_PATH, fill_kind);
                    w.varuint(segments as u64 - 1);
                    w.paint_style(&op.paint, colors)?;
                    emit_shape_segments(w, &op.shape)?;
                }
            }
            return Ok(());
        }
        RShape::Circle { .. } | RShape::Ellipse { .. } => {
            match kind {
                OpKind::Stroke => {
                    w.command(DRAW_LINE_PATH, fill_kind);
                    w.varuint(0);
                    w.paint_line_style(&op.paint, colors, op.width)?;
                    emit_shape_segments(w, &op.shape)?;
                }
                OpKind::OutlineFill => {
                    let outline_kind = style_kind_of(op.outline_paint.as_ref().unwrap());
                    w.command(OUTLINE_FILL_PATH, fill_kind);
                    w.byte(outline_kind << 6);
                    w.paint_style(&op.paint, colors)?;
                    w.paint_style(op.outline_paint.as_ref().unwrap(), colors)?;
                    w.unit(op.width)?;
                    emit_shape_segments(w, &op.shape)?;
                }
                OpKind::Fill => {
                    w.command(FILL_PATH, fill_kind);
                    w.varuint(0);
                    w.paint_style(&op.paint, colors)?;
                    emit_shape_segments(w, &op.shape)?;
                }
            }
            return Ok(());
        }
        RShape::Arc { .. } => {
            if kind != OpKind::Stroke {
                return Err("arcs can only be stroked; fill a pie or chord instead".into());
            }
            w.command(DRAW_LINE_PATH, fill_kind);
            w.varuint(0);
            w.paint_line_style(&op.paint, colors, op.width)?;
            emit_arc_path(w, &op.shape)?;
            return Ok(());
        }
        RShape::Polygon(_) | RShape::Polyline(_) => {}
    }

    let points = match &op.shape {
        RShape::Polygon(p) | RShape::Polyline(p) => p,
        _ => unreachable!(),
    };
    let is_polyline = matches!(op.shape, RShape::Polyline(_));
    if kind == OpKind::OutlineFill && points.len() > MAX_OUTLINE_SEGMENTS {
        let outline_kind = style_kind_of(op.outline_paint.as_ref().unwrap());
        w.command(FILL_POLYGON, fill_kind);
        w.varuint(points.len() as u64 - 1);
        w.paint_style(&op.paint, colors)?;
        for p in points {
            w.point(*p)?;
        }
        w.command(DRAW_LINE_LOOP, outline_kind);
        w.varuint(points.len() as u64 - 1);
        w.paint_line_style(op.outline_paint.as_ref().unwrap(), colors, op.width)?;
        for p in points {
            w.point(*p)?;
        }
    } else {
        match kind {
            OpKind::Stroke => {
                let cmd = if is_polyline {
                    DRAW_LINE_STRIP
                } else {
                    DRAW_LINE_LOOP
                };
                w.command(cmd, fill_kind);
                w.varuint(points.len() as u64 - 1);
                w.paint_line_style(&op.paint, colors, op.width)?;
                for p in points {
                    w.point(*p)?;
                }
            }
            OpKind::OutlineFill => {
                let outline_kind = style_kind_of(op.outline_paint.as_ref().unwrap());
                w.command(OUTLINE_FILL_POLYGON, fill_kind);
                w.byte((points.len() as u8 - 1) | (outline_kind << 6));
                w.paint_style(&op.paint, colors)?;
                w.paint_style(op.outline_paint.as_ref().unwrap(), colors)?;
                w.unit(op.width)?;
                for p in points {
                    w.point(*p)?;
                }
            }
            OpKind::Fill => {
                w.command(FILL_POLYGON, fill_kind);
                w.varuint(points.len() as u64 - 1);
                w.paint_style(&op.paint, colors)?;
                for p in points {
                    w.point(*p)?;
                }
            }
        }
    }
    Ok(())
}

fn style_kind_of(p: &RPaint) -> u8 {
    match p {
        RPaint::Color(_) => STYLE_FLAT,
        RPaint::Linear { .. } => STYLE_LINEAR,
        RPaint::Radial { .. } => STYLE_RADIAL,
    }
}

fn segment_count(shape: &RShape) -> usize {
    match shape {
        RShape::Compound(subs) => subs.iter().map(segment_count).sum(),
        RShape::Path { subpaths, .. } => subpaths.len(),
        _ => 1,
    }
}

fn emit_segment_lengths(w: &mut Writer, shape: &RShape) -> Result<(), String> {
    match shape {
        RShape::Compound(subs) => {
            for s in subs {
                emit_segment_lengths(w, s)?;
            }
        }
        RShape::Path { subpaths, .. } => {
            for sub in subpaths {
                w.varuint(sub.instructions.len() as u64 - 1);
            }
        }
        RShape::Circle { .. } | RShape::Ellipse { .. } => w.varuint(2),
        RShape::Polygon(pts) | RShape::Polyline(pts) => w.varuint(pts.len() as u64 - 1),
        RShape::Arc { .. } => return Err("arc segments are emitted inline".into()),
    }
    Ok(())
}

fn emit_segment_bodies(w: &mut Writer, shape: &RShape) -> Result<(), String> {
    match shape {
        RShape::Compound(subs) => {
            for s in subs {
                emit_segment_bodies(w, s)?;
            }
        }
        RShape::Path { subpaths, .. } => {
            for sub in subpaths {
                w.point(sub.start)?;
                for instr in &sub.instructions {
                    emit_instruction(w, instr)?;
                }
            }
        }
        RShape::Circle { .. } | RShape::Ellipse { .. } => emit_closed_curve_path(w, shape)?,
        RShape::Polygon(pts) => {
            w.point(pts[0])?;
            for p in &pts[1..] {
                w.byte(INSTR_LINE);
                w.point(*p)?;
            }
            w.byte(INSTR_CLOSE_PATH);
        }
        RShape::Polyline(_) => return Err("polylines are stroked directly".into()),
        RShape::Arc { .. } => return Err("arc segments are emitted inline".into()),
    }
    Ok(())
}

fn emit_shape_segments(w: &mut Writer, shape: &RShape) -> Result<(), String> {
    emit_segment_lengths(w, shape)?;
    emit_segment_bodies(w, shape)
}

fn emit_closed_curve_path(w: &mut Writer, shape: &RShape) -> Result<(), String> {
    match shape {
        RShape::Circle { c, r } => {
            let opposite = crate::geom::Pt::new(c.x - r, c.y);
            let origin = crate::geom::Pt::new(c.x + r, c.y);
            w.point(origin)?;
            for target in [opposite, origin] {
                w.byte(INSTR_ARC_CIRCLE);
                emit_arc_flags(w, false, true)?;
                w.unit(*r)?;
                w.point(target)?;
            }
            w.byte(INSTR_CLOSE_PATH);
            Ok(())
        }
        RShape::Ellipse {
            c,
            rx,
            ry,
            rotation_deg,
            ..
        } => {
            let origin = crate::resolve::ellipse_point(*c, *rx, *ry, *rotation_deg, 0.0);
            let opposite =
                crate::resolve::ellipse_point(*c, *rx, *ry, *rotation_deg, std::f64::consts::PI);
            w.point(origin)?;
            for target in [opposite, origin] {
                w.byte(INSTR_ARC_ELLIPSE);
                emit_arc_flags(w, false, true)?;
                w.unit(*rx)?;
                w.unit(*ry)?;
                // TinyVG stores rotation in the mathematical-negative
                // direction: the opposite of clockwise-on-screen degrees.
                w.unit(-rotation_deg)?;
                w.point(target)?;
            }
            w.byte(INSTR_CLOSE_PATH);
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn emit_arc_path(w: &mut Writer, shape: &RShape) -> Result<(), String> {
    let RShape::Arc {
        c,
        r,
        start_deg,
        sweep_deg,
    } = shape
    else {
        unreachable!()
    };
    w.varuint(0); // command count - 1
    let rad0 = start_deg.to_radians();
    let rad1 = (start_deg + sweep_deg).to_radians();
    let start = crate::geom::Pt::new(c.x + r * rad0.cos(), c.y + r * rad0.sin());
    let end = crate::geom::Pt::new(c.x + r * rad1.cos(), c.y + r * rad1.sin());
    w.point(start)?;
    w.byte(INSTR_ARC_CIRCLE);
    emit_arc_flags(w, sweep_deg.abs() > 180.0, *sweep_deg > 0.0)?;
    w.unit(*r)?;
    w.point(end)
}

fn emit_arc_flags(w: &mut Writer, large_arc: bool, sweep_cw: bool) -> Result<(), String> {
    // flags: large_arc (bit 0), sweep (bit 1); sweep 0 = CW on screen
    w.byte((large_arc as u8) | (if sweep_cw { 0 } else { 0b10 }));
    Ok(())
}

fn emit_instruction(w: &mut Writer, instr: &RInstr) -> Result<(), String> {
    match instr {
        RInstr::Line { to } => {
            w.byte(INSTR_LINE);
            w.point(*to)?;
        }
        RInstr::Quad { ctrl, to } => {
            w.byte(INSTR_QUAD);
            w.point(*ctrl)?;
            w.point(*to)?;
        }
        RInstr::Cubic { c1, c2, to } => {
            w.byte(INSTR_CUBIC);
            w.point(*c1)?;
            w.point(*c2)?;
            w.point(*to)?;
        }
        RInstr::ArcCircle {
            radius,
            large,
            sweep_cw,
            to,
        } => {
            w.byte(INSTR_ARC_CIRCLE);
            emit_arc_flags(w, *large, *sweep_cw)?;
            w.unit(*radius)?;
            w.point(*to)?;
        }
        RInstr::ArcEllipse {
            rx,
            ry,
            rotation_deg,
            large,
            sweep_cw,
            to,
        } => {
            w.byte(INSTR_ARC_ELLIPSE);
            emit_arc_flags(w, *large, *sweep_cw)?;
            w.unit(*rx)?;
            w.unit(*ry)?;
            w.unit(-rotation_deg)?; // TinyVG negates rotation
            w.point(*to)?;
        }
        RInstr::Close => w.byte(INSTR_CLOSE_PATH),
    }
    Ok(())
}
