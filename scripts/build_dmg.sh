#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# The Swift package resolves only once the Rust core's xcframework exists
# (Package.swift `takt_coreFFI`); this rebuilds it when core/ changed.
TAKT_CORE_IF_STALE=1 "$ROOT_DIR/scripts/build_core_apple.sh"
PROJECT_DIR="$ROOT_DIR"
XCODEPROJ="$PROJECT_DIR/Takt.xcodeproj"
SCHEME="Takt"
APP_NAME="Takt.app"
VOL_NAME="Takt"

VERSION="${1:-}"
if [[ -n "$VERSION" ]]; then
  DMG_BASENAME="takt-v${VERSION}"
else
  DMG_BASENAME="takt-$(date +%Y%m%d-%H%M%S)"
fi

# Distributing outside the Mac App Store needs a "Developer ID Application"
# certificate; nothing else lets the DMG open on someone else's Mac. If one
# isn't installed we still build, but the result is only good locally, so say
# so loudly rather than handing over a DMG that fails on first launch.
SIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
  | sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1)"
NOTARY_PROFILE="${NOTARY_PROFILE:-priority-notary}"
DISTRIBUTABLE=0

if [[ -n "$SIGN_IDENTITY" ]]; then
  DISTRIBUTABLE=1
  echo "Signing with: $SIGN_IDENTITY"
else
  echo "WARNING: no 'Developer ID Application' certificate found." >&2
  echo "         The DMG will be signed for local use only and Gatekeeper" >&2
  echo "         will block it on any other Mac. See docs/signing.md." >&2
fi

BUILD_DIR="$PROJECT_DIR/build"
DERIVED_DIR="/tmp/takt-derived-release"
STAGE_DIR="$BUILD_DIR/dmg-stage"
RW_DMG="$BUILD_DIR/${DMG_BASENAME}-rw.dmg"
FINAL_DMG="$BUILD_DIR/${DMG_BASENAME}.dmg"

rm -rf "$DERIVED_DIR" "$STAGE_DIR" "$RW_DMG" "$FINAL_DMG"
mkdir -p "$BUILD_DIR" "$STAGE_DIR"

# The version the DMG is named after is also the version the bundle reports,
# the way scripts/build_app_store.sh does it for the phone: MARKETING_VERSION
# from the argument (when given) and CURRENT_PROJECT_VERSION from the commit
# count, so two DMGs built from different commits never share a build number.
BUILD_NUMBER="$(git -C "$ROOT_DIR" rev-list --count HEAD)"
XCODEBUILD_ARGS=(
  -project "$XCODEPROJ"
  -scheme "$SCHEME"
  -configuration Release
  -derivedDataPath "$DERIVED_DIR"
  -quiet
  "CURRENT_PROJECT_VERSION=$BUILD_NUMBER"
)
if [[ -n "$VERSION" ]]; then
  XCODEBUILD_ARGS+=("MARKETING_VERSION=$VERSION")
fi
if [[ $DISTRIBUTABLE -eq 1 ]]; then
  # The team too: a forced identity also reaches the Swift packages' resource
  # bundles (swift-crypto's), which have no team of their own and are signed
  # automatically unless told otherwise; both fail the build.
  XCODEBUILD_ARGS+=("CODE_SIGN_IDENTITY=$SIGN_IDENTITY" "DEVELOPMENT_TEAM=6NQNU5YSC2" "CODE_SIGN_STYLE=Manual")
  # Notarization refuses a binary with the debugger entitlement Xcode adds
  # (get-task-allow) or without a secure timestamp.
  XCODEBUILD_ARGS+=("CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO" "OTHER_CODE_SIGN_FLAGS=--timestamp")
fi

xcodebuild "${XCODEBUILD_ARGS[@]}" build

APP_PATH="$DERIVED_DIR/Build/Products/Release/$APP_NAME"
if [[ ! -d "$APP_PATH" ]]; then
  echo "Build succeeded but app bundle not found: $APP_PATH" >&2
  exit 1
fi

# The MCP server is the bundled `takt` CLI (scripts/bundle_cli.sh, run from
# a build phase). Shipping without it would produce a DMG whose --mcp-server
# does nothing, breaking every MCP client configuration on the user's machine —
# worth failing the release for rather than discovering later.
if [[ ! -x "$APP_PATH/Contents/Helpers/takt" ]]; then
  echo "Build succeeded but the bundled MCP server is missing:" >&2
  echo "  $APP_PATH/Contents/Helpers/takt" >&2
  echo "Was TAKT_SKIP_CLI_BUNDLE=1 set?" >&2
  exit 1
fi

cp -R "$APP_PATH" "$STAGE_DIR/"
ln -s /Applications "$STAGE_DIR/Applications"

# No fixed -size: hdiutil sizes the image from -srcfolder, so a bigger app
# (the bundled CLI alone is tens of MB) can't overflow a hard-coded 64m.
hdiutil create \
  -fs HFS+ \
  -volname "$VOL_NAME" \
  -srcfolder "$STAGE_DIR" \
  -format UDRW \
  "$RW_DMG" >/dev/null

ATTACH_OUTPUT="$(hdiutil attach -readwrite -noverify -noautoopen "$RW_DMG")"
DEVICE="$(echo "$ATTACH_OUTPUT" | awk '/Apple_HFS/ {print $1; exit}')"
MOUNT_POINT="$(echo "$ATTACH_OUTPUT" | awk -F'\t' '/\/Volumes\// {print $NF; exit}')"

if [[ -z "$DEVICE" || -z "$MOUNT_POINT" ]]; then
  echo "Failed to attach DMG for customization" >&2
  exit 1
fi

APPLESCRIPT_APP_NAME="$APP_NAME"
APPLESCRIPT_VOL_NAME="$VOL_NAME"
osascript >/dev/null <<OSA
tell application "Finder"
  tell disk "$APPLESCRIPT_VOL_NAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {180, 180, 780, 520}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 96
    set text size of viewOptions to 12
    set position of item "Applications" of container window to {140, 160}
    set position of item "$APPLESCRIPT_APP_NAME" of container window to {420, 160}
    close
    open
    update without registering applications
    delay 1
  end tell
end tell
OSA

sync
hdiutil detach "$DEVICE" -quiet

hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -o "$FINAL_DMG" >/dev/null
rm -f "$RW_DMG"
rm -rf "$STAGE_DIR"

if [[ $DISTRIBUTABLE -eq 1 ]]; then
  codesign --force --sign "$SIGN_IDENTITY" --timestamp "$FINAL_DMG"

  # Notarization is what actually clears Gatekeeper; a Developer ID signature
  # on its own still trips "cannot be opened" on a machine that has never seen
  # the app. Stapling then lets it launch without a network round trip.
  # The keychain profile if there is one, else the App Store Connect API key
  # scripts/build_app_store.sh uses (~/.config/takt/asc.env), which notarytool
  # takes as well.
  notary=()
  if xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1; then
    notary=(--keychain-profile "$NOTARY_PROFILE")
  elif [[ -f "$HOME/.config/takt/asc.env" ]]; then
    set -a; source "$HOME/.config/takt/asc.env"; set +a
    key_id="${ASC_KEY_ID:-${APP_STORE_CONNECT_API_KEY_ID:-}}"
    issuer="${ASC_ISSUER_ID:-${APP_STORE_CONNECT_API_KEY_ISSUER_ID:-}}"
    content="${ASC_KEY_CONTENT:-${APP_STORE_CONNECT_API_KEY_CONTENT:-}}"
    if [[ -n "$key_id" && -n "$issuer" && -n "$content" ]]; then
      key_dir="$(mktemp -d)"; chmod 700 "$key_dir"
      trap 'rm -rf "$key_dir"' EXIT
      key_path="$key_dir/AuthKey_$key_id.p8"
      if [[ "$content" == *"BEGIN PRIVATE KEY"* ]]; then
        printf '%b\n' "$content" > "$key_path"
      else
        printf '%s' "$content" | base64 -D > "$key_path"
      fi
      notary=(--key "$key_path" --key-id "$key_id" --issuer "$issuer")
    fi
  fi

  if [[ ${#notary[@]} -gt 0 ]]; then
    echo "Submitting to Apple for notarization (this usually takes a few minutes)..."
    xcrun notarytool submit "$FINAL_DMG" "${notary[@]}" --wait
    xcrun stapler staple "$FINAL_DMG"
    xcrun stapler validate "$FINAL_DMG"
    echo "Notarized and stapled."
  else
    echo "WARNING: notarization credentials '$NOTARY_PROFILE' not found." >&2
    echo "         The DMG is signed but NOT notarized, so Gatekeeper will" >&2
    echo "         still block it elsewhere. See docs/signing.md." >&2
  fi
fi

echo "Created DMG: $FINAL_DMG"
