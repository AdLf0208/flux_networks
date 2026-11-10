// Game-side glue for Flux Networks slice 1: one energy network.
//
// Injected into `crate::game` as `fluxnetworks_state`; it needs the game, so it
// is not part of the standalone library (only `runtime.rs` is).
//
// A flux device stores its network id, its priority and its transfer limit in
// its block entity's saved compound, as upstream's `TileFluxDevice` does
// (`network_id`, `priority`, `surge_mode`, `limit`), and its energy in the same
// compound's int `energy`, which API 142 exposes as a real
// `Capabilities.Energy.BLOCK` buffer.
//
// The network is the set of loaded devices of one level with the same
// `network_id`; upstream's `ServerFluxNetwork` keeps that set in memory. Here
// each device queues its position on its own tick, and one pass at
// `level_tick_post` (API 23) reads them back and runs upstream's cycle:
// plugs pull from the block beside them, the cycle moves energy from plug
// buffers to point buffers by priority, and points push into the block beside
// them. Every move is `native_move_energy` (API 142), so no energy is created
// or lost on the way, and the flow is bounded by each device's limit.
//
// Unlike upstream, which refuses every `IFluxDevice` as an outside energy
// block, this port treats a device of *another* network as an outside block:
// it is the only energy block a one-bundle world has, and the network id is the
// boundary the slice models. Recorded in notes.md.
//
// A block entity is out of the world during its own tick, so no device can read
// a neighbour's buffer there: the reads and moves all run at `level_tick_post`.

use std::sync::Mutex;

use horizon_client::containers::{BlockEntity, ModBlockEntity, ModEnergyBuffer};
use horizon_client::text::Text;
use horizon_core::geometry::Direction;
use horizon_world::nbt::{Compound, JavaString, Tag};

use crate::game::fluxnetworks::{
    BASIC_CAPACITY, BASIC_TRANSFER, DEFAULT_LIMIT, Device, DeviceKind, GARGANTUAN_CAPACITY,
    GARGANTUAN_TRANSFER, HERCULEAN_CAPACITY, HERCULEAN_TRANSFER, INVALID_NETWORK,
    comparator_strength, plan_cycle, plug_pull, point_push, pull_cap,
};
use crate::game::native_server::{NativeCommand, NativeServerPlayer};

const PLUG: &str = "fluxnetworks:flux_plug";
const POINT: &str = "fluxnetworks:flux_point";
const BASIC_STORAGE: &str = "fluxnetworks:basic_flux_storage";
const HERCULEAN_STORAGE: &str = "fluxnetworks:herculean_flux_storage";
const GARGANTUAN_STORAGE: &str = "fluxnetworks:gargantuan_flux_storage";

/// The device a block id is, or `None` for the decorative Flux Block and the
/// (not yet ported) Flux Controller.
fn device_kind(block: &str) -> Option<DeviceKind> {
    match block {
        PLUG => Some(DeviceKind::Plug),
        POINT => Some(DeviceKind::Point),
        BASIC_STORAGE | HERCULEAN_STORAGE | GARGANTUAN_STORAGE => Some(DeviceKind::Storage),
        _ => None,
    }
}

/// A block entity's kind, from the id it was created with.
fn entity_kind(entity: &ModBlockEntity) -> Option<DeviceKind> {
    device_kind(&entity.id)
}

fn nbt_key(name: &str) -> JavaString {
    JavaString::from(name)
}

fn read_int(nbt: &Compound, key: &str) -> i32 {
    match nbt.get(&nbt_key(key)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

/// The int in `nbt`, or `absent` when the key is not there: a device that was
/// never put on a network has no `network_id`, and upstream's default is -1.
fn read_int_or(nbt: &Compound, key: &str, absent: i32) -> i32 {
    match nbt.get(&nbt_key(key)) {
        Some(Tag::Int(value)) => *value,
        _ => absent,
    }
}

fn read_flag(nbt: &Compound, key: &str) -> bool {
    match nbt.get(&nbt_key(key)) {
        Some(Tag::Byte(value)) => *value != 0,
        _ => false,
    }
}

fn write_int(nbt: &mut Compound, key: &str, value: i32) {
    nbt.insert(nbt_key(key), Tag::Int(value));
}

/// `FluxStorageHandler.Basic/Herculean/Gargantuan`: capacity and transfer.
fn storage_sizes(block: &str) -> (i32, i32) {
    match block {
        HERCULEAN_STORAGE => (HERCULEAN_CAPACITY, HERCULEAN_TRANSFER),
        GARGANTUAN_STORAGE => (GARGANTUAN_CAPACITY, GARGANTUAN_TRANSFER),
        _ => (BASIC_CAPACITY, BASIC_TRANSFER),
    }
}

/// The buffer a device's block id declares.
fn buffer_of(block: &str) -> Option<ModEnergyBuffer> {
    let kind = device_kind(block)?;
    let (capacity, transfer) = match kind {
        DeviceKind::Plug | DeviceKind::Point => (DEFAULT_LIMIT, DEFAULT_LIMIT),
        DeviceKind::Storage => storage_sizes(block),
    };
    Some(ModEnergyBuffer {
        capacity,
        max_insert: transfer,
        max_extract: transfer,
        insert_faces: [true; 6],
        extract_faces: [true; 6],
    })
}

/// The five blocks have a block entity (`native_block_entity`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if device_kind(block).is_some() {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

/// ... and tick (`native_block_entity_ticks`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity_ticks(block: &str, ticks: &mut bool) {
    if device_kind(block).is_some() {
        *ticks = true;
    }
}

/// The declared FE buffer (`native_block_entity_energy`, API 142).
#[allow(private_interfaces)]
pub fn on_block_entity_energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    *energy = buffer_of(block);
}

/// One device, read from its saved compound. The capacity and the default
/// transfer are properties of the block (upstream reads them from the config),
/// so only the settings a player changes are stored.
fn device_from(entity: &ModBlockEntity) -> Option<Device> {
    let kind = entity_kind(entity)?;
    let (capacity, transfer) = match kind {
        DeviceKind::Storage => storage_sizes(&entity.id),
        DeviceKind::Plug | DeviceKind::Point => (DEFAULT_LIMIT, DEFAULT_LIMIT),
    };
    let mut device = Device::new(
        kind,
        read_int_or(&entity.nbt, "network_id", INVALID_NETWORK),
    );
    device.capacity = capacity;
    device.priority = read_int(&entity.nbt, "priority");
    device.surge = read_flag(&entity.nbt, "surge_mode");
    device.limit = match entity.nbt.get(&nbt_key("limit")) {
        Some(Tag::Int(limit)) => *limit,
        _ => transfer,
    };
    device.buffer = read_int(&entity.nbt, "energy");
    device.clamp();
    Some(device)
}

/// Positions queued for this tick's network pass, by dimension.
static QUEUE: Mutex<Vec<(String, [i32; 3])>> = Mutex::new(Vec::new());

/// `ServerFluxNetwork.mBufferLimiter`: what each network asked for in its last
/// cycle, so a plug does not pull energy nothing wants.
static LIMITER: Mutex<Vec<(String, i32, i32)>> = Mutex::new(Vec::new());

fn limiter_of(dimension: &str, network: i32) -> i32 {
    let limiters = LIMITER.lock().unwrap_or_else(|poison| poison.into_inner());
    limiters
        .iter()
        .find(|(dim, id, _)| dim == dimension && *id == network)
        .map_or(0, |(_, _, value)| *value)
}

fn set_limiter(dimension: &str, network: i32, value: i32) {
    let mut limiters = LIMITER.lock().unwrap_or_else(|poison| poison.into_inner());
    match limiters
        .iter_mut()
        .find(|(dim, id, _)| dim == dimension && *id == network)
    {
        Some((_, _, stored)) => *stored = value,
        None => limiters.push((dimension.to_owned(), network, value)),
    }
}

fn queue(game: &crate::game::Game<'_>, pos: [i32; 3]) {
    let dimension = game.native_dimension().to_owned();
    let mut queue = QUEUE.lock().unwrap_or_else(|poison| poison.into_inner());
    if !queue
        .iter()
        .any(|(dim, at)| dim == &dimension && *at == pos)
    {
        queue.push((dimension, pos));
    }
}

fn offset(pos: [i32; 3], side: Direction) -> [i32; 3] {
    let delta = side.normal();
    [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]]
}

/// One server tick of a flux device (`native_block_entity_tick`, API 17): the
/// device only says it is here. A device that joined no network still queues,
/// so it is visible to a `network` command, but it takes no part in a cycle.
#[allow(private_interfaces)]
pub fn on_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
) {
    if entity_kind(entity).is_some() {
        queue(game, pos);
    }
}

/// Is the block at `pos` a device of this same network? Those are never an
/// outside energy block for the network's own plug or point.
fn is_same_network(game: &crate::game::Game<'_>, pos: [i32; 3], network: i32) -> bool {
    let key = (pos[0], pos[1], pos[2]);
    match game.world.block_entities.get(&key) {
        Some(BlockEntity::Mod(m)) if entity_kind(m).is_some() => {
            read_int(&m.nbt, "network_id") == network
        }
        _ => false,
    }
}

/// After a level's tick: every device queues itself, and one pass runs every
/// network of that level (`native_level_tick_post`, API 23).
#[allow(private_interfaces)]
pub fn on_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    let queued = {
        let mut queue = QUEUE.lock().unwrap_or_else(|poison| poison.into_inner());
        std::mem::take(&mut *queue)
    };
    if queued.is_empty() {
        return;
    }
    let mut later = Vec::new();
    let mut members: Vec<([i32; 3], Device)> = Vec::new();
    for (dim, pos) in queued {
        if dim != dimension {
            later.push((dim, pos));
            continue;
        }
        let key = (pos[0], pos[1], pos[2]);
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&key)
            && let Some(device) = device_from(entity)
        {
            members.push((pos, device));
        }
    }
    if !later.is_empty() {
        QUEUE
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend(later);
    }

    let mut networks: Vec<i32> = members
        .iter()
        .map(|(_, device)| device.network)
        .filter(|network| *network != INVALID_NETWORK)
        .collect();
    networks.sort_unstable();
    networks.dedup();
    for network in networks {
        run_network(game, dimension, network, &mut members);
    }
}

/// One network's cycle, for the devices of `members` whose id is `network`.
fn run_network(
    game: &mut crate::game::Game<'_>,
    dimension: &str,
    network: i32,
    members: &mut [([i32; 3], Device)],
) {
    let indices: Vec<usize> = members
        .iter()
        .enumerate()
        .filter(|(_, (_, device))| device.network == network)
        .map(|(index, _)| index)
        .collect();
    if indices.len() < 2 {
        return;
    }
    // Another network of this level may have run before this one and moved
    // energy through a device of this one (a device of another network is an
    // outside block), so read the buffers again before planning.
    for &index in &indices {
        let key = (
            members[index].0[0],
            members[index].0[1],
            members[index].0[2],
        );
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&key)
            && let Some(device) = device_from(entity)
        {
            members[index].1 = device;
        }
    }

    // 1. Every plug pulls from the outside blocks beside it, up to its limit
    //    and what the network asked for last cycle (upstream's `mBufferLimiter`).
    let limiter = limiter_of(dimension, network);
    for &index in &indices {
        if members[index].1.kind != DeviceKind::Plug {
            continue;
        }
        let pos = members[index].0;
        let cap = pull_cap(members[index].1.limit, limiter);
        let mut pulled = 0;
        for side in Direction::ALL {
            let neighbour = offset(pos, side);
            if is_same_network(game, neighbour, network) {
                continue;
            }
            let Some(target) = game.native_energy(dimension, neighbour, side.opposite()) else {
                continue;
            };
            if !target.can_extract {
                continue;
            }
            let offer = target.amount.min(target.max_extract);
            let amount = plug_pull(members[index].1.buffer, cap, offer, pulled);
            if amount == 0 {
                continue;
            }
            let moved =
                game.native_move_energy(dimension, neighbour, side.opposite(), pos, side, amount);
            pulled += moved;
            members[index].1.buffer = members[index].1.buffer.saturating_add(moved);
        }
    }

    // 2. Every point says what the blocks beside it can take this cycle.
    for &index in &indices {
        if members[index].1.kind != DeviceKind::Point {
            continue;
        }
        let pos = members[index].0;
        let mut desired = 0_i32;
        for side in Direction::ALL {
            let neighbour = offset(pos, side);
            if is_same_network(game, neighbour, network) {
                continue;
            }
            let Some(target) = game.native_energy(dimension, neighbour, side.opposite()) else {
                continue;
            };
            if !target.can_insert {
                continue;
            }
            desired = desired.saturating_add(
                target
                    .capacity
                    .saturating_sub(target.amount)
                    .min(target.max_insert)
                    .max(0),
            );
        }
        members[index].1.desired = desired.min(members[index].1.limit);
    }

    // 3. Upstream's cycle: plugs in priority order feed the points that ask.
    let devices: Vec<Device> = indices
        .iter()
        .map(|&index| members[index].1.clone())
        .collect();
    for transfer in plan_cycle(&devices) {
        let from = members[indices[transfer.from]].0;
        let to = members[indices[transfer.to]].0;
        let moved = game.native_move_energy(
            dimension,
            from,
            Direction::Up,
            to,
            Direction::Up,
            transfer.amount,
        );
        members[indices[transfer.from]].1.buffer -= moved;
        members[indices[transfer.to]].1.buffer += moved;
    }

    // 4. Every point sends what it holds into the blocks beside it.
    for &index in &indices {
        if members[index].1.kind != DeviceKind::Point {
            continue;
        }
        let pos = members[index].0;
        let mut pushed = 0;
        for side in Direction::ALL {
            let neighbour = offset(pos, side);
            if is_same_network(game, neighbour, network) {
                continue;
            }
            let Some(target) = game.native_energy(dimension, neighbour, side.opposite()) else {
                continue;
            };
            if !target.can_insert {
                continue;
            }
            let room = target
                .capacity
                .saturating_sub(target.amount)
                .min(target.max_insert)
                .max(0);
            let amount = point_push(
                members[index].1.buffer,
                members[index].1.limit,
                room,
                pushed,
            );
            if amount == 0 {
                continue;
            }
            let moved =
                game.native_move_energy(dimension, pos, side, neighbour, side.opposite(), amount);
            pushed += moved;
            members[index].1.buffer = members[index].1.buffer.saturating_sub(moved);
        }
    }

    // 5. What this network still wants, for the next cycle's pull.
    let wanted = indices
        .iter()
        .map(|&index| {
            let mut device = members[index].1.clone();
            device.clamp();
            device.request()
        })
        .sum();
    set_limiter(dimension, network, wanted);
}

/// `/fluxnetworks` (`register_commands`, API 8): a readout and the harness
/// controls, because slice 1 has no configurator GUI.
#[allow(private_interfaces)]
pub fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("fluxnetworks info <pos:block_pos>"));
    commands.push(NativeCommand::new(
        "fluxnetworks energy <amount:integer> <pos:block_pos>",
    ));
    commands.push(NativeCommand::new(
        "fluxnetworks network <id:integer> <pos:block_pos>",
    ));
    commands.push(NativeCommand::new(
        "fluxnetworks priority <priority:integer> <pos:block_pos>",
    ));
}

fn parse_pos(words: &[&str]) -> Option<[i32; 3]> {
    Some([
        words.first()?.parse().ok()?,
        words.get(1)?.parse().ok()?,
        words.get(2)?.parse().ok()?,
    ])
}

fn kind_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Plug => "Flux Plug",
        DeviceKind::Point => "Flux Point",
        DeviceKind::Storage => "Flux Storage",
    }
}

/// A device's status line. `t=` is the world game time, so the e2e can tell a
/// fresh reply from a stale one.
fn status_text(game: &crate::game::Game<'_>, pos: [i32; 3]) -> String {
    let key = (pos[0], pos[1], pos[2]);
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&key) else {
        return "No Flux Networks device there.".to_owned();
    };
    let Some(device) = device_from(entity) else {
        if entity.id == "fluxnetworks:flux_controller" {
            return "Flux Controller: carried, no network behaviour in slice 1.".to_owned();
        }
        return "No Flux Networks device there.".to_owned();
    };
    let network = if device.network == INVALID_NETWORK {
        "none".to_owned()
    } else {
        device.network.to_string()
    };
    let extra = match device.kind {
        DeviceKind::Point => format!(", wants {}", device.desired),
        DeviceKind::Plug | DeviceKind::Storage => String::new(),
    };
    format!(
        "{} at {},{},{}: {}/{} FE (comparator {}, network {}, limit {} FE/t, priority {}{}, t={})",
        kind_name(device.kind),
        pos[0],
        pos[1],
        pos[2],
        device.buffer,
        device.capacity,
        comparator_strength(device.buffer, device.capacity),
        network,
        device.limit,
        device.effective_priority(),
        extra,
        game.world.game_time
    )
}

/// The `/fluxnetworks` bodies (`perform_command`, API 8).
#[allow(private_interfaces)]
pub fn on_perform_command(
    game: &mut crate::game::Game<'_>,
    _player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["fluxnetworks", "info", x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal("Usage: /fluxnetworks info <x> <y> <z>"));
                return;
            };
            game.native_feedback(Text::literal(status_text(game, pos)));
        }
        ["fluxnetworks", "energy", amount, x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal(
                    "Usage: /fluxnetworks energy <amount> <x> <y> <z>",
                ));
                return;
            };
            let Ok(amount) = amount.parse::<i32>() else {
                game.native_feedback(Text::literal("The amount must be a whole number of FE."));
                return;
            };
            let set = game.native_with_mod_block_entity(pos, |entity| {
                let device = device_from(entity)?;
                let after = amount.clamp(0, device.capacity);
                write_int(&mut entity.nbt, "energy", after);
                Some(after)
            });
            match set {
                Some(Some(after)) => {
                    game.native_feedback(Text::literal(format!("Energy set to {after} FE.")));
                }
                _ => game.native_feedback(Text::literal("No Flux Networks device there.")),
            }
        }
        ["fluxnetworks", "network", id, x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal(
                    "Usage: /fluxnetworks network <id> <x> <y> <z>",
                ));
                return;
            };
            let Ok(id) = id.parse::<i32>() else {
                game.native_feedback(Text::literal("The network id must be a whole number."));
                return;
            };
            let set = game.native_with_mod_block_entity(pos, |entity| {
                device_kind(&entity.id)?;
                write_int(&mut entity.nbt, "network_id", id);
                Some(())
            });
            match set {
                Some(Some(())) => {
                    game.native_feedback(Text::literal(format!("Network id set to {id}.")));
                }
                _ => game.native_feedback(Text::literal("No Flux Networks device there.")),
            }
        }
        ["fluxnetworks", "priority", priority, x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal(
                    "Usage: /fluxnetworks priority <priority> <x> <y> <z>",
                ));
                return;
            };
            let Ok(priority) = priority.parse::<i32>() else {
                game.native_feedback(Text::literal("The priority must be a whole number."));
                return;
            };
            let set = game.native_with_mod_block_entity(pos, |entity| {
                device_kind(&entity.id)?;
                write_int(
                    &mut entity.nbt,
                    "priority",
                    crate::game::fluxnetworks::clamp_priority(priority),
                );
                Some(())
            });
            match set {
                Some(Some(())) => {
                    game.native_feedback(Text::literal(format!("Priority set to {priority}.")));
                }
                _ => game.native_feedback(Text::literal("No Flux Networks device there.")),
            }
        }
        _ => {}
    }
}

/// A world opened or closed: the queued positions are per world.
#[allow(private_interfaces)]
pub fn on_world_lifecycle(
    _game: &mut crate::game::Game<'_>,
    _phase: crate::game::native_lifecycle::WorldPhase,
) {
    QUEUE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
    LIMITER
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}
