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
    let bytes = windvg::tvg::encode(&ops, doc.width, doc.height, 4)
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
