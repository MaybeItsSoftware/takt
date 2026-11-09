#!/usr/bin/env bash
# Upload a bundle from scripts/build_play_bundle.sh to Play's internal testing
# track, through the fastlane lane in mobile/android/fastlane/Fastfile.
#
#   PLAY_JSON_KEY_FILE=~/keys/takt-play.json scripts/upload_play_internal.sh build/play/takt-0.3.0-483.aab
#
# Credentials are a Google Play service account key, given either as its
# contents (PLAY_SERVICE_ACCOUNT_JSON, as CI does) or as a path to the file
# (PLAY_JSON_KEY_FILE). docs/store-listing.md has the whole release path.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
android="$root/mobile/android"

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <path/to/bundle.aab>" >&2
  exit 2
fi
aab="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
if [[ ! -f "$aab" ]]; then
  echo "error: no such bundle: $1" >&2
  exit 1
fi

if [[ -z "${PLAY_SERVICE_ACCOUNT_JSON:-}" && -z "${PLAY_JSON_KEY_FILE:-}" ]]; then
  cat >&2 <<'MSG'
error: no Play credentials. Set one of:
  PLAY_JSON_KEY_FILE=/path/to/service-account.json   (a path to the key)
  PLAY_SERVICE_ACCOUNT_JSON="$(cat service-account.json)"   (its contents)
The service account needs release access to Takt in the Play Console.
MSG
  exit 1
fi
if [[ -n "${PLAY_JSON_KEY_FILE:-}" && -z "${PLAY_SERVICE_ACCOUNT_JSON:-}" && ! -f "$PLAY_JSON_KEY_FILE" ]]; then
  echo "error: PLAY_JSON_KEY_FILE points at $PLAY_JSON_KEY_FILE, which doesn't exist." >&2
  exit 1
fi

cd "$android"
bundle check >/dev/null 2>&1 || bundle install
# A Wear OS bundle (scripts/build_wear_bundle.sh) goes to the watch's track.
track=internal
[[ "$(basename "$aab")" == takt-wear-* ]] && track=wear:internal
exec bundle exec fastlane android internal aab:"$aab" track:"$track"
