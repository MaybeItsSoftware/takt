#!/usr/bin/env bash
# Regenerates every app icon from the logo masters in design/logo.
#   takt-icon-mac.svg  → the Mac's AppIcon (it draws its own tile)
#   takt-icon.svg      → the iPhone's and the watch's, full bleed
#   takt-mark.svg      → design/logo/takt-mark-512.png; Android's launchers are
#                        vector drawables written by hand from the same shapes
# Needs rsvg-convert and magick (`brew install librsvg imagemagick`).
set -euo pipefail
cd "$(dirname "$0")/.."

logo=design/logo
render() { # svg size out — opaque, since App Store icons may not carry alpha
  rsvg-convert -w "$2" -h "$2" "$1" | magick - -background "#faf8f4" -alpha remove -alpha off "$3"
}
render_clear() { rsvg-convert -w "$2" -h "$2" "$1" -o "$3"; }

mac=desktop/icon
for size in 16 32 128 256 512; do
  render_clear "$logo/takt-icon-mac.svg" "$size" "$mac/mac-$size.png"
  render_clear "$logo/takt-icon-mac.svg" "$((size * 2))" "$mac/mac-$size@2x.png"
done
render "$logo/takt-icon.svg" 1024 mobile/ios/TaktMobile/Resources/Assets.xcassets/AppIcon.appiconset/icon-1024.png
render "$logo/takt-icon.svg" 1024 mobile/ios/TaktWatch/Assets.xcassets/AppIcon.appiconset/icon-1024.png
render "$logo/takt-icon.svg" 512 "$logo/takt-icon-512.png"
render_clear "$logo/takt-mark.svg" 512 "$logo/takt-mark-512.png"
echo "Icons written."
