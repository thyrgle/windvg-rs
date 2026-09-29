# windvg-rs

A fast, dependency-free Rust implementation of the
[`.wvg` language](../windvg/docs/language.md) — the declarative vector
format built around parametric anchors: invisible shapes, points placed by
traveling a percentage of a perimeter clockwise or counter-clockwise, and
segment/grid references. Compiles to [TinyVG](https://tinyvg.tech) and
renders to SVG.

```
source.wvg ──parse──▶ document IR ──resolve──▶ scene ops ──▶ .tvg / .svg
```

## Status

Conforms to the `.wvg` v1 draft spec. Verified against the Python reference
(`windvg`) with **byte-exact** TinyVG output on golden cases covering:
compound shapes, gradients, `along`/`polar` repetition, anchor references
(cw/ccw/wrap/clamp/`from` projection), segment references, grid guides and
cell offsets, ellipse rotation (the TinyVG negation quirk), >64-point
outline-fill fallback, and paths with holes.

## Build

```bash
cargo build --release
```

No dependencies; needs a stable Rust toolchain.

## Usage

```
windvg check <file.wvg>              parse + resolve, report errors
windvg ir    <file.wvg> [-o out]     print the document IR as JSON
windvg ops   <file.wvg> [-o out]     print resolved draw ops as JSON
windvg svg   <file.wvg> [-o out]     render SVG (default: stdout)
windvg tvg   <file.wvg> [-o out.tvg] [--scale N]
                                     encode TinyVG (default: <stem>.tvg,
                                     scale = fraction bits, default 4)
```

Errors are reported as `file:line:col: message`.

Example:

```bash
windvg tvg examples/gear.wvg -o gear.tvg
windvg svg  examples/gear.wvg > gear.svg
```

## Layout

| Module | Role |
| --- | --- |
| `lexer.rs` | tokens (§3 of the spec) |
| `parser.rs` | grammar → IR; desugars `line`/`regular_polygon`/`star`, expands paints, binds names |
| `ir.rs` | document model, shared contract with `windvg.document` |
| `geom.rs` | points, affine transforms, chordal arc-length tables, round-half-even |
| `resolve.rs` | track protocol (perimeter/point/tangent/project/winding), anchors, segments, grid cells, generators, rounded corners |
| `tvg.rs` | TinyVG 1.0 encoder (RGBA8888, 16→32-bit coordinate upgrade, half-even quantization) |
| `svg.rs` | SVG renderer |
| `json.rs` | the two conformance artifacts: document IR and resolved ops as JSON |

## Numeric contract

Byte-exact conformance rests on a few pinned details (spec §7–§8):

- quantization uses **round-half-to-even** (Python's `round()`), not
  half-away-from-zero;
- ellipse track queries go through a 1440-sample chordal arc-length table
  with linear interpolation, and `project` refines with exactly 48 ternary
  search iterations;
- path tracks flatten with tolerance 0.1 (adaptive de Casteljau, depth 16);
- `ARC_ELLIPSE` rotation is written **negated**.

On the same platform these reproduce the Python reference bit-for-bit
(both call the system libm). Golden vectors: `tests/files/`.

## Notes

- `ops` JSON emits a `compound` shape dict, which the Python
  `resolve_to_json` cannot currently represent (it raises on compound
  fills); this is a deliberate superset, documented in the spec (§9/§6).
- The IR keeps parametric references intact (`anchor`, `segment`,
  `grid_cell` dicts) — nothing is baked until resolution, so editors can
  round-trip documents losslessly.
