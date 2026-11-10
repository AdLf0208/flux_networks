#!/usr/bin/env bash
# In-game check for Flux Networks slice 1: one energy network on a real game.
#
# The arena is a stone floor at y=100 in the Overworld, so every block read and
# command is in the player's own dimension (the pin's `ctl tp` is reliable
# there). The native bundle is compiled into the game.
#
# Default run (EMBEDDED=1): the game is launched with **only the native bundle**
# (no `--dev-mod`) and the content comes from the bundle's embedded copy
# (`mixins/fluxnetworks/data/fluxnetworks-content.hmod`, the AE2 pattern). The run
# also checks the file the helper wrote, and CONTROL=1 (default with EMBEDDED)
# first launches the same binary with `.horizon-cache` unusable, where the block
# and item ids must be unknown.
# EMBEDDED=0: the game loads `content/` as a dev mod (the pre-embedding shape).
#
# The network under test (upstream's model, one network id per device):
#
#   (0,100,0) herculean_flux_storage network 10  the source, filled by command
#   (1,100,0) flux_plug             network 1    pulls from the source beside it
#   (3,100,0) flux_point            network 1    two blocks away: not adjacent
#   (4,100,0) herculean_flux_storage network 20  the receiver beside the point
#
# The storages are the herculean tier (16,000,000 FE, 120,000 FE/t) so the
# measurement interval cannot empty the source or fill the receiver.
#
# Default run:
#  - control "no network id": with every device unlinked the source keeps its
#    1,000,000 FE, the plug and the receiver stay empty;
#  - joined, the plug drains the source at the storage's 120,000 FE/t, the
#    network carries it to the point two blocks away, and the point fills the
#    receiver: neither side moves faster than 120,000 FE/t and the whole
#    network's FE is conserved (source + plug + point + receiver = 16,000,000);
#  - control "disconnected point": with the point moved to another network the
#    receiver stops filling and the source stops draining;
#  - a full receiver (16,000,000 FE, its capacity) refuses more, and because
#    nothing asks for energy the source stops draining too;
#  - the readout reports each device's declared limit and network.
#
# SELFTEST=1 flips the "no network id" assertion (it must FAIL), proving the
# scenario can fail.
set -euo pipefail
export LC_ALL=C

# Self-locating: the same script runs on the campaign box and on the second
# build machine (`tools/e2e-remote.sh`), whose prepared resources live under
# `<root>/resources/26.3`.
PORT=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "$PORT/../.." && pwd)
SHARED=$(cd "$PORT/../_shared" && pwd)
CTL=${CTL:-$SHARED/bin/horizon-1d63e0a5/horizon-ctl}   # pin-matched shared ctl
GAME=${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}
RES=${RES:-${HORIZON_RES:-$ROOT/resources/26.3}}
[ -d "$RES/assets" ] || RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}
SOCK=${SOCK:-/tmp/opencode/fluxnetworks-e2e.sock}
SELFTEST=${SELFTEST:-0}
KEEP=${KEEP:-0}
EMBEDDED=${EMBEDDED:-1}
CONTROL=${CONTROL:-$EMBEDDED}
WORLD=FluxNetworksE2E
CONTROL_WORLD=FluxNetworksControl

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d /tmp/opencode/fluxnetworks-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctrl() { "$CTL" --socket "$SOCK" "$@"; }
say() { ctrl chat "/$1" >/dev/null; }
fail() { echo "FAIL: $*" >&2; ctrl logs --lines 120 >&2 || true; exit 1; }

cleanup() {
    rc=$?
    ctrl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "$KEEP" = 1 ]; then
        echo "kept scratch: $SCRATCH" >&2
    else
        rm -rf "$SCRATCH" 2>/dev/null || true
    fi
    echo "EXIT:$rc"
}
trap cleanup EXIT

launch() { # launch [WORLD]
    local world=${1:-$WORLD}
    rm -f "$SOCK"
    cd "$SCRATCH"
    local args=(--game "$GAME" --world "$world" --seed 20261105 --creative --headless --wait
                --resources "$RES")
    if [ "$EMBEDDED" = 1 ]; then
        echo "  launch: no --dev-mod; the bundle carries its own content" >&2
    else
        args+=(--dev-mod "$PORT/content")
    fi
    "$CTL" --socket "$SOCK" launch "${args[@]}" >/dev/null
    ctrl wait-ready 120
    ctrl wait 40
}

# The control for EMBEDDED=1: the same binary (every hook is in it) with the
# embedded content made unavailable — `.horizon-cache` is a plain file, so the
# helper can neither compare nor write its content file and falls back to
# `Continue`. Both ids must then be unknown; if they resolve, the run proves
# nothing about the embedded file.
#
# The helper writes to `std::env::current_dir()/.horizon-cache`; the game only
# calls `set_current_dir` for `--game-dir`, which this script never passes, so the
# cache is `$SCRATCH/.horizon-cache` (the launch `cd`s there). The control blocks
# exactly that path, and the assertion below catches a wrong guess: if the content
# still loads from somewhere else, the ids are known and the leg fails instead of
# passing vacuously.
control_leg() {
    echo "== control: the same game with no embedded content file =="
    rm -rf "$SCRATCH/.horizon-cache"
    : > "$SCRATCH/.horizon-cache"
    launch "$CONTROL_WORLD"
    arena
    ctrl command "item replace entity @s hotbar.0 with fluxnetworks:flux_plug 1" >/dev/null || true
    ctrl wait 4 >/dev/null
    ctrl inventory | jq -e '[.hotbar[], .main[] | select(.item == "fluxnetworks:flux_plug")] | length == 0' >/dev/null \
        || fail "the control game has the flux_plug item, so the content did not come from the embedded file"
    ctrl command "setblock 0 100 0 fluxnetworks:herculean_flux_storage" >/dev/null || true
    ctrl wait 4 >/dev/null
    local name
    name=$(ctrl block 0 100 0 | jq -r '.name // .block // empty')
    [ "$name" = "minecraft:air" ] || fail "the control game holds $name, so the block id was known"
    [ -f "$SCRATCH/.horizon-cache" ] || fail "the control's .horizon-cache was replaced: the helper wrote its content"
    echo "  without the embedded file: no flux_plug item, the block stayed $name"
    ctrl quit >/dev/null 2>&1 || true
    rm -f "$SCRATCH/.horizon-cache"
}

# EMBEDDED=1: the bundle must have written its content out at startup, byte for
# byte the delivered content mod.
embedded_content_check() {
    local cache=$SCRATCH/.horizon-cache/fluxnetworks-embedded-content.hmod
    [ -f "$cache" ] || fail "the bundle did not write its embedded content to $cache"
    cmp -s "$cache" "$PORT/content/fluxnetworks.hmod" \
        || fail "the embedded content differs from content/fluxnetworks.hmod"
    echo "  embedded content: $(wc -c < "$cache" | tr -d ' ') B at $cache, identical to content/fluxnetworks.hmod"
}

arena() {
    ctrl tp 0.5 101 3.5 >/dev/null
    ctrl wait 20
    ctrl command "fill -8 99 -8 24 99 24 minecraft:stone" >/dev/null
    ctrl command "fill -8 100 -8 24 110 24 minecraft:air" >/dev/null
    ctrl command "kill @e[type=!player]" >/dev/null
    ctrl tp 0.5 100 3.5 >/dev/null
    ctrl wait 20
    local y feet floor
    y=$(ctrl state | jq -r '.player.position[1] // 0')
    feet=$(ctrl block 0 100 3 | jq -r '.name // .block // empty')
    floor=$(ctrl block 0 99 3 | jq -r '.name // .block // empty')
    echo "  arena: y=$y floor(0,99,3)=$floor feet(0,100,3)=$feet"
    awk -v y="$y" 'BEGIN { exit !(y >= 99.9) }' || fail "the player is not on the arena floor (y=$y)"
    [ "$feet" = "minecraft:air" ] || fail "the arena volume is not clear ($feet at 0,100,3)"
    [ "$floor" = "minecraft:stone" ] || fail "the arena floor is missing ($floor at 0,99,3)"
}

place() { # place X Y Z ID
    ctrl command "setblock $1 $2 $3 $4" >/dev/null
    ctrl wait 4
    local got
    got=$(ctrl block "$1" "$2" "$3" | jq -r '.name // .block // empty')
    echo "  block($1,$2,$3) = $got"
    [ "$got" = "$4" ] || fail "expected $4 at $1,$2,$3, got '$got'"
}

# info X Y Z: this device's status line, after a fresh reply arrives (the t=
# field makes each reply unique).
info() { # info X Y Z
    local out line before
    before=$(ctrl messages 20 | jq -r '.chat[]' | grep -E " at $1,$2,$3:" | head -1)
    say "fluxnetworks info $1 $2 $3"
    for _ in $(seq 1 25); do
        ctrl wait 4 >/dev/null
        out=$(ctrl messages 20)
        line=$(jq -r '.chat[]' <<<"$out" | grep -E " at $1,$2,$3:|No Flux Networks device" | head -1)
        if [ -n "$line" ] && [ "$line" != "$before" ]; then
            echo "$line"
            return
        fi
    done
    echo "NO-INFO $(jq -c '{chat,action_bar}' <<<"$out")"
}
# set_energy X Y Z AMOUNT: the harness control `/fluxnetworks energy`, retried
# until the device reads the value back (chat commands are asynchronous).
set_energy() { # X Y Z AMOUNT
    local want=$4 line
    say "fluxnetworks energy $want $1 $2 $3"
    for _ in $(seq 1 10); do
        line=$(info "$1" "$2" "$3")
        if [ "$(energy_of "$line")" = "$want" ]; then
            echo "  set($1,$2,$3) = $line"
            return
        fi
        say "fluxnetworks energy $want $1 $2 $3"
    done
    fail "could not set $1,$2,$3 to $want FE: $line"
}
# load_energy X Y Z AMOUNT: sets a buffer while the network is flowing, so the
# reply ("Energy set to N FE.") is the evidence, not a later reading of a value
# the network drains as fast as it is written.
load_energy() { # X Y Z AMOUNT
    local want=$4 out
    say "fluxnetworks energy $want $1 $2 $3"
    for _ in $(seq 1 10); do
        ctrl wait 4 >/dev/null
        out=$(ctrl messages 20)
        if jq -r '.chat[]' <<<"$out" | grep -qx "Energy set to $want FE."; then
            echo "  load($1,$2,$3) = $want FE"
            return
        fi
        say "fluxnetworks energy $want $1 $2 $3"
    done
    fail "could not load $1,$2,$3 with $want FE"
}

# join_network X Y Z ID: `/fluxnetworks network`, retried until it reads back.
join_network() { # X Y Z ID
    local want=$4 line
    say "fluxnetworks network $want $1 $2 $3"
    for _ in $(seq 1 10); do
        line=$(info "$1" "$2" "$3")
        if [ "$(network_of "$line")" = "$want" ]; then
            echo "  network($1,$2,$3) = $line"
            return
        fi
        say "fluxnetworks network $want $1 $2 $3"
    done
    fail "could not put $1,$2,$3 on network $want: $line"
}
expect_info() { # expect_info X Y Z SUBSTRING
    local line
    line=$(info "$1" "$2" "$3")
    echo "  info($1,$2,$3): $line"
    case "$line" in
        *"$4"*) ;;
        *) fail "expected /fluxnetworks info to say '$4', got '$line'" ;;
    esac
}

# The energy in an info line: the number before "/<capacity> FE".
energy_of() { sed -nE 's/.* at [-0-9]+,[-0-9]+,[-0-9]+: ([0-9]+)\/[0-9]+ FE.*/\1/p' <<<"$1"; }
# ... the world game time after "t=".
time_of() { sed -nE 's/.* t=([0-9]+)\).*/\1/p' <<<"$1"; }
# ... the network the device reads, "none" when unlinked.
network_of() { sed -nE 's/.*network ([A-Za-z0-9]+),.*/\1/p' <<<"$1"; }

echo "== launch (Overworld arena at y=100, content + native bundle) =="
if [ "$CONTROL" = 1 ]; then control_leg; fi
launch
if [ "$EMBEDDED" = 1 ]; then embedded_content_check; fi
arena

echo "== place the source, the plug, the point and the receiver =="
place 0 100 0 fluxnetworks:herculean_flux_storage
place 1 100 0 fluxnetworks:flux_plug
place 3 100 0 fluxnetworks:flux_point
place 4 100 0 fluxnetworks:herculean_flux_storage
air=$(ctrl block 2 100 0 | jq -r '.name // .block // empty')
echo "  block(2,100,0) = $air (the plug and the point are not adjacent)"
[ "$air" = "minecraft:air" ] || fail "the gap between the plug and the point is not air ($air)"

echo "== control: no network id, nothing moves =="
set_energy 0 100 0 1000000
ctrl wait 40
source_line=$(info 0 100 0)
plug_line=$(info 1 100 0)
receiver_line=$(info 4 100 0)
echo "  source:   $source_line"
echo "  plug:     $plug_line"
echo "  receiver: $receiver_line"
[ "$(network_of "$plug_line")" = none ] || fail "the plug reads a network before one was set: $plug_line"
[ "$(energy_of "$source_line")" = 1000000 ] || fail "the unlinked source lost energy: $source_line"
[ "$(energy_of "$plug_line")" = 0 ] || fail "the unlinked plug holds energy: $plug_line"
[ "$(energy_of "$receiver_line")" = 0 ] || fail "the unlinked receiver holds energy: $receiver_line"
if [ "$SELFTEST" = 1 ]; then
    echo "  SELFTEST: expecting the unlinked network to have moved energy (this must fail)"
    [ "$(energy_of "$receiver_line")" != 0 ] || fail "SELFTEST flipped: an unlinked network moved no energy, as it should"
fi

echo "== join the network and let it run =="
join_network 0 100 0 10
join_network 1 100 0 1
join_network 3 100 0 1
join_network 4 100 0 20
expect_info 1 100 0 "limit 800000 FE/t"
expect_info 0 100 0 "limit 120000 FE/t"

# The joins above take a while, so the flow has already run: empty both ends,
# let the plug and the point drain into the receiver, then load the source
# again. The network then holds exactly 16,000,000 FE.
load_energy 0 100 0 0
ctrl wait 10
load_energy 4 100 0 0
load_energy 0 100 0 16000000

# The rates first: each device is measured over its own pair of reads, which
# take about 40 ticks each. The source (16,000,000 FE at 120,000 FE/t) lasts
# ~133 ticks, so both windows are measured while it is still flowing.
before_source=$(info 0 100 0)
after_source=$(info 0 100 0)
before_receiver=$(info 4 100 0)
after_receiver=$(info 4 100 0)
plug_line=$(info 1 100 0)
point_line=$(info 3 100 0)
# ... then the whole network with the flow stopped, for conservation: reads are
# ~8 ticks apart, so a running network would double-count what moved between
# them. Moving the point off the network freezes every buffer.
join_network 3 100 0 99
ctrl wait 8
sum_source=$(info 0 100 0)
sum_receiver=$(info 4 100 0)
sum_plug=$(info 1 100 0)
sum_point=$(info 3 100 0)
echo "  source before: $before_source"
echo "  source after:  $after_source"
echo "  receiver after: $after_receiver"
echo "  plug:  $plug_line"
echo "  point: $point_line"

source_ticks=$(( $(time_of "$after_source") - $(time_of "$before_source") ))
moved=$(( $(energy_of "$before_source") - $(energy_of "$after_source") ))
receiver_ticks=$(( $(time_of "$after_receiver") - $(time_of "$before_receiver") ))
arrived=$(( $(energy_of "$after_receiver") - $(energy_of "$before_receiver") ))
held=$(( $(energy_of "$sum_source") + $(energy_of "$sum_receiver") \
       + $(energy_of "$sum_plug") + $(energy_of "$sum_point") ))
echo "  source: -$moved FE in $source_ticks ticks; receiver: +$arrived FE in $receiver_ticks ticks; network holds $held FE"

[ "$source_ticks" -gt 0 ] || fail "the two source reads have the same game time"
[ "$receiver_ticks" -gt 0 ] || fail "the two receiver reads have the same game time"
[ "$moved" -gt 0 ] || fail "the network moved no energy from the source"
[ "$arrived" -gt 0 ] || fail "the receiver was not filled"
[ "$moved" -le $(( 120000 * source_ticks )) ] || fail "$moved FE in $source_ticks ticks is faster than the storage's 120,000 FE/t"
[ "$moved" -ge $(( 120000 * (source_ticks - 2) )) ] || fail "$moved FE in $source_ticks ticks is slower than the storage's 120,000 FE/t"
[ "$arrived" -le $(( 120000 * receiver_ticks )) ] || fail "$arrived FE in $receiver_ticks ticks is faster than the storage's 120,000 FE/t"
[ "$arrived" -ge $(( 120000 * (receiver_ticks - 2) )) ] || fail "$arrived FE in $receiver_ticks ticks is slower than the storage's 120,000 FE/t"
[ "$held" -eq 16000000 ] || fail "the network's energy is not conserved: $held FE, not 16000000"
[ "$(energy_of "$plug_line")" -le 800000 ] || fail "the plug holds more than its 800,000 FE buffer: $plug_line"
join_network 3 100 0 1

echo "== control: a point on another network carries nothing =="
join_network 3 100 0 2
# load both ends, so "nothing moved" is not true by accident
set_energy 0 100 0 16000000
set_energy 4 100 0 0
held_source=$(info 0 100 0)
held_receiver=$(info 4 100 0)
ctrl wait 40
still_source=$(info 0 100 0)
still_receiver=$(info 4 100 0)
echo "  disconnected: source $held_source -> $still_source"
echo "                receiver $held_receiver -> $still_receiver"
[ "$(energy_of "$still_source")" = "$(energy_of "$held_source")" ] \
    || fail "the source drained while the point was on another network: $still_source"
[ "$(energy_of "$still_receiver")" = "$(energy_of "$held_receiver")" ] \
    || fail "the receiver filled while the point was on another network: $still_receiver"
join_network 3 100 0 1

echo "== a full receiver refuses more =="
load_energy 4 100 0 16000000
ctrl wait 20
# the source was drained by the measurement above: load it again, so "the
# source did not drain" is not true by accident
load_energy 0 100 0 16000000
ctrl wait 20
full_source=$(info 0 100 0)
ctrl wait 40
after_full_source=$(info 0 100 0)
full_receiver=$(info 4 100 0)
echo "  full receiver: $full_receiver"
echo "  source: $full_source -> $after_full_source"
[ "$(energy_of "$full_receiver")" = 16000000 ] || fail "a full storage did not stay full: $full_receiver"
[ "$(energy_of "$after_full_source")" = "$(energy_of "$full_source")" ] \
    || fail "the source drained into a network with nothing to fill: $after_full_source"

echo "PASS: an unlinked network moved nothing; joined, the plug drained the source at 120,000 FE/t, the network carried it two blocks to the point, the point filled the receiver, FE was conserved, a point on another network carried nothing, and a full receiver refused more"
