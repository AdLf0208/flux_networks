// Flux Networks slice 1 — the network rules, pure and testable.
//
// Upstream (`SonarSonic/Flux-Networks`) keeps one `TransferHandler` per
// device: a plug buffers what it pulls from the block beside it, a point
// buffers what it pushes into the block beside it, and a storage is both. One
// server tick runs a *cycle* over the devices of a network: the devices are
// sorted by priority, every point states how much its consumers can take, and
// the cycle moves that much from the plugs' buffers into the points', never
// more than a device's own limit per cycle.
//
// This module is that cycle and the constants behind it, with no game access,
// so the numbers can be pinned by tests. The glue (`support.rs`) reads a block
// entity, calls in here, and writes the result back.

/// `FluxConstants.INVALID_NETWORK_ID`: a device that joined no network.
pub const INVALID_NETWORK: i32 = -1;

/// `FluxConfig.defaultLimit`: a plug's and a point's transfer per cycle.
pub const DEFAULT_LIMIT: i32 = 800_000;

/// `FluxConfig.basicCapacity` / `basicTransfer`.
pub const BASIC_CAPACITY: i32 = 2_000_000;
pub const BASIC_TRANSFER: i32 = 20_000;

/// `FluxConfig.herculeanCapacity` / `herculeanTransfer`.
pub const HERCULEAN_CAPACITY: i32 = 16_000_000;
pub const HERCULEAN_TRANSFER: i32 = 120_000;

/// `FluxConfig.gargantuanCapacity` / `gargantuanTransfer`.
pub const GARGANTUAN_CAPACITY: i32 = 128_000_000;
pub const GARGANTUAN_TRANSFER: i32 = 720_000;

/// `TransferHandler.PRI_USER_MIN` / `PRI_USER_MAX`: the user's own priority.
pub const PRI_USER_MIN: i32 = -9_999;
pub const PRI_USER_MAX: i32 = 9_999;

/// `TransferHandler.PRI_GAIN_MAX`: surge mode's priority, above every user one.
pub const PRI_GAIN_MAX: i32 = 100_000;

/// `TransferHandler.STORAGE_PRI_DIFF`: a storage always sorts below every other
/// device, however high its own priority is set.
pub const STORAGE_PRI_DIFF: i32 = 1_000_000;

/// What a flux block is. A storage is both a plug and a point, as upstream's
/// `IFluxStorage extends IFluxPoint, IFluxPlug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Plug,
    Point,
    Storage,
}

impl DeviceKind {
    /// Does this device hold energy the network may take (`IFluxPlug`)?
    pub const fn is_plug(self) -> bool {
        matches!(self, Self::Plug | Self::Storage)
    }

    /// Does this device take energy from the network (`IFluxPoint`)?
    pub const fn is_point(self) -> bool {
        matches!(self, Self::Point | Self::Storage)
    }

    /// The network buffer's capacity: a plug's and a point's buffer holds one
    /// cycle's limit, a storage its tier's capacity.
    pub const fn capacity(self) -> i32 {
        match self {
            Self::Plug | Self::Point => DEFAULT_LIMIT,
            Self::Storage => BASIC_CAPACITY,
        }
    }

    /// The transfer limit this block starts with (`FluxConfig`).
    pub const fn transfer(self) -> i32 {
        match self {
            Self::Plug | Self::Point => DEFAULT_LIMIT,
            Self::Storage => BASIC_TRANSFER,
        }
    }
}

/// One device of one network, as a tick sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub kind: DeviceKind,
    /// The network it joined, or [`INVALID_NETWORK`].
    pub network: i32,
    /// The user's priority; negative is allowed.
    pub priority: i32,
    /// Surge mode: `getPriority()` answers [`PRI_GAIN_MAX`].
    pub surge: bool,
    /// The most it moves in one cycle.
    pub limit: i32,
    /// Its buffer's capacity: a plug's and a point's limit, a storage's tier.
    pub capacity: i32,
    /// The energy in its own buffer.
    pub buffer: i32,
    /// A point's `mDesired`: what its consumers can take this cycle (already
    /// capped by its limit, as `FluxPointHandler.onCycleStart` does).
    pub desired: i32,
}

impl Device {
    /// A device with upstream's defaults for its kind, joined to `network`.
    pub fn new(kind: DeviceKind, network: i32) -> Self {
        Self {
            kind,
            network,
            priority: 0,
            surge: false,
            limit: kind.transfer(),
            capacity: kind.capacity(),
            buffer: 0,
            desired: 0,
        }
    }

    /// `TransferHandler.getPriority()` with `FluxStorageHandler.getPriority()`:
    /// a storage is always the lowest, whatever it is set to.
    pub fn effective_priority(&self) -> i32 {
        let base = if self.surge {
            PRI_GAIN_MAX
        } else {
            clamp_priority(self.priority)
        };
        match self.kind {
            DeviceKind::Storage => base.saturating_sub(STORAGE_PRI_DIFF),
            DeviceKind::Plug | DeviceKind::Point => base,
        }
    }

    /// The room its buffer has left.
    pub fn room(&self) -> i32 {
        self.capacity.saturating_sub(self.buffer).max(0)
    }

    /// `TransferHandler.getRequest()`: how much this device wants from the
    /// network this cycle. A plug never asks; a point asks for what it still
    /// lacks of its desire; a storage asks for the room it has, up to its own
    /// limit.
    pub fn request(&self) -> i32 {
        match self.kind {
            DeviceKind::Plug => 0,
            DeviceKind::Point => self.desired.saturating_sub(self.buffer).max(0),
            DeviceKind::Storage => self.room().min(self.limit.max(0)).max(0),
        }
    }

    /// Pull the stored values into their allowed ranges.
    pub fn clamp(&mut self) {
        self.priority = clamp_priority(self.priority);
        self.limit = clamp_limit(self.limit, self.capacity);
        self.buffer = self.buffer.clamp(0, self.capacity);
        self.desired = self.desired.clamp(0, self.limit);
    }
}

/// One move the cycle decided on: `amount` from `from` to `to`, both indices
/// into the slice handed to [`plan_cycle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transfer {
    pub from: usize,
    pub to: usize,
    pub amount: i32,
}

/// `ServerFluxNetwork.onEndServerTick`: the internal transfer cycle.
///
/// Plugs and points are taken in descending priority (storages last, see
/// [`Device::effective_priority`]). For each point, starting where the previous
/// one left off, the first plug with energy gives up to what the point still
/// asks, its own buffer and its per-cycle limit; a plug that cannot give (its
/// limit is spent) is passed over for the rest of the cycle, and the cycle ends
/// when no plug is left. A storage is both, but two storages never move energy
/// between themselves (`break CYCLE` when the two devices have the same kind).
///
/// The slice must be the devices of **one** network; `desired` must already be
/// filled in for its points.
pub fn plan_cycle(devices: &[Device]) -> Vec<Transfer> {
    let mut transfers = Vec::new();
    let mut buffer: Vec<i32> = devices.iter().map(|d| d.buffer.max(0)).collect();
    // `FluxPlugHandler.mRemoved`: what a device has given this cycle.
    let mut given: Vec<i32> = vec![0; devices.len()];
    // `FluxStorageHandler.mAdded`: what a storage has taken this cycle.
    let mut taken: Vec<i32> = vec![0; devices.len()];

    let mut plugs: Vec<usize> = (0..devices.len())
        .filter(|&i| devices[i].kind.is_plug())
        .collect();
    let mut points: Vec<usize> = (0..devices.len())
        .filter(|&i| devices[i].kind.is_point())
        .collect();
    plugs.sort_by_key(|&i| std::cmp::Reverse(devices[i].effective_priority()));
    points.sort_by_key(|&i| std::cmp::Reverse(devices[i].effective_priority()));

    // `TransferHandler.getRequest`, against the cycle's working buffer.
    fn request(devices: &[Device], buffer: &[i32], taken: &[i32], i: usize) -> i32 {
        match devices[i].kind {
            DeviceKind::Plug => 0,
            DeviceKind::Point => (devices[i].desired - buffer[i]).max(0),
            DeviceKind::Storage => (devices[i].capacity - buffer[i])
                .min(devices[i].limit.max(0) - taken[i])
                .max(0),
        }
    }

    let mut pi = 0;
    let mut qi = 0;
    'cycle: while pi < points.len() {
        let point = points[pi];
        if request(devices, &buffer, &taken, point) <= 0 {
            pi += 1;
            continue;
        }
        loop {
            // `TransferIterator.hasNext`: pass over a plug with nothing to give.
            while qi < plugs.len() && buffer[plugs[qi]] <= 0 {
                qi += 1;
            }
            if qi >= plugs.len() {
                // All plugs have been used; every remaining point goes unfed.
                break 'cycle;
            }
            let plug = plugs[qi];
            if devices[plug].kind == devices[point].kind {
                // A storage never feeds a storage.
                break 'cycle;
            }
            let give = request(devices, &buffer, &taken, point)
                .min(buffer[plug])
                .min(devices[plug].limit.max(0) - given[plug])
                .min(devices[point].capacity - buffer[point]);
            if give > 0 {
                buffer[plug] -= give;
                buffer[point] += give;
                given[plug] += give;
                taken[point] += give;
                transfers.push(Transfer {
                    from: plug,
                    to: point,
                    amount: give,
                });
                continue 'cycle;
            }
            // This plug's limit is spent (`removeFromBuffer` answered 0).
            qi += 1;
        }
    }
    transfers
}

/// `FluxPlugHandler.receive`'s buffer target: a plug holds no more than its
/// own limit and no more than the network asked for in the previous cycle
/// (`ServerFluxNetwork.mBufferLimiter`), so a network with nothing to feed does
/// not drain the blocks beside it.
pub fn pull_cap(limit: i32, limiter: i32) -> i32 {
    limit.min(limiter).max(0)
}

/// What a plug takes from one neighbour this cycle: no more than its buffer's
/// room, its per-cycle limit and what the neighbour offers.
pub fn plug_pull(buffer: i32, limit: i32, offer: i32, pulled: i32) -> i32 {
    let room = limit.saturating_sub(buffer);
    room.min(limit.max(0) - pulled).min(offer.max(0)).max(0)
}

/// What a point sends into one neighbour this cycle: no more than its buffer,
/// its per-cycle limit and the room the neighbour offers.
pub fn point_push(buffer: i32, limit: i32, room: i32, pushed: i32) -> i32 {
    buffer.min(limit.max(0) - pushed).min(room.max(0)).max(0)
}

/// `Mth.clamp(priority, PRI_USER_MIN, PRI_USER_MAX)`.
pub fn clamp_priority(priority: i32) -> i32 {
    priority.clamp(PRI_USER_MIN, PRI_USER_MAX)
}

/// `TransferHandler.setLimit`: never negative, never above the buffer.
pub fn clamp_limit(limit: i32, capacity: i32) -> i32 {
    limit.clamp(0, capacity)
}

/// `EnergyType.FE.getStorage`'s comparator strength, as the readout prints it.
pub fn comparator_strength(energy: i32, capacity: i32) -> i32 {
    if capacity <= 0 {
        return 0;
    }
    let stored = energy.clamp(0, capacity);
    if stored == 0 {
        0
    } else {
        1 + (stored.saturating_mul(14) / capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plug(buffer: i32) -> Device {
        Device {
            buffer,
            ..Device::new(DeviceKind::Plug, 1)
        }
    }

    fn point(desired: i32) -> Device {
        Device {
            desired,
            ..Device::new(DeviceKind::Point, 1)
        }
    }

    fn storage(buffer: i32) -> Device {
        Device {
            buffer,
            ..Device::new(DeviceKind::Storage, 1)
        }
    }

    #[test]
    fn a_plug_feeds_a_point_on_the_same_network() {
        let devices = [plug(1_000), point(1_000)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 1_000
            }]
        );
    }

    #[test]
    fn a_plug_gives_only_what_it_buffered() {
        let devices = [plug(400), point(1_000)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 400
            }]
        );
    }

    #[test]
    fn a_point_takes_only_what_it_asks_for() {
        let devices = [plug(1_000), point(300)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 300
            }]
        );
    }

    #[test]
    fn a_point_already_holding_energy_asks_for_the_rest() {
        let p = Device {
            buffer: 400,
            ..point(1_000)
        };
        let devices = [plug(1_000), p];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 600
            }]
        );
    }

    #[test]
    fn a_plug_gives_its_limit_once_per_cycle() {
        // 800,000 is the plug's limit: two points asking 800,000 each get
        // 800,000 and 0.
        let devices = [plug(1_000_000), point(800_000), point(800_000)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 800_000
            }]
        );
    }

    #[test]
    fn a_second_plug_makes_up_what_the_first_ran_out_of() {
        let mut first = plug(100);
        first.limit = 100;
        let mut second = plug(100);
        second.limit = 100;
        let devices = [first, second, point(150)];
        assert_eq!(
            plan_cycle(&devices),
            vec![
                Transfer {
                    from: 0,
                    to: 2,
                    amount: 100
                },
                Transfer {
                    from: 1,
                    to: 2,
                    amount: 50
                },
            ]
        );
    }

    #[test]
    fn the_higher_priority_point_takes_first() {
        let mut high = point(100);
        high.priority = 5;
        let mut low = point(100);
        low.priority = 1;
        // The slice order is the network's, not the priority order.
        let devices = [plug(100), low, high];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 2,
                amount: 100
            }]
        );
    }

    #[test]
    fn a_storage_sorts_below_a_point_whatever_its_priority() {
        let mut store = storage(0);
        store.priority = PRI_USER_MAX;
        let mut sink = point(100);
        sink.priority = PRI_USER_MIN;
        assert!(store.effective_priority() < sink.effective_priority());
        let devices = [plug(100), store, sink];
        // The point takes the 100 first; the storage gets nothing.
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 2,
                amount: 100
            }]
        );
    }

    #[test]
    fn surge_mode_is_the_highest_priority() {
        let mut surged = point(100);
        surged.surge = true;
        let mut plain = point(100);
        plain.priority = PRI_USER_MAX;
        assert!(surged.effective_priority() > plain.effective_priority());
        let devices = [plug(100), plain, surged];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 2,
                amount: 100
            }]
        );
    }

    #[test]
    fn a_storage_takes_only_its_transfer_limit_per_cycle() {
        let devices = [plug(800_000), storage(0)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: BASIC_TRANSFER
            }]
        );
    }

    #[test]
    fn a_storage_gives_only_its_transfer_limit_per_cycle() {
        let devices = [storage(1_000_000), point(800_000)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: BASIC_TRANSFER
            }]
        );
    }

    #[test]
    fn a_storage_fills_to_its_capacity_and_refuses_more() {
        let nearly_full = BASIC_CAPACITY - 1_000;
        let devices = [plug(800_000), storage(nearly_full)];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: 1_000
            }]
        );
        let full = [plug(800_000), storage(BASIC_CAPACITY)];
        assert!(plan_cycle(&full).is_empty());
    }

    #[test]
    fn two_storages_never_move_energy_between_themselves() {
        let devices = [storage(1_000_000), storage(0)];
        assert!(plan_cycle(&devices).is_empty());
    }

    #[test]
    fn the_cycle_conserves_energy() {
        let devices = [plug(500_000), storage(1_000), point(300_000), point(50_000)];
        let before: i32 = devices.iter().map(|d| d.buffer).sum();
        let moved: i32 = plan_cycle(&devices).iter().map(|t| t.amount).sum();
        let after = before + moved - moved;
        assert_eq!(before, after, "the cycle creates no energy");
        assert!(moved > 0);
        assert!(moved <= 800_000, "no more than the plug's per-cycle limit");
    }

    #[test]
    fn a_full_point_asks_for_nothing() {
        let full = Device {
            buffer: DEFAULT_LIMIT,
            ..point(DEFAULT_LIMIT)
        };
        assert_eq!(full.request(), 0);
        assert!(plan_cycle(&[plug(1_000), full]).is_empty());
    }

    #[test]
    fn an_unlinked_device_has_no_priority_effect() {
        // The support skips INVALID_NETWORK devices; the flag is what it tests.
        let joined = Device::new(DeviceKind::Plug, 3);
        let unlinked = Device::new(DeviceKind::Plug, INVALID_NETWORK);
        assert_ne!(joined.network, INVALID_NETWORK);
        assert_eq!(unlinked.network, INVALID_NETWORK);
    }

    #[test]
    fn a_plug_pull_stops_at_its_limit_the_offer_and_its_room() {
        assert_eq!(plug_pull(0, 800_000, 20_000, 0), 20_000);
        assert_eq!(plug_pull(799_000, 800_000, 20_000, 0), 1_000);
        assert_eq!(plug_pull(0, 800_000, 20_000, 795_000), 5_000);
        assert_eq!(plug_pull(800_000, 800_000, 20_000, 0), 0);
    }

    #[test]
    fn a_point_push_stops_at_its_limit_the_room_and_its_buffer() {
        assert_eq!(point_push(800_000, 800_000, 20_000, 0), 20_000);
        assert_eq!(point_push(500, 800_000, 20_000, 0), 500);
        assert_eq!(point_push(800_000, 800_000, 20_000, 790_000), 10_000);
        assert_eq!(point_push(800_000, 800_000, 0, 0), 0);
    }

    #[test]
    fn priorities_and_limits_are_clamped_to_their_ranges() {
        assert_eq!(clamp_priority(-20_000), PRI_USER_MIN);
        assert_eq!(clamp_priority(20_000), PRI_USER_MAX);
        assert_eq!(clamp_limit(-1, BASIC_CAPACITY), 0);
        assert_eq!(clamp_limit(i32::MAX, BASIC_CAPACITY), BASIC_CAPACITY);
    }

    #[test]
    fn a_storage_asks_for_its_room_up_to_its_limit() {
        let empty = storage(0);
        assert_eq!(empty.request(), BASIC_TRANSFER);
        let almost = storage(BASIC_CAPACITY - 5);
        assert_eq!(almost.request(), 5);
        assert_eq!(storage(BASIC_CAPACITY).request(), 0);
    }

    #[test]
    fn the_comparator_strength_follows_the_fill() {
        assert_eq!(comparator_strength(0, BASIC_CAPACITY), 0);
        assert_eq!(comparator_strength(BASIC_CAPACITY, BASIC_CAPACITY), 15);
        assert_eq!(comparator_strength(BASIC_CAPACITY / 2, BASIC_CAPACITY), 8);
    }

    #[test]
    fn a_plug_does_not_pull_more_than_the_network_asked() {
        // Nothing asked last cycle: the plug pulls nothing.
        assert_eq!(pull_cap(DEFAULT_LIMIT, 0), 0);
        // A point asked for 20,000: that is all the plug may hold.
        assert_eq!(pull_cap(DEFAULT_LIMIT, 20_000), 20_000);
        // A large request is still capped by the plug's own limit.
        assert_eq!(pull_cap(DEFAULT_LIMIT, 5_000_000), DEFAULT_LIMIT);
    }

    #[test]
    fn a_full_receiver_stops_the_source_draining() {
        // The receiver is full, so no point asks for anything: the network
        // asks for nothing, so the plug pulls nothing, so the source keeps its
        // energy.
        let mut full = storage(BASIC_CAPACITY);
        full.desired = 0;
        let devices = [plug(0), full, point(0)];
        assert!(plan_cycle(&devices).is_empty());
        assert_eq!(pull_cap(DEFAULT_LIMIT, 0), 0);
    }

    #[test]
    fn a_herculean_storage_keeps_its_own_capacity_and_transfer() {
        // The tier is a property of the block, not of the kind: the port sets
        // both from the block id (a basic storage would be 2,000,000 / 20,000).
        let mut big = Device::new(DeviceKind::Storage, 1);
        big.capacity = HERCULEAN_CAPACITY;
        big.limit = HERCULEAN_TRANSFER;
        assert_eq!(big.room(), HERCULEAN_CAPACITY);
        assert_eq!(big.request(), HERCULEAN_TRANSFER);
        let devices = [plug(HERCULEAN_CAPACITY), big];
        assert_eq!(
            plan_cycle(&devices),
            vec![Transfer {
                from: 0,
                to: 1,
                amount: HERCULEAN_TRANSFER
            }]
        );
    }
}
