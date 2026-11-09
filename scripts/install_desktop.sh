#!/usr/bin/env bash
set -euo pipefail

# Builds the Rust desktop app (desktop/, takt-desktop) as a .app bundle and
# installs it as /Applications/Takt.app, then launches it. It keeps the
# retired Swift app's name and bundle ID (so notification and login-item grants
# carry over), and the executable is still Contents/MacOS/Takt, so an MCP
# client configured with `Takt --mcp-server` keeps working. Whatever is at
# that path, the interim "Takt Desktop.app" and an old Priority.app are quit
# and removed, so two apps never share one workspace.
#
# The bundle holds the executable, the `takt` CLI as a
# signed helper at Contents/Helpers/takt (its MCP server: `--mcp-server`
# execs it), and the app icon. It is signed with the first valid Apple
# Development identity, or ad hoc when there is none; set
# TAKT_SIGN_IDENTITY to choose. Like install_local.sh it supersedes the
# installed copy outright and touches /Applications only after a good build.
#
# TAKT_BUNDLE_ONLY=1 stops once build/desktop/<name>.app is signed, for
# scripts/build_desktop_dmg.sh; TAKT_SIGN_TIMESTAMP=1 asks Apple for a
# secure timestamp, which notarisation needs.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Takt"
BUNDLE_ID="uk.co.maybeitssoftware.takt"
BINARY="takt-desktop"
EXECUTABLE="Takt"
# Earlier installs this one replaces.
SUPERSEDED=("/Applications/Takt Desktop.app" "/Applications/Priority.app")
INSTALL_PATH="/Applications/$APP_NAME.app"
STAGE_DIR="$ROOT_DIR/build/desktop"
APP_PATH="$STAGE_DIR/$APP_NAME.app"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/desktop/Cargo.toml" | head -1)"
BUILD_NUMBER="$(git -C "$ROOT_DIR" rev-list --count HEAD 2>/dev/null || echo 1)"

echo "==> Building takt-desktop and the takt CLI (release)…"
# Takt's own Google Desktop OAuth client, so people connect Google without
# a Cloud project of their own. Kept out of git (the repo is public):
# TAKT_GOOGLE_CLIENT_ID and TAKT_GOOGLE_CLIENT_SECRET, compiled in.
if [[ -f "$HOME/.config/takt/google-desktop.env" ]]; then
  set -a; source "$HOME/.config/takt/google-desktop.env"; set +a
fi
if [[ -z "${TAKT_GOOGLE_CLIENT_ID:-}" ]]; then
  echo "note: no TAKT_GOOGLE_CLIENT_ID; Google needs a client ID entered in Settings" >&2
fi
cargo build --release --manifest-path "$ROOT_DIR/desktop/Cargo.toml"
cargo build --release --manifest-path "$ROOT_DIR/cli/Cargo.toml"

echo "==> Assembling $APP_NAME.app ($VERSION, $BUILD_NUMBER)…"
rm -rf "$APP_PATH"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Helpers" "$APP_PATH/Contents/Resources"
cp "$ROOT_DIR/desktop/target/release/$BINARY" "$APP_PATH/Contents/MacOS/$EXECUTABLE"
cp "$ROOT_DIR/cli/target/release/takt" "$APP_PATH/Contents/Helpers/takt"

# The icon (desktop/icon, written by scripts/build_icons.sh).
ICONSET="$STAGE_DIR/AppIcon.iconset"
SOURCE="$ROOT_DIR/desktop/icon"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  cp "$SOURCE/mac-$size.png" "$ICONSET/icon_${size}x${size}.png"
  cp "$SOURCE/mac-$size@2x.png" "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$APP_PATH/Contents/Resources/AppIcon.icns"

cat > "$APP_PATH/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>$APP_NAME</string>
	<key>CFBundleExecutable</key>
	<string>$EXECUTABLE</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$APP_NAME</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$BUILD_NUMBER</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.productivity</string>
	<key>LSMinimumSystemVersion</key>
	<string>15.6</string>
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>$BUNDLE_ID.auth</string>
			<key>CFBundleURLSchemes</key>
			<array><string>takt</string></array>
		</dict>
	</array>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
PLIST

IDENTITY="${TAKT_SIGN_IDENTITY:-}"
if [[ -z "$IDENTITY" ]]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | grep 'Apple Development' | grep -v REVOKED | head -1 | awk '{print $2}' || true)"
fi
[[ -z "$IDENTITY" ]] && IDENTITY="-"
echo "==> Signing with ${IDENTITY}…"
# Inside out: the helper before the bundle around it. No entitlements: like
# the Swift app before it, it isn't sandboxed, so there is nothing for one to grant.
TIMESTAMP="--timestamp=none"
[[ "${TAKT_SIGN_TIMESTAMP:-0}" == "1" ]] && TIMESTAMP="--timestamp"
codesign --force --sign "$IDENTITY" --options runtime "$TIMESTAMP" \
  "$APP_PATH/Contents/Helpers/takt"
codesign --force --sign "$IDENTITY" --options runtime "$TIMESTAMP" "$APP_PATH"
codesign --verify --strict "$APP_PATH"

if [[ "${TAKT_BUNDLE_ONLY:-0}" == "1" ]]; then
  echo "Bundled: $APP_PATH"
  exit 0
fi

echo "==> Quitting any running instance…"
for name in "$APP_NAME" "Takt Desktop" "Priority"; do
  osascript -e "tell application \"$name\" to quit" 2>/dev/null || true
done
sleep 1
pkill -x "$EXECUTABLE" 2>/dev/null || true
pkill -x "$BINARY" 2>/dev/null || true
pkill -x Priority 2>/dev/null || true

for old in "$INSTALL_PATH" "${SUPERSEDED[@]}"; do
  if [[ -d "$old" ]]; then
    echo "==> Removing $old"
    "$LSREGISTER" -u "$old" 2>/dev/null || true
    rm -rf "$old"
  fi
done

echo "==> Installing to $INSTALL_PATH"
ditto "$APP_PATH" "$INSTALL_PATH"
xattr -cr "$INSTALL_PATH" 2>/dev/null || true
"$LSREGISTER" -f "$INSTALL_PATH" 2>/dev/null || true

if [[ "${TAKT_NO_LAUNCH:-0}" != "1" ]]; then
  echo "==> Launching…"
  open "$INSTALL_PATH"
fi

echo
echo "Installed."
