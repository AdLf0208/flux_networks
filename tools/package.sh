#!/usr/bin/env bash
# Package both artifacts:
#  - the .hmod via the pin's own cargo-horizon-mod (build + check), from content/
#  - the source-only .hnmod (tools/package.py, deterministic bytes, in the port root)
# The bundle embeds the content (`mixins/fluxnetworks/data/fluxnetworks-content.hmod`), so this
# re-syncs that copy from the freshly built .hmod before packing; tools/package.py refuses a
# bundle whose embedded copy is not the built one.
# Run inside a build slot (it compiles the packaging tool):
#   ../tools/build-slot.sh run --job fluxn-package --port . --step package -- \
#       env CARGO_INCREMENTAL=0 tools/package.sh
set -euo pipefail

PORT=$(cd "$(dirname "$0")/.." && pwd)
HORIZON=${HORIZON:-${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}/ports/_shared/pins/horizon-1d63e0a}
LOGS=${LOGS:-$PORT/target/ladder}
mkdir -p "$LOGS"
cd "$PORT"

CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PORT/target/carry-tools}
export CARGO_TARGET_DIR
(cd content && cargo run --locked --offline --quiet \
    --manifest-path "$HORIZON/crates/cargo-horizon-mod/Cargo.toml" -- horizon-mod build) \
    2>&1 | tee "$LOGS/package-hmod.log"
[ -f content/target/horizon-mod/fluxnetworks.hmod ] \
    && cp content/target/horizon-mod/fluxnetworks.hmod content/fluxnetworks.hmod
(cd content && cargo run --locked --offline --quiet \
    --manifest-path "$HORIZON/crates/cargo-horizon-mod/Cargo.toml" -- horizon-mod check fluxnetworks.hmod) \
    2>&1 | tee -a "$LOGS/package-hmod.log"

# The bundle ships this build's content, never a stale copy: sync it in after the
# build, then let tools/package.py refuse anything that no longer matches.
cp content/fluxnetworks.hmod mixins/fluxnetworks/data/fluxnetworks-content.hmod

python3 tools/package.py | tee "$LOGS/package-hnmod.log"

(cd content && cargo run --locked --offline --quiet \
    --manifest-path "$HORIZON/crates/cargo-horizon-mod/Cargo.toml" -- horizon-mod check \
    ../fluxnetworks.hnmod --kind native-mod) \
    2>&1 | tee "$LOGS/check-native.log"

echo "== artifacts =="
for f in content/fluxnetworks.hmod fluxnetworks.hnmod; do
    [ -f "$f" ] || { echo "missing $f"; exit 1; }
    size=$(stat -c %s "$f")
    sha=$(sha256sum "$f" | cut -d' ' -f1)
    entries=$(python3 -c "import zipfile,sys; print(len(zipfile.ZipFile(sys.argv[1]).namelist()))" "$f")
    echo "$f size=$size sha256=$sha entries=$entries"
done
echo "== embedded copy =="
cmp content/fluxnetworks.hmod mixins/fluxnetworks/data/fluxnetworks-content.hmod \
    && echo "data/fluxnetworks-content.hmod is the built content"
echo "== bundle contents =="
python3 -c "import zipfile,sys; [print(' ', n) for n in zipfile.ZipFile(sys.argv[1]).namelist()]" fluxnetworks.hnmod
