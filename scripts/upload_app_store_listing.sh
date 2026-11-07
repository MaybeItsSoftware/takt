#!/usr/bin/env bash
# Uploads Takt's App Store listing: the text from docs/store-listing.md and the
# framed screenshots in design/store (6.9" iPhone and 13" iPad). No binary and
# no submission for review — the listing only, in en-GB.
#
#   scripts/upload_app_store_listing.sh --dry-run   # assemble build/app-store/listing only
#   scripts/upload_app_store_listing.sh
#
# The key is an App Store Connect API key, the one open-parliament uses, read
# from ~/.config/takt/asc.env (ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_CONTENT, or
# open-parliament's APP_STORE_CONNECT_API_KEY_* names; the .p8 raw or as
# base64) unless they are already in the environment.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
listing="$root/build/app-store/listing"
locale="${TAKT_ASC_LOCALE:-en-GB}"
rm -rf "$listing"
mkdir -p "$listing/metadata/$locale" "$listing/screenshots/$locale"

python3 -I - "$root/docs/store-listing.md" "$listing/metadata" "$locale" <<'PY'
import re, sys, pathlib
doc = pathlib.Path(sys.argv[1]).read_text()
meta = pathlib.Path(sys.argv[2])
here = meta / sys.argv[3]
field = lambda label: re.search(r"\*\*" + re.escape(label) + r"\*\*[^\n]*?\n?`([^`]+)`", doc).group(1)
description = re.search(r"\*\*Description\*\* \[4000\]:\s*```\n(.*?)```", doc, re.S).group(1)
paragraphs = [re.sub(r"\n(?!•) *", " ", p.strip()) for p in description.strip().split("\n\n")]
# Apple turns away a listing that names other platforms (guideline 2.3.10).
paragraphs = [p.replace(", and an ongoing notification on Android", "") for p in paragraphs]
assert not any("Android" in p for p in paragraphs), "the App Store description mentions Android"
privacy = re.search(r"\*\*Privacy policy URL:\*\* (\S+)", doc).group(1)
values = {
    "name.txt": (field("Name"), 30),
    "subtitle.txt": (field("Subtitle"), 30),
    "promotional_text.txt": (field("Promotional text"), 170),
    "description.txt": ("\n\n".join(paragraphs), 4000),
    "keywords.txt": (field("Keywords"), 100),
    "privacy_url.txt": (privacy, 255),
    "support_url.txt": ("https://github.com/MaybeItsSoftware/takt/issues", 255),
}
for name, (text, limit) in values.items():
    assert 0 < len(text) <= limit, f"{name} is {len(text)} characters, over {limit}"
    (here / name).write_text(text)
(meta / "copyright.txt").write_text("2026 MaybeItsSoftware")
(meta / "primary_category.txt").write_text("PRODUCTIVITY")
PY

# deliver tells the devices apart by pixel size; the prefix keeps the order.
for f in "$root/design/store/iphone/"0*.png; do cp "$f" "$listing/screenshots/$locale/iphone-$(basename "$f")"; done
for f in "$root/design/store/ipad/"0*.png; do cp "$f" "$listing/screenshots/$locale/ipad-$(basename "$f")"; done

if [[ "${1:-}" == "--dry-run" ]]; then find "$listing" -type f | sort; exit 0; fi

if [[ -z "${ASC_KEY_ID:-}" && -f "$HOME/.config/takt/asc.env" ]]; then
  set -a; source "$HOME/.config/takt/asc.env"; set +a
fi
# open-parliament's .env names them APP_STORE_CONNECT_API_KEY_*; either works.
export ASC_KEY_ID="${ASC_KEY_ID:-${APP_STORE_CONNECT_API_KEY_ID:-}}"
export ASC_ISSUER_ID="${ASC_ISSUER_ID:-${APP_STORE_CONNECT_API_KEY_ISSUER_ID:-}}"
export ASC_KEY_CONTENT="${ASC_KEY_CONTENT:-${APP_STORE_CONNECT_API_KEY_CONTENT:-}}"
: "${ASC_KEY_ID:?set it, or put it in ~/.config/takt/asc.env}" "${ASC_ISSUER_ID:?}" "${ASC_KEY_CONTENT:?}"
# fastlane is told the key is base64; a raw .p8 is encoded here. '%b', as
# build_app_store.sh writes it, so a key kept on one line with literal \n
# between its lines is sent as the PEM it stands for.
if [[ "$ASC_KEY_CONTENT" == *"BEGIN PRIVATE KEY"* ]]; then
  ASC_KEY_CONTENT="$(printf '%b\n' "$ASC_KEY_CONTENT" | base64 | tr -d '\n')"
  export ASC_KEY_CONTENT
fi

cd "$root/mobile/ios"
bundle exec fastlane ios listing listing:"$listing"
