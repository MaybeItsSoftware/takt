# Takt's logo

A metronome: *Takt* is the beat. A solid grape body with its face cut open,
the azure pendulum caught mid-swing against a dim scale. Ink `#444054`,
accent `#007fff`, scale `#b6b3bf`, paper `#faf8f4` — the house palette.

| File | Use |
| --- | --- |
| `takt-mark.svg` | The mark alone, on nothing. Store art, the web, anything that brings its own ground |
| `takt-icon.svg` | Full bleed on chalk, for the platforms that cut their own shape: iPhone, watch, Play's 512 |
| `takt-icon-mac.svg` | The Mac's, which draws its own tile — the 824 grid square, a hairline instead of a shadow. Since 2026-11-09 it is drawn after Zed, as the desktop app is: a dark grape tile (`#1c1a23`), the body as one even white monoline, and the azure pendulum breaking through its top edge, the frame cut back around it. The phones keep the solid mark for now |

`scripts/build_icons.sh` writes every PNG app icon from these. Android's and
Wear OS's launchers are vector drawables (`res/drawable/ic_launcher_*.xml`)
holding the same shapes, scaled into the adaptive icon's safe circle; change
them alongside the SVGs. The window is a hole in the body rather than a chalk
fill, so Android's themed monochrome icon still shows the pendulum.

`concepts/` and `refine/` are the directions that were drawn on the way.
