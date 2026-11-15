#!/usr/bin/env bash
# Retakes Google Play's raw screenshots on the connected emulator, from the
# demo workspace, into design/store/raw/android/. Then
# scripts/build_store_assets.sh frames them.
#
#   ~/Library/Android/sdk/emulator/emulator -avd takt-pixel &   # or any phone
#   ./scripts/capture_store_android.sh
#
# A debug build, for `--ez demo true` (DemoSeed.kt), installed over whatever
# is there and cleared first. The app has no other screenshot hooks, so the
# script finds controls by their text in a uiautomator dump and taps them.
# That input goes to the emulator alone, never to the Mac's frontmost window.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/design/store/raw/android"
PKG="uk.co.maybeitssoftware.takt"
ACTIVITY="$PKG/uk.co.maybeitsadam.takt.MainActivity"
TAB_EXTRA="uk.co.maybeitsadam.priority.TAB"
wait="${TAKT_SHOTS_WAIT:-4}"

adb get-state >/dev/null 2>&1 || { echo "no device on adb: start an emulator first" >&2; exit 1; }
(cd "$ROOT/mobile/android" && ./gradlew -q :app:assembleDebug)
adb install -r "$ROOT/mobile/android/app/build/outputs/apk/debug/app-debug.apk" >/dev/null

# The status bar in demo mode: 9:41, full wifi and battery, nothing else.
adb shell settings put global sysui_demo_allowed 1
demo() { adb shell am broadcast -a com.android.systemui.demo -e command "$@" >/dev/null; }
demo enter
demo clock -e hhmm 0941
demo battery -e level 100 -e plugged false
demo network -e wifi show -e level 4 -e fully true
demo network -e mobile hide
demo notifications -e visible false

dump=$(mktemp)
# tap <text>: tap the centre of the first element whose text or description
# is exactly <text> (or starts with it, for `<text>*`), after waiting for it
# to appear.
tap() {
  local target=$1 bounds="" _
  for _ in 1 2 3 4 5; do
    adb shell uiautomator dump /sdcard/takt-ui.xml >/dev/null 2>&1 || true
    adb exec-out cat /sdcard/takt-ui.xml >"$dump"
    bounds=$(python3 -I - "$dump" "$target" <<'EOF'
import re, sys
xml, target = open(sys.argv[1]).read(), sys.argv[2]
for node in re.finditer(r'<node [^>]*>', xml):
    attrs = dict(re.findall(r'([\w-]+)="([^"]*)"', node.group(0)))
    labels = [attrs.get("text") or "", attrs.get("content-desc") or ""]
    if any(l == target or (target.endswith("*") and l.startswith(target[:-1])) for l in labels if l):
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", attrs["bounds"]))
        print((x1 + x2) // 2, (y1 + y2) // 2)
        break
EOF
)
    [[ -n "$bounds" ]] && break
    sleep 1
  done
  [[ -n "$bounds" ]] || { echo "can't find \"$target\" on screen" >&2; exit 1; }
  # shellcheck disable=SC2086 # two coordinates
  adb shell input tap $bounds
  sleep 1
}
shot() {
  sleep "$wait"
  adb exec-out screencap -p >"$OUT/$1"
  echo "$OUT/$1"
}
launch() {
  adb shell am force-stop "$PKG"
  adb shell am start -n "$ACTIVITY" "$@" >/dev/null
  sleep "$wait"
}

mkdir -p "$OUT"
adb shell pm clear "$PKG" >/dev/null
# Granted up front, so the permission prompt doesn't sit over the first shot.
adb shell pm grant "$PKG" android.permission.POST_NOTIFICATIONS
launch --ez demo true
sleep 4 # the seed writes after the first frame

launch --es "$TAB_EXTRA" TODAY
shot 01-today.png

tap "Lists" # the navigation drawer
tap "Work"
shot 03-outline.png

tap "Outline" # one toggle, cycling outline → board → matrix
tap "In progress*" # the column chip, labelled with its count
shot 04-board.png

launch --es "$TAB_EXTRA" TODAY
tap "Add a task"
# `input text` takes %s for a space; the shell would eat # and ! otherwise.
adb shell input text "'Write%sthe%srelease%snotes%s45m%s#work%s@fri%s!1'"
shot 05-capture.png

# Last, as a running block shows on the other screens: start one, and let it
# run long enough that the timer has visibly moved.
launch --es "$TAB_EXTRA" TODAY
tap "Start focus on Fix the widget refresh"
sleep "${TAKT_SHOTS_FOCUS_RUN:-75}"
launch --es "$TAB_EXTRA" FOCUS
shot 02-focus.png

demo exit
