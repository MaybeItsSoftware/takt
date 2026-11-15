#!/usr/bin/env bash
# Retakes the App Store's raw screenshots on the simulators, from the demo
# workspace, into design/store/raw/{iphone,ipad}/. Then
# scripts/build_store_assets.sh frames them.
#
#   ./scripts/capture_store_ios.sh            # both devices
#   TAKT_SHOTS_DEVICES=iphone ./scripts/capture_store_ios.sh
#
# A Debug build, for its screenshot hooks (TaktApp.swift `handleLaunchArguments`
# and the `takt://` debug routes): `-demo` seeds the workspace, `-open` follows
# a link, `-listViewMode` and `-boardColumn` pick the board, `-quickAddText`
# types a capture, `takt://focus?task=` starts a block. Each device is a
# simulator of its own, erased first, with the status bar at 9:41 and full.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IOS="$ROOT/mobile/ios"
DERIVED="$IOS/build/store-shots"
BUNDLE_ID="uk.co.maybeitssoftware.takt"
RAW="$ROOT/design/store/raw"
devices="${TAKT_SHOTS_DEVICES:-iphone ipad}"

TAKT_CORE_IF_STALE=1 "$ROOT/scripts/build_core_apple.sh"
(cd "$IOS" && xcodegen --quiet)
log="$(mktemp)"
if ! xcodebuild -project "$IOS/TaktMobile.xcodeproj" -scheme TaktMobile -configuration Debug \
    -destination 'generic/platform=iOS Simulator' -derivedDataPath "$DERIVED" build >"$log" 2>&1; then
  grep -E "error:" "$log" | sort -u >&2 || true
  echo "** BUILD FAILED ** (full log: $log)" >&2
  exit 1
fi
APP="$DERIVED/Build/Products/Debug-iphonesimulator/Takt.app"

# simulator <name> <device type>: the udid of a simulator by that name, made
# if it doesn't exist.
simulator() {
  local udid
  udid=$(xcrun simctl list devices -j | python3 -c "import json,sys; n=sys.argv[1]; print(next((d['udid'] for r in json.load(sys.stdin)['devices'].values() for d in r if d['name']==n), ''))" "$1")
  [[ -n "$udid" ]] || udid=$(xcrun simctl create "$1" "$2")
  echo "$udid"
}

# shot <udid> <out> <args…>: launch fresh with the args, wait, capture.
shot() {
  local udid=$1 out=$2; shift 2
  xcrun simctl terminate "$udid" "$BUNDLE_ID" >/dev/null 2>&1 || true
  xcrun simctl launch "$udid" "$BUNDLE_ID" -demo "$@" >/dev/null
  sleep "${TAKT_SHOTS_WAIT:-$wait}"
  xcrun simctl io "$udid" screenshot --type=png "$out" >/dev/null
  echo "$out"
}

for device in $devices; do
  case "$device" in
    iphone) name="Takt Store iPhone" type="com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max" wait=5 ;;
    # The iPad is slower to draw a launch; at 5s a shot can catch the launch screen.
    ipad) name="Takt Store iPad" type="com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M5-12GB" wait=10 ;;
    *) echo "unknown device $device" >&2; exit 1 ;;
  esac
  udid=$(simulator "$name" "$type")
  xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true
  xcrun simctl erase "$udid"
  xcrun simctl boot "$udid"
  xcrun simctl bootstatus "$udid" >/dev/null
  xcrun simctl ui "$udid" appearance light
  # Otherwise the keyboard's swipe-typing tutorial covers the capture shot;
  # the keyboard reads this only at boot, hence the reboot.
  xcrun simctl spawn "$udid" defaults write com.apple.keyboard.preferences DidShowContinuousPathIntroduction -bool true
  xcrun simctl spawn "$udid" defaults write com.apple.Preferences DidShowContinuousPathIntroduction -bool true
  xcrun simctl spawn "$udid" defaults write com.apple.Preferences KeyboardContinuousPathEnabled -bool false
  xcrun simctl shutdown "$udid"
  xcrun simctl boot "$udid"
  xcrun simctl bootstatus "$udid" >/dev/null
  xcrun simctl status_bar "$udid" override --time "09:41" --dataNetwork wifi --wifiMode active \
    --wifiBars 3 --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100
  xcrun simctl install "$udid" "$APP"
  # The first launch seeds; give it time before the shots.
  xcrun simctl launch "$udid" "$BUNDLE_ID" -demo >/dev/null
  sleep 6

  out="$RAW/$device"
  mkdir -p "$out"
  inspect=()
  [[ "$device" == ipad ]] && inspect=(-inspect "Polish the outline rows")
  # The iPad's inspector is open rather than left empty, but not over the
  # board, which then draws blank.
  shot "$udid" "$out/01-today.png" -open "takt://today" ${inspect[@]+"${inspect[@]}"}
  shot "$udid" "$out/03-outline.png" -open "takt://lists/Work" -listViewMode outline ${inspect[@]+"${inspect[@]}"}
  shot "$udid" "$out/04-board.png" -open "takt://lists/Work" -listViewMode board -boardColumn in-progress
  shot "$udid" "$out/05-capture.png" -open "takt://add" -quickAddText "Write the release notes 45m #work @fri !1"
  # Last: a running block shows on the other screens too.
  shot "$udid" "$out/02-focus.png" -open "takt://focus?task=Polish%20the%20outline%20rows" ${inspect[@]+"${inspect[@]}"}
  xcrun simctl shutdown "$udid"
done
