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
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from reference_builders import BUILDERS  # noqa: E402


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
