//! Golden conformance tests: `.wvg` → TinyVG bytes must match the Python
//! reference byte-for-byte. Expected files were generated with windvg
//! (see tests/README or the repo history for the generator script).

use std::path::PathBuf;

fn run_case(name: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/files");
    let src = std::fs::read_to_string(dir.join(format!("{name}.wvg")))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let expected = std::fs::read(dir.join("expected").join(format!("{name}.tvg")))
        .unwrap_or_else(|e| panic!("{name}: {e}"));

    let mut doc = windvg::parser::parse(&src).unwrap_or_else(|e| panic!("{name}: parse: {e}"));
    windvg::parser::assign_ids(&mut doc);
    let ops = windvg::resolve::resolve(&doc).unwrap_or_else(|e| panic!("{name}: resolve: {e}"));
    let bytes = windvg::tvg::encode(&ops, doc.width, doc.height, 4, false)
        .unwrap_or_else(|e| panic!("{name}: encode: {e}"));

    if bytes != expected {
        // find first differing byte for a useful message
        let i = bytes
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(bytes.len().min(expected.len()));
        panic!(
            "{name}: tvg mismatch at byte {i} (got {} bytes, expected {}): got {:x?}, want {:x?}",
            bytes.len(),
            expected.len(),
            bytes.get(i..(i + 8).min(bytes.len())).unwrap_or(&[]),
            expected.get(i..(i + 8).min(expected.len())).unwrap_or(&[]),
        );
    }
}

#[test]
fn golden_smoke_compound_gradient_generators() {
    run_case("smoke");
}

#[test]
fn golden_v5_tangent_offsets() {
    run_case("v5_tangent");
}

#[test]
fn golden_v6_constants() {
    run_case("v6_consts");
}

#[test]
fn golden_v6_repeat() {
    run_case("v6_repeat");
}

/// Text documents (tier B) conform via ops JSON + SVG, not .tvg bytes.
#[test]
fn golden_v4_text_svg_and_metadata() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/files");
    let src = std::fs::read_to_string(dir.join("v4_text.wvg")).unwrap();
    let expected_svg = std::fs::read_to_string(dir.join("expected/v4_text.svg")).unwrap();
    let expected_ops = std::fs::read_to_string(dir.join("v4_text.ops.json")).unwrap();

    let mut doc = windvg::parser::parse(&src).unwrap();
    windvg::parser::assign_ids(&mut doc);
    let ops = windvg::resolve::resolve(&doc).unwrap();

    let got_svg = windvg::svg::render(&ops, doc.width, doc.height);
    assert_eq!(got_svg, expected_svg, "SVG export must match the reference");

    let got_ops = windvg::json::ops_json(&ops);
    assert_eq!(
        got_ops.trim_end(),
        expected_ops.trim_end(),
        "ops JSON must match the reference byte-for-byte"
    );

    // TinyVG refuses text unless dropped
    assert!(windvg::tvg::encode(&ops, doc.width, doc.height, 4, false).is_err());
    let dropped =
        windvg::tvg::encode(&ops, doc.width, doc.height, 4, true).expect("drop-text encode");
    assert_eq!(&dropped[..2], b"\x72\x56");
}

#[test]
fn golden_anchors_grid_cells() {
    run_case("anchors");
}

#[test]
fn golden_ellipse_rotation_gradient() {
    run_case("ellipse_grad");
}

#[test]
fn golden_many_point_outline_fallback() {
    run_case("manygon");
}

#[test]
fn golden_path_with_hole() {
    run_case("path_hole");
}

#[test]
fn golden_v2_transform() {
    run_case("v2_transform");
}

#[test]
fn golden_v2_group() {
    run_case("v2_group");
}

#[test]
fn golden_v2_rect_pie() {
    run_case("v2_rect_pie");
}

#[test]
fn golden_v2_between() {
    run_case("v2_between");
}
