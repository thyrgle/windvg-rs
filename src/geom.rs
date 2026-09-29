//! Geometry primitives: points, affine transforms, arc-length tables.
//! The numeric behavior mirrors windvg's `geometry.py`, `arclength.py`,
//! and `ext/transform.py` so results match the Python reference exactly.

/// Round-half-to-even (banker's rounding), matching Python's `round()`.
/// Rust's `f64::round` rounds half away from zero, so ties need care.
pub fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    let fl = x.floor();
    if x - fl == 0.5 {
        // tie between fl and fl+1: pick the even one
        if fl.rem_euclid(2.0) == 0.0 {
            fl
        } else {
            fl + 1.0
        }
    } else {
        r
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

#[allow(clippy::should_implement_trait)]
impl Pt {
    pub fn new(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }
    pub fn add(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }
    pub fn mul(self, s: f64) -> Pt {
        Pt::new(self.x * s, self.y * s)
    }
    pub fn dot(self, o: Pt) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(a: Pt, b: Pt) -> f64 {
        a.x * b.y - a.y * b.x
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: Pt) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }
    pub fn lerp(self, o: Pt, t: f64) -> Pt {
        self.add(o.sub(self).mul(t))
    }
}

/// A 2D affine transform: x' = a*x + c*y + e, y' = b*x + d*y + f.
/// Positive rotation angles run clockwise on screen (y-down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
}

const EPS: f64 = 1e-9;

impl Transform {
    pub fn translate(tx: f64, ty: f64) -> Transform {
        Transform {
            e: tx,
            f: ty,
            ..Transform::default()
        }
    }

    pub fn rotate_deg(deg: f64) -> Transform {
        let rad = deg.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        Transform {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn rotate_about(deg: f64, about: Pt) -> Transform {
        Transform::translate(about.x, about.y)
            .then(Transform::rotate_deg(deg))
            .then(Transform::translate(-about.x, -about.y))
    }

    /// Compose: `(self.then(other))` applies `other` first, then `self`.
    pub fn then(self, o: Transform) -> Transform {
        Transform {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    pub fn apply(self, p: Pt) -> Pt {
        Pt::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    pub fn det(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn is_similarity(self) -> bool {
        let cross_term = (self.a * self.c + self.b * self.d).abs() < EPS;
        let nx = self.a * self.a + self.b * self.b;
        let ny = self.c * self.c + self.d * self.d;
        cross_term && (nx - ny).abs() < EPS && self.det().abs() > EPS
    }

    pub fn scale_factor(self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }
}

/// The linear map taking unit-circle space into an ellipse's local frame.
pub fn local_frame(rx: f64, ry: f64, rotation_deg: f64) -> Transform {
    let rad = rotation_deg.to_radians();
    let (c, s) = (rad.cos(), rad.sin());
    Transform {
        a: rx * c,
        b: rx * s,
        c: -ry * s,
        d: ry * c,
        e: 0.0,
        f: 0.0,
    }
}

/// The ellipse that the linear map [[a, c], [b, d]] makes from the unit
/// circle: singular values are the semi-axes, rotation the major-axis
/// direction in y-down degrees.
pub fn ellipse_from_linear(a: f64, b: f64, c: f64, d: f64, center: Pt) -> (Pt, f64, f64, f64) {
    let g00 = a * a + c * c;
    let g01 = a * b + c * d;
    let g11 = b * b + d * d;
    let mean = (g00 + g11) / 2.0;
    let radius = ((g00 - g11) / 2.0).hypot(g01);
    let rotation_deg = ((2.0_f64 * g01).atan2(g00 - g11) / 2.0).to_degrees();
    (
        center,
        (mean + radius).sqrt(),
        (mean - radius).sqrt(),
        rotation_deg,
    )
}

/// Chordal arc-length model of a parametric curve over one parameter span
/// (`windvg.arclength.ArcLengthTable`): uniformly sampled parameters with
/// cumulative chord lengths and linear interpolation both ways.
#[derive(Debug, Clone)]
pub struct ArcTable {
    pub params: Vec<f64>,
    pub cum: Vec<f64>,
    pub total: f64,
}

impl ArcTable {
    pub fn new(
        param_start: f64,
        param_end: f64,
        samples: usize,
        point_at: &dyn Fn(f64) -> Pt,
    ) -> ArcTable {
        assert!(samples >= 2);
        let step = (param_end - param_start) / (samples - 1) as f64;
        let params: Vec<f64> = (0..samples)
            .map(|i| param_start + i as f64 * step)
            .collect();
        let mut cum = Vec::with_capacity(samples);
        cum.push(0.0);
        let mut prev = point_at(params[0]);
        for &t in &params[1..] {
            let cur = point_at(t);
            let last = *cum.last().unwrap();
            cum.push(last + prev.dist(cur));
            prev = cur;
        }
        let total = *cum.last().unwrap();
        ArcTable { params, cum, total }
    }

    pub fn param_to_distance(&self, t: f64) -> f64 {
        let n = self.params.len();
        let i = self.params.partition_point(|&p| p <= t).saturating_sub(1);
        let i = i.min(n - 2);
        let (t0, t1) = (self.params[i], self.params[i + 1]);
        let f = if t1 == t0 {
            0.0
        } else {
            ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
        };
        self.cum[i] + f * (self.cum[i + 1] - self.cum[i])
    }

    pub fn distance_to_param(&self, d: f64) -> f64 {
        let d = d.clamp(0.0, self.total);
        let n = self.cum.len();
        let i = self.cum.partition_point(|&c| c <= d).saturating_sub(1);
        let i = i.min(n - 2);
        let seg = self.cum[i + 1] - self.cum[i];
        let f = if seg == 0.0 {
            0.0
        } else {
            (d - self.cum[i]) / seg
        };
        self.params[i] + f * (self.params[i + 1] - self.params[i])
    }
}
