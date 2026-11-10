#!/usr/bin/env bash
# fluxnetworks slice 1 — the evidence ladder, one log per step.
#
# Run through the campaign runner from the port directory:
#   ../tools/build-slot.sh run --job fluxn-evidence --port . --step evidence -- \
#       bash tools/evidence-ladder.sh
#
# Writes to ports/_shared/evidence/fluxnetworks/ (resolved relative to the port,
# so the same script works on the second build machine).
set -uo pipefail

PORT=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "$PORT/.." && pwd)
EV="$ROOT/_shared/evidence/fluxnetworks"
mkdir -p "$EV"
cd "$PORT" || exit 90

header() { # logfile command-string
    {
        echo "# fluxnetworks slice 1 evidence"
        echo "# date: $(date -Is)"
        echo "# host: $(hostname)  slot: ${BUILD_SLOT:-none}  jobs: ${CARGO_BUILD_JOBS:-default}"
        echo "# stage: ${HORIZON_STAGE_ROOT:-default}  target: ${HORIZON_TARGET_DIR:-default}"
        echo "# cwd: $(pwd)"
        echo "# command: $2"
        echo "# ---------------------------------------------------------------"
    } >"$1"
}

UNIT="${HORIZON_TARGET_DIR:-$PORT/target}/unit"

# 1. runtime unit tests (port crate only, slot-scoped target)
header "$EV/test.log" "CARGO_TARGET_DIR=\$HORIZON_TARGET_DIR/unit cargo test --locked --offline"
CARGO_TARGET_DIR="$UNIT" cargo test --locked --offline >>"$EV/test.log" 2>&1
echo "EXIT:$?" >>"$EV/test.log"

# 2. formatting
header "$EV/fmt.log" "cargo fmt --check"
cargo fmt --check >>"$EV/fmt.log" 2>&1
echo "EXIT:$?" >>"$EV/fmt.log"

# 2b. the content_root helper is not in the crate (horizon-mixins compiles it), so
# rustfmt checks it directly.
header "$EV/fmt-helper.log" "rustfmt --check --edition 2024 mixins/fluxnetworks/embedded_content.rs"
rustfmt --check --edition 2024 mixins/fluxnetworks/embedded_content.rs >>"$EV/fmt-helper.log" 2>&1
echo "EXIT:$?" >>"$EV/fmt-helper.log"

# 3. clippy on the port crate template form, warnings denied
header "$EV/clippy.log" "CARGO_TARGET_DIR=\$HORIZON_TARGET_DIR/unit cargo clippy --all-targets --locked --offline -- -D warnings"
CARGO_TARGET_DIR="$UNIT" cargo clippy --all-targets --locked --offline -- -D warnings >>"$EV/clippy.log" 2>&1
echo "EXIT:$?" >>"$EV/clippy.log"

# 4. staged check at the pin (horizon.py keeps the raw cargo output in
# target/horizon-check.log; copy it, since target/ is disposable)
header "$EV/check.log" "python3 tools/horizon.py check"
python3 tools/horizon.py check >>"$EV/check.log" 2>&1
echo "EXIT:$?" >>"$EV/check.log"
[ -f target/horizon-check.log ] && cp target/horizon-check.log "$EV/cargo-check.log"

# 5. embedded tests (staged pin, game::fluxnetworks::)
header "$EV/test-embedded.log" "python3 tools/horizon.py test"
python3 tools/horizon.py test >>"$EV/test-embedded.log" 2>&1
echo "EXIT:$?" >>"$EV/test-embedded.log"
[ -f target/horizon-test.log ] && cp target/horizon-test.log "$EV/cargo-test.log"

# 6. fast build of the game binary
header "$EV/build.log" "python3 tools/horizon.py build"
python3 tools/horizon.py build >>"$EV/build.log" 2>&1
echo "EXIT:$?" >>"$EV/build.log"
[ -f target/horizon-build.log ] && cp target/horizon-build.log "$EV/cargo-build.log"

echo "== summaries =="
grep -H "^EXIT:" "$EV/test.log" "$EV/fmt.log" "$EV/fmt-helper.log" "$EV/clippy.log" "$EV/check.log" "$EV/test-embedded.log" "$EV/build.log"
grep -h "test result:" "$EV/test.log" "$EV/test-embedded.log"
grep -h '"cargo_exit"' "$EV/check.log" "$EV/test-embedded.log" "$EV/build.log"
