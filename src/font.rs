//! Bundled-font text baking (spec §7.19).
//!
//! Maps a string through the bundled Noto Sans Regular font (OFL 1.1):
//! cmap lookup, per-character advances, TrueType outlines (ttf-parser
//! already decomposes qCurve implied on-curves into successive
//! `quad_to` calls), scaled by `size / units_per_em` and flipped into
//! y-down canvas space. Kerning is off in v1. One `RShape::Path` per
//! glyph; counters (holes) rely on even-odd fill.

use crate::geom::Pt;
use crate::resolve::{RInstr, RShape, RSubPath};

static FONT_DATA: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

pub const BUNDLED_FONTS: &[&str] = &["sans"];

fn face() -> ttf_parser::Face<'static> {
    ttf_parser::Face::parse(FONT_DATA, 0).expect("bundled font is valid")
}

fn check_font(font: &str) -> Result<(), String> {
    if BUNDLED_FONTS.contains(&font) {
        Ok(())
    } else {
        Err(format!("unknown font `{font}` (bundled: sans)"))
    }
}

fn anchor_shift(anchor: &str) -> Result<f64, String> {
    match anchor {
        "start" => Ok(0.0),
        "middle" => Ok(0.5),
        "end" => Ok(1.0),
        other => Err(format!("unknown anchor `{other}`")),
    }
}

/// Total advance width of `content` at `size` in `font`.
pub fn measure(content: &str, size: f64, font: &str) -> Result<f64, String> {
    check_font(font)?;
    let face = face();
    let upem = f64::from(face.units_per_em());
    let scale = size / upem;
    // Python parity: sum the integer advances first, then scale once.
    let mut total_units = 0u64;
    for ch in content.chars() {
        let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
        let adv = face
            .glyph_hor_advance(gid)
            .ok_or_else(|| format!("no advance for U+{:04X}", ch as u32))?;
        total_units += u64::from(adv);
    }
    Ok(total_units as f64 * scale)
}

/// Bake `content` into glyph paths with the anchored baseline start at `at`.
pub fn bake(
    at: Pt,
    content: &str,
    size: f64,
    font: &str,
    anchor: &str,
) -> Result<Vec<RShape>, String> {
    check_font(font)?;
    let shift = anchor_shift(anchor)?;
    let face = face();
    let upem = f64::from(face.units_per_em());
    let scale = size / upem;
    let width = measure(content, size, font)?;
    let pen_x0 = at.x - width * shift;
    let mut out = Vec::new();
    let mut pen_x = pen_x0;
    for ch in content.chars() {
        let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
        let adv = face
            .glyph_hor_advance(gid)
            .ok_or_else(|| format!("no advance for U+{:04X}", ch as u32))?;
        let mut pen = GlyphPen::default();
        face.outline_glyph(gid, &mut pen);
        let subpaths: Vec<RSubPath> = pen
            .contours
            .drain(..)
            .map(|c| {
                let (start, instrs) = c;
                let sx = start.x;
                let sy = start.y;
                RSubPath {
                    start: Pt::new(pen_x + sx * scale, at.y - sy * scale),
                    instructions: instrs
                        .into_iter()
                        .map(|i| match i {
                            PenInstr::Line(x, y) => RInstr::Line {
                                to: Pt::new(
                                    pen_x + f64::from(x) * scale,
                                    at.y - f64::from(y) * scale,
                                ),
                            },
                            PenInstr::Quad(cx, cy, x, y) => RInstr::Quad {
                                ctrl: Pt::new(
                                    pen_x + f64::from(cx) * scale,
                                    at.y - f64::from(cy) * scale,
                                ),
                                to: Pt::new(
                                    pen_x + f64::from(x) * scale,
                                    at.y - f64::from(y) * scale,
                                ),
                            },
                        })
                        .collect(),
                }
            })
            .collect();
        if !subpaths.is_empty() {
            out.push(RShape::Path {
                subpaths,
                geoms: Vec::new(),
                tol: 0.1,
            });
        }
        pen_x += f64::from(adv) * scale;
    }
    Ok(out)
}

enum PenInstr {
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
}

#[derive(Default)]
struct GlyphPen {
    contours: Vec<(Pt, Vec<PenInstr>)>,
}

impl ttf_parser::OutlineBuilder for GlyphPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.contours
            .push((Pt::new(f64::from(x), f64::from(y)), Vec::new()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if let Some((_, instrs)) = self.contours.last_mut() {
            instrs.push(PenInstr::Line(x, y));
        }
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if let Some((_, instrs)) = self.contours.last_mut() {
            instrs.push(PenInstr::Quad(cx, cy, x, y));
        }
    }

    fn curve_to(&mut self, _c1x: f32, _c1y: f32, _c2x: f32, _c2y: f32, _x: f32, _y: f32) {
        // TrueType outlines contain no cubics; ttf-parser never calls this.
    }

    fn close(&mut self) {
        // Contour return-to-start is implicit in a closed subpath.
    }
}
