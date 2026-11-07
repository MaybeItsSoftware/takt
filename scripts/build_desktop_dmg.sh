#!/usr/bin/env bash
set -euo pipefail

# A DMG of the Rust desktop app (desktop/), the way scripts/build_dmg.sh
# makes one of the Swift app: signed with a Developer ID when one is
# installed, then notarised and stapled when credentials are found, else
# signed for this Mac only and said so. The bundle itself is
# scripts/install_desktop.sh's, built with TAKT_BUNDLE_ONLY=1.
#
#   ./scripts/build_desktop_dmg.sh [version]

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Takt Desktop"
VOL_NAME="Takt Desktop"
VERSION="${1:-}"
if [[ -n "$VERSION" ]]; then
  DMG_BASENAME="takt-desktop-v${VERSION}"
else
  DMG_BASENAME="takt-desktop-$(date +%Y%m%d-%H%M%S)"
fi

SIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
  | sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1)"
NOTARY_PROFILE="${NOTARY_PROFILE:-priority-notary}"
DISTRIBUTABLE=0
if [[ -n "$SIGN_IDENTITY" ]]; then
  DISTRIBUTABLE=1
  echo "Signing with: $SIGN_IDENTITY"
  export TAKT_SIGN_IDENTITY="$SIGN_IDENTITY" TAKT_SIGN_TIMESTAMP=1
else
  echo "WARNING: no 'Developer ID Application' certificate found." >&2
  echo "         The DMG will be signed for local use only and Gatekeeper" >&2
  echo "         will block it on any other Mac. See docs/signing.md." >&2
fi

TAKT_BUNDLE_ONLY=1 "$ROOT_DIR/scripts/install_desktop.sh"

APP_PATH="$ROOT_DIR/build/desktop/$APP_NAME.app"
if [[ ! -x "$APP_PATH/Contents/Helpers/takt" ]]; then
  echo "The bundled MCP server is missing: $APP_PATH/Contents/Helpers/takt" >&2
  exit 1
fi

BUILD_DIR="$ROOT_DIR/build"
STAGE_DIR="$BUILD_DIR/desktop-dmg-stage"
FINAL_DMG="$BUILD_DIR/${DMG_BASENAME}.dmg"
rm -rf "$STAGE_DIR" "$FINAL_DMG"
mkdir -p "$STAGE_DIR"
ditto "$APP_PATH" "$STAGE_DIR/$APP_NAME.app"
ln -s /Applications "$STAGE_DIR/Applications"

hdiutil create -fs HFS+ -volname "$VOL_NAME" -srcfolder "$STAGE_DIR" \
  -format UDZO -imagekey zlib-level=9 "$FINAL_DMG" >/dev/null
rm -rf "$STAGE_DIR"

if [[ $DISTRIBUTABLE -eq 1 ]]; then
  codesign --force --sign "$SIGN_IDENTITY" --timestamp "$FINAL_DMG"
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
    echo "         The DMG is signed but NOT notarized. See docs/signing.md." >&2
  fi
fi

echo "Created DMG: $FINAL_DMG"
