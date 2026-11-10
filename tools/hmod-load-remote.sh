#!/usr/bin/env bash
# Runs tools/hmod-load.sh on the second build machine.
#
# Two differences from the campaign box:
#  * the prepared 26.3 resources are `<root>/resources/26.3`;
#  * the published shared game binary was linked against a newer glibc than the
#    far box has, so `GAME` is this port's own remote build. It still carries no
#    content, so the negative leg (the same ids without `--dev-mod`) is exactly
#    the control the script wants.
set -euo pipefail
PORT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="$HOME/jqbin:$PATH" LD_LIBRARY_PATH="$HOME/jqbin/lib"
export GAME="${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}"
exec "$(dirname "$0")/hmod-load.sh" "$@"
