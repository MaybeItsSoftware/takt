#!/usr/bin/env bash
# Build the Android App Bundle to upload to the Play Console.
#
#   scripts/build_play_bundle.sh [versionName]
#
# Signs with the upload key named in mobile/android/keystore.properties (never
# committed; see mobile/android/README.md). Play re-signs for distribution
# with its own app signing key, so this key only proves an upload came from
# you — but losing it means asking Google to reset it, so back it up.
#
# versionCode must rise with every upload. It is the number of commits on
# HEAD, which only ever grows on main, so nobody has to remember to bump it.
# Takt Sync's purchase needs RevenueCat's public Play key (`goog_…`): the
# Gradle property taktRevenueCatKey, TAKT_REVENUECAT_ANDROID_KEY,
# `taktRevenueCatKey=` in local.properties, or that variable's line in
# ~/.config/takt/revenuecat.env. Without it the bundle can't sell Takt Sync.
#
# TAKT_VERSION_CODE overrides that (Tramline assigns its own build numbers).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
android="$root/mobile/android"
version_name="${1:-$(grep -m1 'versionName' "$android/app/build.gradle.kts" | sed -E 's/.*"(.*)".*/\1/')}"
version_code="${TAKT_VERSION_CODE:-$(git -C "$root" rev-list --count HEAD)}"

if [[ ! -f "$android/keystore.properties" ]]; then
  echo "error: mobile/android/keystore.properties is missing, so the bundle would be signed with the debug key, which Play rejects." >&2
  exit 1
fi

if [[ -z "${TAKT_REVENUECAT_ANDROID_KEY:-}" && -f "$HOME/.config/takt/revenuecat.env" ]]; then
  TAKT_REVENUECAT_ANDROID_KEY="$(sed -n 's/^TAKT_REVENUECAT_ANDROID_KEY=//p' "$HOME/.config/takt/revenuecat.env" | head -n1)"
  export TAKT_REVENUECAT_ANDROID_KEY
fi
if [[ -z "${TAKT_REVENUECAT_ANDROID_KEY:-}" ]] && ! grep -qE '^taktRevenueCatKey=.+' "$android/local.properties" 2>/dev/null; then
  echo "warning: no RevenueCat Play key, so this bundle can't sell Takt Sync." >&2
fi

echo "Building Takt $version_name ($version_code)"
cd "$android"
./gradlew --no-daemon -q \
  -PversionCode="$version_code" -PversionName="$version_name" \
  :core:test :data:testDebugUnitTest :app:testDebugUnitTest :app:lintRelease :app:bundleRelease

bundle="$android/app/build/outputs/bundle/release/app-release.aab"
out_dir="$root/build/play"
mkdir -p "$out_dir"
out="$out_dir/takt-$version_name-$version_code.aab"
cp "$bundle" "$out"

# A debug-signed bundle is the one mistake Play reports only after the upload.
if jarsigner -verify -verbose -certs "$out" 2>/dev/null | grep -q 'CN=Android Debug'; then
  echo "error: $out is signed with the debug key." >&2
  exit 1
fi
jarsigner -verify "$out" >/dev/null

echo "Signed bundle: $out ($(du -h "$out" | cut -f1))"
