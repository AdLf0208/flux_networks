# Flux Networks — findings (slice 1)

Ported from **Flux Networks 8.0.0** (`SonarSonic/Flux-Networks`, **MIT**), the
exact ATM10 jar `FluxNetworks-1.21.1-8.0.0.jar`.
Provenance, hashes and the jar itself:
[`ports/_shared/licence-cache/FluxNetworks-1.21.1-8.0.0.jar`](../../_shared/licence-cache/FluxNetworks-1.21.1-8.0.0.jar)
(SHA-1 `183342f454428084748f2784c8efd748664a9f9c`,
SHA-256 `29e511b2d76f85bd247aa213a92bb059ecc037fcc3fed82baffcff97b638d3a6`).

Upstream behaviour was read from the repository's `1.21` branch (commit
`ea8b6e0cf708098ab2f02bf5c3d5c1f0ed2a1f5f`, `mod_version=8.1.0-SNAPSHOT`) —
analysis only, the port is a clean-room Rust rewrite. The branch is one minor
release ahead of the pack jar; the files read are named below, and nothing in
them contradict the jar's data (the jar's recipes, loot and blockstates were
read directly).

## Inventory (the jar's own files)

Blocks (`assets/fluxnetworks/blockstates/`, `data/.../loot_table/blocks/`):

| id | class | block entity |
|---|---|---|
| `fluxnetworks:flux_block` | plain `Block` | none |
| `fluxnetworks:flux_plug` | `FluxPlugBlock` (`FluxConnectorBlock`) | `TileFluxPlug` |
| `fluxnetworks:flux_point` | `FluxPointBlock` (`FluxConnectorBlock`) | `TileFluxPoint` |
| `fluxnetworks:flux_controller` | `FluxControllerBlock` | `TileFluxController` |
| `fluxnetworks:basic_flux_storage` | `FluxStorageBlock.Basic` | `TileFluxStorage.Basic` |
| `fluxnetworks:herculean_flux_storage` | `FluxStorageBlock.Herculean` | `TileFluxStorage.Herculean` |
| `fluxnetworks:gargantuan_flux_storage` | `FluxStorageBlock.Gargantuan` | `TileFluxStorage.Gargantuan` |

Items (`RegistryItems`, `assets/fluxnetworks/models/item/`): `flux_dust`,
`flux_core`, `flux_configurator`, `admin_configurator`. The seven block items
come from the blocks.

Recipes (16): seven `minecraft:crafting_shaped`, two
`fluxnetworks:flux_storage_recipe` (storage upgrades), six
`fluxnetworks:config_wipe_recipe` (clear a device's settings). Tags:
`fluxnetworks:storage` (the three storages).

## Block properties (`RegistryBlocks.register`)

Every block is `mapColor(METAL).sound(METAL).strength(1.0F, 1000F)`; every
device (all but `flux_block`) is also `noOcclusion()`. `FluxConnectorBlock`
adds the six connection booleans (`down/up/north/south/west/east`) and
`waterlogged`; `FluxShapes` gives the plug an 8×8×8 centre box, the point a
5×5×5 one, the controller a base plus four struts.

## The network model

Read from `common/connection/ServerFluxNetwork.java`,
`common/connection/TransferHandler.java`, `TransferIterator.java`,
`common/device/{TileFluxDevice,FluxPlugHandler,FluxPointHandler,FluxStorageHandler,SideTransfer}.java`,
`common/util/EnergyUtils.java` and `FluxConfig.java`.

- **Membership is the network id.** `TileFluxDevice.mNetworkID` is persisted
  (`network_id`), defaults to `INVALID_NETWORK_ID` (-1), and a device only joins
  a `ServerFluxNetwork` when it has one. An unlinked device takes no part in any
  transfer.
- **Settings per device**, persisted: `priority` (int, clamped to -9,999..9,999),
  `surge_mode` (boolean; the effective priority becomes 100,000), `limit`
  (long), `disable_limit`, and the buffer `energy`. A storage is *both* a point
  and a plug (`IFluxStorage extends IFluxPoint, IFluxPlug`) and its effective
  priority is always `priority - 1_000_000`, so it sorts below everything.
- **One cycle a tick** (`onEndServerTick`): plugs and points are sorted by
  descending priority. Each point states `mDesired = sendToConsumers(limit,
  simulate)` — what the blocks beside it can take this tick. Then a shared
  plug iterator walks the plugs: a plug gives
  `min(what the point asks, its buffer, its limit - what it already gave this
  cycle)`; a plug whose limit is spent is passed over for the rest of the cycle;
  when the plug iterator runs out, the cycle ends (the remaining points go
  unfed). A plug never feeds a point of the same kind, which is what stops two
  storages from moving energy between themselves.
- **Plug pull** (`FluxPlugHandler.receive`): a plug holds at most
  `min(its limit, the network's last-cycle request)` (`mBufferLimiter`), so a
  network with nothing to feed does not drain the blocks beside it.
- **Point push** (`onCycleEnd`): `min(buffer, limit)` into the blocks beside it,
  in side order.
- **Outside blocks** (`EnergyUtils.getConnector`): anything with an
  `IBlockEnergyConnector` capability through that side, **except** an
  `IFluxDevice` and except the configured block blacklist (default
  `actuallyadditions:block_phantom_energyface`). A flux device is never an
  outside energy block for another flux device.
- **Rates** (`FluxConfig` defaults): a plug and a point move 800,000 FE per
  cycle (`defaultLimit`); a basic storage holds 2,000,000 FE and moves 20,000 a
  cycle, a herculean 16,000,000 / 120,000, a gargantuan 128,000,000 / 720,000.
- **Energy format**: everything is a `long` upstream; this port stores the
  buffer as the int `energy` of API 142, so every value above is clamped to the
  int range on read (all the defaults fit).

## What the port takes from this

One network, proven end to end:

- `flux_plug`: pulls from the outside blocks beside it at its limit, buffers up
  to its limit, gives to the network's points.
- `flux_point`: takes from the network's plugs by priority, pushes `min(buffer,
  limit)` into the outside blocks beside it.
- `basic/herculean/gargantuan_flux_storage`: 2M/16M/128M FE at 20k/120k/720k per
  cycle, a member of the network, always the lowest priority.
- the network id, priority, surge mode and limit live in the block entity's
  saved compound, as upstream's `network_id`/`priority`/`surge_mode`/`limit`.
- the cycle runs at `native_level_tick_post` (API 23), where every block entity
  is back in the world, and every move is `native_move_energy` (API 142), so the
  numbers are conserved by the engine, not by bookkeeping.

Deviations and gaps are listed in [`notes.md`](notes.md).
