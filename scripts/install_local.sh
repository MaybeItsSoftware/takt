#!/usr/bin/env bash
set -euo pipefail

# Installs Takt on this Mac: the Rust app (desktop/) as /Applications/Takt.app,
# superseding whatever is there — an old Swift build included — and relaunches
# it. Nothing is kept; git is the way back. /Applications is only touched after
# a good build. See scripts/install_desktop.sh.

exec "$(cd "$(dirname "$0")" && pwd)/install_desktop.sh" "$@"
