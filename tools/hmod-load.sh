#!/usr/bin/env bash
# Proves the built content package (content/fluxnetworks.hmod) loads on a real
# game: the shared, unmodded 1d63e0a5 game with `--dev-mod <the .hmod>` must
# register the seven blocks and their items. A second launch WITHOUT
# `--dev-mod` is the negative control: the same ids must be unknown there, so
# the check cannot pass vacuously.
set -euo pipefail
export LC_ALL=C

PORT=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "$PORT/../.." && pwd)
SHARED=$(cd "$PORT/../_shared" && pwd)
PIN_BIN=$SHARED/bin/horizon-1d63e0a5
CTL=${CTL:-$PIN_BIN/horizon-ctl}
GAME=${GAME:-$PIN_BIN/horizon-client}
HMOD=${HMOD:-$PORT/content/fluxnetworks.hmod}
RES=${RES:-${HORIZON_RES:-$ROOT/resources/26.3}}
[ -d "$RES/assets" ] || RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}
SOCK=${SOCK:-/tmp/opencode/fluxnetworks-hmod.sock}

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -f "$HMOD" ] || { echo "missing package: $HMOD" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d /tmp/opencode/fluxnetworks-hmod-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctrl() { "$CTL" --socket "$SOCK" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

cleanup() {
    rc=$?
    ctrl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "${KEEP:-0}" = 1 ]; then echo "kept scratch: $SCRATCH" >&2; else rm -rf "$SCRATCH" 2>/dev/null || true; fi
    echo "EXIT:$rc"
}
trap cleanup EXIT

launch() { # launch WORLD [--dev-mod PATH]
    local world=$1; shift
    rm -f "$SOCK"
    cd "$SCRATCH"
    "$CTL" --socket "$SOCK" launch \
        --game "$GAME" --world "$world" --seed 20261105 --creative --headless --wait \
        --resources "$RES" "$@" >/dev/null
    ctrl wait-ready 120
    ctrl wait 20
}

BLOCKS="fluxnetworks:flux_block fluxnetworks:flux_plug fluxnetworks:flux_point fluxnetworks:flux_controller fluxnetworks:basic_flux_storage fluxnetworks:herculean_flux_storage fluxnetworks:gargantuan_flux_storage"
ITEMS="fluxnetworks:flux_dust fluxnetworks:flux_core fluxnetworks:flux_configurator fluxnetworks:admin_configurator"

echo "== launch WITH the package (--dev-mod $HMOD) =="
launch FluxNetworksHmod --dev-mod "$HMOD"
ctrl command "execute in minecraft:the_end run tp @s 0.5 64 3.5" >/dev/null
ctrl wait 10
x=0
for block in $BLOCKS; do
    ctrl command "execute in minecraft:the_end run setblock $x 64 0 $block" >/dev/null
    x=$((x + 2))
done
ctrl wait 6
x=0
for block in $BLOCKS; do
    got=$(ctrl block "$x" 64 0 | jq -r '.name // .block // empty')
    echo "block $x,64,0 = $got"
    [ "$got" = "$block" ] || fail "the package did not register $block"
    x=$((x + 2))
done
for item in $ITEMS; do
    ctrl give "$item" 1 >/dev/null
done
ctrl wait 6
inventory=$(ctrl inventory)
for item in $ITEMS; do
    jq -e --arg item "$item" \
        '[.hotbar[] | select(.item == $item)] | length >= 1' <<<"$inventory" >/dev/null \
        || fail "the package did not register the $item item"
done
echo "items: $(jq -c '[.hotbar[] | select((.item // "") | startswith("fluxnetworks:")) | {item, count}]' <<<"$inventory")"
ctrl quit >/dev/null 2>&1 || true
sleep 2

echo "== negative control: the SAME ids without --dev-mod =="
launch FluxNetworksHmodControl
ctrl command "execute in minecraft:the_end run tp @s 0.5 64 3.5" >/dev/null
ctrl wait 10
out=$(ctrl command "execute in minecraft:the_end run setblock 0 64 0 fluxnetworks:flux_plug")
echo "setblock reply: $out"
if grep -q 'Changed the block' <<<"$out"; then
    fail "the unmodded game accepted fluxnetworks:flux_plug (the check is vacuous)"
fi
got=$(ctrl block 0 64 0 | jq -r '.name // .block // empty')
echo "control block 0,64,0 = $got"
[ "$got" != "fluxnetworks:flux_plug" ] || fail "the control block is the flux plug"
ctrl give fluxnetworks:basic_flux_storage 1 >/dev/null || true
ctrl wait 4
ctrl inventory | jq -e '[.hotbar[] | select(.item == "fluxnetworks:basic_flux_storage")] | length == 0' >/dev/null \
    || fail "the unmodded game gave a Flux Networks storage item"

echo "PASS: fluxnetworks.hmod registered the seven blocks and the four items on a real game, and the same ids are unknown without the package"
