#!/usr/bin/env bash
set -euo pipefail

# Builds the Swift app (Release) and installs it as /Applications/Takt.app,
# superseding the Rust app that scripts/install_local.sh installs there. The
# way back to the Swift app, kept while the Rust one beds in.
#
# Unlike `build_dmg.sh` this skips the DMG, Finder scripting, and notarization —
# none of which matter for putting a build on the machine that produced it.
#
# The new build supersedes the installed app outright: nothing is kept. Git
# is the way back to an older build. /Applications is only touched *after* a
# successful build, so a compile failure can never leave you without a working
# app.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# The Swift package resolves only once the Rust core's xcframework exists
# (Package.swift `takt_coreFFI`); this rebuilds it when core/ changed.
TAKT_CORE_IF_STALE=1 "$ROOT_DIR/scripts/build_core_apple.sh"
XCODEPROJ="$ROOT_DIR/Takt.xcodeproj"
SCHEME="Takt"
APP_NAME="Takt.app"
PROCESS_NAME="Takt"
INSTALL_PATH="/Applications/$APP_NAME"
# The app was called Priority until it became Takt. An install of it is
# quit and removed too, so there are never two apps sharing one workspace in
# /Applications. Its data is untouched — Takt copied it on first launch.
LEGACY_INSTALL_PATH="/Applications/Priority.app"
LEGACY_PROCESS_NAME="Priority"

BUILD_DIR="$ROOT_DIR/build"
DERIVED_DIR="/tmp/takt-derived-install"
# Where earlier versions of this script kept backups; emptied on every run.
OLD_BACKUP_DIR="$BUILD_DIR/backup.noindex"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

echo "==> Building Release…"
rm -rf "$DERIVED_DIR"
mkdir -p "$BUILD_DIR"

xcodebuild \
  -project "$XCODEPROJ" \
  -scheme "$SCHEME" \
  -configuration Release \
  -derivedDataPath "$DERIVED_DIR" \
  -destination 'platform=macOS' \
  build

APP_PATH="$DERIVED_DIR/Build/Products/Release/$APP_NAME"
if [[ ! -d "$APP_PATH" ]]; then
  echo "Build reported success but no app bundle at: $APP_PATH" >&2
  exit 1
fi

echo "==> Quitting any running instance…"
for name in "$PROCESS_NAME" "$LEGACY_PROCESS_NAME"; do
  osascript -e "tell application \"$name\" to quit" 2>/dev/null || true
done
sleep 1
killall "$PROCESS_NAME" "$LEGACY_PROCESS_NAME" 2>/dev/null || true

for old in "$INSTALL_PATH" "$LEGACY_INSTALL_PATH"; do
  if [[ -d "$old" ]]; then
    echo "==> Removing $old"
    "$LSREGISTER" -u "$old" 2>/dev/null || true
    rm -rf "$old"
  fi
done
if [[ -d "$OLD_BACKUP_DIR" ]]; then
  "$LSREGISTER" -u "$OLD_BACKUP_DIR"/* 2>/dev/null || true
  rm -rf "$OLD_BACKUP_DIR"
fi

echo "==> Installing to $INSTALL_PATH"
ditto "$APP_PATH" "$INSTALL_PATH"

# The build is locally signed, so strip any quarantine attribute rather than
# letting Gatekeeper block first launch.
xattr -cr "$INSTALL_PATH" 2>/dev/null || true

echo "==> Launching…"
open "$INSTALL_PATH"

echo
echo "Installed."
