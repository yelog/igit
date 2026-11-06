#!/bin/sh
set -eu
command -v python3 >/dev/null 2>&1 || { printf '%s\n' 'IGit requires Python 3.9+ for native installation.' >&2; exit 1; }
install_source=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 "$install_source/scripts/install/install.py" "$@"
