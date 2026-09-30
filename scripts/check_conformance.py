#!/usr/bin/env python3
"""Golden cross-check: the Python reference vs the windvg-rs CLI.

For every golden document in `tests/files/`, encode TinyVG with the Python
reference (see reference_builders.py) and require the Rust CLI's output to
match byte-for-byte. This keeps the two implementations honest: the Rust
unit tests compare against committed .tvg files, while this script
regenerates the expectation from Python live.

Usage (from the windvg-rs repository root):
    python scripts/check_conformance.py [--rust-bin target/release/windvg]
                                        [--sources tests/files]

Import windvg by putting its source tree on PYTHONPATH, or run through the
windvg project environment:
    uv run --project <path-to-windvg> python scripts/check_conformance.py
"""

import argparse
import json
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from reference_builders import BUILDERS  # noqa: E402

# Text documents (spec §7.19, tier B) carry no .tvg golden: hosts bake
# glyphs with engine-local settings. They conform via resolved metadata
# (ops JSON) and exact SVG export, and their TinyVG encode must FAIL
# unless --drop-text is passed.
TEXT_DOCS = {"v4_text"}


def check_text_doc(rust_bin: str, name: str, src: pathlib.Path, doc) -> int:
    """Conformance for a tier-B text document (spec §9 carve-out)."""
    failures = 0
    rust_ops = subprocess.run(
        [rust_bin, "ops", str(src)], check=True, capture_output=True, text=True
    ).stdout
    try:
        got = json.loads(rust_ops)
        want = json.loads(json.dumps(doc.resolve_to_json()))
        if got != want:
            print(f"FAIL {name}: ops JSON differs from the Python reference")
            failures += 1
        else:
            print(f"OK   {name}: ops JSON matches (text metadata)")
    except json.JSONDecodeError as e:
        print(f"FAIL {name}: ops JSON invalid ({e})")
        failures += 1

    rust_svg = subprocess.run(
        [rust_bin, "svg", str(src)], check=True, capture_output=True, text=True
    ).stdout
    if rust_svg == doc.resolve().to_svg():
        print(f"OK   {name}: SVG export matches ({len(rust_svg)} bytes)")
    else:
        print(f"FAIL {name}: SVG export differs from the Python reference")
        failures += 1

    refused = subprocess.run(
        [rust_bin, "tvg", str(src), "-o", "/tmp/conformance_text.tvg"],
        capture_output=True,
        text=True,
    )
    if refused.returncode != 0:
        print(f"OK   {name}: TinyVG encode refused by default")
    else:
        print(f"FAIL {name}: TinyVG encode should refuse text without --drop-text")
        failures += 1

    dropped = subprocess.run(
        [rust_bin, "tvg", str(src), "--drop-text", "-o", "/tmp/conformance_text.tvg"],
        capture_output=True,
        text=True,
    )
    if dropped.returncode == 0:
        print(f"OK   {name}: --drop-text encode succeeds")
    else:
        print(f"FAIL {name}: --drop-text encode should succeed")
        failures += 1
    return failures


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rust-bin", default="target/release/windvg")
    ap.add_argument("--sources", default="tests/files")
    args = ap.parse_args()

    if not pathlib.Path(args.rust_bin).exists():
        print(f"windvg CLI not found at {args.rust_bin!r}; build it first")
        return 2

    failures = 0
    for name, build in sorted(BUILDERS.items()):
        src = pathlib.Path(args.sources) / f"{name}.wvg"
        if not src.exists():
            print(f"SKIP {name}: missing {src}")
            continue
        built = build()
        if name in TEXT_DOCS:
            failures += check_text_doc(args.rust_bin, name, src, built)
            continue
        scene = built.resolve() if hasattr(built, "resolve") else built
        expected: bytes = scene.to_tinyvg()
        out = pathlib.Path("/tmp") / f"conformance_{name}.tvg"
        subprocess.run(
            [args.rust_bin, "tvg", str(src), "-o", str(out)],
            check=True,
        )
        actual = out.read_bytes()
        if actual == expected:
            print(f"OK   {name}: {len(expected)} bytes, byte-exact")
            continue
        i = next(
            (
                k
                for k, (a, b) in enumerate(zip(actual, expected))
                if a != b
            ),
            min(len(actual), len(expected)),
        )
        print(
            f"FAIL {name}: differs at byte {i} "
            f"(got {len(actual)} bytes, expected {len(expected)})"
        )
        failures += 1

    if failures:
        print(f"{failures} document(s) FAILED")
        return 1
    print("all golden documents match the Python reference byte-for-byte")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
