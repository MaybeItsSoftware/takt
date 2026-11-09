# Takt's logo

A T standing in a run of open arches, after Zed's nested-line mark: the bar
of the T is the outer arch's top, and its stem cuts the inner arches in two,
each a step dimmer as it goes in. Drawn 2026-11-09, in place of the
metronome. Every stroke is a rectangle with slightly rounded corners (8 of
1024 units). On the dark grape tile (`#1c1a23`) the outer arch and stem are
ink `#f5f4f7`, stepping down through `#b6b3bf`, `#8a8698` and `#6e6b7c`; on
paper the steps run the other way, from `#444054`.

| File | Use |
| --- | --- |
| `draw_mark.py` | **The source.** Holds the rectangles and writes every file below, the SVGs and Android's and Wear OS's launcher drawables |
| `takt-mark.svg` | The mark alone, dark, for a light ground: store art, the web |
| `takt-icon.svg` | Full bleed on the grape tile, for the platforms that cut their own shape: iPhone, watch, Play's 512 |
| `takt-icon-mac.svg` | The Mac's, which draws its own tile — the 824 grid square, a hairline instead of a shadow — with the mark at 78% so it sits inside it |

To change it, edit `draw_mark.py`, then:

```bash
python3 design/logo/draw_mark.py && scripts/build_icons.sh
```

`build_icons.sh` writes every PNG app icon from the SVGs. Android's launchers
are vector drawables (`res/drawable/ic_launcher_*.xml`) the script writes
directly, the mark scaled into the adaptive icon's 66dp safe circle; the
themed monochrome icon uses the same foreground. `scripts/build_store_assets.sh`
redraws the Play feature graphic from `takt-mark.svg`.

`concepts/` and `refine/` are the directions that were drawn on the way.
