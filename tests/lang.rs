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
        ("wvg 9\nscene 10 10\n", "version"),
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
    assert!(resolve_src("wvg 6\nscene 10 10\n").is_err());
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
    assert!(windvg::parser::parse("wvg 6 scene 10 10").is_err());
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
        ("tangent 20", (160.0, 120.0)),          // with travel
        ("tangent 20 deg 90", (140.0, 100.0)),   // right-hand normal
        ("tangent 20 deg 180", (160.0, 80.0)),   // reverse
        ("tangent -20", (160.0, 80.0)),          // negative = reverse
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
    assert!((x - 90.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9, "({x}, {y})");

    // zero-length segment tangent is an error
    let bad = "wvg 5 scene 200 200\nstroke rail = polygon points=[(40,40), (40,40), (80,40)] color=red\nfill d = circle center=@rail seg 0 50% tangent 10 radius=1 color=red\n";
    assert!(resolve_src(bad).is_err());

    // version gate: 5 accepted, 6 rejected
    assert!(windvg::parser::parse("wvg 5 scene 10 10").is_ok());
    assert!(windvg::parser::parse("wvg 6 scene 10 10").is_err());
}
