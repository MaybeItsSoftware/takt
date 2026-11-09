#!/usr/bin/env python3
"""Draws Takt's mark into every master that holds it.

The mark is a T standing in a run of open arches, each a step dimmer as it
goes in: the bar of the T is the outer arch's top, and its stem cuts the
inner ones in two. Every stroke is a rectangle with slightly rounded
corners, so the SVGs and Android's vector drawables come from the one list.

    python3 design/logo/draw_mark.py && scripts/build_icons.sh
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOGO = ROOT / "design" / "logo"

TILE = "#1c1a23"  # the dark grape the desktop app is drawn in
TILE_EDGE = "#34313f"
RADIUS = 8  # the corners' rounding, in the 1024 units below

# Light on the dark tile: the outer arch and the stem in ink, the inner
# arches stepping down toward the tile.
ON_DARK = ["#f5f4f7", "#b6b3bf", "#8a8698", "#6e6b7c"]
# Dark on paper, for the mark used on its own: stepping up toward the paper.
ON_LIGHT = ["#444054", "#6e6b7c", "#8a8698", "#b6b3bf"]

W = 34  # stroke
GAP = 43  # half the opening either side of the stem's centre


def rects():
    """(x, y, w, h, tone) in a 1024 square, tone 0 brightest."""
    h = W // 2
    out = []
    # The outer arch, whole, its top the bar of the T.
    out += [
        (210 - h, 230 - h, 604 + W, W, 0),
        (210 - h, 230 - h, W, 330 + h, 0),
        (814 - h, 230 - h, W, 330 + h, 0),
    ]
    # The inner arches, each opened where the stem passes.
    for tone, (left, top) in enumerate([(286, 306), (362, 382), (438, 458)], start=1):
        right = 1024 - left
        out += [
            (left - h, top - h, (512 - GAP) - (left - h), W, tone),
            (left - h, top - h, W, 800 - (top - h), tone),
            (512 + GAP, top - h, (right + h) - (512 + GAP), W, tone),
            (right - h, top - h, W, 800 - (top - h), tone),
        ]
    # The stem, from the bar down past the arches' feet.
    out.append((512 - h, 230 - h, W, 814 - (230 - h), 0))
    return out


CENTRE = (512, 515)  # the middle of the drawing above


def svg_rects(colours, scale, centre=(512, 512)):
    cx, cy = CENTRE
    tx, ty = centre
    parts = []
    for x, y, w, hgt, tone in rects():
        parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{hgt}" rx="{RADIUS}" fill="{colours[tone]}"/>'
        )
    body = "\n    ".join(parts)
    return (
        f'<g transform="translate({tx} {ty}) scale({scale}) translate({-cx} {-cy})">\n'
        f"    {body}\n  </g>"
    )


def rounded_rect_path(x, y, w, h, r):
    return (
        f"M{x + r},{y}h{w - 2 * r}a{r},{r} 0 0 1 {r},{r}v{h - 2 * r}"
        f"a{r},{r} 0 0 1 -{r},{r}h-{w - 2 * r}a{r},{r} 0 0 1 -{r},-{r}"
        f"v-{h - 2 * r}a{r},{r} 0 0 1 {r},-{r}z"
    )


def vector_foreground(scale_dp):
    """Android's adaptive foreground: 108dp, the mark inside the 66dp circle."""
    cx, cy = CENTRE
    paths = []
    for tone, colour in enumerate(ON_DARK):
        data = " ".join(
            rounded_rect_path(x, y, w, h, RADIUS) for x, y, w, h, t in rects() if t == tone
        )
        argb = "#FF" + colour[1:].upper()
        paths.append(
            f'        <path android:fillColor="{argb}"\n            android:pathData="{data}" />'
        )
    tx = 54 - cx * scale_dp
    ty = 54 - cy * scale_dp
    return f"""<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp" android:height="108dp" android:viewportWidth="108" android:viewportHeight="108">
    <!-- Written by design/logo/draw_mark.py: Takt's mark, its 1024 units
         scaled into the 66dp safe circle. One colour per step, so the themed
         monochrome icon reads as the same shape. -->
    <group android:scaleX="{scale_dp}" android:scaleY="{scale_dp}" android:translateX="{tx:.3f}" android:translateY="{ty:.3f}">
{chr(10).join(paths)}
    </group>
</vector>
"""


VECTOR_BACKGROUND = f"""<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp" android:height="108dp" android:viewportWidth="108" android:viewportHeight="108">
    <path android:fillColor="#FF{TILE[1:].upper()}" android:pathData="M0,0h108v108H0z" />
</vector>
"""


def main():
    head = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
    (LOGO / "takt-icon-mac.svg").write_text(
        f"""{head}
  <!-- The Mac's: its own tile on Apple's 824 grid, a hairline for an edge.
       Written by draw_mark.py. -->
  <rect x="100" y="100" width="824" height="824" rx="185" fill="{TILE}" stroke="{TILE_EDGE}" stroke-width="6"/>
  {svg_rects(ON_DARK, 0.78)}
</svg>
"""
    )
    (LOGO / "takt-icon.svg").write_text(
        f"""{head}
  <!-- Full bleed, for the platforms that cut their own shape (iOS, watchOS,
       Play). Written by draw_mark.py. -->
  <rect width="1024" height="1024" fill="{TILE}"/>
  {svg_rects(ON_DARK, 0.84)}
</svg>
"""
    )
    (LOGO / "takt-mark.svg").write_text(
        f"""{head}
  <!-- Takt's mark on nothing, dark for a light ground. Written by
       draw_mark.py; see README.md. -->
  {svg_rects(ON_LIGHT, 1.0)}
</svg>
"""
    )
    res = ROOT / "mobile" / "android"
    for module in ("app", "wear"):
        drawable = res / module / "src" / "main" / "res" / "drawable"
        (drawable / "ic_launcher_foreground.xml").write_text(vector_foreground(0.068))
        (drawable / "ic_launcher_background.xml").write_text(VECTOR_BACKGROUND)


if __name__ == "__main__":
    main()
