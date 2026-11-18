# Flux Networks — a Horizon port

**Flux Networks** is a Minecraft mod. Wireless Energy - Minecraft Forge Mod. This repository is a **port of it to [Horizon](https://github.com/horizon-rust/horizon)**, the Rust engine for Minecraft Java 26.3. It carries the complete source of the port plus the content files it ships, and installs as a single `.hmod` package.

Upstream: [SonarSonic/Flux-Networks](https://github.com/SonarSonic/Flux-Networks) — licence **MIT**.

| | |
|---|---|
| Upstream mod | Flux Networks ([source](https://github.com/SonarSonic/Flux-Networks)) |
| Ported to | Horizon 0b14f8a, Minecraft Java 26.3, mod API `0.2` |
| Package | `fluxnetworks.hmod` (install it with the Horizon launcher) |
| Licence | **MIT** — the upstream mod's licence; see [Licence and credit](#licence-and-credit) |

## What this port implements

### What works

- **`flux_plug`** pulls FE from the blocks beside it, buffers it, and gives it
  to its network.
- **`flux_point`** takes FE from its network and pushes it into the blocks
  beside it.
- **`basic` / `herculean` / `gargantuan_flux_storage`** are network buffers:
  2,000,000 / 16,000,000 / 128,000,000 FE, moving 20,000 / 120,000 / 720,000 FE
  a tick, always the lowest priority in the network.
- **The network is the `network_id` saved in each device**, as upstream's is:
  plugs and points of the same id in the same dimension transfer to each other
  whatever the distance. Priority (and surge mode) decide who is served first.
- Every move goes through Horizon's `horizon:energy` capability
  (`EnergyHandler`, the SDK's `Capabilities.Energy.BLOCK`), so FE is conserved by
  the engine and every device is a real energy buffer other mods can insert into
  or extract from (`Registrar::block_energy`, the amount in the entity's saved
  `energy` int).
- `/fluxnetworks info|energy|network|priority` — a readout and the harness
  controls, because there is no configurator GUI yet.
- The seven blocks, four items, loot tables and the seven vanilla-shaped
  recipes are carried from the jar (`assets/`, `data/`; the carry pipeline is
  `tools/carry_content.py` and its patch table is reproduced in `notes.md`).

## Not yet ported

- **A device of another network is an outside energy block.** Upstream refuses
- **No configurator, no GUI.** Membership, priority and limit are set with
- **No wireless charging.** The controller's `FluxControllerHandler` charges
- **No security, members or network creation.** Upstream networks are created
- **No chunk loading, statistics, colours or connection rendering.** The
- **Waterlogging is not declared.** Upstream's connectors are
- **Breaking a device loses its FE and its settings** (the loot table's
- **No block blacklist.** Upstream refuses a configurable list of blocks

## Installing

1. Install the [Horizon launcher](https://github.com/horizon-rust/horizon/releases) (0.2.70 or newer).
2. Add `fluxnetworks.hmod` to the instance's mods.
3. The package is pinned to a specific Horizon build; the launcher checks that pin.

## Licence and credit

Flux Networks was created by its upstream authors; the code, models and assets in the original mod
remain theirs and are used here under **MIT**. The Rust in this repository is a clean-room
rewrite for Horizon, not a translation of the original source.

- `LICENSE` — the upstream licence text.
- `LICENSE-NOTICE.md` — what this port carries and where it came from.
- The upstream project: https://github.com/SonarSonic/Flux-Networks

## Technical details

```
horizon-mod.toml   package manifest (id, version, api, [native] pin)
Cargo.toml         the crate; depends on `horizon-mod-api` from the fetched SDK
src/               lib.rs (the one `impl Hooks`), runtime.rs (pure logic + tests), glue
assets/ data/      the content the package ships
tools/             content generators and the e2e scenario
```

- Build: `cargo horizon-mod build` (packages; it does not compile against the game).
- Test: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- In-game: `ports/tools/launcher-game.sh run fluxnetworks` in the campaign tree, or install the
  `.hmod` and play; the harness scenario is `tools/e2e.sh`.

This repository is generated from the campaign's port at `ports/fluxnetworks/`; the port's own
`notes.md` is the full record (decisions, traps, evidence).
