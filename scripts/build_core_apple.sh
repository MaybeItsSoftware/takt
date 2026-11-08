#!/usr/bin/env bash
# Builds the Rust core (core/) for every Apple slice and packages it as
#
#   build/core/takt_coreFFI.xcframework          (not committed; rebuilt here)
#   Sources/TaktRustCore/TaktRustCore.swift      (committed; the UniFFI bindings)
#
# Package.swift's `takt_coreFFI` binary target points at the xcframework, so
# the Swift package — and with it `swift test` and the iPhone app — cannot
# resolve until this has run once. The scripts that build those
# (install_ios.sh, build_app_store.sh) call it first.
#
#   scripts/build_core_apple.sh              release build (what the apps ship)
#   PROFILE=debug scripts/build_core_apple.sh
#   TAKT_CORE_IF_STALE=1 scripts/build_core_apple.sh   skip when nothing in core/ changed
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT_DIR/core"
OUT="$ROOT_DIR/build/core"
XCFRAMEWORK="$OUT/takt_coreFFI.xcframework"
BINDINGS="$ROOT_DIR/Sources/TaktRustCore/TaktRustCore.swift"
PROFILE=${PROFILE:-release}
FLAG=$([ "$PROFILE" = release ] && echo --release || true)
LIB=libtakt_core.a

if [[ "${TAKT_CORE_IF_STALE:-}" == 1 && -d "$XCFRAMEWORK" ]]; then
  newer=$(find "$CORE/src" "$CORE/Cargo.toml" "$CORE/Cargo.lock" -newer "$XCFRAMEWORK" -print -quit 2>/dev/null || true)
  if [[ -z "$newer" ]]; then
    echo "takt-core: up to date"
    exit 0
  fi
fi

# The package's own floors (Package.swift `platforms:`).
export MACOSX_DEPLOYMENT_TARGET=15.0
export IPHONEOS_DEPLOYMENT_TARGET=18.0

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
for t in "${TARGETS[@]}"; do
  echo "takt-core: $t"
  cargo build -q $FLAG --manifest-path "$CORE/Cargo.toml" --target "$t" --lib
done

# Swift bindings, generated from the compiled library's metadata. Library mode
# reads `cargo metadata`, so it runs inside core/.
gen=$(mktemp -d)
trap 'rm -rf "$gen"' EXIT
host_dylib="$CORE/target/aarch64-apple-darwin/$PROFILE/libtakt_core.dylib"
(cd "$CORE" && cargo run -q --features bindgen --bin uniffi-bindgen -- \
  generate --library "$host_dylib" --language swift --out-dir "$gen")

mkdir -p "$(dirname "$BINDINGS")"
cp "$gen/takt_core.swift" "$BINDINGS"

headers="$gen/headers"
mkdir -p "$headers"
cp "$gen/takt_coreFFI.h" "$headers/"
cp "$gen/takt_coreFFI.modulemap" "$headers/module.modulemap"

# One universal Mac slice (Apple silicon and Intel).
mkdir -p "$gen/macos"
lipo -create \
  "$CORE/target/aarch64-apple-darwin/$PROFILE/$LIB" \
  "$CORE/target/x86_64-apple-darwin/$PROFILE/$LIB" \
  -output "$gen/macos/$LIB"

# One simulator slice for both: a generic simulator build compiles x86_64
# alongside arm64, and links fail without it.
mkdir -p "$gen/ios-sim"
lipo -create \
  "$CORE/target/aarch64-apple-ios-sim/$PROFILE/$LIB" \
  "$CORE/target/x86_64-apple-ios/$PROFILE/$LIB" \
  -output "$gen/ios-sim/$LIB"

mkdir -p "$OUT"
rm -rf "$XCFRAMEWORK"
xcodebuild -create-xcframework \
  -library "$gen/macos/$LIB" -headers "$headers" \
  -library "$CORE/target/aarch64-apple-ios/$PROFILE/$LIB" -headers "$headers" \
  -library "$gen/ios-sim/$LIB" -headers "$headers" \
  -output "$XCFRAMEWORK" >/dev/null

echo "takt-core: $XCFRAMEWORK"
