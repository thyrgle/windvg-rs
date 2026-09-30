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
        ("wvg 10\nscene 10 10\n", "version"),
        ("scene 10 10\n", "wvg"),
        ("wvg 1\n", "scene"),
        ("wvg 1\nscene 10 10\nwvg 1\n", "statement"),
        ("wvg 1\nscene 10 10\nfill a = circle center=(0,0) radius=0 color=red\n", "positive"),
        ("wvg 1\nscene 10 10\nfill a = arc center=(0,0) radius=5 start_deg=0 sweep_deg=360 color=red\n", "360"),
        ("wvg 1\nscene 10 10\nfill a = polygon points=[(0,0), (1,1)] color=red\n", "3 points"),
        ("wvg 1\nscene 10 10\nfill a = circle center=(0,0) radius=1 color=red\nfill a = circle center=(0,0) radius=1 color=red\n", "duplicate"),
        ("wvg 1\nscene 10 10\npaint deg = #fff\n", "reserved"),
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

// ---- v2 constructs ---------------------------------------------------------

#[test]
fn v2_transforms_parse_and_version_two_is_accepted() {
    let src = "wvg 2\nscene 10 10\n\nfill a = circle center=(0,0) radius=1 transform=rotate 45 about (5,5) color=red\n";
    let ops = resolve_src(src).unwrap();
    match &ops[0].shape {
        RShape::Circle { c, .. } => {
            // (0,0) rotated 45° cw-on-screen about (5,5) lands at (5, −2.07)
            assert!(
                (c.x - 5.0).abs() < 1e-2 && (c.y + 2.071).abs() < 1e-2,
                "{c:?}"
            );
        }
        _ => panic!("expected a circle"),
    }
    // v1 and v2 files remain valid
    resolve_src("wvg 1\nscene 10 10\n\nfill a = circle center=(0,0) radius=1 color=red\n").unwrap();
    resolve_src("wvg 2\nscene 10 10\n\nfill a = circle center=(0,0) radius=1 color=red\n").unwrap();
    // future versions are rejected
    assert!(resolve_src("wvg 10\nscene 10 10\n").is_err());
}

#[test]
fn v2_group_composes_transforms() {
    // group translate(40,20) ∘ child scale(1.2): circle center (60,60)
    // resolves to (40 + 72, 20 + 72) = (112, 92)
    let src = "
wvg 2
scene 200 200

group transform=translate 40 20 {
  fill a = circle center=(60,60) radius=25 transform=scale 1.2 color=red
}
";
    let ops = resolve_src(src).unwrap();
    match &ops[0].shape {
        RShape::Circle { c, r } => {
            assert!(
                (c.x - 112.0).abs() < 1e-9 && (c.y - 92.0).abs() < 1e-9,
                "{c:?}"
            );
            assert!((r - 30.0).abs() < 1e-9);
        }
        _ => panic!("expected a circle"),
    }
}

#[test]
fn v2_rect_pie_chord_resolve() {
    let src = "
wvg 2
scene 200 200

fill box = rect center=(50,50) size=(40,20) color=red
fill wedge = pie center=(100,50) radius=30 start_deg=0 sweep_deg=90 color=blue
fill lid = chord center=(100,120) radius=30 start_deg=0 sweep_deg=90 color=cyan
";
    let ops = resolve_src(src).unwrap();
    match &ops[0].shape {
        RShape::Polygon(pts) => {
            assert!((pts[0].x - 30.0).abs() < 1e-9 && (pts[0].y - 40.0).abs() < 1e-9);
        }
        _ => panic!("rect resolves to a polygon"),
    }
    for (op, sub_count) in [(1, 3usize), (2, 2usize)] {
        match &ops[op].shape {
            RShape::Path { subpaths, .. } => {
                assert_eq!(subpaths.len(), 1);
                assert_eq!(subpaths[0].instructions.len(), sub_count);
            }
            _ => panic!("pie/chord resolve to paths"),
        }
    }
}

#[test]
fn v2_between_resolves_and_extrapolates() {
    let src = "
wvg 2
scene 100 100

fill a = circle center=between (10,10) (30,30) 50% radius=2 color=red
fill b = circle center=between (10,10) (30,30) 150% radius=2 color=blue
";
    let ops = resolve_src(src).unwrap();
    for (op, want) in [(0, (20.0, 20.0)), (1, (40.0, 40.0))] {
        match &ops[op].shape {
            RShape::Circle { c, .. } => {
                assert!(
                    (c.x - want.0).abs() < 1e-9 && (c.y - want.1).abs() < 1e-9,
                    "{c:?}"
                );
            }
            _ => panic!("expected a circle"),
        }
    }
}

// ---- v4: strings and the text node (spec §7.19) ----

#[test]
fn v4_string_lexing_and_escapes() {
    let toks = windvg::lexer::lex(r#"content="say \"hi\" \\ ok""#).unwrap();
    match &toks[0].tok {
        windvg::lexer::Tok::Ident(s) => assert_eq!(s, "content"),
        other => panic!("expected ident, found {other:?}"),
    }
    let windvg::lexer::Tok::Str(s) = &toks[2].tok else {
        panic!("expected string, found {:?}", toks[2].tok);
    };
    assert_eq!(s, r#"say "hi" \ ok"#);

    // unterminated and invalid escapes are lexer errors
    assert!(windvg::lexer::lex(r#""unterminated"#).is_err());
    assert!(windvg::lexer::lex(r#""bad \n escape""#).is_err());
    assert!(windvg::lexer::lex("\"two\nlines\"").is_err());
}

#[test]
fn v4_text_node_parses_and_resolves() {
    let src = r#"
wvg 4
scene 200 100

text t1 = at=(20,50) content="Hi" size=16 color=red
text t2 = at=(100,50) content="Hi" size=16 anchor=middle
text t3 = at=(180,50) content="Hi" size=16 anchor=end hidden
"#;
    let mut doc = windvg::parser::parse(src).unwrap();
    windvg::parser::assign_ids(&mut doc);
    assert_eq!(doc.nodes.len(), 3);
    let ops = windvg::resolve::resolve(&doc).unwrap();
    // hidden node contributes nothing
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].kind, windvg::ir::OpKind::Text);
    let meta = ops[0].text.as_ref().unwrap();
    assert_eq!(meta.content, "Hi");
    assert_eq!(meta.anchor, "start");
    let _ = meta.width;
    // meta.at carries the raw baseline point; the anchor shift applies at
    // export/bake time (covered by the SVG golden)
    let m2 = ops[1].text.as_ref().unwrap();
    assert!((m2.at.x - 100.0).abs() < 1e-9);
    assert_eq!(m2.anchor, "middle");

    // anchors: at must accept full parametric points
    let src2 = r#"
wvg 4
scene 200 100

fill base = circle center=(100,50) radius=40 color=blue
text label = at=@base 50% + (0,40) content="wheel" size=14

"#;
    let mut doc2 = windvg::parser::parse(src2).unwrap();
    windvg::parser::assign_ids(&mut doc2);
    let ops2 = windvg::resolve::resolve(&doc2).unwrap();
    let m = ops2[1].text.as_ref().unwrap();
    // @base 50% is the left side of the circle (60,50); + (0,40) → (60,90)
    assert!((m.at.x - 60.0).abs() < 1e-9 && (m.at.y - 90.0).abs() < 1e-9);
}

#[test]
fn v4_text_validation_errors() {
    // size must be positive
    let bad_size = r#"
wvg 4
scene 100 100
text t = at=(10,10) content="x" size=0
"#;
    assert!(windvg::parser::parse(bad_size).is_err());

    // anchor must be start|middle|end
    let bad_anchor = r#"
wvg 4
scene 100 100
text t = at=(10,10) content="x" size=10 anchor=left
"#;
    assert!(windvg::parser::parse(bad_anchor).is_err());

    // unknown font fails at resolve
    let unknown_font = r#"
wvg 4
scene 100 100
text t = at=(10,10) content="x" size=10 font=comic
"#;
    let mut doc = windvg::parser::parse(unknown_font).unwrap();
    windvg::parser::assign_ids(&mut doc);
    assert!(windvg::resolve::resolve(&doc).is_err());

    // `text` is reserved and cannot be a node name
    let reserved = r#"
wvg 4
scene 100 100
fill text = circle center=(10,10) radius=5 color=red
"#;
    assert!(windvg::parser::parse(reserved).is_err());
}

#[test]
fn v4_version_gates() {
    assert!(windvg::parser::parse("wvg 4 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 5 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 6 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 7 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 8 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 9 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 10 scene 10 10").is_err());
}

#[test]
fn v4_text_in_group_and_tier_b_refusal() {
    let src = r#"
wvg 4
scene 200 100

group {
  text label = at=(20,50) content="in group" size=12
}
"#;
    let mut doc = windvg::parser::parse(src).unwrap();
    windvg::parser::assign_ids(&mut doc);
    let ops = windvg::resolve::resolve(&doc).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(windvg::tvg::encode(&ops, doc.width, doc.height, 4, false).is_err());
    assert!(windvg::tvg::encode(&ops, doc.width, doc.height, 4, true).is_ok());
}

// ---- v5: tangent offsets (spec §7.20) ----

#[test]
fn v5_tangent_resolves_all_four_directions() {
    // cw circle at 0% = (160,100); travel = (0,1) (down)
    let cases: &[(&str, (f64, f64))] = &[
        ("tangent 20", (160.0, 120.0)),                 // with travel
        ("tangent 20 deg 90", (140.0, 100.0)),          // right-hand normal
        ("tangent 20 deg 180", (160.0, 80.0)),          // reverse
        ("tangent -20", (160.0, 80.0)),                 // negative = reverse
        ("tangent 20 deg 90 + (5, 0)", (145.0, 100.0)), // composes with offset
    ];
    for (clause, want) in cases {
        let src = format!(
            "wvg 5 scene 200 200\nstroke rim = circle center=(100,100) radius=60 color=red\nfill d = circle center=@rim {clause} radius=1 color=red\n"
        );
        let ops = resolve_src(&src).unwrap();
        let (x, y) = match &ops.last().unwrap().shape {
            windvg::resolve::RShape::Circle { c, .. } => (c.x, c.y),
            other => panic!("expected circle, found {other:?}"),
        };
        assert!(
            (x - want.0).abs() < 1e-9 && (y - want.1).abs() < 1e-9,
            "`{clause}`: got ({x}, {y}), want {want:?}"
        );
    }
}

#[test]
fn v5_segment_tangent_and_errors() {
    let src = "wvg 5 scene 200 200\nstroke rail = polygon points=[(40,40), (140,40), (140,90)] color=red\nfill d = circle center=@rail seg 0 50% tangent 10 deg 90 radius=1 color=red\n";
    let ops = resolve_src(src).unwrap();
    let (x, y) = match &ops.last().unwrap().shape {
        windvg::resolve::RShape::Circle { c, .. } => (c.x, c.y),
        other => panic!("expected circle, found {other:?}"),
    };
    // travel is +x; right-hand normal is +y
    assert!(
        (x - 90.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9,
        "({x}, {y})"
    );

    // zero-length segment tangent is an error
    let bad = "wvg 5 scene 200 200\nstroke rail = polygon points=[(40,40), (40,40), (80,40)] color=red\nfill d = circle center=@rail seg 0 50% tangent 10 radius=1 color=red\n";
    assert!(resolve_src(bad).is_err());

    // version gate: 7 accepted, 8 rejected
    assert!(windvg::parser::parse("wvg 7 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 8 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 9 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 10 scene 10 10").is_err());
}

// ---- v6: constants, expressions, repeat (spec §7.21) ----

#[test]
fn v6_constants_and_expressions() {
    let src = "
wvg 6
scene 200 200

let R = 60
let cx = 100
let cy = 40 + 2 * 30
fill a = circle center=(cx, cy) radius=R color=red
fill b = circle center=((cx + R), cy) radius=((R / 4) + 1) color=blue
fill c = circle center=(cx, (cy + R + 10)) radius=-(-20) color=green
";
    let ops = resolve_src(src).unwrap();
    assert_eq!(ops.len(), 3);
    match &ops[0].shape {
        windvg::resolve::RShape::Circle { c, r } => {
            assert!((c.x - 100.0).abs() < 1e-9 && (c.y - 100.0).abs() < 1e-9);
            assert!((r - 60.0).abs() < 1e-9);
        }
        other => panic!("expected circle, found {other:?}"),
    }
    match &ops[1].shape {
        windvg::resolve::RShape::Circle { c, r } => {
            assert!((c.x - 160.0).abs() < 1e-9);
            assert!((r - 16.0).abs() < 1e-9);
        }
        other => panic!("expected circle, found {other:?}"),
    }
    match &ops[2].shape {
        windvg::resolve::RShape::Circle { c, .. } => {
            assert!((c.y - 170.0).abs() < 1e-9);
        }
        other => panic!("expected circle, found {other:?}"),
    }
}

#[test]
fn v6_positional_terms_and_precedence() {
    // positional slots take a single term: `translate 100 -40` is two values
    let src = "
wvg 6
scene 200 200

fill a = rect center=(50,50) size=(60, 30) transform=translate 100 -40 color=red
";
    let ops = resolve_src(src).unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Polygon(pts) => {
            // translated by (100, -40): center (150, 10)
            let (lo, hi) = pts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.y), hi.max(p.y))
                });
            assert!(((lo + hi) / 2.0 - 10.0).abs() < 1e-9, "y center {lo}..{hi}");
        }
        other => panic!("expected polygon, found {other:?}"),
    }
}

#[test]
fn v6_repeat_expands_and_substitutes() {
    let src = "
wvg 6
scene 220 120

let teeth = 3
guide rim = circle center=(60,60) radius=40
repeat i = teeth {
  fill dot~ = circle center=(i * 40, (20 + i * 10)) radius=4 color=blue
}
fill after = circle center=(180,100) radius=2 color=red
";
    let mut doc = windvg::parser::parse(src).unwrap();
    windvg::parser::assign_ids(&mut doc);
    assert_eq!(doc.nodes.len(), 5, "guide + 3 dots + after");
    // names: dot1..dot3
    let names: Vec<String> = doc.nodes.iter().map(|n| n.name.clone()).collect();
    assert_eq!(names, vec!["rim", "dot1", "dot2", "dot3", "after"]);
    let ops = windvg::resolve::resolve(&doc).unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Circle { c, r } => {
            assert!((c.x - 40.0).abs() < 1e-9 && (c.y - 30.0).abs() < 1e-9);
            assert!((r - 4.0).abs() < 1e-9);
        }
        other => panic!("expected circle, found {other:?}"),
    }
    match &ops[2].shape {
        windvg::resolve::RShape::Circle { c, .. } => {
            assert!((c.x - 120.0).abs() < 1e-9 && (c.y - 50.0).abs() < 1e-9);
        }
        other => panic!("expected circle, found {other:?}"),
    }
}

#[test]
fn v6_repeat_index_in_names_and_refs() {
    let src = "
wvg 6
scene 200 200

fill base = circle center=(100,100) radius=80 color=#dddddd
repeat i = 4 {
  fill mark~ = circle center=@base i% radius=2 color=red
  stroke ring~ = circle center=@mark~ 0% radius=6 color=red width=1
}
";
    let ops = resolve_src(src).unwrap();
    assert_eq!(ops.len(), 1 + 4 * 2);
    let mut doc = windvg::parser::parse(src).unwrap();
    windvg::parser::assign_ids(&mut doc);
    let names: Vec<String> = doc.nodes.iter().map(|n| n.name.clone()).collect();
    assert_eq!(names[1], "mark1");
    assert_eq!(names[6], "ring3");
}

#[test]
fn v6_errors() {
    // unknown constant
    assert!(
        resolve_src("wvg 6 scene 10 10\nfill a = circle center=(1,1) radius=R color=red\n")
            .is_err()
    );
    // duplicate constant
    assert!(resolve_src("wvg 6 scene 10 10\nlet a = 1\nlet a = 2\n").is_err());
    // division by zero
    assert!(resolve_src("wvg 6 scene 10 10\nlet a = 1 / 0\n").is_err());
    // non-integral repeat count
    assert!(resolve_src(
        "wvg 6 scene 10 10\nrepeat i = 2.5 { fill a = circle center=(1,1) radius=1 color=red }\n"
    )
    .is_err());
    // repeat count out of range
    assert!(resolve_src(
        "wvg 6 scene 10 10\nrepeat i = 0 { fill a = circle center=(1,1) radius=1 color=red }\n"
    )
    .is_err());
    assert!(resolve_src(
        "wvg 6 scene 10 10\nrepeat i = 1001 { fill a = circle center=(1,1) radius=1 color=red }\n"
    )
    .is_err());
    // nested repeat
    assert!(resolve_src(
        "wvg 6 scene 10 10\nrepeat i = 2 { repeat j = 2 { fill a~ = circle center=(1,1) radius=1 color=red } }\n"
    )
    .is_err());
    // ~ outside a repeat
    assert!(
        resolve_src("wvg 6 scene 10 10\nfill a~ = circle center=(1,1) radius=1 color=red\n")
            .is_err()
    );
    // non-integral segment index expression
    assert!(resolve_src(
        "wvg 6 scene 10 10\nstroke r = polygon points=[(0,0), (10,0), (10,10), (0,10)] color=red\nfill d = circle center=@r seg (5 / 2) radius=1 color=red\n"
    )
    .is_err());
    // constant name is reserved
    assert!(resolve_src("wvg 6 scene 10 10\nlet fill = 1\n").is_err());
    // constants share the namespace with nodes
    assert!(resolve_src(
        "wvg 6 scene 10 10\nlet a = 1\nfill a = circle center=(1,1) radius=1 color=red\n"
    )
    .is_err());
}

// ---- v7: arc_between + intersects (spec §7.22/§7.23) ----

#[test]
fn v7_arc_between_math() {
    let ops = resolve_src(
        "wvg 7 scene 10 10\nstroke deck = arc_between p1=(40,120) p2=(180,120) deg=90 color=red width=1\n",
    )
    .unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Arc {
            c,
            r,
            start_deg,
            sweep_deg,
        } => {
            assert!((c.x - 110.0).abs() < 1e-9 && (c.y - 190.0).abs() < 1e-9);
            assert!((r - 70.0 * std::f64::consts::SQRT_2).abs() < 1e-9);
            assert!((start_deg - (-135.0)).abs() < 1e-9);
            assert!((sweep_deg - 90.0).abs() < 1e-9);
        }
        other => panic!("expected arc, found {other:?}"),
    }
}

#[test]
fn v7_arc_between_signs_and_validation() {
    // negative sweep bulges the other way
    let ops = resolve_src(
        "wvg 7 scene 10 10\nstroke a = arc_between p1=(40,120) p2=(180,120) deg=-60 color=red width=1\n",
    )
    .unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Arc {
            c, r, sweep_deg, ..
        } => {
            assert!((r - 140.0).abs() < 1e-9);
            assert!(c.y < 120.0);
            assert!((sweep_deg + 60.0).abs() < 1e-9);
        }
        other => panic!("expected arc, found {other:?}"),
    }
    for bad in [
        "wvg 7 scene 10 10\nstroke a = arc_between p1=(5,5) p2=(5,5) deg=90 color=red\n",
        "wvg 7 scene 10 10\nstroke a = arc_between p1=(0,0) p2=(10,0) deg=0 color=red\n",
        "wvg 7 scene 10 10\nstroke a = arc_between p1=(0,0) p2=(10,0) deg=360 color=red\n",
    ] {
        assert!(resolve_src(bad).is_err(), "should reject: {bad}");
    }
}

#[test]
fn v7_intersects_finds_crossings_in_order() {
    let src = "
wvg 7
scene 220 160

guide rail = line p1=(30,110) p2=(190,50)
guide hoop = circle center=(110,80) radius=45
fill hit1 = circle center=intersects rail hoop radius=3 color=red
fill hit2 = circle center=intersects rail hoop 2 radius=3 color=blue
";
    let ops = resolve_src(src).unwrap();
    assert_eq!(ops.len(), 2);
    let (a, b) = match (&ops[0].shape, &ops[1].shape) {
        (RShape::Circle { c: a, .. }, RShape::Circle { c: b, .. }) => (*a, *b),
        other => panic!("expected circles, found {other:?}"),
    };
    assert!(a.x < b.x, "chain order runs along the rail: {a:?} {b:?}");
    // both on the hoop
    for p in [a, b] {
        let d = ((p.x - 110.0).powi(2) + (p.y - 80.0).powi(2)).sqrt();
        assert!((d - 45.0).abs() < 0.2, "not on the hoop: {d}");
    }
}

#[test]
fn v7_intersects_errors() {
    // missing k-th crossing
    assert!(resolve_src(
        "wvg 7 scene 10 10\nguide a = line p1=(0,0) p2=(10,10)\nguide b = line p1=(100,100) p2=(110,110)\nfill c = circle center=intersects a b 1 radius=1 color=red\n"
    )
    .is_err());
    // unknown node
    assert!(resolve_src(
        "wvg 7 scene 10 10\nguide a = line p1=(0,0) p2=(10,10)\nfill c = circle center=intersects a ghost radius=1 color=red\n"
    )
    .is_err());
    // 1-based k: k=0 rejects
    assert!(resolve_src(
        "wvg 7 scene 10 10\nguide a = line p1=(0,0) p2=(10,10)\nguide b = circle center=(5,5) radius=4\nfill c = circle center=intersects a b 0 radius=1 color=red\n"
    )
    .is_err());
}

// ---- v8: property defaults (spec §5.4) ----

#[test]
fn v8_color_defaults_to_black() {
    let ops = resolve_src(
        "wvg 8 scene 100 100\nfill a = circle center=(50,50) radius=10\nstroke b = line p1=(0,0) p2=(10,10) width=2\n",
    )
    .unwrap();
    for op in &ops {
        match op.paint {
            windvg::resolve::RPaint::Color(c) => {
                assert_eq!((c.r, c.g, c.b, c.a), (0.0, 0.0, 0.0, 1.0));
            }
            other => panic!("expected flat black, found {other:?}"),
        }
    }
    // explicit color still wins
    let ops =
        resolve_src("wvg 8 scene 100 100\nfill a = circle center=(50,50) radius=10 color=red\n")
            .unwrap();
    match ops[0].paint {
        windvg::resolve::RPaint::Color(c) => assert_eq!((c.r, c.g, c.b), (1.0, 0.0, 0.0)),
        other => panic!("expected flat red, found {other:?}"),
    }
}

#[test]
fn v8_text_size_defaults_to_16() {
    let ops = resolve_src("wvg 8 scene 100 100\ntext t = at=(10,50) content=\"hi\"\n").unwrap();
    let meta = ops[0].text.as_ref().unwrap();
    assert_eq!(meta.size, 16.0);
    // explicit size still wins; positive check still applies
    let ops =
        resolve_src("wvg 8 scene 100 100\ntext t = at=(10,50) content=\"hi\" size=24\n").unwrap();
    assert_eq!(ops[0].text.as_ref().unwrap().size, 24.0);
    assert!(
        resolve_src("wvg 8 scene 100 100\ntext t = at=(10,50) content=\"hi\" size=0\n").is_err()
    );
}

#[test]
fn v8_arc_start_deg_defaults_to_zero() {
    let ops =
        resolve_src("wvg 8 scene 100 100\nstroke a = arc center=(50,50) radius=30 sweep_deg=90\n")
            .unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Arc {
            start_deg,
            sweep_deg,
            ..
        } => {
            assert!((start_deg - 0.0).abs() < 1e-12);
            assert!((sweep_deg - 90.0).abs() < 1e-12);
        }
        other => panic!("expected arc, found {other:?}"),
    }
    // explicit start_deg still wins
    let ops = resolve_src(
        "wvg 8 scene 100 100\nstroke a = arc center=(50,50) radius=30 start_deg=45 sweep_deg=90\n",
    )
    .unwrap();
    match &ops[0].shape {
        windvg::resolve::RShape::Arc { start_deg, .. } => {
            assert!((start_deg - 45.0).abs() < 1e-12);
        }
        other => panic!("expected arc, found {other:?}"),
    }
}

#[test]
fn v8_version_gate() {
    assert!(windvg::parser::parse("wvg 8 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 9 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 10 scene 10 10").is_err());
}

// ---- v9: anonymous nodes + positional shape properties (spec §5.5/§5.6) ----

#[test]
fn v9_anonymous_nodes() {
    let mut doc = windvg::parser::parse(
        "wvg 9 scene 200 200\nfill circle (100,100) 40 color=red\nstroke line (0,0) (10,10) width=2\ntext (10,50) \"hi\"\n",
    )
    .unwrap();
    windvg::parser::assign_ids(&mut doc);
    let names: Vec<String> = doc.nodes.iter().map(|n| n.name.clone()).collect();
    assert_eq!(
        names,
        vec!["circle1", "line1", "text1"],
        "per-keyword counters"
    );

    // explicit and anonymous share the name space: the auto-name bumps
    // past an explicitly taken name instead of erroring
    let mut doc2 = windvg::parser::parse(
        "wvg 9 scene 200 200\nfill circle1 = circle (0,0) 1 color=red\nfill circle (5,5) 1 color=red\n",
    )
    .unwrap();
    windvg::parser::assign_ids(&mut doc2);
    assert_eq!(doc2.nodes[1].name, "circle2", "auto-name skips taken names");

    // text: keyword + counter
    let mut doc3 = windvg::parser::parse("wvg 9 scene 100 100\ntext (10,10) \"a\"\n").unwrap();
    windvg::parser::assign_ids(&mut doc3);
    assert_eq!(doc3.nodes[0].name, "text1");
}

#[test]
fn v9_positional_shapes_parse_like_named() {
    // every positional form must resolve identically to its named twin
    let pairs: &[(&str, &str)] = &[
        (
            "fill a = circle center=(100,100) radius=80 color=red",
            "fill circle (100,100) 80 color=red",
        ),
        (
            "stroke a = line p1=(0,0) p2=(10,10) color=red",
            "stroke line (0,0) (10,10) color=red",
        ),
        (
            "fill a = rect center=(50,50) size=(60,30) color=red",
            "fill rect (50,50) (60,30) color=red",
        ),
        (
            "fill a = ellipse center=(50,50) rx=40 ry=20 color=red",
            "fill ellipse (50,50) 40 20 color=red",
        ),
        (
            "stroke a = arc center=(50,50) radius=30 start_deg=45 sweep_deg=90 color=red",
            "stroke arc (50,50) 30 start_deg=45 sweep_deg=90 color=red",
        ),
        (
            "fill a = pie center=(50,50) radius=30 start_deg=0 sweep_deg=135 color=red",
            "fill pie (50,50) 30 start_deg=0 sweep_deg=135 color=red",
        ),
        (
            "stroke a = arc_between p1=(40,140) p2=(180,140) deg=90 color=red width=1",
            "stroke arc_between (40,140) (180,140) deg=90 color=red width=1",
        ),
        (
            "fill a = regular_polygon center=(50,50) radius=30 sides=6 color=red",
            "fill regular_polygon (50,50) 30 sides=6 color=red",
        ),
        (
            "fill a = star center=(50,50) outer_radius=30 inner_radius=12 points=5 color=red",
            "fill star (50,50) 30 12 points=5 color=red",
        ),
        (
            "fill a = rounded shape=polygon points=[(10,10), (30,10), (30,30), (10,30)] radius=4 color=red",
            "fill rounded polygon [(10,10), (30,10), (30,30), (10,30)] radius=4 color=red",
        ),
        (
            "stroke a = along track=circle center=(100,100) radius=57 motifs=[line p1=(-8,0) p2=(8,0)] n=9 align=tangent color=red width=1",
            "stroke along circle (100,100) 57 [line (-8,0) (8,0)] n=9 align=tangent color=red width=1",
        ),
        (
            "fill a = polar center=(100,100) motifs=[circle center=(0,0) radius=5] n=8 radius=40 color=red",
            "fill polar (100,100) [circle (0,0) 5] n=8 radius=40 color=red",
        ),
        (
            "fill a = grid motifs=[rect center=(0,0) size=(10,10)] cols=4 rows=3 dx=20 dy=20 color=red",
            "fill grid [rect (0,0) (10,10)] cols=4 rows=3 dx=20 dy=20 color=red",
        ),
        (
            "fill a = compound shapes=[circle center=(0,0) radius=10, circle center=(5,5) radius=10] color=red",
            "fill compound [circle (0,0) 10, circle (5,5) 10] color=red",
        ),
    ];
    for (named, positional) in pairs {
        let a = resolve_src(&format!("wvg 9 scene 200 200\n{named}\n")).unwrap();
        let b = resolve_src(&format!("wvg 9 scene 200 200\n{positional}\n")).unwrap();
        let ja = windvg::json::ops_json(&a);
        let jb = windvg::json::ops_json(&b);
        assert_eq!(ja, jb, "positional form diverged:\n{named}\n{positional}");
    }
}

#[test]
fn v9_text_positional_and_references_still_need_names() {
    let ops = resolve_src("wvg 9 scene 100 100\ntext (10,50) \"hi\" size=24\n").unwrap();
    let meta = ops[0].text.as_ref().unwrap();
    assert_eq!(meta.content, "hi");
    assert_eq!(meta.size, 24.0);
    assert_eq!(meta.at.x, 10.0);

    // generated names are referenceable but order-dependent (§5.5):
    // possible, discouraged for anything load-bearing
    let ops = resolve_src(
        "wvg 9 scene 100 100\nfill circle (50,50) 40 color=red\nfill d = circle center=@circle1 100% radius=2 color=red\n",
    )
    .unwrap();
    assert_eq!(ops.len(), 2);
}

#[test]
fn v9_reserved_name_gets_a_clear_error() {
    // `p1` is reserved (a line prop): the error must say so, not
    // "expected a shape"
    let err = resolve_src(
        "wvg 9 scene 100 100\nfill p1 = circle center=(10,10) radius=2 color=red\n",
    )
    .unwrap_err();
    assert!(
        err.msg.contains("reserved") && err.msg.contains("p1"),
        "error should name the reserved word: {}",
        err.msg
    );
}
