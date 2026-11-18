# Flux Networks for Horizon — slice 1: one energy network

A clean-room Rust rewrite of [Flux Networks](https://github.com/SonarSonic/Flux-Networks)
8.0.0 for [Horizon](https://github.com/horizon-rust/horizon), ported from the
ATM10 1.21.1 jar. One flat SDK package (mod API `0.2`): the code, the carried
assets and the carried data travel in one `fluxnetworks.hmod`.

**This is a partial port, deliberately.** It implements one energy network end
to end and carries the whole block and item set. It does not implement the
configurator GUI, wireless charging, chunk loading, network security or the
statistics screens.

## What works

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

## What does not work

See the gaps list in [`notes.md`](notes.md). The short version: no configurator,
no wireless item charging, no controller behaviour, no chunk loading, no
security/members, no GUI, and breaking a device loses its stored FE.

## Building

```sh
ports/tools/build-slot.sh run --job fluxnetworks-sdk --port ports/fluxnetworks --step test -- \
    bash -c 'cd ports/fluxnetworks && CARGO_INCREMENTAL=0 cargo test --offline'
python3 ports/fluxnetworks/tools/carry_content.py --check   # the carried files, from the jar
ports/tools/build-slot.sh run --job fluxnetworks-sdk --port ports/fluxnetworks --step build -- \
    bash -c 'cd ports/fluxnetworks && rm -f Cargo.lock && cargo horizon-mod build'
```

`cargo horizon-mod` fetches the pinned SDK into `.horizon/sdk/` (gitignored, never
packed) and `build` packs `horizon-mod.toml`, `Cargo.toml`, `src/`, `assets/`,
`data/` and the root documents into `target/horizon-mod/fluxnetworks.hmod`;
the delivered copy lives in this directory. The native compile is the launcher's
(`horizon-launcher native-build . --instance <instance> --trust-native-code`),
which is not installed in this environment — see the gaps in `notes.md`.

Licence: MIT (Sonar Sonic, BloCamLimb). Local-only until the maintainer
approves publishing; see `notes.md`.
