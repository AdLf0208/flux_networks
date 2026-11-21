#!/usr/bin/env bash
# In-game check for Flux Networks (SDK package 0.2.2): one energy network, the
# "creating flux" mechanic that makes flux dust, and every carried shaped recipe
# in a real crafting table.
#
#   GAME=<launcher-built horizon-client>  required: the game with this package compiled in
#   CONTROL_GAME=<a game without it>      optional: proves the arena needs the package (both ids
#                                         unknown there); skipped with a NOTE when unset
#   CTL=<horizon-ctl>                     optional: defaults to the pin-matched shared control
#   SELFTEST=1                            flips an assertion, so the run must FAIL
#
# The leg launches `--offscreen` (not `--headless`): `click-slot`, which the
# crafting phase needs, requires a renderer.
#
#   GAME=<launcher-built horizon-client>  required: the game with this package compiled in
#   CONTROL_GAME=<a game without it>      optional: proves the arena needs the package (both ids
#                                         unknown there); skipped with a NOTE when unset
#   CTL=<horizon-ctl>                     optional: defaults to the pin-matched shared control
#   SELFTEST=1                            flips the "no network id" assertion, so the run must FAIL
#
# The arena is a stone floor at y=100 in the Overworld, so every block read and
# command is in the player's own dimension (the pin's `ctl tp` is reliable
# there).
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
# What it proves:
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
set -euo pipefail
export LC_ALL=C

# Self-locating: the same script runs on the campaign box and on the second
# build machine (`tools/e2e-remote.sh`), whose prepared resources live under
# `<root>/resources/26.3`.
PORT=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "$PORT/../.." && pwd)
SHARED=$(cd "$PORT/../_shared" && pwd)
CTL=${CTL:-$SHARED/bin/horizon-1d63e0a5/horizon-ctl}   # pin-matched shared ctl
GAME=${GAME:-}
CONTROL_GAME=${CONTROL_GAME:-}
RES=${RES:-${HORIZON_RES:-$ROOT/resources/26.3}}
[ -d "$RES/assets" ] || RES=/home/adlf/Desktop/horizon/.local/horizon-client/26.3
SOCK=${SOCK:-${E2E_TMP:-/tmp/opencode}/fluxnetworks-e2e.sock}
SELFTEST=${SELFTEST:-0}
KEEP=${KEEP:-0}
WORLD=FluxNetworksE2E
CONTROL_WORLD=FluxNetworksControl

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
if [ -z "$GAME" ]; then
    echo "GAME is not set: point it at a game the launcher built with this package" >&2
    echo "  horizon-launcher native-build ports/fluxnetworks --instance <instance> --trust-native-code" >&2
    exit 1
fi
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d "${E2E_TMP:-/tmp/opencode}/fluxnetworks-home.XXXXXX")
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

launch() { # launch [WORLD] [GAME]
    local world=${1:-$WORLD} game=${2:-$GAME}
    rm -f "$SOCK"
    cd "$SCRATCH"
    # No --dev-mod: the package's code, assets and data are compiled into the game by the
    # launcher, and a native .hmod cannot load on an unmodded one.
    "$CTL" --socket "$SOCK" launch --game "$game" --world "$world" --seed 20261105 \
        --creative --offscreen --wait --resources "$RES" >/dev/null
    ctrl wait-ready 120
    ctrl wait 40
}

# Optional control: the same scenario on a game the launcher built **without** this package. Both
# the block and the item must be unknown there, so the arena and every assertion below would fail
# without the package: the run is not vacuous. Skipped (with a NOTE) when CONTROL_GAME is unset.
control_leg() {
    if [ -z "$CONTROL_GAME" ] || [ ! -x "$CONTROL_GAME" ]; then
        echo "NOTE: CONTROL_GAME is unset or not executable; the non-vacuous control leg is skipped"
        return
    fi
    echo "== control: a game built without this package must not know the ids =="
    launch "$CONTROL_WORLD" "$CONTROL_GAME"
    arena
    ctrl command "item replace entity @s hotbar.0 with fluxnetworks:flux_plug 1" >/dev/null || true
    ctrl wait 4 >/dev/null
    ctrl inventory | jq -e '[.hotbar[], .main[] | select(.item == "fluxnetworks:flux_plug")] | length == 0' >/dev/null \
        || fail "the control game has the flux_plug item, so the package is not what provided it"
    ctrl command "setblock 0 100 0 fluxnetworks:herculean_flux_storage" >/dev/null || true
    ctrl wait 4 >/dev/null
    local name
    name=$(ctrl block 0 100 0 | jq -r '.name // .block // empty')
    [ "$name" = "minecraft:air" ] || fail "the control game holds $name, so the block id was known"
    echo "  without the package: no flux_plug item, the block stayed $name"
    ctrl quit >/dev/null 2>&1 || true
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

echo "== launch (Overworld arena at y=100, package compiled into the game) =="
control_leg
launch
arena

# ---- slice 2: "creating flux" ---------------------------------------------------------------
# Upstream's `EventHandler.onPlayerInteract`: an obsidian block with a bedrock (or flux-block)
# base two blocks below it, redstone items lying in the block between, and a left-click on the
# obsidian. The click is cancelled: the obsidian is spent, the redstone becomes one flux dust
# per item (at most 512), and the gap becomes cobblestone or the obsidian that slid down.

block_name() { ctrl block "$1" "$2" "$3" | jq -r '.name // .block // empty'; }
dust() { ctrl inventory | jq -r '.counts["fluxnetworks:flux_dust"] // 0'; }
# The redstone item entities **in one press's column** (a previous press's pile is still on the
# ground a few blocks away, so a radius around the player would count it); plus a whole-radius
# count, for the cap phase, where the press's own pile is the only redstone left in the world.
redstone_at() { # X Z: the matching items, one JSON object per line
    ctrl entities 12 item | jq -c --argjson x "$1" --argjson z "$2" '
        .entities[]
        | select(.kind == "item" and .item.item == "minecraft:redstone")
        | select((.pos[0] - $x) < 0.9 and (.pos[0] - $x) > -0.9)
        | select((.pos[2] - $z) < 0.9 and (.pos[2] - $z) > -0.9)'
}
redstone_stacks() { redstone_at "$1" "$2" | grep -c . || true; }
redstone_items() { redstone_at "$1" "$2" | jq -s 'map(.item.count) | add // 0'; }
redstone_near() {
    ctrl entities 12 item \
        | jq '[.entities[] | select(.kind == "item" and .item.item == "minecraft:redstone") | .item.count] | add // 0'
}
redstone_near_stacks() {
    ctrl entities 12 item \
        | jq '[.entities[] | select(.kind == "item" and .item.item == "minecraft:redstone")] | length'
}
# Drop every item at a press: used after a control that must *leave* its redstone, so later
# phases count only their own pile.
clear_at() { # X Z
    ctrl command "execute positioned $1 101 $2 run kill @e[type=minecraft:item,distance=..2]" >/dev/null
    ctrl wait 4
}
# press_base X Z BASE: the base block at y=100, air in the gap, obsidian at y=102.
press_base() {
    ctrl command "setblock $1 100 $2 $3" >/dev/null
    ctrl command "setblock $1 101 $2 minecraft:air" >/dev/null
    ctrl command "setblock $1 102 $2 minecraft:obsidian" >/dev/null
    ctrl wait 4
}
# stand_by X Z: two blocks from the press, out of item-pickup range (a player one block away
# would swallow the redstone before the click and the assertion would count nothing), and well
# inside the reach of the obsidian at y=102.
stand_by() { ctrl tp "$(( $1 + 2 )).5" 100 "$2.5" >/dev/null; ctrl wait 6; }
# collect X Z: step next to the press and take what the click dropped.
collect() { ctrl tp "$(( $1 + 1 )).5" 100 "$2.5" >/dev/null; ctrl wait 20; }
# summon_redstone X Z TOTAL: whole stacks of 64, in the gap block at y=101.
summon_redstone() {
    local left=$3 stack
    while [ "$left" -gt 0 ]; do
        stack=$(( left > 64 ? 64 : left ))
        ctrl command "summon minecraft:item $1.5 101.0 $2.5 {Item:{id:\"minecraft:redstone\",count:$stack}}" >/dev/null
        left=$(( left - stack ))
    done
    ctrl wait 6
}
# click_obsidian X Z: left-click the press's obsidian until it is gone (a cancelled click removes
# it; a plain mine breaks it). `break` waits for that, so it returns as soon as the block is air.
click_obsidian() { ctrl break "$1" 102 "$2" >/dev/null 2>&1 || true; ctrl wait 20; }

echo "== creating flux: a stone base is refused (the differential) =="
stand_by 6 6
press_base 6 6 minecraft:stone
summon_redstone 6 6 8
before_dust=$(dust)
echo "  redstone in the gap: $(redstone_items 6 6) in $(redstone_stacks 6 6) stack(s); dust $before_dust"
click_obsidian 6 6
after_dust=$(dust)
gap=$(block_name 6 101 6)
echo "  after the click: obsidian at 6,102,6 is '$(block_name 6 102 6)', gap '$gap', redstone left $(redstone_items 6 6), dust $after_dust"
[ "$(redstone_items 6 6)" = 8 ] || fail "a stone base consumed the redstone ($(redstone_items 6 6) left)"
[ "$after_dust" = "$before_dust" ] || fail "a stone base made flux dust ($before_dust -> $after_dust)"
[ "$(block_name 6 102 6)" = "minecraft:air" ] || fail "the stone-based obsidian was not mined, so the click was cancelled anyway"
if [ "$SELFTEST" = 1 ]; then
    echo "  SELFTEST: expecting the stone base to have eaten the redstone (this must fail)"
    [ "$(redstone_items 6 6)" != 8 ] || fail "SELFTEST flipped: the redstone survived a stone base, as it should"
fi
# This control keeps its pile on purpose; the later phases count their own press only.
clear_at 6 6

echo "== creating flux: 8 redstone between bedrock and obsidian =="
stand_by 8 8
press_base 8 8 minecraft:bedrock
summon_redstone 8 8 8
before_dust=$(dust)
click_obsidian 8 8
left=$(redstone_items 8 8)
gap=$(block_name 8 101 8)
echo "  redstone left $left; obsidian at 8,102,8 is '$(block_name 8 102 8)'; gap '$gap'"
[ "$left" = 0 ] || fail "the redstone was not consumed ($left left)"
[ "$(block_name 8 102 8)" = "minecraft:air" ] || fail "the obsidian was not consumed"
case "$gap" in
    minecraft:cobblestone | minecraft:obsidian) ;;
    *) fail "the gap below became '$gap', not cobblestone or the obsidian" ;;
esac
collect 8 8
after_dust=$(dust)
echo "  dust $before_dust -> $after_dust"
[ $(( after_dust - before_dust )) -eq 8 ] || fail "8 redstone made $(( after_dust - before_dust )) flux dust"

echo "== creating flux: no redstone, nothing happens =="
stand_by 10 10
press_base 10 10 minecraft:bedrock
before_dust=$(dust)
click_obsidian 10 10
after_dust=$(dust)
echo "  dust $before_dust -> $after_dust; gap '$(block_name 10 101 10)'; redstone $(redstone_items 10 10)"
[ "$after_dust" = "$before_dust" ] || fail "an empty press made flux dust"
[ "$(block_name 10 101 10)" = "minecraft:air" ] || fail "an empty press wrote a block into the gap"

echo "== creating flux: a flux block is the other base =="
stand_by 12 12
press_base 12 12 fluxnetworks:flux_block
summon_redstone 12 12 8
before_dust=$(dust)
click_obsidian 12 12
[ "$(redstone_items 12 12)" = 0 ] || fail "a flux-block press did not consume its redstone"
collect 12 12
after_dust=$(dust)
echo "  dust $before_dust -> $after_dust; gap '$(block_name 12 101 12)'"
[ $(( after_dust - before_dust )) -eq 8 ] || fail "a flux-block base made $(( after_dust - before_dust )) flux dust"

echo "== creating flux: 576 redstone gives 512 dust and leaves a stack =="
stand_by 14 14
press_base 14 14 minecraft:bedrock
summon_redstone 14 14 576
before_dust=$(dust)
echo "  redstone in the gap: $(redstone_items 14 14) in $(redstone_stacks 14 14) stacks"
click_obsidian 14 14
left=$(redstone_near)
stacks=$(redstone_near_stacks)
echo "  redstone left $left in $stacks stack(s)"
[ "$left" = 64 ] || fail "the cap left $left redstone, not the ninth stack"
[ "$stacks" = 1 ] || fail "the cap left $stacks stacks, not one"
collect 14 14
after_dust=$(dust)
echo "  dust $before_dust -> $after_dust"
[ $(( after_dust - before_dust )) -eq 512 ] || fail "576 redstone made $(( after_dust - before_dust )) dust, not the 512 cap"

echo "== the seven carried shaped recipes, in a real crafting table =="
place 17 100 10 minecraft:crafting_table
python3 "$PORT/tools/craft_check.py" "$SOCK" "$CTL" 17 100 10 "$PORT" \
    || fail "a carried recipe did not craft (see the rows above)"

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

echo "PASS: redstone between bedrock or a flux block and obsidian becomes flux dust on a left-click (a stone base is refused, an empty press does nothing, 576 redstone gives 512 dust and leaves a stack), all seven carried shaped recipes craft with their declared id and count and consume their ingredients (two wrong patterns stay empty), an unlinked network moved nothing; joined, the plug drained the source at 120,000 FE/t, the network carried it two blocks to the point, the point filled the receiver, FE was conserved, a point on another network carried nothing, and a full receiver refused more"
