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


# ---- v2 goldens: transforms, groups, rect/pie/chord, between -----------------

def build_v2_transform():
    import windvg.document as wvd
    from windvg.ext.transform import Transform

    doc = Document(200.0, 200.0)
    doc.fill("g", wv.Circle((100, 100), 60), wv.BLACK, visible=False)

    def spec(t: Transform, shape):
        return wvd.TransformSpec((t.a, t.b, t.c, t.d, t.e, t.f), shape)

    doc.fill("moved", spec(
        Transform.translate(40, 15), wvd.CircleSpec((100.0, 100.0), 25.0),
    ), _hex("E53935"))
    doc.fill("spun", spec(
        Transform.rotate(45, (100, 100)),
        wvd.PolySpec(closed=True, points=(
            (90.0, 90.0), (110.0, 90.0), (110.0, 110.0), (90.0, 110.0),
        )),
    ), _hex("43A047"))
    doc.fill("grown", spec(
        Transform.scale(1.5, 1, (160, 50)),
        wvd.EllipseSpec((160.0, 50.0), 20.0, 10.0),
    ), _hex("1E88E5"))
    return doc.resolve()


def build_v2_group():
    import windvg.document as wvd
    from windvg.ext.transform import Transform

    doc = Document(200.0, 200.0)
    # group translate(40,20) composed onto child scale(1.2)
    group_t = Transform.translate(40, 20)
    child_t = Transform.scale(1.2, 1.2)
    combined = group_t @ child_t
    doc.fill(
        "a",
        wvd.TransformSpec(
            (combined.a, combined.b, combined.c, combined.d, combined.e, combined.f),
            wvd.CircleSpec((60.0, 60.0), 25.0),
        ),
        _hex("8E24AA"),
    )
    # stroke child has no own transform: group transform alone
    doc.stroke("b", wvd.TransformSpec(
        (group_t.a, group_t.b, group_t.c, group_t.d, group_t.e, group_t.f),
        wvd.PolySpec(closed=False, points=((20.0, 60.0), (100.0, 60.0))),
    ), _hex("FF9AA2"), 3.0)
    doc.fill("c", wvd.RectSpec((100.0, 160.0), 60.0, 24.0), _hex("00897B"))
    return doc.resolve()


def build_v2_rect_pie():
    import windvg.document as wvd

    doc = Document(220.0, 200.0)
    doc.fill("g", wv.Circle((60, 60), 40), wv.BLACK, visible=False)
    doc.fill(
        "box",
        wvd.RectSpec(wvd.AnchorPoint("g", pct=25.0), 50.0, 30.0),
        _hex("FB8C00"),
    )
    doc.fill(
        "wedge",
        wvd.PieSpec((150.0, 60.0), 40.0, 0.0, 135.0),
        _hex("43A047"),
    )
    doc.fill(
        "lid",
        wvd.PieSpec((150.0, 150.0), 40.0, 180.0, 120.0, chord=True),
        _hex("1E88E5"),
    )
    doc.stroke(
        "rim",
        wvd.PieSpec((60.0, 150.0), 35.0, 90.0, 180.0),
        _hex("D81B60"), 2.0,
    )
    return doc.resolve()


def build_v2_between():
    import windvg.document as wvd

    doc = Document(200.0, 200.0)
    doc.fill("a", wv.Circle((50, 50), 30), wv.BLACK, visible=False)
    doc.fill("b", wv.Circle((150, 50), 30), wv.BLACK, visible=False)

    def bt(pa, pb, pct):
        return wvd.CircleSpec(wvd.BetweenPoint(pa, pb, pct), 12.0)

    doc.fill(
        "mid",
        bt(wvd.AnchorPoint("a", pct=0.0), wvd.AnchorPoint("b", pct=0.0), 50.0),
        _hex("FDD835"),
    )
    doc.fill(
        "lerp",
        wvd.PolySpec(closed=True, points=(
            wvd.BetweenPoint((20.0, 20.0), (100.0, 160.0), 50.0),
            wvd.BetweenPoint((180.0, 180.0), (180.0, 20.0), 25.0),
            wvd.BetweenPoint((60.0, 90.0), (180.0, 180.0), 50.0),
        )),
        _hex("7FD1C0"),
    )
    doc.fill(
        "ext",
        wvd.CircleSpec(
            wvd.BetweenPoint(
                wvd.AnchorPoint("a", pct=0.0), wvd.AnchorPoint("b", pct=0.0), 125.0
            ),
            4.0,
        ),
        _hex("E53935"),
    )
    return doc.resolve()


def build_v3_offset():
    doc = Document(220.0, 200.0)
    doc.fill("g", wv.Circle((60, 100), 40), wv.BLACK, visible=False)
    doc.fill("lattice", wvd.GridGuideSpec(origin=(140.0, 40.0), cols=4, rows=3, dx=40.0, dy=40.0),
             wv.BLACK, visible=False)
    doc.fill("off", wvd.CircleSpec(
        wvd.AnchorPoint("g", pct=25.0, offset=(10.0, 0.0)), 5.0), _hex("E53935"))
    doc.fill("blend", wvd.CircleSpec(
        wvd.BetweenPoint((20.0, 20.0), (60.0, 20.0), 50.0, offset=(0.0, 30.0)), 4.0), _hex("1E88E5"))
    doc.fill("cell", wvd.CircleSpec(
        wvd.GridCellPoint("lattice", 2, 1, offset=(4.0, -4.0)), 3.0), _hex("43A047"))
    return doc


def build_v3_polar():
    doc = Document(200.0, 200.0)
    doc.fill("g", wv.Circle((100, 100), 40), wv.BLACK, visible=False)
    doc.fill("planet1", wvd.CircleSpec(
        wvd.PolarPoint((100.0, 100.0), 70.0, 30.0), 5.0), _hex("E53935"))
    doc.fill("planet2", wvd.CircleSpec(
        wvd.PolarPoint(wvd.AnchorPoint("g", pct=0.0), 55.0, 30.0), 8.0), _hex("1E88E5"))
    return doc


def build_v3_defs():
    doc = Document(200.0, 200.0)
    doc.define("tooth", wvd.PolySpec(closed=True, points=(
        (-6.0, -2.0), (6.0, -2.0), (9.0, -15.0), (-9.0, -15.0))))
    doc.fill("t1", wvd.UseSpec("tooth"), _hex("31465E"))
    doc.fill("t2", wvd.TransformSpec(
        (1.0, 0.0, 0.0, 1.0, 30.0, 0.0), wvd.UseSpec("tooth")), _hex("31465E"))
    return doc


def build_v3_markers():
    doc = Document(200.0, 200.0)
    doc.stroke("tie", wvd.PolySpec(closed=False, points=((40.0, 40.0), (40.0, 160.0))),
               _hex("333333"), 2.0, markers=[wvd.Marker("end", "triangle", 12.0)])
    doc.stroke("rail", wvd.PolySpec(closed=False, points=((80.0, 40.0), (160.0, 40.0))),
               _hex("1E88E5"), 2.0, markers=[wvd.Marker("both", "bar", 8.0)])
    doc.stroke("dim", wvd.PolySpec(closed=False, points=((180.0, 40.0), (180.0, 160.0))),
               _hex("E53935"), 1.5,
               markers=[wvd.Marker("both", "triangle", 10.0, _hex("43A047"))])

    return doc


# ---- v4_text.wvg: text nodes (tier B; SVG + ops JSON conformance only) ----

def build_v4_text():
    doc = Document(240.0, 120.0)
    doc.text("title", (80.0, 40.0), "Hi", 16.0, color=_hex("000000"))
    doc.text("mid", (200.0, 40.0), "Hi", 16.0, anchor="middle", color=_hex("E53935"))
    doc.text("right", (200.0, 80.0), "Hi", 16.0, anchor="end")
    doc.text("quote", (80.0, 100.0), 'say "hi" \\ ok', 12.0)
    doc.text("ghost", (120.0, 100.0), "boo", 12.0, visible=False)
    return doc


BUILDERS = {
    "smoke": build_smoke,
    "anchors": build_anchors,
    "ellipse_grad": build_ellipse_grad,
    "manygon": build_manygon,
    "path_hole": build_path_hole,
    "v2_transform": build_v2_transform,
    "v2_group": build_v2_group,
    "v2_rect_pie": build_v2_rect_pie,
    "v2_between": build_v2_between,
    "v3_offset": build_v3_offset,
    "v3_polar": build_v3_polar,
    "v3_defs": build_v3_defs,
    "v3_markers": build_v3_markers,
    "v4_text": build_v4_text,
}
