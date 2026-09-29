//! Resolution: IR → concrete draw ops (spec §7). Implements the track
//! protocol (§7.4), anchor/segment/grid references (§7.3, §7.5, §7.6),
//! generators (§7.8), and rounded corners (§7.9) with Python-parity math.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::geom::{ellipse_from_linear, local_frame, ArcTable, Pt, Transform};
use crate::ir::*;

const TOL: f64 = 0.1; // path track flattening tolerance (fixed in v1)

// ---- resolved shapes ----------------------------------------------------

#[derive(Debug, Clone)]
pub enum RInstr {
    Line {
        to: Pt,
    },
    Quad {
        ctrl: Pt,
        to: Pt,
    },
    Cubic {
        c1: Pt,
        c2: Pt,
        to: Pt,
    },
    ArcCircle {
        radius: f64,
        large: bool,
        sweep_cw: bool,
        to: Pt,
    },
    ArcEllipse {
        rx: f64,
        ry: f64,
        rotation_deg: f64,
        large: bool,
        sweep_cw: bool,
        to: Pt,
    },
    Close,
}

#[derive(Debug, Clone)]
pub struct RSubPath {
    pub start: Pt,
    pub instructions: Vec<RInstr>,
}

#[derive(Debug, Clone)]
pub struct SubGeom {
    pub chain: Vec<Pt>,
    pub length: f64,
    pub closed: bool,
}

#[derive(Debug, Clone)]
pub enum RShape {
    Polygon(Vec<Pt>),
    Polyline(Vec<Pt>),
    Circle {
        c: Pt,
        r: f64,
    },
    Ellipse {
        c: Pt,
        rx: f64,
        ry: f64,
        rotation_deg: f64,
        table: Rc<ArcTable>,
    },
    Arc {
        c: Pt,
        r: f64,
        start_deg: f64,
        sweep_deg: f64,
    },
    Path {
        subpaths: Vec<RSubPath>,
        geoms: Vec<SubGeom>,
        tol: f64,
    },
    Compound(Vec<RShape>),
}

#[derive(Debug, Clone, Copy)]
pub enum RPaint {
    Color(Color),
    Linear {
        start: Pt,
        end: Pt,
        start_color: Color,
        end_color: Color,
    },
    Radial {
        center: Pt,
        edge: Pt,
        center_color: Color,
        edge_color: Color,
    },
}

#[derive(Debug, Clone)]
pub struct ROp {
    pub id: String,
    pub kind: OpKind,
    pub shape: RShape,
    pub paint: RPaint,
    pub outline_paint: Option<RPaint>,
    pub width: f64,
}

pub fn ellipse_point(c: Pt, rx: f64, ry: f64, rot_deg: f64, t: f64) -> Pt {
    let phi = rot_deg.to_radians();
    let (cp, sp) = (phi.cos(), phi.sin());
    let u = rx * t.cos();
    let v = ry * t.sin();
    Pt::new(c.x + u * cp - v * sp, c.y + u * sp + v * cp)
}

fn make_ellipse(c: Pt, rx: f64, ry: f64, rotation_deg: f64) -> RShape {
    let table = ArcTable::new(0.0, 2.0 * std::f64::consts::PI, 1440, &|t| {
        ellipse_point(c, rx, ry, rotation_deg, t)
    });
    RShape::Ellipse {
        c,
        rx,
        ry,
        rotation_deg,
        table: Rc::new(table),
    }
}

pub fn make_path(subpaths: Vec<RSubPath>) -> RShape {
    let geoms = subpaths
        .iter()
        .map(|sub| {
            let chain = flatten_subpath(sub, TOL);
            let length: f64 = chain.windows(2).map(|w| w[0].dist(w[1])).sum();
            let closed = chain[0].dist(*chain.last().unwrap()) < 1e-9;
            SubGeom {
                chain,
                length,
                closed,
            }
        })
        .collect();
    RShape::Path {
        subpaths,
        geoms,
        tol: TOL,
    }
}

// ---- flattening (windvg.path) -------------------------------------------

fn line_dist(pt: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b.sub(a);
    let len = ab.length();
    if len == 0.0 {
        return pt.dist(a);
    }
    Pt::cross(pt.sub(a), ab).abs() / len
}

fn flatten_bezier(out: &mut Vec<Pt>, p0: Pt, ctrls: &[Pt], p1: Pt, tol: f64, depth: u32) {
    let flat = ctrls.iter().all(|&c| line_dist(c, p0, p1) <= tol);
    if depth >= 16 || flat {
        out.push(p1);
        return;
    }
    let halves: Vec<(Pt, Vec<Pt>, Pt)> = if ctrls.len() == 1 {
        let c = ctrls[0];
        let left_c = p0.add(c.sub(p0).mul(0.5));
        let mid = p0.add(c.mul(2.0)).add(p1).mul(0.25);
        let right_c = c.add(p1.sub(c).mul(0.5));
        vec![(p0, vec![left_c], mid), (mid, vec![right_c], p1)]
    } else {
        let (c1, c2) = (ctrls[0], ctrls[1]);
        let p01 = p0.lerp(c1, 0.5);
        let p12 = c1.lerp(c2, 0.5);
        let p23 = c2.lerp(p1, 0.5);
        let left_c = p01.lerp(p12, 0.5);
        let right_c = p12.lerp(p23, 0.5);
        let mid = left_c.lerp(right_c, 0.5);
        vec![(p0, vec![p01, left_c], mid), (mid, vec![right_c, p23], p1)]
    };
    for (start, controls, end) in halves {
        flatten_bezier(out, start, &controls, end, tol, depth + 1);
    }
}

/// SVG 1.1 F.6.5 endpoint-parameterized elliptical arc sampling.
#[allow(clippy::too_many_arguments)]
fn flatten_arc(
    out: &mut Vec<Pt>,
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    large: bool,
    sweep_cw: bool,
    p0: Pt,
    p1: Pt,
    tol: f64,
) {
    let (rx, ry) = (rx.abs(), ry.abs());
    let phi = rotation_deg.to_radians();
    let (cos_p, sin_p) = (phi.cos(), phi.sin());
    let dx2 = (p0.x - p1.x) / 2.0;
    let dy2 = (p0.y - p1.y) / 2.0;
    let x1p = cos_p * dx2 + sin_p * dy2;
    let y1p = -sin_p * dx2 + cos_p * dy2;

    let (mut rx, mut ry) = (rx, ry);
    let lam = (x1p / rx).powi(2) + (y1p / ry).powi(2);
    if lam > 1.0 {
        let scale = lam.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let root = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    let sign = if large == sweep_cw { -1.0 } else { 1.0 };
    let cxp = sign * root * rx * y1p / ry;
    let cyp = sign * -root * ry * x1p / rx;
    let cx = cos_p * cxp - sin_p * cyp + (p0.x + p1.x) / 2.0;
    let cy = sin_p * cxp + cos_p * cyp + (p0.y + p1.y) / 2.0;

    let theta1 = (y1p - cyp).atan2(x1p - cxp);
    let theta2 = (-y1p - cyp).atan2(-x1p - cxp);
    let mut delta = theta2 - theta1;
    if !sweep_cw && delta > 0.0 {
        delta -= 2.0 * std::f64::consts::PI;
    } else if sweep_cw && delta < 0.0 {
        delta += 2.0 * std::f64::consts::PI;
    }

    let max_angle = 2.0 * (1.0 - tol / rx.max(ry)).clamp(-1.0, 1.0).acos();
    let steps = ((delta.abs() / max_angle).ceil() as u64).max(4);
    for i in 1..=steps {
        let u = theta1 + delta * i as f64 / steps as f64;
        let (ux, uy) = (rx * u.cos(), ry * u.sin());
        out.push(Pt::new(
            cx + ux * cos_p - uy * sin_p,
            cy + ux * sin_p + uy * cos_p,
        ));
    }
}

fn flatten_subpath(sub: &RSubPath, tol: f64) -> Vec<Pt> {
    let mut chain = vec![sub.start];
    let mut current = sub.start;
    for instr in &sub.instructions {
        match instr {
            RInstr::Line { to } => {
                chain.push(*to);
                current = *to;
            }
            RInstr::Quad { ctrl, to } => {
                flatten_bezier(&mut chain, current, std::slice::from_ref(ctrl), *to, tol, 0);
                current = *to;
            }
            RInstr::Cubic { c1, c2, to } => {
                flatten_bezier(&mut chain, current, &[*c1, *c2], *to, tol, 0);
                current = *to;
            }
            RInstr::ArcCircle {
                radius,
                large,
                sweep_cw,
                to,
            } => {
                flatten_arc(
                    &mut chain, *radius, *radius, 0.0, *large, *sweep_cw, current, *to, tol,
                );
                current = *to;
            }
            RInstr::ArcEllipse {
                rx,
                ry,
                rotation_deg,
                large,
                sweep_cw,
                to,
            } => {
                flatten_arc(
                    &mut chain,
                    *rx,
                    *ry,
                    *rotation_deg,
                    *large,
                    *sweep_cw,
                    current,
                    *to,
                    tol,
                );
                current = *to;
            }
            RInstr::Close => {
                chain.push(sub.start);
                current = sub.start;
            }
        }
    }
    chain
}

// ---- chain walking (windvg.shapes helpers) --------------------------------

fn point_on_chain(points: &[Pt], closed: bool, d: f64) -> Pt {
    let n = points.len();
    let pairs = if closed { n } else { n - 1 };
    let mut rem = d;
    for i in 0..pairs {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let seg = a.dist(b);
        if rem <= seg {
            return if seg == 0.0 { a } else { a.lerp(b, rem / seg) };
        }
        rem -= seg;
    }
    *points.last().unwrap()
}

fn tangent_on_chain(points: &[Pt], closed: bool, d: f64) -> Pt {
    let n = points.len();
    let pairs = if closed { n } else { n - 1 };
    let mut rem = d;
    for i in 0..pairs {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let seg_len = a.dist(b);
        if rem <= seg_len {
            let seg = b.sub(a);
            return if seg_len > 0.0 {
                seg.mul(1.0 / seg_len)
            } else {
                Pt::new(1.0, 0.0)
            };
        }
        rem -= seg_len;
    }
    Pt::new(1.0, 0.0)
}

fn project_on_chain(points: &[Pt], closed: bool, pt: Pt) -> f64 {
    let n = points.len();
    let pairs = if closed { n } else { n - 1 };
    let mut best_dist = f64::INFINITY;
    let mut best_pos = 0.0;
    let mut traveled = 0.0;
    for i in 0..pairs {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let seg = b.sub(a);
        let seg_len = seg.length();
        let t = if seg_len == 0.0 {
            0.0
        } else {
            pt.sub(a).dot(seg) / (seg_len * seg_len)
        }
        .clamp(0.0, 1.0);
        let dist = a.add(seg.mul(t)).dist(pt);
        if dist < best_dist {
            best_dist = dist;
            best_pos = traveled + t * seg_len;
        }
        traveled += seg_len;
    }
    best_pos
}

fn shoelace(chain: &[Pt]) -> f64 {
    let mut total = 0.0;
    for w in chain.windows(2) {
        total += Pt::cross(w[0], w[1]);
    }
    total / 2.0
}

fn polygon_shoelace(points: &[Pt]) -> f64 {
    let n = points.len();
    let mut total = 0.0;
    for i in 0..n {
        total += Pt::cross(points[i], points[(i + 1) % n]);
    }
    total / 2.0
}

// ---- the track protocol (spec §7.4) ---------------------------------------

impl RShape {
    pub fn perimeter(&self) -> f64 {
        match self {
            RShape::Polygon(pts) => {
                let n = pts.len();
                (0..n).map(|i| pts[i].dist(pts[(i + 1) % n])).sum()
            }
            RShape::Polyline(pts) => pts.windows(2).map(|w| w[0].dist(w[1])).sum(),
            RShape::Circle { r, .. } => 2.0 * std::f64::consts::PI * r,
            RShape::Ellipse { table, .. } => table.total,
            RShape::Arc { r, sweep_deg, .. } => {
                sweep_deg.abs() / 360.0 * 2.0 * std::f64::consts::PI * r
            }
            RShape::Path { geoms, .. } => geoms.iter().map(|g| g.length).sum(),
            RShape::Compound(subs) => subs.iter().map(|s| s.perimeter()).sum(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            RShape::Arc { .. } | RShape::Polyline(_) => false,
            RShape::Path { geoms, .. } => geoms.iter().all(|g| g.closed),
            _ => true,
        }
    }

    pub fn fillable(&self) -> bool {
        match self {
            RShape::Arc { .. } | RShape::Polyline(_) => false,
            RShape::Path { geoms, .. } => geoms.iter().all(|g| g.closed),
            _ => true,
        }
    }

    pub fn winding_sign(&self) -> i32 {
        match self {
            RShape::Polygon(pts) => {
                if polygon_shoelace(pts) > 0.0 {
                    1
                } else {
                    -1
                }
            }
            RShape::Polyline(_) => 1,
            RShape::Circle { .. } | RShape::Ellipse { .. } => 1,
            RShape::Arc { sweep_deg, .. } => {
                if *sweep_deg > 0.0 {
                    1
                } else {
                    -1
                }
            }
            RShape::Path { geoms, .. } => {
                let total: f64 = geoms
                    .iter()
                    .filter(|g| g.closed)
                    .map(|g| shoelace(&g.chain))
                    .sum();
                if total >= 0.0 {
                    1
                } else {
                    -1
                }
            }
            RShape::Compound(_) => 1,
        }
    }

    pub fn point_at_distance(&self, d: f64) -> Result<Pt, String> {
        match self {
            RShape::Polygon(pts) => Ok(point_on_chain(pts, true, d.rem_euclid(self.perimeter()))),
            RShape::Polyline(pts) => Ok(point_on_chain(pts, false, d.clamp(0.0, self.perimeter()))),
            RShape::Circle { c, r } => {
                let angle = (d / r).rem_euclid(2.0 * std::f64::consts::PI);
                Ok(Pt::new(c.x + r * angle.cos(), c.y + r * angle.sin()))
            }
            RShape::Ellipse {
                c,
                rx,
                ry,
                rotation_deg,
                table,
            } => {
                let t = table.distance_to_param(d.rem_euclid(self.perimeter()));
                Ok(ellipse_point(*c, *rx, *ry, *rotation_deg, t))
            }
            RShape::Arc {
                c,
                r,
                start_deg,
                sweep_deg,
            } => {
                let p = sweep_deg.abs() / 360.0 * 2.0 * std::f64::consts::PI * r;
                let frac = (d / p).clamp(0.0, 1.0);
                let rad = (start_deg + sweep_deg * frac).to_radians();
                Ok(Pt::new(c.x + r * rad.cos(), c.y + r * rad.sin()))
            }
            RShape::Path { geoms, .. } => {
                let p = self.perimeter();
                let d = if self.is_closed() {
                    d.rem_euclid(p)
                } else {
                    d.clamp(0.0, p)
                };
                let (gi, local) = locate(geoms, d);
                Ok(point_on_chain(&geoms[gi].chain, false, local))
            }
            RShape::Compound(_) => Err("a Compound has no single track".into()),
        }
    }

    pub fn tangent_at_distance(&self, d: f64) -> Result<Pt, String> {
        match self {
            RShape::Polygon(pts) => Ok(tangent_on_chain(pts, true, d.rem_euclid(self.perimeter()))),
            RShape::Polyline(pts) => {
                Ok(tangent_on_chain(pts, false, d.clamp(0.0, self.perimeter())))
            }
            RShape::Circle { r, .. } => {
                let angle = (d / r).rem_euclid(2.0 * std::f64::consts::PI);
                Ok(Pt::new(-angle.sin(), angle.cos()))
            }
            RShape::Ellipse {
                rx,
                ry,
                rotation_deg,
                table,
                ..
            } => {
                let t = table.distance_to_param(d.rem_euclid(self.perimeter()));
                let phi = rotation_deg.to_radians();
                let (cp, sp) = (phi.cos(), phi.sin());
                let dx = -rx * t.sin();
                let dy = ry * t.cos();
                let (vx, vy) = (dx * cp - dy * sp, dx * sp + dy * cp);
                let norm = vx.hypot(vy);
                Ok(Pt::new(vx / norm, vy / norm))
            }
            RShape::Arc {
                start_deg,
                sweep_deg,
                ..
            } => {
                let per = self.perimeter();
                let frac = (d / per).clamp(0.0, 1.0);
                let rad = (start_deg + sweep_deg * frac).to_radians();
                if *sweep_deg > 0.0 {
                    Ok(Pt::new(-rad.sin(), rad.cos()))
                } else {
                    Ok(Pt::new(rad.sin(), -rad.cos()))
                }
            }
            RShape::Path { geoms, .. } => {
                let p = self.perimeter();
                let d = if self.is_closed() {
                    d.rem_euclid(p)
                } else {
                    d.clamp(0.0, p)
                };
                let (gi, local) = locate(geoms, d);
                Ok(tangent_on_chain(&geoms[gi].chain, false, local))
            }
            RShape::Compound(_) => Err("a Compound has no single track".into()),
        }
    }

    pub fn project(&self, pt: Pt) -> Result<f64, String> {
        match self {
            RShape::Polygon(pts) => Ok(project_on_chain(pts, true, pt)),
            RShape::Polyline(pts) => Ok(project_on_chain(pts, false, pt)),
            RShape::Circle { c, r } => {
                let delta = pt.sub(*c);
                if delta.length() == 0.0 {
                    return Ok(0.0);
                }
                Ok(delta
                    .y
                    .atan2(delta.x)
                    .rem_euclid(2.0 * std::f64::consts::PI)
                    * r)
            }
            RShape::Ellipse {
                c,
                rx,
                ry,
                rotation_deg,
                table,
            } => {
                // nearest table sample, then 48 ternary-search iterations
                // inside the adjacent bracket (windvg.shapes.Ellipse.project)
                let params = &table.params;
                let dist_at = |t: f64| ellipse_point(*c, *rx, *ry, *rotation_deg, t).dist(pt);
                let mut best_i = 0usize;
                let mut best_d = f64::INFINITY;
                for (i, &p) in params.iter().enumerate() {
                    let d = dist_at(p);
                    if d < best_d {
                        best_d = d;
                        best_i = i;
                    }
                }
                let step = params[1] - params[0];
                let last = params.len() - 1;
                let mut lo = params[best_i.saturating_sub(1)];
                let mut hi = params[(best_i + 1).min(last)];
                if hi - lo > step {
                    if best_i == 0 {
                        hi = lo + step;
                    }
                    if best_i == last {
                        lo = hi - step;
                    }
                }
                for _ in 0..48 {
                    let m1 = lo + (hi - lo) / 3.0;
                    let m2 = hi - (hi - lo) / 3.0;
                    if dist_at(m1) <= dist_at(m2) {
                        hi = m2;
                    } else {
                        lo = m1;
                    }
                }
                let t = ((lo + hi) / 2.0).rem_euclid(2.0 * std::f64::consts::PI);
                Ok(table.param_to_distance(t))
            }
            RShape::Arc {
                c,
                r: _,
                start_deg,
                sweep_deg,
            } => {
                let delta = pt.sub(*c);
                if delta.length() == 0.0 {
                    return Ok(0.0);
                }
                let phi = (delta.y.atan2(delta.x).to_degrees() - start_deg).rem_euclid(360.0);
                let sweep = *sweep_deg;
                let travel = if sweep >= 0.0 {
                    if phi <= sweep {
                        phi
                    } else if phi < 360.0 - sweep / 2.0 {
                        0.0
                    } else {
                        sweep
                    }
                } else {
                    let phi_ccw = (360.0 - phi).rem_euclid(360.0);
                    let span = -sweep;
                    let near_start = phi_ccw < 360.0 - span / 2.0;
                    if phi_ccw <= span {
                        phi_ccw
                    } else if near_start {
                        0.0
                    } else {
                        span
                    }
                };
                Ok(travel.abs() / sweep.abs() * self.perimeter())
            }
            RShape::Path { geoms, .. } => {
                let mut best = f64::INFINITY;
                let mut best_d = 0.0;
                let mut offset = 0.0;
                for g in geoms {
                    let pos = project_on_chain(&g.chain, false, pt);
                    let total = offset + pos;
                    let dist = self.point_at_distance(total)?.dist(pt);
                    if dist < best {
                        best = dist;
                        best_d = total;
                    }
                    offset += g.length;
                }
                Ok(best_d)
            }
            RShape::Compound(_) => Err("a Compound has no single track".into()),
        }
    }

    pub fn bbox(&self) -> (Pt, Pt) {
        match self {
            RShape::Polygon(pts) | RShape::Polyline(pts) => bbox_of_points(pts),
            RShape::Circle { c, r } => (Pt::new(c.x - r, c.y - r), Pt::new(c.x + r, c.y + r)),
            RShape::Ellipse {
                c,
                rx,
                ry,
                rotation_deg,
                ..
            } => {
                let phi = rotation_deg.to_radians();
                let (cp, sp) = (phi.cos(), phi.sin());
                let hw = (rx * cp).hypot(ry * sp);
                let hh = (rx * sp).hypot(ry * cp);
                (Pt::new(c.x - hw, c.y - hh), Pt::new(c.x + hw, c.y + hh))
            }
            RShape::Arc {
                c,
                r,
                start_deg,
                sweep_deg,
            } => {
                let at = |deg: f64| {
                    let rad = deg.to_radians();
                    Pt::new(c.x + r * rad.cos(), c.y + r * rad.sin())
                };
                let mut pts = vec![at(*start_deg), at(start_deg + sweep_deg)];
                for deg in [0.0, 90.0, 180.0, 270.0] {
                    let delta = (deg - start_deg).rem_euclid(360.0);
                    let swept = if *sweep_deg > 0.0 {
                        delta <= *sweep_deg
                    } else {
                        (360.0 - delta).rem_euclid(360.0) <= -sweep_deg
                    };
                    if swept {
                        pts.push(at(deg));
                    }
                }
                bbox_of_points(&pts)
            }
            RShape::Path { geoms, .. } => {
                let pts: Vec<Pt> = geoms.iter().flat_map(|g| g.chain.iter().copied()).collect();
                bbox_of_points(&pts)
            }
            RShape::Compound(subs) => {
                let mut acc = subs[0].bbox();
                for s in &subs[1..] {
                    let b = s.bbox();
                    acc.0 = Pt::new(acc.0.x.min(b.0.x), acc.0.y.min(b.0.y));
                    acc.1 = Pt::new(acc.1.x.max(b.1.x), acc.1.y.max(b.1.y));
                }
                acc
            }
        }
    }
}

fn locate(geoms: &[SubGeom], d: f64) -> (usize, f64) {
    let mut offset = 0.0;
    for (i, g) in geoms.iter().enumerate() {
        if d <= offset + g.length || i == geoms.len() - 1 {
            return (i, (d - offset).clamp(0.0, g.length));
        }
        offset += g.length;
    }
    (0, 0.0)
}

fn bbox_of_points(pts: &[Pt]) -> (Pt, Pt) {
    let mut lo = Pt::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in pts {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

// ---- transforms baked into shapes (windvg.ext.transform) ------------------

fn transform_arc_params(
    t: Transform,
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    sweep_cw: bool,
    to: Pt,
) -> (f64, f64, f64, bool, Pt) {
    let total = t.then(local_frame(rx, ry, rotation_deg));
    let (c, irx, iry, irot) = ellipse_from_linear(total.a, total.b, total.c, total.d, t.apply(to));
    let flipped = sweep_cw != (t.det() < 0.0);
    (irx, iry, irot, flipped, c)
}

pub fn transform_shape(s: RShape, t: &Transform) -> RShape {
    match s {
        RShape::Polygon(pts) => RShape::Polygon(pts.iter().map(|&p| t.apply(p)).collect()),
        RShape::Polyline(pts) => RShape::Polyline(pts.iter().map(|&p| t.apply(p)).collect()),
        RShape::Circle { c, r } => {
            let center = t.apply(c);
            if t.is_similarity() {
                RShape::Circle {
                    c: center,
                    r: r * t.scale_factor(),
                }
            } else {
                let (_, rx, ry, rot) =
                    ellipse_from_linear(t.a * r, t.b * r, t.c * r, t.d * r, center);
                make_ellipse(center, rx, ry, rot)
            }
        }
        RShape::Ellipse {
            c,
            rx,
            ry,
            rotation_deg,
            ..
        } => {
            let e = t.then(local_frame(rx, ry, rotation_deg));
            let (center, nx, ny, nrot) = ellipse_from_linear(e.a, e.b, e.c, e.d, t.apply(c));
            make_ellipse(center, nx, ny, nrot)
        }
        RShape::Arc {
            c,
            r,
            start_deg,
            sweep_deg,
        } => {
            let center = t.apply(c);
            if t.is_similarity() {
                let start = {
                    let rad = start_deg.to_radians();
                    t.apply(Pt::new(c.x + r * rad.cos(), c.y + r * rad.sin()))
                };
                let sd = (start.y - center.y).atan2(start.x - center.x).to_degrees();
                let sweep = sweep_deg * if t.det() > 0.0 { 1.0 } else { -1.0 };
                RShape::Arc {
                    c: center,
                    r: r * t.scale_factor(),
                    start_deg: sd,
                    sweep_deg: sweep,
                }
            } else {
                let rad0 = start_deg.to_radians();
                let rad1 = (start_deg + sweep_deg).to_radians();
                let start = t.apply(Pt::new(c.x + r * rad0.cos(), c.y + r * rad0.sin()));
                let end = t.apply(Pt::new(c.x + r * rad1.cos(), c.y + r * rad1.sin()));
                let (rx, ry, rot, sweep_cw, to) =
                    transform_arc_params(*t, r, r, 0.0, sweep_deg > 0.0, end);
                let large = sweep_deg.abs() > 180.0;
                make_path(vec![RSubPath {
                    start,
                    instructions: vec![RInstr::ArcEllipse {
                        rx,
                        ry,
                        rotation_deg: rot,
                        large,
                        sweep_cw,
                        to,
                    }],
                }])
            }
        }
        RShape::Path { subpaths, tol, .. } => {
            let mut subs = Vec::with_capacity(subpaths.len());
            for sub in &subpaths {
                let start = t.apply(sub.start);
                let mut instrs = Vec::with_capacity(sub.instructions.len());
                for instr in &sub.instructions {
                    instrs.push(match instr {
                        RInstr::Line { to } => RInstr::Line { to: t.apply(*to) },
                        RInstr::Quad { ctrl, to } => RInstr::Quad {
                            ctrl: t.apply(*ctrl),
                            to: t.apply(*to),
                        },
                        RInstr::Cubic { c1, c2, to } => RInstr::Cubic {
                            c1: t.apply(*c1),
                            c2: t.apply(*c2),
                            to: t.apply(*to),
                        },
                        RInstr::ArcCircle {
                            radius,
                            large,
                            sweep_cw,
                            to,
                        } => {
                            if t.is_similarity() {
                                RInstr::ArcCircle {
                                    radius: radius * t.scale_factor(),
                                    large: *large,
                                    sweep_cw: *sweep_cw != (t.det() < 0.0),
                                    to: t.apply(*to),
                                }
                            } else {
                                let (rx, ry, rot, sweep, to) =
                                    transform_arc_params(*t, *radius, *radius, 0.0, *sweep_cw, *to);
                                RInstr::ArcEllipse {
                                    rx,
                                    ry,
                                    rotation_deg: rot,
                                    large: *large,
                                    sweep_cw: sweep,
                                    to,
                                }
                            }
                        }
                        RInstr::ArcEllipse {
                            rx,
                            ry,
                            rotation_deg,
                            large,
                            sweep_cw,
                            to,
                        } => {
                            let (nrx, nry, nrot, sweep, to) =
                                transform_arc_params(*t, *rx, *ry, *rotation_deg, *sweep_cw, *to);
                            RInstr::ArcEllipse {
                                rx: nrx,
                                ry: nry,
                                rotation_deg: nrot,
                                large: *large,
                                sweep_cw: sweep,
                                to,
                            }
                        }
                        RInstr::Close => RInstr::Close,
                    });
                }
                subs.push(RSubPath {
                    start,
                    instructions: instrs,
                });
            }
            make_path_with_tol(subs, tol)
        }
        RShape::Compound(subs) => {
            RShape::Compound(subs.into_iter().map(|s| transform_shape(s, t)).collect())
        }
    }
}

fn make_path_with_tol(subpaths: Vec<RSubPath>, tol: f64) -> RShape {
    let geoms = subpaths
        .iter()
        .map(|sub| {
            let chain = flatten_subpath(sub, tol);
            let length: f64 = chain.windows(2).map(|w| w[0].dist(w[1])).sum();
            let closed = chain[0].dist(*chain.last().unwrap()) < 1e-9;
            SubGeom {
                chain,
                length,
                closed,
            }
        })
        .collect();
    RShape::Path {
        subpaths,
        geoms,
        tol,
    }
}

// ---- anchors and references ------------------------------------------------

#[derive(Clone)]
struct TrackAnchor {
    shape: RShape,
    start_distance: f64,
    direction: Orientation,
}

impl TrackAnchor {
    fn signed(&self) -> f64 {
        self.direction.mult() * self.shape.winding_sign() as f64
    }

    fn delta(&self, pct: f64) -> f64 {
        (pct / 100.0) * self.shape.perimeter() * self.signed()
    }

    fn point(&self, pct: f64) -> Result<Pt, String> {
        self.shape
            .point_at_distance(self.start_distance + self.delta(pct))
    }

    fn distance_of(&self, pct: f64) -> f64 {
        (self.start_distance + self.delta(pct)) % self.shape.perimeter()
    }

    fn tangent(&self, pct: f64) -> Result<Pt, String> {
        Ok(self
            .shape
            .tangent_at_distance(self.distance_of(pct))?
            .mul(self.signed()))
    }
}

fn origin_anchor(shape: &RShape, direction: Orientation) -> Result<TrackAnchor, String> {
    let start = shape.point_at_distance(0.0)?;
    let d0 = shape.project(start)?;
    Ok(TrackAnchor {
        shape: shape.clone(),
        start_distance: d0,
        direction,
    })
}

// ---- the resolver -----------------------------------------------------------

struct Env<'a> {
    doc: &'a Document,
    by_name: HashMap<&'a str, usize>,
    memo: Vec<Option<Rc<Vec<RShape>>>>,
    in_progress: HashSet<usize>,
    def_stack: HashSet<String>,
}

impl<'a> Env<'a> {
    fn new(doc: &'a Document) -> Env<'a> {
        let by_name = doc
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.name.as_str(), i))
            .collect();
        Env {
            doc,
            by_name,
            memo: (0..doc.nodes.len()).map(|_| None).collect(),
            in_progress: HashSet::new(),
            def_stack: HashSet::new(),
        }
    }

    fn lookup(&self, node: &str, span: Span) -> Result<usize, Diag> {
        self.by_name
            .get(node)
            .copied()
            .ok_or_else(|| Diag::new(format!("unknown node `{node}`"), span.line, span.col))
    }

    fn node_shapes(&mut self, idx: usize) -> Result<Rc<Vec<RShape>>, Diag> {
        if let Some(cached) = &self.memo[idx] {
            return Ok(cached.clone());
        }
        if self.in_progress.contains(&idx) {
            let name = self.doc.nodes[idx].name.clone();
            let span = self.doc.nodes[idx].span;
            return Err(Diag::new(
                format!("cyclic reference involving node `{name}`"),
                span.line,
                span.col,
            ));
        }
        self.in_progress.insert(idx);
        let shape = self.doc.nodes[idx].shape.clone();
        let result = self.expand_shape(&shape);
        self.in_progress.remove(&idx);
        let shapes = Rc::new(result?);
        self.memo[idx] = Some(shapes.clone());
        Ok(shapes)
    }

    fn first_shape(&mut self, node: &str, span: Span) -> Result<RShape, Diag> {
        let idx = self.lookup(node, span)?;
        let shapes = self.node_shapes(idx)?;
        shapes.first().cloned().ok_or_else(|| {
            Diag::new(
                format!("node `{node}` expands to no shapes"),
                span.line,
                span.col,
            )
        })
    }

    fn resolve_point(&mut self, p: &PPoint) -> Result<Pt, Diag> {
        match &p.kind {
            PKind::Literal(x, y) => Ok(Pt::new(*x, *y)),
            PKind::Anchor {
                node,
                pct,
                start,
                dir,
                offset,
            } => {
                let shape = self.first_shape(node, p.span)?;
                let wrap = |e: String| Diag::new(e, p.span.line, p.span.col);
                let hint = match start {
                    Some((x, y)) => Pt::new(*x, *y),
                    None => shape.point_at_distance(0.0).map_err(wrap)?,
                };
                let d0 = shape.project(hint).map_err(wrap)?;
                let signed = dir.mult() * shape.winding_sign() as f64;
                let delta = (pct / 100.0) * shape.perimeter() * signed;
                let mut result = shape.point_at_distance(d0 + delta).map_err(wrap)?;
                if let Some((dx, dy)) = offset {
                    result = Pt::new(result.x + dx, result.y + dy);
                }
                Ok(result)
            }
            PKind::Segment {
                node,
                index,
                pct,
                offset,
            } => {
                let shape = self.first_shape(node, p.span)?;
                let pts = match &shape {
                    RShape::Polygon(pts) | RShape::Polyline(pts) => pts.clone(),
                    _ => {
                        return Err(Diag::new(
                            format!(
                                "segment reference target `{node}` must be a polygon or polyline"
                            ),
                            p.span.line,
                            p.span.col,
                        ))
                    }
                };
                let max_k = match &shape {
                    RShape::Polygon(_) => pts.len() as i64 - 1,
                    _ => pts.len() as i64 - 2,
                };
                if *index < 0 || *index > max_k {
                    return Err(Diag::new(
                        format!(
                            "segment index {} is out of range for `{node}` (valid: 0..={max_k})",
                            index
                        ),
                        p.span.line,
                        p.span.col,
                    ));
                }
                let a = pts[*index as usize];
                let b = pts[((*index as usize) + 1) % pts.len()];
                let t = pct.clamp(0.0, 100.0) / 100.0;
                let mut result = a.lerp(b, t);
                if let Some((dx, dy)) = offset {
                    result = Pt::new(result.x + dx, result.y + dy);
                }
                Ok(result)
            }
            PKind::Between { a, b, pct, offset } => {
                let pa = self.resolve_point(a)?;
                let pb = self.resolve_point(b)?;
                let t = pct / 100.0;
                let mut result = Pt::new(pa.x + (pb.x - pa.x) * t, pa.y + (pb.y - pa.y) * t);
                if let Some((dx, dy)) = offset {
                    result = Pt::new(result.x + dx, result.y + dy);
                }
                Ok(result)
            }
            PKind::Polar {
                center,
                radius,
                deg,
            } => {
                let c = self.resolve_point(center)?;
                let rad = deg.to_radians();
                Ok(Pt::new(c.x + radius * rad.cos(), c.y + radius * rad.sin()))
            }
            PKind::GridCell {
                node,
                col,
                row,
                offset,
            } => {
                let idx = self.lookup(node, p.span)?;
                let spec = &self.doc.nodes[idx].shape;
                let (origin, dx, dy) = match &spec.kind {
                    SKind::GridGuide { origin, dx, dy, .. } => (origin, *dx, *dy),
                    _ => {
                        return Err(Diag::new(
                            format!("node `{node}` is not a grid guide"),
                            p.span.line,
                            p.span.col,
                        ))
                    }
                };
                let o = self.resolve_point(origin)?;
                let mut q = Pt::new(o.x + *col as f64 * dx, o.y + *row as f64 * dy);
                if let Some((ox, oy)) = offset {
                    q = q.add(Pt::new(*ox, *oy));
                }
                Ok(q)
            }
        }
    }

    fn expand_shape(&mut self, s: &PShape) -> Result<Vec<RShape>, Diag> {
        let err = |e: String| Diag::new(e, s.span.line, s.span.col);
        match &s.kind {
            SKind::GridGuide { .. } => Ok(vec![]),
            SKind::Circle { center, radius } => Ok(vec![RShape::Circle {
                c: self.resolve_point(center)?,
                r: *radius,
            }]),
            SKind::Ellipse {
                center,
                rx,
                ry,
                rotation_deg,
            } => Ok(vec![make_ellipse(
                self.resolve_point(center)?,
                *rx,
                *ry,
                *rotation_deg,
            )]),
            SKind::Arc {
                center,
                radius,
                start_deg,
                sweep_deg,
            } => Ok(vec![RShape::Arc {
                c: self.resolve_point(center)?,
                r: *radius,
                start_deg: *start_deg,
                sweep_deg: *sweep_deg,
            }]),
            SKind::Polygon { points } => {
                let pts = self.resolve_points(points)?;
                if polygon_shoelace(&pts) == 0.0 {
                    return Err(err("degenerate polygon (zero signed area)".into()));
                }
                Ok(vec![RShape::Polygon(pts)])
            }
            SKind::Polyline { points } => Ok(vec![RShape::Polyline(self.resolve_points(points)?)]),
            SKind::Path { subpaths } => {
                let mut subs = Vec::with_capacity(subpaths.len());
                for sub in subpaths {
                    let start = self.resolve_point(&sub.start)?;
                    let mut instrs = Vec::with_capacity(sub.instructions.len());
                    for instr in &sub.instructions {
                        instrs.push(self.resolve_instr(instr)?);
                    }
                    subs.push(RSubPath {
                        start,
                        instructions: instrs,
                    });
                }
                Ok(vec![make_path(subs)])
            }
            SKind::Compound { shapes } => {
                let mut all = Vec::new();
                for child in shapes {
                    all.extend(self.expand_shape(child)?);
                }
                if all.iter().any(|sh| !sh.fillable()) {
                    return Err(err("compound parts must all be fillable".into()));
                }
                Ok(vec![RShape::Compound(all)])
            }
            SKind::Along {
                track,
                motifs,
                n,
                offset_pct,
                align,
                direction,
            } => {
                let mut t = self.expand_shape(track)?;
                if t.len() != 1 {
                    return Err(err("along() track must expand to exactly one shape".into()));
                }
                let track_shape = t.pop().unwrap();
                let mshapes = self.expand_motifs(motifs)?;
                let anchor = origin_anchor(&track_shape, *direction).map_err(err)?;
                let step = spacing(&track_shape, *n);
                let mut out = Vec::new();
                for i in 0..*n {
                    let pct = offset_pct + i as f64 * step;
                    let wrap = |e: String| Diag::new(e, s.span.line, s.span.col);
                    let origin = anchor.point(pct).map_err(wrap)?;
                    let mut placement = Transform::translate(origin.x, origin.y);
                    if let AlignMode::Tangent = align {
                        let u = anchor.tangent(pct).map_err(wrap)?;
                        placement =
                            placement.then(Transform::rotate_deg(u.y.atan2(u.x).to_degrees()));
                    }
                    for m in &mshapes {
                        out.push(transform_shape(m.clone(), &placement));
                    }
                }
                Ok(out)
            }
            SKind::Polar {
                center,
                motifs,
                n,
                radius,
                start_deg,
                align,
            } => {
                let c = self.resolve_point(center)?;
                let mut circle = RShape::Circle { c, r: *radius };
                if *start_deg != 0.0 {
                    circle = transform_shape(circle, &Transform::rotate_about(*start_deg, c));
                }
                let mshapes = self.expand_motifs(motifs)?;
                let anchor = origin_anchor(&circle, Orientation::Cw).map_err(err)?;
                let step = spacing(&circle, *n);
                let mut out = Vec::new();
                for i in 0..*n {
                    let pct = 0.0 + i as f64 * step;
                    let wrap = |e: String| Diag::new(e, s.span.line, s.span.col);
                    let origin = anchor.point(pct).map_err(wrap)?;
                    let mut placement = Transform::translate(origin.x, origin.y);
                    if let Some(AlignMode::Tangent) = align {
                        let u = anchor.tangent(pct).map_err(wrap)?;
                        placement =
                            placement.then(Transform::rotate_deg(u.y.atan2(u.x).to_degrees()));
                    }
                    for m in &mshapes {
                        out.push(transform_shape(m.clone(), &placement));
                    }
                }
                Ok(out)
            }
            SKind::Grid {
                motifs,
                cols,
                rows,
                dx,
                dy,
                origin,
            } => {
                let o = self.resolve_point(origin)?;
                let mshapes = self.expand_motifs(motifs)?;
                let mut out = Vec::new();
                for row in 0..*rows {
                    for col in 0..*cols {
                        let placement =
                            Transform::translate(o.x + col as f64 * dx, o.y + row as f64 * dy);
                        for m in &mshapes {
                            out.push(transform_shape(m.clone(), &placement));
                        }
                    }
                }
                Ok(out)
            }
            SKind::Transform { t, shape } => {
                let t = Transform {
                    a: t[0],
                    b: t[1],
                    c: t[2],
                    d: t[3],
                    e: t[4],
                    f: t[5],
                };
                let inner = self.expand_shape(shape)?;
                Ok(inner.into_iter().map(|s| transform_shape(s, &t)).collect())
            }
            SKind::Rect {
                center,
                width,
                height,
            } => {
                let c = self.resolve_point(center)?;
                let (hw, hh) = (width / 2.0, height / 2.0);
                let pts = vec![
                    Pt::new(c.x - hw, c.y - hh),
                    Pt::new(c.x + hw, c.y - hh),
                    Pt::new(c.x + hw, c.y + hh),
                    Pt::new(c.x - hw, c.y + hh),
                ];
                if polygon_shoelace(&pts) == 0.0 {
                    return Err(err("degenerate polygon (zero signed area)".into()));
                }
                Ok(vec![RShape::Polygon(pts)])
            }
            SKind::Pie {
                center,
                radius,
                start_deg,
                sweep_deg,
                chord,
            } => {
                let c = self.resolve_point(center)?;
                let rad0 = start_deg.to_radians();
                let rad1 = (start_deg + sweep_deg).to_radians();
                let start = Pt::new(c.x + radius * rad0.cos(), c.y + radius * rad0.sin());
                let end = Pt::new(c.x + radius * rad1.cos(), c.y + radius * rad1.sin());
                let mut instrs = vec![RInstr::ArcCircle {
                    radius: *radius,
                    large: sweep_deg.abs() > 180.0,
                    sweep_cw: *sweep_deg > 0.0,
                    to: end,
                }];
                if !chord {
                    instrs.push(RInstr::Line { to: c });
                }
                instrs.push(RInstr::Close);
                Ok(vec![make_path(vec![RSubPath {
                    start,
                    instructions: instrs,
                }])])
            }
            SKind::Use { def_name } => {
                let def_shape = self
                    .doc
                    .defs
                    .iter()
                    .find(|(n, _)| n == def_name)
                    .map(|(_, sp)| sp.clone())
                    .ok_or_else(|| err(format!("unknown def {def_name:?}")))?;
                if !self.def_stack.insert(def_name.clone()) {
                    return Err(err(format!("cyclic def chain involving {def_name:?}")));
                }
                let result = self.expand_shape(&def_shape);
                self.def_stack.remove(def_name);
                result
            }
            SKind::Rounded { shape, radius } => {
                let inner = self.expand_shape(shape)?;
                if inner.len() != 1 {
                    return Err(err("rounded() needs exactly one shape".into()));
                }
                match &inner[0] {
                    RShape::Polygon(pts) => Ok(vec![rounded_polygon(pts, *radius).map_err(err)?]),
                    _ => Err(err("rounded() operand must be a polygon".into())),
                }
            }
        }
    }

    fn resolve_points(&mut self, points: &[PPoint]) -> Result<Vec<Pt>, Diag> {
        points.iter().map(|p| self.resolve_point(p)).collect()
    }

    fn resolve_instr(&mut self, instr: &PInstr) -> Result<RInstr, Diag> {
        let mut rp = |p: &PPoint| self.resolve_point(p);
        Ok(match &instr.kind {
            IKind::Line { to } => RInstr::Line { to: rp(to)? },
            IKind::Quad { ctrl, to } => RInstr::Quad {
                ctrl: rp(ctrl)?,
                to: rp(to)?,
            },
            IKind::Cubic { c1, c2, to } => RInstr::Cubic {
                c1: rp(c1)?,
                c2: rp(c2)?,
                to: rp(to)?,
            },
            IKind::ArcCircle {
                radius,
                large,
                sweep_cw,
                to,
            } => RInstr::ArcCircle {
                radius: *radius,
                large: *large,
                sweep_cw: *sweep_cw,
                to: rp(to)?,
            },
            IKind::ArcEllipse {
                rx,
                ry,
                rotation_deg,
                large,
                sweep_cw,
                to,
            } => RInstr::ArcEllipse {
                rx: *rx,
                ry: *ry,
                rotation_deg: *rotation_deg,
                large: *large,
                sweep_cw: *sweep_cw,
                to: rp(to)?,
            },
            IKind::Close => RInstr::Close,
        })
    }

    fn expand_motifs(&mut self, motifs: &[PShape]) -> Result<Vec<RShape>, Diag> {
        let mut out = Vec::new();
        for m in motifs {
            out.extend(self.expand_shape(m)?);
        }
        Ok(out)
    }
}

fn spacing(shape: &RShape, n: i64) -> f64 {
    if n < 2 {
        100.0
    } else if shape.is_closed() {
        100.0 / n as f64
    } else {
        100.0 / (n - 1) as f64
    }
}

/// Rounded corners (spec §7.9), ported from `windvg.ext.rounded`.
fn rounded_polygon(points: &[Pt], radius: f64) -> Result<RShape, String> {
    if radius <= 0.0 {
        return Err("fillet radius must be positive".into());
    }
    let n = points.len();
    let sweep_cw = polygon_shoelace(points) > 0.0;
    let mut start: Option<Pt> = None;
    let mut instrs: Vec<RInstr> = Vec::new();
    for i in 0..n {
        let corner = points[i];
        let prev = points[(i + n - 1) % n];
        let next = points[(i + 1) % n];
        let incoming = prev.sub(corner);
        let outgoing = next.sub(corner);
        let (l1, l2) = (incoming.length(), outgoing.length());
        if l1 == 0.0 || l2 == 0.0 {
            return Err("rounded polygons cannot have repeated points".into());
        }
        let cos_a = (incoming.dot(outgoing) / (l1 * l2)).clamp(-1.0, 1.0);
        let alpha = cos_a.acos();
        let cut = radius / (alpha / 2.0).tan();
        let limit = 0.5 * l1.min(l2);
        if cut > limit {
            return Err(format!(
                "fillet radius {radius} does not fit corner {i}; largest safe radius is about {}",
                limit * (alpha / 2.0).tan()
            ));
        }
        if cut < 1e-9 {
            continue;
        }
        let t_in = corner.add(incoming.mul(cut / l1));
        let t_out = corner.add(outgoing.mul(cut / l2));
        match start {
            None => start = Some(t_in),
            Some(_) => instrs.push(RInstr::Line { to: t_in }),
        }
        instrs.push(RInstr::ArcCircle {
            radius,
            large: false,
            sweep_cw,
            to: t_out,
        });
    }
    if start.is_none() {
        let s = points[0];
        for pt in &points[1..] {
            instrs.push(RInstr::Line { to: *pt });
        }
        start = Some(s);
    }
    instrs.push(RInstr::Close);
    Ok(make_path(vec![RSubPath {
        start: start.unwrap(),
        instructions: instrs,
    }]))
}

// ---- marker geometry (spec §7.18) ---------------------------------------------

fn marker_polygons(shape: &RShape, marker: &Marker) -> Result<Vec<Vec<Pt>>, String> {
    match marker.placement.as_str() {
        "start" | "end" | "both" => {}
        other => return Err(format!("unknown marker placement {other:?}")),
    }
    match marker.kind.as_str() {
        "triangle" | "bar" => {}
        other => return Err(format!("unknown marker kind {other:?}")),
    }
    if marker.size <= 0.0 {
        return Err("marker size must be positive".into());
    }
    let per = shape.perimeter();
    let ds: Vec<f64> = match marker.placement.as_str() {
        "start" => vec![0.0],
        "end" => vec![per],
        _ => vec![0.0, per],
    };
    let mut out = Vec::new();
    for d in ds {
        let p = shape.point_at_distance(d).map_err(|e| e.to_string())?;
        let t = shape.tangent_at_distance(d).map_err(|e| e.to_string())?;
        let n = Pt::new(t.y, -t.x);
        let sz = marker.size;
        if marker.kind == "triangle" {
            out.push(vec![
                p,
                Pt::new(
                    p.x - t.x * sz + n.x * 0.4 * sz,
                    p.y - t.y * sz + n.y * 0.4 * sz,
                ),
                Pt::new(
                    p.x - t.x * sz - n.x * 0.4 * sz,
                    p.y - t.y * sz - n.y * 0.4 * sz,
                ),
            ]);
        } else {
            let (hx, hy) = (t.x * sz / 2.0, t.y * sz / 2.0);
            let (qx, qy) = (n.x * sz / 10.0, n.y * sz / 10.0);
            out.push(vec![
                Pt::new(p.x + hx + qx, p.y + hy + qy),
                Pt::new(p.x + hx - qx, p.y + hy - qy),
                Pt::new(p.x - hx - qx, p.y - hy - qy),
                Pt::new(p.x - hx + qx, p.y - hy + qy),
            ]);
        }
    }
    Ok(out)
}

// ---- ops ---------------------------------------------------------------------

pub fn resolve_paint(p: &Paint) -> RPaint {
    match p {
        Paint::Color(c) => RPaint::Color(*c),
        Paint::Linear {
            start,
            end,
            start_color,
            end_color,
        } => RPaint::Linear {
            start: Pt::new(start.0, start.1),
            end: Pt::new(end.0, end.1),
            start_color: *start_color,
            end_color: *end_color,
        },
        Paint::Radial {
            center,
            edge,
            center_color,
            edge_color,
        } => RPaint::Radial {
            center: Pt::new(center.0, center.1),
            edge: Pt::new(edge.0, edge.1),
            center_color: *center_color,
            edge_color: *edge_color,
        },
    }
}

pub fn resolve(doc: &Document) -> Result<Vec<ROp>, Diag> {
    let mut env = Env::new(doc);
    let mut ops = Vec::new();
    for (idx, node) in doc.nodes.iter().enumerate() {
        if !node.visible {
            continue;
        }
        if !node.markers.is_empty() && node.op != OpKind::Stroke {
            return Err(Diag::new(
                format!("markers are only valid on stroke nodes ({})", node.name),
                node.span.line,
                node.span.col,
            ));
        }
        let shapes = env.node_shapes(idx)?;
        for shape in shapes.iter() {
            if node.op == OpKind::Fill && !shape.fillable() {
                return Err(Diag::new(
                    format!("node `{}` cannot be filled", node.name),
                    node.span.line,
                    node.span.col,
                ));
            }
            if node.op == OpKind::OutlineFill
                && (!shape.fillable() || matches!(shape, RShape::Compound(_)))
            {
                return Err(Diag::new(
                    format!("node `{}` cannot be outline-filled", node.name),
                    node.span.line,
                    node.span.col,
                ));
            }
            ops.push(ROp {
                id: node.id.clone(),
                kind: node.op,
                shape: shape.clone(),
                paint: resolve_paint(&node.paint),
                outline_paint: node.outline_paint.as_ref().map(resolve_paint),
                width: node.stroke_width,
            });
            if node.op == OpKind::Stroke {
                for marker in &node.markers {
                    for polygon in marker_polygons(shape, marker)
                        .map_err(|e| Diag::new(e, node.span.line, node.span.col))?
                    {
                        ops.push(ROp {
                            id: node.id.clone(),
                            kind: OpKind::Fill,
                            shape: RShape::Polygon(polygon),
                            paint: marker
                                .paint
                                .as_ref()
                                .map(resolve_paint)
                                .unwrap_or_else(|| resolve_paint(&node.paint)),
                            outline_paint: None,
                            width: 1.0,
                        });
                    }
                }
            }
        }
    }
    Ok(ops)
}
