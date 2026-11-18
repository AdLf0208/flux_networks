//! Flux Networks for Horizon — one energy network over five blocks.
//!
//! A flux device keeps its network id, its priority and its transfer limit in its block entity's
//! saved compound, as upstream's `TileFluxDevice` does (`network_id`, `priority`, `surge_mode`,
//! `limit`), and its energy in the same compound's int `energy`, which the SDK's
//! `Registrar::block_energy` exposes as a real `horizon:energy` buffer (the amount lives there,
//! under `energy`, so the buffer survives restarts and this tick reads it in the same place).
//!
//! The network is the set of loaded devices of one level with the same `network_id`; upstream's
//! `ServerFluxNetwork` keeps that set in memory. Here each device queues its position on its own
//! tick, and one pass at `level_tick_post` reads them back and runs upstream's cycle: plugs pull
//! from the block beside them, the cycle moves energy from plug buffers to point buffers by
//! priority, and points push into the block beside them. Every move is `move_energy` over the
//! `ENERGY` capability, so no energy is created or lost on the way, and the flow is bounded by
//! each device's limit.
//!
//! Unlike upstream, which refuses every `IFluxDevice` as an outside energy block, this port treats
//! a device of *another* network as an outside block: it is the only energy block a one-bundle
//! world has, and the network id is the boundary the slice models. Recorded in notes.md.
//!
//! A block entity is out of the world during its own tick, so no device can read a neighbour's
//! buffer there: the reads and moves all run at `level_tick_post`.

pub mod runtime;

use std::sync::Mutex;

use mod_api::{
    self, BlockEntityEvent, BlockEntityPart, BlockEntityTickEvent, BlockEntityTickReply,
    BlockEntityTicksEvent, BlockPart, Command, CommandArgumentValue, CommandInvocation, Compound,
    Direction, ENERGY, Flow, Hooks, Host, HostSide, HostTransaction, LevelRef, LevelSide,
    LevelTickPostEvent, ModEnergyBuffer, RegisterCommandsEvent, Registrar, RegistrationContext,
    RunModCommandEvent, ServerHost, Tag, Text, WorldLifecycleEvent, move_energy,
};

use crate::runtime::{
    BASIC_CAPACITY, BASIC_TRANSFER, DEFAULT_LIMIT, Device, DeviceKind, GARGANTUAN_CAPACITY,
    GARGANTUAN_TRANSFER, HERCULEAN_CAPACITY, HERCULEAN_TRANSFER, INVALID_NETWORK, clamp_priority,
    comparator_strength, plan_cycle, plug_pull, point_push, pull_cap,
};

/// The five devices (`RegistryBlocks` in the jar): a plug, a point and the three storages.
const PLUG: &str = "fluxnetworks:flux_plug";
const POINT: &str = "fluxnetworks:flux_point";
const BASIC_STORAGE: &str = "fluxnetworks:basic_flux_storage";
const HERCULEAN_STORAGE: &str = "fluxnetworks:herculean_flux_storage";
const GARGANTUAN_STORAGE: &str = "fluxnetworks:gargantuan_flux_storage";
/// The carried but inert controller, named by the readout so a player knows why it does nothing.
const CONTROLLER: &str = "fluxnetworks:flux_controller";

/// Answer of the readout when the position holds no device (byte-identical to 0.1.0, which the
/// e2e asserts on).
const NO_DEVICE: &str = "No Flux Networks device there.";
/// The controller's answer: carried, no behaviour in this slice.
const CONTROLLER_LINE: &str = "Flux Controller: carried, no network behaviour in slice 1.";

/// The five blocks this package gives a block entity and an energy buffer, with the buffer's
/// capacity and per-call insert/extract: a plug's and a point's one cycle's limit, a storage its
/// tier's capacity and transfer.
const DEVICES: [(&str, i32, i32); 5] = [
    (PLUG, DEFAULT_LIMIT, DEFAULT_LIMIT),
    (POINT, DEFAULT_LIMIT, DEFAULT_LIMIT),
    (BASIC_STORAGE, BASIC_CAPACITY, BASIC_TRANSFER),
    (HERCULEAN_STORAGE, HERCULEAN_CAPACITY, HERCULEAN_TRANSFER),
    (GARGANTUAN_STORAGE, GARGANTUAN_CAPACITY, GARGANTUAN_TRANSFER),
];

/// The device a block id is, or `None` for the decorative Flux Block and the carried controller.
fn device_kind(block: &str) -> Option<DeviceKind> {
    match block {
        PLUG => Some(DeviceKind::Plug),
        POINT => Some(DeviceKind::Point),
        BASIC_STORAGE | HERCULEAN_STORAGE | GARGANTUAN_STORAGE => Some(DeviceKind::Storage),
        _ => None,
    }
}

/// `FluxStorageHandler.Basic/Herculean/Gargantuan`: capacity and transfer.
fn storage_sizes(block: &str) -> (i32, i32) {
    match block {
        HERCULEAN_STORAGE => (HERCULEAN_CAPACITY, HERCULEAN_TRANSFER),
        GARGANTUAN_STORAGE => (GARGANTUAN_CAPACITY, GARGANTUAN_TRANSFER),
        _ => (BASIC_CAPACITY, BASIC_TRANSFER),
    }
}

/// The buffer a device's block id declares, or `None` for a block that is not one of ours.
///
/// This is a per-block declaration (`Registrar::block_energy`) rather than the retired
/// `block_entity_energy` hook, so a block another package declares is never touched: the old
/// helper had to avoid writing `None` for a foreign block.
fn buffer_of(block: &str) -> Option<ModEnergyBuffer> {
    let kind = device_kind(block)?;
    let (capacity, transfer) = match kind {
        DeviceKind::Storage => storage_sizes(block),
        DeviceKind::Plug | DeviceKind::Point => (DEFAULT_LIMIT, DEFAULT_LIMIT),
    };
    Some(ModEnergyBuffer {
        capacity,
        max_insert: transfer,
        max_extract: transfer,
        insert_faces: [true; 6],
        extract_faces: [true; 6],
    })
}

/// `FluxConfig`'s numbers as the readout names them.
fn kind_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Plug => "Flux Plug",
        DeviceKind::Point => "Flux Point",
        DeviceKind::Storage => "Flux Storage",
    }
}

fn read_int(nbt: &Compound, key: &str) -> i32 {
    match nbt.get(&key.into()) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

/// The int in `nbt`, or `absent` when the key is not there: a device that was never put on a
/// network has no `network_id`, and upstream's default is -1.
fn read_int_or(nbt: &Compound, key: &str, absent: i32) -> i32 {
    match nbt.get(&key.into()) {
        Some(Tag::Int(value)) => *value,
        _ => absent,
    }
}

fn read_flag(nbt: &Compound, key: &str) -> bool {
    match nbt.get(&key.into()) {
        Some(Tag::Byte(value)) => *value != 0,
        _ => false,
    }
}

/// One device, read from its block id and its saved compound. The capacity and the default
/// transfer are properties of the block (upstream reads them from the config), so only the
/// settings a player changes are stored.
fn device_from(block: &str, nbt: &Compound) -> Option<Device> {
    let kind = device_kind(block)?;
    let (capacity, transfer) = match kind {
        DeviceKind::Storage => storage_sizes(block),
        DeviceKind::Plug | DeviceKind::Point => (DEFAULT_LIMIT, DEFAULT_LIMIT),
    };
    let mut device = Device::new(kind, read_int_or(nbt, "network_id", INVALID_NETWORK));
    device.capacity = capacity;
    device.priority = read_int(nbt, "priority");
    device.surge = read_flag(nbt, "surge_mode");
    device.limit = read_int_or(nbt, "limit", transfer);
    device.buffer = read_int(nbt, "energy");
    device.clamp();
    Some(device)
}

/// A device's status line. `t=` is the world game time, so the e2e can tell a fresh reply from a
/// stale one. Byte-identical to 0.1.0's readout, which the e2e parses.
fn status_line(device: &Device, pos: [i32; 3], time: i64) -> String {
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
        time
    )
}

/// The subcommand a literal path names, or `None` for a command that is not ours.
fn subcommand(literals: &[String]) -> Option<Subcommand> {
    if literals.first().map(String::as_str) != Some("fluxnetworks") {
        return None;
    }
    match literals.get(1).map(String::as_str)? {
        "info" => Some(Subcommand::Info),
        "energy" => Some(Subcommand::Energy),
        "network" => Some(Subcommand::Network),
        "priority" => Some(Subcommand::Priority),
        _ => None,
    }
}

/// The four usages `/fluxnetworks` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subcommand {
    /// Read a device back.
    Info,
    /// Set a device's buffer (`/fluxnetworks energy`), a harness control and a repair tool.
    Energy,
    /// Put a device on a network (upstream's configurator does this in its GUI).
    Network,
    /// Set a device's priority (also the configurator's).
    Priority,
}

/// Positions queued for this tick's network pass, by level.
static QUEUE: Mutex<Vec<(LevelRef, [i32; 3])>> = Mutex::new(Vec::new());

/// `ServerFluxNetwork.mBufferLimiter`: what each network asked for in its last cycle, so a plug
/// does not pull energy nothing wants.
static LIMITER: Mutex<Vec<(LevelRef, i32, i32)>> = Mutex::new(Vec::new());

fn limiter_of(level: LevelRef, network: i32) -> i32 {
    let limiters = LIMITER
        .lock()
        .expect("the limiter lock is held for one read");
    limiters
        .iter()
        .find(|(queued_level, id, _)| *queued_level == level && *id == network)
        .map_or(0, |(_, _, value)| *value)
}

fn set_limiter(level: LevelRef, network: i32, value: i32) {
    let mut limiters = LIMITER
        .lock()
        .expect("the limiter lock is held for one write");
    match limiters
        .iter_mut()
        .find(|(queued_level, id, _)| *queued_level == level && *id == network)
    {
        Some((_, _, stored)) => *stored = value,
        None => limiters.push((level, network, value)),
    }
}

fn queue(level: LevelRef, pos: [i32; 3]) {
    let mut queue = QUEUE.lock().expect("the queue lock is held for one push");
    if !queue
        .iter()
        .any(|(queued, at)| *queued == level && *at == pos)
    {
        queue.push((level, pos));
    }
}

/// The mod has no settings; the only state is the two per-world tables above.
#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Declare the five devices' block entity types and energy buffers. A type is declared per
    /// block so the entity's id is the block's id, as the retired `block_entity` hook made it.
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        for (block, _, _) in DEVICES {
            let key = r.block_key(block);
            let Some(buffer) = buffer_of(block) else {
                continue;
            };
            r.block_energy(&key, buffer);
            let kind = r.block_entity_type(block).blocks(&[block]).build();
            // The type was declared on the line above; a refusal here is a programming error.
            r.block_entity_part(&kind, DevicePart)
                .expect("a block entity type this mod declares takes its own part");
        }
    }

    /// `/fluxnetworks info|energy|network|priority` (upstream's configurator, as commands).
    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event
            .commands
            .push(Command::new("fluxnetworks info <pos:block_pos>"));
        event.commands.push(Command::new(
            "fluxnetworks energy <amount:integer> <pos:block_pos>",
        ));
        event.commands.push(Command::new(
            "fluxnetworks network <id:integer> <pos:block_pos>",
        ));
        event.commands.push(Command::new(
            "fluxnetworks priority <priority:integer> <pos:block_pos>",
        ));
    }

    /// The `/fluxnetworks` bodies.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        let Some(subcommand) = subcommand(&event.command.literals) else {
            return Flow::Continue;
        };
        let Some(level) = level_of(host, event.command) else {
            feedback(host, NO_DEVICE);
            return Flow::Cancel;
        };
        match subcommand {
            Subcommand::Info => {
                let Some(pos) = position_argument(event.command.argument("pos")) else {
                    feedback(host, "Usage: /fluxnetworks info <x> <y> <z>");
                    return Flow::Cancel;
                };
                let line = readout(host, level, pos);
                feedback(host, &line);
            }
            Subcommand::Energy => {
                let Some(pos) = position_argument(event.command.argument("pos")) else {
                    feedback(host, "Usage: /fluxnetworks energy <amount> <x> <y> <z>");
                    return Flow::Cancel;
                };
                let Some(amount) = integer_argument(event.command.argument("amount")) else {
                    feedback(host, "The amount must be a whole number of FE.");
                    return Flow::Cancel;
                };
                match set_energy(host, level, pos, amount) {
                    Some(after) => feedback(host, &format!("Energy set to {after} FE.")),
                    None => feedback(host, NO_DEVICE),
                }
            }
            Subcommand::Network => {
                let Some(pos) = position_argument(event.command.argument("pos")) else {
                    feedback(host, "Usage: /fluxnetworks network <id> <x> <y> <z>");
                    return Flow::Cancel;
                };
                let Some(id) = integer_argument(event.command.argument("id")) else {
                    feedback(host, "The network id must be a whole number.");
                    return Flow::Cancel;
                };
                if set_device_int(host, level, pos, "network_id", id) {
                    feedback(host, &format!("Network id set to {id}."));
                } else {
                    feedback(host, NO_DEVICE);
                }
            }
            Subcommand::Priority => {
                let Some(pos) = position_argument(event.command.argument("pos")) else {
                    feedback(host, "Usage: /fluxnetworks priority <priority> <x> <y> <z>");
                    return Flow::Cancel;
                };
                let Some(priority) = integer_argument(event.command.argument("priority")) else {
                    feedback(host, "The priority must be a whole number.");
                    return Flow::Cancel;
                };
                if set_device_int(host, level, pos, "priority", clamp_priority(priority)) {
                    feedback(host, &format!("Priority set to {priority}."));
                } else {
                    feedback(host, NO_DEVICE);
                }
            }
        }
        Flow::Cancel
    }

    /// A world opened or closed: the queued positions and the last cycle's requests are per world.
    fn world_lifecycle(&self, _host: &mut dyn ServerHost, _event: &mut WorldLifecycleEvent<'_>) {
        QUEUE
            .lock()
            .expect("the queue lock is held for one clear")
            .clear();
        LIMITER
            .lock()
            .expect("the limiter lock is held for one clear")
            .clear();
    }

    /// After a level's tick: every network of that level runs one cycle. A block entity is out of
    /// the world during its own tick, so this is where the neighbour reads and the moves happen.
    fn level_tick_post(&self, host: HostSide, event: &mut LevelTickPostEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        run_networks(&mut *host, event.level);
    }
}

/// One device's block entity: it ticks, and it queues itself so the network pass can read it back.
struct DevicePart;

#[mod_api::horizon_part]
impl BlockPart for DevicePart {}

#[mod_api::horizon_part]
impl BlockEntityPart for DevicePart {
    /// A fresh device starts with an empty compound: every setting falls back to its default
    /// (`network_id` absent means [`INVALID_NETWORK`], `energy` 0), as 0.1.0's hook made it.
    fn block_entity(
        &self,
        _level: LevelSide,
        _event: &mut BlockEntityEvent<'_>,
    ) -> Option<Compound> {
        Some(Compound::new())
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        event.ticks = true;
    }

    /// A device only says it is here; the tick changes nothing in its own compound, so it answers
    /// `Unchanged` and does not dirty the chunk every tick.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let Some(level) = level_in_place(host) else {
            return;
        };
        queue(level, event.pos);
        *event.reply = BlockEntityTickReply::Unchanged;
    }
}

/// Every network of `level` runs one cycle, over the devices queued since their last tick.
fn run_networks(host: &mut dyn ServerHost, level: LevelRef) {
    let queued = {
        let mut queue = QUEUE.lock().expect("the queue lock is held for one take");
        std::mem::take(&mut *queue)
    };
    if queued.is_empty() {
        return;
    }
    let mut later = Vec::new();
    let mut members: Vec<([i32; 3], Device)> = Vec::new();
    for (queued_level, pos) in queued {
        if queued_level != level {
            later.push((queued_level, pos));
            continue;
        }
        if let Some(device) = device_at(host, level, pos) {
            members.push((pos, device));
        }
    }
    if !later.is_empty() {
        QUEUE
            .lock()
            .expect("the queue lock is held for one append")
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
        run_network(host, level, network, &mut members);
    }
}

/// One network's cycle, for the devices of `members` whose id is `network`.
fn run_network(
    host: &mut dyn ServerHost,
    level: LevelRef,
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
    // Another network of this level may have run before this one and moved energy through a
    // device of this one (a device of another network is an outside block), so read the buffers
    // again before planning.
    for &index in &indices {
        if let Some(device) = device_at(host, level, members[index].0) {
            members[index].1 = device;
        }
    }

    // 1. Every plug pulls from the outside blocks beside it, up to its limit and what the network
    //    asked for last cycle (upstream's `mBufferLimiter`).
    let limiter = limiter_of(level, network);
    for &index in &indices {
        if members[index].1.kind != DeviceKind::Plug {
            continue;
        }
        let pos = members[index].0;
        let cap = pull_cap(members[index].1.limit, limiter);
        let mut pulled = 0;
        for side in Direction::ALL {
            let neighbour = offset(pos, side);
            if is_same_network(host, level, neighbour, network) {
                continue;
            }
            let Some(offer) = energy_of(host, level, neighbour, side.opposite()) else {
                continue;
            };
            let amount = plug_pull(members[index].1.buffer, cap, offer, pulled);
            if amount == 0 {
                continue;
            }
            let moved = move_fe(
                host,
                level,
                (neighbour, side.opposite()),
                (pos, side),
                amount,
            );
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
            if is_same_network(host, level, neighbour, network) {
                continue;
            }
            let Some(room) = room_of(host, level, neighbour, side.opposite()) else {
                continue;
            };
            desired = desired.saturating_add(room);
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
        // Every device's buffer declares all six faces for a fill and a drain, so the face the
        // capability is asked through does not matter; 0.1.0 asked through `Up` as well.
        let moved = move_fe(
            host,
            level,
            (from, Direction::Up),
            (to, Direction::Up),
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
            if is_same_network(host, level, neighbour, network) {
                continue;
            }
            let Some(room) = room_of(host, level, neighbour, side.opposite()) else {
                continue;
            };
            let amount = point_push(
                members[index].1.buffer,
                members[index].1.limit,
                room,
                pushed,
            );
            if amount == 0 {
                continue;
            }
            let moved = move_fe(
                host,
                level,
                (pos, side),
                (neighbour, side.opposite()),
                amount,
            );
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
    set_limiter(level, network, wanted);
}

/// The FE the buffer at `pos`, reached through `side`, holds right now; `None` where the side
/// reaches no buffer.
fn energy_of(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    side: Direction,
) -> Option<i32> {
    let buffer = level.capability(host, &ENERGY, pos, Some(side))?;
    Some(i32::try_from(buffer.amount(host)).unwrap_or(i32::MAX))
}

/// The room the buffer at `pos`, reached through `side`, has left; `None` where the side reaches
/// no buffer.
fn room_of(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    side: Direction,
) -> Option<i32> {
    let buffer = level.capability(host, &ENERGY, pos, Some(side))?;
    let room = buffer.capacity(host).saturating_sub(buffer.amount(host));
    Some(i32::try_from(room).unwrap_or(i32::MAX))
}

/// Moves up to `max` FE out of the buffer at `from` into the one at `to`, in one transaction so a
/// refused insert leaves both untouched (`EnergyHandlerUtil.move`).
fn move_fe(
    host: &mut dyn ServerHost,
    level: LevelRef,
    from: ([i32; 3], Direction),
    to: ([i32; 3], Direction),
    max: i32,
) -> i32 {
    let Ok(max) = u32::try_from(max) else {
        return 0;
    };
    if max == 0 {
        return 0;
    }
    host.transaction(|host, tx| -> Result<u32, ()> {
        let Some(source) = level.capability(host, &ENERGY, from.0, Some(from.1)) else {
            return Ok(0);
        };
        let Some(target) = level.capability(host, &ENERGY, to.0, Some(to.1)) else {
            return Ok(0);
        };
        move_energy(host, &source, &target, max, tx).map_err(|_| ())
    })
    .unwrap_or(0)
    .min(max) as i32
}

fn offset(pos: [i32; 3], side: Direction) -> [i32; 3] {
    let delta = side.normal();
    [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]]
}

/// Is the block at `pos` a device of this same network? Those are never an outside energy block
/// for the network's own plug or point.
fn is_same_network(host: &dyn ServerHost, level: LevelRef, pos: [i32; 3], network: i32) -> bool {
    let Some(block) = block_at(host, level, pos) else {
        return false;
    };
    if device_kind(&block).is_none() {
        return false;
    }
    let Some(entity) = host.block_entity(level, pos) else {
        return false;
    };
    let Some(data) = entity.data(host) else {
        return false;
    };
    read_int(&data, "network_id") == network
}

/// The device at `pos`, read from its block id and its saved compound.
fn device_at(host: &dyn ServerHost, level: LevelRef, pos: [i32; 3]) -> Option<Device> {
    let block = block_at(host, level, pos)?;
    let entity = host.block_entity(level, pos)?;
    let data = entity.data(host)?;
    device_from(&block, &data)
}

/// The block at `pos` in `level`, as its id.
fn block_at(host: &dyn ServerHost, level: LevelRef, pos: [i32; 3]) -> Option<String> {
    Some(level.block(host, pos)?.id(host))
}

/// The level the server ticks now; its block entities and capabilities are the ones a tick may
/// read. A block-entity tick is not told its level.
fn level_in_place(host: &dyn ServerHost) -> Option<LevelRef> {
    host.levels()
        .list_levels()
        .into_iter()
        .find(|level| level.in_place(host))
}

/// The level a command executes in, from the dimension its invocation named.
fn level_of(host: &dyn ServerHost, invocation: &CommandInvocation) -> Option<LevelRef> {
    invocation
        .dimension
        .id(host)
        .and_then(|id| host.level(&id))
        .or_else(|| level_in_place(host))
}

/// A device's status line, or the answer a position that holds no device gives.
fn readout(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) -> String {
    let Some(block) = block_at(host, level, pos) else {
        return NO_DEVICE.to_owned();
    };
    let time = level.game_time(host).unwrap_or(0);
    let Some(entity) = host.block_entity(level, pos) else {
        return NO_DEVICE.to_owned();
    };
    let data = entity.data(host).unwrap_or_default();
    match device_from(&block, &data) {
        Some(device) => status_line(&device, pos, time),
        None if block == CONTROLLER => CONTROLLER_LINE.to_owned(),
        None => NO_DEVICE.to_owned(),
    }
}

/// Writes a device's buffer directly (`/fluxnetworks energy`): a harness control, not upstream
/// gameplay (the same precedent as `/powah set`). Answers the value the buffer ended up with.
fn set_energy(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    amount: i32,
) -> Option<i32> {
    let block = block_at(host, level, pos)?;
    let entity = host.block_entity(level, pos)?;
    let data = entity.data(host).unwrap_or_default();
    let device = device_from(&block, &data)?;
    let after = amount.clamp(0, device.capacity);
    if set_int(host, entity, "energy", after) {
        Some(after)
    } else {
        None
    }
}

/// Writes one int setting of the device at `pos` (`/fluxnetworks network|priority`).
fn set_device_int(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    key: &str,
    value: i32,
) -> bool {
    let Some(block) = block_at(host, level, pos) else {
        return false;
    };
    if device_kind(&block).is_none() {
        return false;
    }
    let Some(entity) = host.block_entity(level, pos) else {
        return false;
    };
    set_int(host, entity, key, value)
}

/// Merges one int key into a block entity's saved data. `Ok(false)` means "already that value",
/// which is still a successful write: a command that sets a value it already has is not an error.
fn set_int(
    host: &mut dyn ServerHost,
    entity: mod_api::BlockEntityRef,
    key: &str,
    value: i32,
) -> bool {
    let mut patch = Compound::new();
    patch.insert(key.into(), Tag::Int(value));
    entity.merge_data(host, patch).is_ok()
}

fn position_argument(value: Option<&CommandArgumentValue>) -> Option<[i32; 3]> {
    match value? {
        CommandArgumentValue::BlockPos(pos) => Some(*pos),
        _ => None,
    }
}

fn integer_argument(value: Option<&CommandArgumentValue>) -> Option<i32> {
    match value? {
        CommandArgumentValue::Integer(value) => Some(*value),
        _ => None,
    }
}

fn feedback(host: &mut dyn ServerHost, line: &str) {
    let _ = host.commands().feedback(Text::literal(line));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_five_devices_declare_a_buffer() {
        for (block, capacity, transfer) in DEVICES {
            let buffer = buffer_of(block).expect("a device declares a buffer");
            assert_eq!(buffer.capacity, capacity, "{block}'s capacity");
            assert_eq!(buffer.max_insert, transfer, "{block}'s insert per call");
            assert_eq!(buffer.max_extract, transfer, "{block}'s extract per call");
            assert_eq!(
                buffer.insert_faces, [true; 6],
                "{block} takes FE from every face"
            );
            assert_eq!(
                buffer.extract_faces, [true; 6],
                "{block} gives FE from every face"
            );
        }
        assert_eq!(
            buffer_of("othermod:machine"),
            None,
            "a foreign block is never declared"
        );
        assert_eq!(buffer_of("minecraft:furnace"), None, "nor a vanilla one");
    }

    #[test]
    fn a_device_read_from_its_compound_keeps_its_settings() {
        let mut nbt = Compound::new();
        nbt.insert("network_id".into(), Tag::Int(10));
        nbt.insert("priority".into(), Tag::Int(5));
        nbt.insert("surge_mode".into(), Tag::Byte(1));
        nbt.insert("limit".into(), Tag::Int(1234));
        nbt.insert("energy".into(), Tag::Int(4321));
        let device = device_from(PLUG, &nbt).expect("a plug is a device");
        assert_eq!(device.kind, DeviceKind::Plug);
        assert_eq!(device.network, 10);
        assert_eq!(device.priority, 5);
        assert!(device.surge);
        assert_eq!(device.limit, 1234);
        assert_eq!(device.capacity, DEFAULT_LIMIT);
        assert_eq!(device.buffer, 4321);
    }

    #[test]
    fn a_fresh_device_has_no_network_and_its_tier_defaults() {
        let empty = Compound::new();
        let plug = device_from(POINT, &empty).expect("a point is a device");
        assert_eq!(
            plug.network, INVALID_NETWORK,
            "no network_id means unlinked"
        );
        assert_eq!(plug.priority, 0);
        assert!(!plug.surge);
        assert_eq!(plug.limit, DEFAULT_LIMIT);
        assert_eq!(plug.buffer, 0);

        let herculean = device_from(HERCULEAN_STORAGE, &empty).expect("a storage is a device");
        assert_eq!(herculean.capacity, HERCULEAN_CAPACITY);
        assert_eq!(
            herculean.limit, HERCULEAN_TRANSFER,
            "its tier's transfer by default"
        );
        let gargantuan = device_from(GARGANTUAN_STORAGE, &empty).expect("a storage is a device");
        assert_eq!(gargantuan.capacity, GARGANTUAN_CAPACITY);
        assert_eq!(gargantuan.limit, GARGANTUAN_TRANSFER);
        let basic = device_from(BASIC_STORAGE, &empty).expect("a storage is a device");
        assert_eq!(basic.capacity, BASIC_CAPACITY);
        assert_eq!(basic.limit, BASIC_TRANSFER);

        assert_eq!(
            device_from(CONTROLLER, &empty),
            None,
            "the controller has no network role"
        );
        assert_eq!(device_from("minecraft:furnace", &empty), None);
    }

    #[test]
    fn the_readout_is_the_line_the_e2e_parses() {
        let mut plug = Device::new(DeviceKind::Plug, 1);
        plug.buffer = 120_000;
        plug.limit = 800_000;
        plug.capacity = 800_000;
        assert_eq!(
            status_line(&plug, [1, 100, 0], 412),
            "Flux Plug at 1,100,0: 120000/800000 FE (comparator 3, network 1, limit 800000 FE/t, priority 0, t=412)"
        );

        let mut point = Device::new(DeviceKind::Point, 1);
        point.limit = 800_000;
        point.capacity = 800_000;
        point.desired = 120_000;
        assert_eq!(
            status_line(&point, [3, 100, 0], 413),
            "Flux Point at 3,100,0: 0/800000 FE (comparator 0, network 1, limit 800000 FE/t, priority 0, wants 120000, t=413)"
        );

        let mut storage = Device::new(DeviceKind::Storage, INVALID_NETWORK);
        storage.capacity = HERCULEAN_CAPACITY;
        storage.limit = HERCULEAN_TRANSFER;
        storage.buffer = 15_880_000;
        // Vanilla's container formula: 1 + floor(14 * 15,880,000 / 16,000,000) = 14.
        assert_eq!(
            status_line(&storage, [0, 100, 0], 414),
            "Flux Storage at 0,100,0: 15880000/16000000 FE (comparator 14, network none, limit 120000 FE/t, priority -1000000, t=414)"
        );
    }

    #[test]
    fn a_command_path_is_ours_only_for_the_four_usages() {
        let our = |sub: &str| vec!["fluxnetworks".to_owned(), sub.to_owned()];
        assert_eq!(subcommand(&our("info")), Some(Subcommand::Info));
        assert_eq!(subcommand(&our("energy")), Some(Subcommand::Energy));
        assert_eq!(subcommand(&our("network")), Some(Subcommand::Network));
        assert_eq!(subcommand(&our("priority")), Some(Subcommand::Priority));
        assert_eq!(
            subcommand(&our("nothing")),
            None,
            "an unknown subcommand is not ours"
        );
        assert_eq!(
            subcommand(&["othermod".to_owned(), "info".to_owned()]),
            None,
            "another mod's root is not ours"
        );
        assert_eq!(subcommand(&our("")), None);
    }
}
