"""Reference builders for golden conformance.

Each builder constructs, through the *Python* windvg reference, the same
drawing as its sibling `tests/files/<name>.wvg` — the Python side has no
`.wvg` parser, so the documents are built by hand through the Document/Scene
APIs. `scripts/check_conformance.py` encodes each builder to TinyVG and
requires the windvg-rs CLI output to match byte-for-byte.
"""

import sys
from pathlib import Path

# Make the Python reference importable when run outside uv
# (`PYTHONPATH=<path-to-windvg>/src`).
_SRC = [p for p in (
    Path(__file__).resolve().parent.parent / "windvg-py" / "src",
    Path("/home/christophers/drawing_stuff/windvg/src"),
) if p.is_dir()]
for p in _SRC:
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

import windvg as wv  # noqa: E402
import windvg.document as wvd  # noqa: E402
from windvg.document import Document  # noqa: E402
from windvg.geometry import Point  # noqa: E402
from windvg.scene import Scene  # noqa: E402


def _hex(h):
    return wv.rgb(int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255)


def _rgba(h, a=1.0):
    return wv.rgba(int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255, a)


# ---- smoke.wvg: donut with gradient, hidden hull, segment tick, sprinkles ----

def build_smoke():
    doc = Document(200.0, 200.0)
    glaze = wv.RadialGradient(
        (90, 90), (140, 110), _rgba("FF9AA2"), _rgba("C2404D"),
    )
    doc.fill("outer", wv.Circle((100, 100), 80), wv.BLACK, visible=False)
    doc.fill(
        "donut",
        wvd.CompoundSpec(shapes=(
            wvd.CircleSpec((100.0, 100.0), 80.0),
            wvd.CircleSpec((100.0, 100.0), 35.0),
        )),
        glaze,
    )
    hull_shape = wv.regular_polygon((100, 100), 85, 6)
    doc.fill("hull", hull_shape, _hex("3E6FA8"), visible=False)

    pts = hull_shape.points

    def seg(k, pct):
        a, b = pts[k], pts[(k + 1) % len(pts)]
        t = pct / 100.0
        return (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)

    doc.stroke(
        "tick", wv.Polyline([seg(2, 50), seg(3, 50)]), _hex("5A8FD6"), 1.5,
    )
    doc.stroke(
        "sprinkles",
        wvd.AlongSpec(
            track=wvd.CircleSpec((100.0, 100.0), 57.0),
            motifs=(wvd.PolySpec(closed=False, points=((-8.0, 0.0), (8.0, 0.0))),),
            n=9,
            offset_pct=4.0,
            align="tangent",
            direction=wv.CW,
        ),
        wv.WHITE,
        4.0,
    )
    return doc.resolve()


# ---- anchors.wvg: parametric anchors, grid cells, forward references ---------

def build_anchors():
    doc = Document(230.0, 200.0)
    doc.fill("box", wvd.PolySpec(
        closed=True,
        points=((20.0, 50.0), (120.0, 50.0), (120.0, 150.0), (20.0, 150.0)),
    ), _hex("4073CC"))
    doc.fill("ring", wv.Circle((160, 100), 35), wv.BLACK, visible=False)
    doc.stroke("tie", wvd.PolySpec(closed=False, points=(
        wvd.AnchorPoint("box", pct=25.0),
        wvd.AnchorPoint("ring", pct=60.0, start=Point(160.0, 135.0), direction=wv.CCW),
    )), _hex("111111"), 2.0)
    doc.fill("dot", wvd.CircleSpec(
        wvd.GridCellPoint("lattice", 2, 1, offset=Point(5.0, -5.0)), 4.0,
    ), wv.RED)
    doc.fill(
        "lattice",
        wvd.GridGuideSpec(origin=(0.0, 0.0), cols=6, rows=5, dx=40.0, dy=40.0),
        wv.BLACK, visible=False,
    )
    doc.fill("hub", wvd.CircleSpec((160.0, 100.0), 8.0), wv.BLUE)
    return doc.resolve()


# ---- ellipse_grad.wvg: rotated ellipse + linear gradient ----------------------

def build_ellipse_grad():
    scene = Scene(120, 120)
    scene.fill(
        wv.Ellipse((60, 60), 50, 25, 30),
        wv.LinearGradient((0, 0), (120, 120), wv.RED, wv.GREEN),
    )
    return scene


# ---- manygon.wvg: 100-point polygon outline fill (fallback path) --------------

def build_manygon():
    import math
    n = 100
    pts = [
        (100 + 90 * math.cos(2 * math.pi * i / n),
         100 + 90 * math.sin(2 * math.pi * i / n))
        for i in range(n)
    ]
    scene = Scene(200, 200)
    scene.outline_fill(wv.Polygon(pts), wv.YELLOW, wv.BLACK, 1.0)
    return scene


# ---- path_hole.wvg: two-subpath even-odd fill ----------------------------------

def build_path_hole():
    P = wv.Point
    scene = Scene(100, 100)
    scene.fill(wv.Path([
        wv.SubPath(P(10, 10), (wv.Line(P(90, 10)), wv.Line(P(50, 90)), wv.Close())),
        wv.SubPath(P(40, 40), (wv.Line(P(60, 40)), wv.Line(P(50, 60)), wv.Close())),
    ]), wv.CYAN)
    return scene


BUILDERS = {
    "smoke": build_smoke,
    "anchors": build_anchors,
    "ellipse_grad": build_ellipse_grad,
    "manygon": build_manygon,
    "path_hole": build_path_hole,
}
