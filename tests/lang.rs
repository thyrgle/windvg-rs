//! Language tests: anchor math, segment references, generator spacing,
//! parse/validation errors.

use windvg::ir::Diag;
use windvg::resolve::RShape;

fn resolve_src(src: &str) -> Result<Vec<windvg::resolve::ROp>, Diag> {
    let mut doc = windvg::parser::parse(src)?;
    windvg::parser::assign_ids(&mut doc);
    windvg::resolve::resolve(&doc)
}

fn ops_of(src: &str) -> Vec<windvg::resolve::ROp> {
    resolve_src(src).unwrap_or_else(|e| panic!("parse/resolve failed: {e}"))
}

/// Last resolved op's first point (the stroke line follows any fill ops).
fn first_point(op: &windvg::resolve::ROp) -> (f64, f64) {
    match &op.shape {
        RShape::Polyline(pts) | RShape::Polygon(pts) => (pts[0].x, pts[0].y),
        other => panic!("expected a chain shape, got {other:?}"),
    }
}

fn last_point_of(src: &str) -> (f64, f64) {
    let ops = ops_of(src);
    first_point(ops.last().unwrap())
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ---- anchors ------------------------------------------------------------

const CIRCLE_DOC: &str = "
wvg 1
scene 100 100

guide c = circle center=(0,0) radius=40
stroke l = line p1=@c {pct} p2=(999,999) color=black width=1
";

#[test]
fn anchor_circle_percentages() {
    // circle r=40 centered at origin; d=0 is the rightmost point, cw on screen
    let cases: &[(&str, f64, f64)] = &[
        ("0%", 40.0, 0.0),  // origin
        ("25%", 0.0, 40.0), // quarter turn cw lands at +y (down)
        ("50%", -40.0, 0.0),
        ("75%", 0.0, -40.0),
        ("100%", 40.0, 0.0),  // wrap
        ("125%", 0.0, 40.0),  // wrap == 25%
        ("-25%", 0.0, -40.0), // negative travels ccw
    ];
    for (pct, x, y) in cases {
        let ops = ops_of(&CIRCLE_DOC.replace("{pct}", pct));
        let (gx, gy) = first_point(&ops[0]);
        assert!(
            approx(gx, *x) && approx(gy, *y),
            "pct {pct}: got ({gx}, {gy})"
        );
    }
}

#[test]
fn anchor_ccw_and_from_projection() {
    let ops = ops_of(&CIRCLE_DOC.replace("{pct}", "ccw 25%"));
    let (x, y) = first_point(&ops[0]);
    assert!(
        approx(x, 0.0) && approx(y, -40.0),
        "ccw 25%: got ({x}, {y})"
    );

    // `from (0,-40)` projects to the 270-degree point; +25% cw from there
    // wraps around to (0, -40) + 25% -> (40, 0)
    let src = "
wvg 1
scene 100 100

guide c = circle center=(0,0) radius=40
stroke l = line p1=@c 25% from (0,-40) p2=(999,999) color=black width=1
";
    let ops = ops_of(src);
    let (x, y) = first_point(&ops[0]);
    assert!(
        approx(x, 40.0) && approx(y, 0.0),
        "from-projection: got ({x}, {y})"
    );
}

#[test]
fn anchor_open_track_clamps() {
    let src = "
wvg 1
scene 100 100

guide l = polyline points=[(0,0), (100,0)]
stroke m = line p1=@l 150% p2=(999,999) color=black width=1
";
    let ops = ops_of(src);
    let (x, _) = first_point(&ops[0]);
    assert!(approx(x, 100.0), "open tracks clamp, got x={x}");
}

// ---- segment references ---------------------------------------------------

const SQUARE_CW: &str = "
wvg 1
scene 100 100

fill s = polygon points=[(0,0), (100,0), (100,100), (0,100)] color=blue
stroke m = line p1=@s {sel} p2=(999,999) color=black width=1
";

#[test]
fn segment_indices_and_clamping() {
    let cases: &[(&str, f64, f64)] = &[
        ("seg 0 0%", 0.0, 0.0),     // v0
        ("seg 0 50%", 50.0, 0.0),   // edge v0->v1 midpoint
        ("seg 0 100%", 100.0, 0.0), // v1
        ("seg 1 50%", 100.0, 50.0), // edge v1->v2 midpoint
        ("seg 3 25%", 0.0, 75.0),   // closing edge v3->v0
        ("seg 0 150%", 100.0, 0.0), // clamps to segment end
        ("seg 0 -50%", 0.0, 0.0),   // clamps to segment start
        ("seg 2", 100.0, 100.0),    // default pct = 0%
    ];
    for (sel, x, y) in cases {
        let (gx, gy) = last_point_of(&SQUARE_CW.replace("{sel}", sel));
        assert!(
            approx(gx, *x) && approx(gy, *y),
            "sel `{sel}`: got ({gx}, {gy})"
        );
    }
}

#[test]
fn segment_refs_ignore_winding() {
    // same square, wound the other way; segment addressing is by vertex
    // order regardless of winding_sign
    let src = "
wvg 1
scene 100 100

fill s = polygon points=[(0,0), (0,100), (100,100), (100,0)] color=blue
stroke m = line p1=@s seg 0 50% p2=(999,999) color=black width=1
";
    let (x, y) = last_point_of(src);
    assert!(approx(x, 0.0) && approx(y, 50.0), "got ({x}, {y})");
}

#[test]
fn segment_out_of_range_is_error() {
    let err = resolve_src(&SQUARE_CW.replace("{sel}", "seg 4 0%")).unwrap_err();
    assert!(err.msg.contains("out of range"), "{}", err.msg);
    let err = resolve_src(&SQUARE_CW.replace("{sel}", "seg 3 0%")).err();
    assert!(err.is_none(), "closing edge 3 must be valid");
}

#[test]
fn segment_target_must_be_chain() {
    let src = "
wvg 1
scene 100 100

guide c = circle center=(50,50) radius=10
stroke m = line p1=@c seg 0 0% p2=(999,999) color=black width=1
";
    let err = resolve_src(src).unwrap_err();
    assert!(err.msg.contains("polygon or polyline"), "{}", err.msg);
}

// ---- generators -------------------------------------------------------------

#[test]
fn along_closed_vs_open_spacing() {
    // closed: origins at 0/25/50/75% around r=1 circle => 4 placements
    let src = "
wvg 1
scene 10 10

stroke m = along
  track=circle center=(0,0) radius=1
  motifs=[line p1=(0,0) p2=(1,0)]
  n=4 align=none
  color=red
";
    let ops = ops_of(src);
    assert_eq!(ops.len(), 4);
    let origins: Vec<(f64, f64)> = ops
        .iter()
        .map(|op| match &op.shape {
            RShape::Polyline(pts) => (pts[0].x, pts[0].y),
            _ => panic!(),
        })
        .collect();
    assert!(approx(origins[0].0, 1.0) && approx(origins[0].1, 0.0));
    assert!(approx(origins[1].0, 0.0) && approx(origins[1].1, 1.0));
    assert!(approx(origins[2].0, -1.0) && approx(origins[2].1, 0.0));

    // open track with n=3: endpoints included, spacing 50%
    let src = "
wvg 1
scene 10 10

stroke m = along
  track=polyline points=[(0,0), (3,0)]
  motifs=[line p1=(0,0) p2=(1,0)]
  n=3 align=none
  color=red
";
    let ops = ops_of(src);
    let pts = match &ops[2].shape {
        RShape::Polyline(pts) => pts[0],
        _ => panic!(),
    };
    assert!(approx(pts.x, 3.0) && approx(pts.y, 0.0));
}

#[test]
fn grid_is_row_major() {
    let src = "
wvg 1
scene 100 100

fill m = grid motifs=[circle center=(0,0) radius=1] cols=2 rows=2 dx=10 dy=20 origin=(0,0) color=red
";
    let ops = ops_of(src);
    let centers: Vec<(f64, f64)> = ops
        .iter()
        .map(|op| match &op.shape {
            RShape::Circle { c, .. } => (c.x, c.y),
            _ => panic!(),
        })
        .collect();
    let expect = [(0.0, 0.0), (10.0, 0.0), (0.0, 20.0), (10.0, 20.0)];
    for (got, want) in centers.iter().zip(expect.iter()) {
        assert!(approx(got.0, want.0) && approx(got.1, want.1), "{got:?}");
    }
}

// ---- validation ---------------------------------------------------------------

#[test]
fn parse_and_validation_errors() {
    let cases = [
        ("wvg 2\nscene 10 10\n", "version"),
        ("scene 10 10\n", "wvg"),
        ("wvg 1\n", "scene"),
        ("wvg 1\nscene 10 10\nwvg 1\n", "statement"),
        ("wvg 1\nscene 10 10\nfill a = circle center=(0,0) radius=0 color=red\n", "positive"),
        ("wvg 1\nscene 10 10\nfill a = arc center=(0,0) radius=5 start_deg=0 sweep_deg=360 color=red\n", "360"),
        ("wvg 1\nscene 10 10\nfill a = polygon points=[(0,0), (1,1)] color=red\n", "3 points"),
        ("wvg 1\nscene 10 10\nfill a = circle center=(0,0) radius=1 color=red\nfill a = circle center=(0,0) radius=1 color=red\n", "duplicate"),
        ("wvg 1\nscene 10 10\nfill circle = circle center=(0,0) radius=1 color=red\n", "reserved"),
        ("wvg 1\nscene 10 10\nfill a = circle center=(0,0) radius=1 color=blue\nfill b = circle center=(0,0) radius=1 color=a\n", "unknown paint"),
        ("wvg 1\nscene 10 10\nstroke a = line p1=@ghost 0% p2=(1,1) color=red\n", "unknown node"),
        ("wvg 1\nscene 10 10\nfill a = circle center=@a 0% radius=1 color=red\n", "cyclic"),
        ("wvg 1\nscene 10 10\nfill a = polygon points=[(0,0), (4,0), (2,0.0000001), (2,-0.0000001)] color=red\n", "degenerate"),
        ("wvg 1\nscene 10 10\nfill a = grid motifs=[circle center=(0,0) radius=1] cols=0 rows=1 dx=1 dy=1 color=red\n", "1 column"),
    ];
    for (src, needle) in cases {
        match resolve_src(src) {
            Ok(_) => panic!("expected error mentioning `{needle}` for: {src}"),
            Err(e) => assert!(
                e.msg.to_lowercase().contains(needle),
                "error `{}` does not mention `{needle}`",
                e.msg
            ),
        }
    }
}

#[test]
fn rounded_fillet_limit() {
    let ok = "
wvg 1
scene 100 100

fill r = rounded shape=polygon points=[(0,0), (100,0), (100,100), (0,100)] radius=40 color=red
";
    assert!(resolve_src(ok).is_ok());

    let too_big = "
wvg 1
scene 100 100

fill r = rounded shape=polygon points=[(0,0), (100,0), (100,100), (0,100)] radius=60 color=red
";
    let err = resolve_src(too_big).unwrap_err();
    assert!(err.msg.contains("does not fit"), "{}", err.msg);
}

#[test]
fn hidden_nodes_draw_nothing_but_stay_referenceable() {
    let src = "
wvg 1
scene 100 100

guide g = circle center=(50,0) radius=50
stroke m = line p1=@g 25% p2=(999,999) color=black width=1
";
    let ops = ops_of(src);
    assert_eq!(ops.len(), 1, "the guide must not draw");
    let (x, y) = first_point(&ops[0]);
    assert!(approx(x, 50.0) && approx(y, 50.0));
}
