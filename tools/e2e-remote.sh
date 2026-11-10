#!/usr/bin/env bash
# Runs tools/e2e.sh on the second build machine, whose prepared 26.3 resources
# are `<root>/resources/26.3` and which has no `jq` in a system path.
# The campaign box does not need this wrapper; it is what
# `remote-slot.sh e2e --port ports/fluxnetworks -- tools/e2e-remote.sh` runs.
set -euo pipefail
export PATH="$HOME/jqbin:$PATH" LD_LIBRARY_PATH="$HOME/jqbin/lib"
exec "$(dirname "$0")/e2e.sh" "$@"
