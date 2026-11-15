#!/usr/bin/env bash
# Frames the raw screenshots in design/store/raw/<device>/ into store images
# in design/store/<device>/, and draws Play's feature graphic.
#
#   design/store/raw/iphone/0N-*.png  → design/store/iphone/  1320×2868 (App Store 6.9")
#   design/store/raw/ipad/0N-*.png    → design/store/ipad/    2064×2752 (App Store 13")
#   design/store/raw/android/0N-*.png → design/store/android/ 1080×1920 (Play phone)
#   design/logo/takt-mark.svg         → design/store/play-feature.png 1024×500
#
# Each image is One Light paper (the default theme's), a headline in IBM Plex Sans above the screen, and
# the screen itself with a hairline, running off the foot of the frame. The
# headlines are below, keyed by the raw file's number. Needs magick and
# rsvg-convert.
set -euo pipefail
cd "$(dirname "$0")/.."

fonts=mobile/android/app/src/main/res/font
headline_font=$fonts/ibmplexsans_semibold.ttf
body_font=$fonts/ibmplexsans_regular.ttf
store=${TAKT_STORE_DIR:-design/store}
paper="#fafafa" ink="#242529" muted="#58585a" hairline="#dfdfe0"

headline() {
  case "$1" in
    01) echo "Your day, in the right order" ;;
    02) echo "Start a block. Keep the beat." ;;
    03) echo "Lists that fold like an outline" ;;
    04) echo "The same tasks, as a board" ;;
    05) echo "Type it the way you'd say it" ;;
  esac
}
subline() {
  case "$1" in
    01) echo "Planned, due and dailies, and when you'll finish." ;;
    02) echo "A focus timer on the task in hand, scored when it ends." ;;
    03) echo "Nest tasks as deep as you like, and fold them away." ;;
    04) echo "Drag cards between columns, or sort them by urgency." ;;
    05) echo "45m, @fri, #work and !1 fill in the details." ;;
  esac
}

# frame <raw> <out> <width> <height>
frame() {
  local raw=$1 out=$2 width=$3 height=$4
  local n; n=$(basename "$raw" | cut -c1-2)
  local margin=$((width * 9 / 100))
  local text_width=$((width - 2 * margin))
  local head_size=$((width * 56 / 1000)) sub_size=$((width * 30 / 1000))
  local top=$((height * 7 / 100))
  local shot_width=$((width * 80 / 100))
  local radius=$((shot_width * 6 / 100))
  local tmp; tmp=$(mktemp -d)

  magick -background none -fill "$ink" -font "$headline_font" -pointsize "$head_size" \
    -size "${text_width}x" -gravity center caption:"$(headline "$n")" "$tmp/head.png"
  magick -background none -fill "$muted" -font "$body_font" -pointsize "$sub_size" \
    -size "${text_width}x" -gravity center caption:"$(subline "$n")" "$tmp/sub.png"
  local head_h sub_h; head_h=$(magick identify -format %h "$tmp/head.png"); sub_h=$(magick identify -format %h "$tmp/sub.png")
  # Fixed, not from the text, so every frame in a set puts its screen at the
  # same height.
  local shot_top=$((top + head_size * 3 / 2 + sub_size * 3 + height * 4 / 100))

  # The screen with rounded corners and a hairline, as a device screen reads.
  magick "$raw" -resize "${shot_width}x" "$tmp/shot.png"
  local shot_h; shot_h=$(magick identify -format %h "$tmp/shot.png")
  magick -size "${shot_width}x${shot_h}" xc:none -fill white \
    -draw "roundrectangle 0,0,$((shot_width - 1)),$((shot_h - 1)),$radius,$radius" "$tmp/mask.png"
  magick "$tmp/shot.png" "$tmp/mask.png" -alpha off -compose CopyOpacity -composite "$tmp/shot.png"
  magick -size "$((shot_width + 6))x$((shot_h + 6))" xc:none -fill none -stroke "$hairline" -strokewidth 3 \
    -draw "roundrectangle 1,1,$((shot_width + 4)),$((shot_h + 4)),$((radius + 2)),$((radius + 2))" "$tmp/ring.png"

  magick -size "${width}x${height}" "xc:$paper" \
    "$tmp/head.png" -geometry "+${margin}+${top}" -composite \
    "$tmp/sub.png" -geometry "+${margin}+$((top + head_h + sub_size / 2))" -composite \
    "$tmp/ring.png" -geometry "+$(((width - shot_width) / 2 - 3))+$((shot_top - 3))" -composite \
    "$tmp/shot.png" -geometry "+$(((width - shot_width) / 2))+${shot_top}" -composite \
    -alpha off "$out"
  rm -rf "$tmp"
}

for device in iphone:1320:2868 ipad:2064:2752 android:1080:1920; do
  IFS=: read -r name width height <<<"$device"
  [ -d "$store/raw/$name" ] || continue
  mkdir -p "$store/$name"
  for raw in "$store/raw/$name"/0*.png; do
    [ -e "$raw" ] || continue
    frame "$raw" "$store/$name/$(basename "$raw")" "$width" "$height"
    echo "$store/$name/$(basename "$raw")"
  done
done

# Play's feature graphic: the mark, the name and the line, on faint graph
# paper — the one full-bleed backdrop the house style allows.
tmp=$(mktemp -d)
rsvg-convert -w 360 -h 360 design/logo/takt-mark.svg -o "$tmp/mark.png"
magick -size 1024x500 "xc:$paper" -fill none -stroke "#f1eff1" -strokewidth 2 \
  -draw "$(for x in $(seq 0 32 1024); do printf 'line %d,0 %d,500 ' "$x" "$x"; done; for y in $(seq 4 32 500); do printf 'line 0,%d 1024,%d ' "$y" "$y"; done)" \
  -stroke none -fill "$paper" -draw "polygon 223,167 277,167 308,314 192,314" \
  "$tmp/mark.png" -geometry +70+66 -composite \
  -stroke none -fill "$ink" -font "$headline_font" -pointsize 128 -annotate +430+262 "Takt" \
  -fill "$muted" -font "$body_font" -pointsize 40 -annotate +436+332 "Plan the day, then do it" \
  -alpha off "$store/play-feature.png"
rm -rf "$tmp"
echo "$store/play-feature.png"
