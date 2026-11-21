# Flux Networks → Horizon — slice 1: one energy network

Ported from **Flux Networks 8.0.0** (`SonarSonic/Flux-Networks`, **MIT**), the
exact ATM10 NeoForge 1.21.1 jar `FluxNetworks-1.21.1-8.0.0.jar` (SHA-1
`183342f454428084748f2784c8efd748664a9f9c`, SHA-256
`29e511b2d76f85bd247aa213a92bb059ecc037fcc3fed82baffcff97b638d3a6`; kept at
`ports/_shared/licence-cache/`). Upstream behaviour was read from the
repository's `1.21` branch (commit
`ea8b6e0cf708098ab2f02bf5c3d5c1f0ed2a1f5f`, `mod_version=8.1.0-SNAPSHOT`) —
analysis only, this is a clean-room Rust rewrite. What was read and the numbers
extracted: [`FINDINGS.md`](FINDINGS.md).

**LOCAL-ONLY** until the maintainer approves publishing (ARCHITECTURE.md §8).
MIT is licence rank 0 (permissive); the approval is still required.

Status: **slice 1 implemented and verified in-game** — one energy network over
five blocks (`flux_plug`, `flux_point` and the three storages), with the seven
blocks and four items carried. Everything else in the mod is a recorded gap.
Since 2026-11-18 the port is **one flat SDK package** (`api = "0.2"`,
`fluxnetworks.hmod` 0.2.0): the native code, `assets/` and `data/` travel in the
same file, so installing it gives the blocks, the items and the behaviour. The
retired `.hnmod` (0.1.0) and the AE2 embedded-content workaround are history:
see `## Migration to the SDK format` at the end of this file.

## Class + tier, approach, licence

- **Class `content`, tier `machines`** (`ports/ledger.csv`, from
  `ports/_shared/source-map/atm10-sources.csv`), core, P0. The jar's
  `neoforge.mods.toml` says `license="MIT"` → **rank 0 (permissive)**.
- Queue position: rank 0, mechanics, core, no dependency on another unported
  mod. It is being ported now because it is the pack's wireless-energy base and
  the first mod in the queue whose only missing Horizon surface is a *network*
  (API 142 already moves FE between blocks).
- The pin's `docs/modpacks.md` shape for a `machines` tier is: a block entity,
  its state in NBT, a capability (FE), a tick, and the behaviour provable in
  game. This slice takes exactly that shape — no GUI, no menus, no screens,
  which the tier allows but the slice does not need.
- **The mod is not re-tiered**: the slice is `machines`. The GUI, the
  configurator and the client colour handler are `client` halves that are
  deferred, not silently dropped.

## What the slice is

One network, five blocks, one readout:

- **`fluxnetworks:flux_plug`** — pulls FE from the blocks beside it (up to its
  transfer limit and what the network asked for last cycle), buffers it, and
  gives it to the network.
- **`fluxnetworks:flux_point`** — takes FE from the network's plugs by
  priority, up to what the blocks beside it can accept, and pushes it there.
- **`fluxnetworks:basic_flux_storage` / `herculean_…` / `gargantuan_…`** —
  network buffers of 2,000,000 / 16,000,000 / 128,000,000 FE moving
  20,000 / 120,000 / 720,000 FE a cycle; both a source and a sink, and always
  the lowest priority in the network.
- **`fluxnetworks:flux_block`** and **`fluxnetworks:flux_controller`** are
  carried as blocks. The Flux Block is decorative upstream; the Controller's
  network statistics and wireless charging are a later slice.
- **`/fluxnetworks info|energy|network|priority`** — the readout the harness
  asserts on, and the harness controls (slice 1 has no configurator GUI).

## Behaviour (upstream → this port)

| | Upstream | This port |
|---|---|---|
| membership | `TileFluxDevice.mNetworkID`, persisted as `network_id`, `-1` = none; a device joins a `ServerFluxNetwork` only with a valid id | same: the int `network_id` in the block entity's compound, `-1` = unlinked; an unlinked device takes no part in any transfer (and does not pull) |
| cycle | `ServerFluxNetwork.onEndServerTick`: plugs and points sorted by descending priority, each point states what its consumers can take, a shared plug iterator feeds them | same algorithm in `runtime::plan_cycle`, unit-tested against upstream's iteration (including "a spent plug is passed over" and "a storage never feeds a storage") |
| priority | user `-9,999..9,999`; surge mode = `100,000`; a storage is always `priority - 1,000,000` | same constants (`PRI_USER_MIN/MAX`, `PRI_GAIN_MAX`, `STORAGE_PRI_DIFF`) |
| plug | buffer, `receive` capped by `min(limit, last cycle's request)`, `removeFromBuffer` capped by the limit per cycle | same: `runtime::pull_cap` + `plug_pull`, a per-cycle budget in `plan_cycle` |
| point | `mDesired = sendToConsumers(limit, simulate)`, `request = desired - buffer`, pushes `min(buffer, limit)` on cycle end | same: the support simulates the neighbours' room, `point_push` bounds the push |
| storage | capacity/transfer per tier, `request = min(capacity - buffer, limit - added)` | same, through the block's declared buffer and the runtime's `in_budget` |
| outside blocks | any block with the energy capability through that side, **never** an `IFluxDevice`, minus a configurable blacklist | any block with an API-142 buffer, **except a device of the same network** (see the deviation below); the blacklist is not ported (no config) |
| energy format | `long` | the int `energy` of API 142 (`Capabilities.Energy.BLOCK`), clamped on read; every default fits |
| FE movement | `EnergyHandlerUtil.move` between the device and its neighbour | `native_move_energy` (API 142) everywhere, so the engine keeps the books |
| when | the network ticks at the end of the level tick | `native_level_tick_post` (API 23): every device queues itself on its own tick, the pass reads them back and runs each network |
| block | metal, `strength(1.0, 1000)`, devices `noOcclusion`, six connection booleans + waterlogged, shape from `FluxShapes` | metal template with the same numbers, `occludes: false`, the six booleans declared, a static centre collision box |
| breaking | the loot table copies `fluxnetworks:device_config` and `stored_energy` into the item | the plain block drops; **the stored FE and the settings are lost** — gap |

## Hooks

| Hook (API) | Helper | What it does |
|---|---|---|
| `block_entity` (15) | `block_entity.rs` | The five devices get a block entity. |
| `block_entity_ticks` (15) | `ticks.rs` | They tick on the server. |
| `block_entity_energy` (142) | `energy.rs` | The FE buffer each device declares (capacity + per-call insert/extract = its transfer limit, all six faces). |
| `block_entity_tick` (17) | `tick.rs` | The device queues its position for the network pass. |
| `level_tick_post` (23) | `level_tick_post.rs` | One pass runs every network of the level: plug pulls, the cycle, point pushes, and the next cycle's request. |
| `register_commands` (8) | `register.rs` | `/fluxnetworks info\|energy\|network\|priority`. |
| `perform_command` (8) | `perform.rs` | The command bodies. |
| `world_lifecycle` | `world.rs` | Drops the queued positions and the per-network request when a world closes. |
| `content_root` (raw target, `native_data!` API 41) | `embedded_content.rs` | Writes the bundled `.hmod` out and appends it to the dev-mod list. |

Nine injections: **eight stamped hooks at v1** (`block_entity`,
`block_entity_ticks`, `block_entity_energy`, `block_entity_tick`,
`level_tick_post`, `register_commands`, `perform_command`, `world_lifecycle`) and
the one documented **raw target** `horizon_client::game::mods::content_root`
(not a hook — `native-hooks.json` does not list it), **0 unstamped hooks**. Two
pin facts shape the design (both recorded by earlier ports and re-confirmed
here):

- API 142's `native_energy`/`native_move_energy` work between **any two
  positions** in the level, not only adjacent ones (`native_energy.rs:190`
  checks only `from != to`) — which is exactly what makes a *wireless* network
  expressible: the plug and the point need not touch.
- `native_energy_at` refuses a block entity **that is ticking at that moment**,
  so every cross-block move runs at `level_tick_post`.

## Command (readout + harness controls)

```
Flux Plug at 1,100,0: 120000/800000 FE (comparator 3, network 1, limit 800000 FE/t, priority 0, t=412)
Flux Point at 3,100,0: 0/800000 FE (comparator 0, network 1, limit 800000 FE/t, priority 0, wants 120000, t=413)
Flux Storage at 0,100,0: 15880000/16000000 FE (comparator 15, network 10, limit 120000 FE/t, priority -1000000, t=414)
```

`t=` is the world game time, so the e2e can tell a fresh reply from a stale one.
`energy`, `network` and `priority` are harness controls, like `/powah set` and
`/mekanism energy`; upstream's equivalents are its configurator GUI, which is
not ported.

## Content (`.hmod`)

[`content/`](content/README.md) is generated from the jar by
[`tools/carry_content.py`](tools/carry_content.py) (`--check` re-reads and
diffs). 119 files: 7 blockstates, 8 block models, 11 item models, 11 26.3 item
definitions, 44 textures, 9 languages, 7 block declarations, 4 item
declarations, 7 26.3 loot tables, 7 recipes, 1 mod tag, 2 vanilla block tags.
The two custom recipe types (`flux_storage_recipe`, `config_wipe_recipe`) are
parked in `content/outside/` with the reason. Every patch is in
`content/README.md`.

## Embedded content (AE2 pattern) — one file ships both halves

The registry stores **one file per project** (playbook §1.0), so a port with both
halves cannot be listed faithfully: an `.hnmod` is code-only and an `.hmod` is
content. This bundle now carries its content, exactly as the merged AE2 entry
does:

- **`mixins/fluxnetworks/data/fluxnetworks-content.hmod`** is the built
  `content/fluxnetworks.hmod`, byte for byte (98,176 B,
  `0766d6f3…333cac`). `tools/package.py` packs the bundle's top-level
  `.rs`/`.toml` plus everything under `data/` as `mixins/<id>/data/…` (so
  `native_data!` compiles it in), with fixed zip timestamps. When the plan
  declares a `content_root` injection it **requires** `content/<id>.hmod` and a
  byte-identical `data/<id>-content.hmod` and refuses the bundle otherwise (both
  guards were exercised: a missing copy and a one-byte-different copy each exit
  1), so a rebuilt content mod can never ship stale inside it.
- **`mixins/fluxnetworks/embedded_content.rs`** with the `embedded-content`
  `[[injection]]` (`target = "horizon_client::game::mods::content_root"`, no hook,
  `kind = "HEAD"`, `cancellable = true`, bindings `root`/`enabled`/`dev`) writes
  the file to `.horizon-cache/fluxnetworks-embedded-content.hmod` when the copy
  on disk differs, appends it to the dev-mod list and returns its `content_root`.
  The helper calls `content_root` once more and stops at its own
  `dev.contains(&file)` guard, so the second call runs the vanilla body — the
  same one-level recursion AE2 uses. An unwritable cache logs
  `Horizon: Flux Networks' content could not be written to …` and `Continue`s
  (no content) instead of failing the game.
- The file goes to `std::env::current_dir()/.horizon-cache/`; the game only
  `set_current_dir`s for `--game-dir`, which this port's e2e never passes, so the
  cache is the e2e scratch root (`$SCRATCH`). The control leg blocks **that**
  path (a plain file), not a guessed one, so it cannot pass vacuously.
- `tools/package.sh` is the packaging step in one command: it rebuilds and
  checks the `.hmod` with the pin's `cargo-horizon-mod` (reproduces 98,176 B /
  `0766d6f3…`), re-syncs `data/`, packs the `.hnmod`, and runs
  `cargo horizon-mod check ../fluxnetworks.hnmod --kind native-mod`.

Verified in game with **only the bundle** (no `--dev-mod`): the whole network
scenario passes, the file the helper wrote is byte-identical to
`content/fluxnetworks.hmod`, and the control (same binary, `.horizon-cache`
unusable) leaves both ids unknown — `fluxnetworks:flux_plug` never arrives in
the inventory (`Unknown item 'fluxnetworks:flux_plug'`) and
`setblock … fluxnetworks:herculean_flux_storage` leaves `minecraft:air`
(`Unknown block type …`). `SELFTEST=1` still fails the flipped no-network
assertion (`EXIT:1`).

This makes the `.hnmod` the single listable artifact of this port; it does not by
itself publish it (licence approval is still required, ARCHITECTURE.md §8).

## Deviations and gaps (slice 1)

1. **A device of another network is an outside energy block.** Upstream refuses
   *every* `IFluxDevice` as an outside block. A one-bundle world has no other
   block with an FE buffer, so the network id is the boundary this slice
   models: a plug or point ignores the devices of its *own* network and treats a
   device of another network as an ordinary FE block. Consequence: two networks
   placed side by side exchange FE through their plugs and points. `FINDINGS.md`
   and the support module say so; the e2e's source and receiver are devices on
   networks 10 and 20.
2. **No configurator, no GUI.** Membership, priority and limit are set with
   `/fluxnetworks network|priority`; upstream sets them in the configurator
   screen. The `flux_configurator` and `admin_configurator` items are carried
   but inert.
3. **No wireless charging.** The controller's `FluxControllerHandler` charges
   players' items; the controller block is carried and does nothing.
4. **No security, members or network creation.** Upstream networks are created
   in the GUI with an owner, members, access levels and a password; here a
   network exists as soon as two devices share an id.
5. **No chunk loading, statistics, colours or connection rendering.** The
   `flux_connection` model and the connection booleans exist, but nothing sets
   the booleans, so a plug or point draws as its centre model.
6. **Waterlogging is not declared.** Upstream's connectors are
   `SimpleWaterloggedBlock`.
7. **Breaking a device loses its FE and its settings** (the loot table's
   `copy_components` is dropped, because `fluxnetworks:device_config` and
   `fluxnetworks:stored_energy` are components this port does not register).
8. **No block blacklist.** Upstream refuses a configurable list of blocks
   (default `actuallyadditions:block_phantom_energyface`) as outside energy
   blocks; this slice has no config.
9. **The storage upgrade and config-wipe recipes are not craftable** (custom
   recipe types upstream).
10. **`flux_dust` has no source recipe**: upstream's "creating flux" is a code
    recipe with no data file, so the crafting chain starts at `flux_core`.
    **Closed in slice 2** (`## Slice 2 — creating flux`): the mechanic is ported and every carried
    shaped recipe now crafts in game.
11. **A harness `energy` command writes a buffer directly.** It is a test
    control, not upstream gameplay (same precedent as `/powah set`).
12. **The slice is not the whole mod**: the controller's behaviour, the
    configurator, the GUI, the wireless network security model and the
    client-side colour/tint handler are unported.

## Verification

Pin `1d63e0a5`, mixins `5a64af93`. Full ladder, commands and the in-game
transcript: [`VALIDATION.md`](VALIDATION.md); evidence in
`ports/_shared/evidence/fluxnetworks/`.

| | Result | Evidence |
|---|---|---|
| `cargo fmt --check` | clean | `evidence/fluxnetworks/fmt.log` (`EXIT:0`) |
| `rustfmt --check mixins/fluxnetworks/embedded_content.rs` | clean | `fmt-helper.log` (`EXIT:0`) |
| `cargo clippy --all-targets -- -D warnings` | clean, 0 warnings | `clippy.log` (`EXIT:0`) |
| runtime tests | **24 passed, 0 failed** | `test.log` (`EXIT:0`) |
| `horizon.py check` | `cargo_exit 0`, 0 warnings, `stage_changed: true` (the new injection) | `check.log`, `cargo-check.log` |
| `horizon.py test` (embedded) | `cargo_exit 0`, **24 passed**, 1240 filtered | `test-embedded.log`, `cargo-test.log` |
| `horizon.py build` | `cargo_exit 0`, `fast` profile | `build.log`, `cargo-build.log` |
| e2e, **one file only** (no `--dev-mod`) | **PASS**, `EXIT:0`; control leg (same binary, `.horizon-cache` a plain file) leaves both ids unknown | `e2e-embedded.log` |
| `SELFTEST=1` (embedded) | **FAIL**, `EXIT:1` at the flipped no-network assertion | `e2e-embedded-selftest.log` |
| pre-embedding history (`--dev-mod content/`) | PASS `EXIT:0` / FAIL `EXIT:1` | `e2e-pre-embedding{,-selftest}.log` |
| `.hmod` | **98,176 B**, `0766d6f3…333cac`, 119 entries, content only, `no code: content only` | `package.log`; rebuilt 2026-11-10 |
| `.hnmod` (content embedded) | **92,951 B**, `f749c928…d7e95`, **14 entries** incl. `mixins/fluxnetworks/data/fluxnetworks-content.hmod` (98,176 B, the built `.hmod`) | `package.log` |
| `cargo horizon-mod check <file> --kind native-mod` | exit 0, `fluxnetworks 0.1.0 (native-mod, format native-1/1d63e0a5…/5a64af93…, for 26.3)` | `package.log` |
| `tools/package.py` twice | **reproducible**: same size and SHA-256 on this box | in-session (92,951 B / `f749c928…` both runs) |
| `check-e2e-logs.py fluxnetworks --strict` | **4/4 ok** | — |
| `upstream_ready.py fluxnetworks` | **READY**, exit 0 (MIT; one `.hnmod` with the content inside is no longer a split port) | — |

`evidence/fluxnetworks/` means `ports/_shared/evidence/fluxnetworks/`. The
format, clippy, test, check, embedded-test and build logs were re-taken on
2026-11-10 with `tools/evidence-ladder.sh` in one build-slot lease on the second
build machine (slot 6), **after** the embedding; `fmt-helper.log` is new with it
(the helper is not in the crate, so `rustfmt` checks it directly). The
`.hmod`/`.hnmod` were then rebuilt by `tools/package.sh`: the `.hmod` is
byte-identical to the previous one (98,176 B / `0766d6f3…`), the `.hnmod` now
carries the content and is 92,951 B / `f749c928…` with 14 entries.
`cargo-check.log`, `cargo-test.log` and `cargo-build.log` are the raw `cargo`
output that `horizon.py` writes to `target/` (disposable), copied next to its
JSON report.

**Why two packing runs gave two hashes.** The same sources packed on the second
machine give 92,880 B / `8d1b2ed4…`, on this box 92,951 B / `f749c928…`
(`package-remote.log` is the far-machine run). The zip's *contents* are
identical; the deflate stream differs because this box's Python links
**zlib-ng 1.3.1** while the far box links plain zlib 1.3.1. Each machine
reproduces its own bytes exactly (two consecutive runs here: `f749c928…` both
times), so nothing non-deterministic is in the bundle; the delivered file is the
one this box produces, and `cargo horizon-mod check … --kind native-mod` was run
on **it** (exit 0).

The red-first run of the implementing session (5 passed / 18 failed on stubs)
left no log, and its tally is 23, not 24 (a test was added after the red phase),
so it is reported as history and is **not** evidence. The green **24** is the
measured count.

The measured numbers the slice is judged on: **120,000 FE/t** through the
network (the herculean storage's transfer, on both the source's loss and the
receiver's gain over the same 8-tick window), **16,000,000 FE conserved** across
the four buffers with the flow stopped, a plug that never holds more than its
800,000 FE buffer, and a full storage that stops the source draining.

Skills: superpowers/using-superpowers, brainstorming, writing-plans, test-driven-development, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices, rust-port. Embedding slice: superpowers/using-superpowers, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices (fmt, clippy `-D warnings`, tests)

## Review repairs (PR #599, 2026-11-11)

Review: `ports/_shared/horizon-mods/reviews/pr-599-fluxnetworks.md` (CHANGES_REQUESTED, 2 items + supplement).

| Reviewer's item | This job |
|---|---|
| 1. artifact not staged | **orchestrator** (no write access) |
| 2. relative link `(SonarSonic/Flux-Networks)` | already absolute; re-checked |
| supplement (1): `support.rs:139-140` overwrote the energy declaration for every block | **fixed** (see below) |
| supplement (2): neither archive carries the MIT notice | **fixed** by the packager (`mixins/fluxnetworks/LICENSE`) |

### the energy hook only writes for its own devices

`on_block_entity_energy` assigned `buffer_of(block)` unconditionally, so `None` for every
foreign block erased a buffer another bundle had registered. It now assigns only when
`buffer_of` returns a buffer for one of the five devices.

Tests (in `support.rs`, run as `game::fluxnetworks_state::tests`):
`a_block_that_is_not_a_device_keeps_the_buffer_it_already_had` (red on the old code) and
`a_flux_device_declares_its_own_buffer`.

Verified: `python3 tools/horizon.py test` in a build slot — staged compile `cargo_exit: 0`,
**26/26** embedded tests pass, including `game::fluxnetworks_state::tests::
a_block_that_is_not_a_device_keeps_the_buffer_it_already_had` and
`a_flux_device_declares_its_own_buffer`
(`ports/_shared/evidence/fluxnetworks/review-repairs-test.log`).

Artifact: `fluxnetworks.hnmod` **94,952 B** `e560bcde7bbde79501f5a45121eef93c2baa13e2207f17086df8002713c42a5a`
(content `fluxnetworks.hmod`).

## Migration to the SDK format (2026-11-18, SDK `0b14f8a`)

Horizon's mod-API merge (#16941, launcher ≥ 0.2.70, PR #17992) retired the `.hnmod` container, and
the registry closed every `.hnmod` submission on 2026-11-18 with one reason. This port's former
submission, **PR #853** (`fluxnetworks` 0.1.0), was closed by that purge. **0.2.0** is the same
behaviour republished as one flat SDK `.hmod`; the changelog text is at the end of this section.

- **Pin:** SDK nightly **`0b14f8acfd97fa2c750e42795249dcc981178ca4`** (2026-11-18,
  `package_format` 6, `content_api` "0.2", toolchain rust 1.99.1), horizon-mixins
  **`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`**, launcher 0.2.78, `cargo-horizon-mod` from
  `horizon-rust/horizon@main`. The SDK is cached in-tree at `../_shared/sdk/0b14f8a/` and was
  copied into `.horizon/sdk/` (gitignored, never packed) so the ladder ran offline.
- **Version:** the port's own **0.2.0** (the retired bundle was 0.1.0; a resubmission is always a
  new version).
- **Class + tier unchanged:** class **content**, tier **machines** — block entities, an energy
  capability and a per-level tick, over the same pure runtime. No machine or deep feature is added
  and none of the recorded gaps is closed by the format.
- **Licence:** read from the jar itself this wave — `FluxNetworks-1.21.1-8.0.0.jar`,
  `META-INF/neoforge.mods.toml` → `license="MIT"`, SHA-256
  `29e511b2d76f85bd247aa213a92bb059ecc037fcc3fed82baffcff97b638d3a6` (kept at
  `ports/_shared/licence-cache/`). MIT = **rank 0**; the port stays **local-only until the
  maintainer approves publishing** (ARCHITECTURE.md §8), unchanged by the migration.

### What moved, and where the retired tree is

Everything the retired format owned was **moved before the first build** into
`ports/_shared/evidence/fluxnetworks/retired-hnmod-0.1.0/` (the tool refuses a root holding
non-package paths: `tools/audit/` is a second crate, and a surviving `mixins/` or `content/` is
refused the same way). The retired tree is evidence, not deleted.

| 0.1.0 path | Now | Evidence |
|---|---|---|
| `mixins/fluxnetworks/runtime.rs` (630 lines) | `src/runtime.rs`, **`cp` + `cmp` clean** | `retired-hnmod-0.1.0/mixins/fluxnetworks/runtime.rs` |
| `mixins/fluxnetworks/support.rs` (675 lines) + 9 helper `.rs` files | `src/lib.rs` (the one `impl Hooks`, the `DevicePart` and the glue) | `retired-hnmod-0.1.0/mixins/` |
| `mixins/fluxnetworks/horizon-mixin.toml` (2 `[[module]]`, 9 `[[injection]]`) | no plan: `Registrar` declarations + typed `Hooks`/Part methods | `retired-hnmod-0.1.0/mixins/` |
| `mixins/fluxnetworks/data/fluxnetworks-content.hmod` + `embedded_content.rs` (AE2 pattern) | **gone**: the package carries `assets/` and `data/` itself | `retired-hnmod-0.1.0/mixins/` |
| `content/` (`horizon-mod.toml`, `assets/`, `data/`, `outside/`, `target/`, `fluxnetworks.hmod`) | `assets/` + `data/` at the package root; the rest is history | `retired-hnmod-0.1.0/content/` |
| `fluxnetworks.hnmod` (94,952 B, content embedded) | `fluxnetworks.hmod` (125,597 B, native **and** content) | `retired-hnmod-0.1.0/fluxnetworks.hnmod` |
| `compatibility.json`, `rust-toolchain.toml` | `[native]` in `horizon-mod.toml`; the pinned commit decides the toolchain | `retired-hnmod-0.1.0/` |
| `tools/horizon.py`, `package.py`, `package.sh`, `test_stage.py`, `test_template.py`, `notice.py`, `evidence-ladder.sh`, `hmod-load*.sh`, `e2e-remote.sh`, `tools/audit/`, `.github/`, the port's old `AGENTS.md` | deleted from the port: staging, `.hnmod` packaging, the content-notice re-zip and the plugin audit belong to the retired format | `retired-hnmod-0.1.0/tools/`, `…/tools-audit/`, `…/github/`, `…/AGENTS.md` |
| `Cargo.lock` | removed: it is packed and changes the artifact (trap 15) | not kept |

**Kept and updated:** `tools/e2e.sh` (same scenario and assertions; `GAME=`/`CONTROL_GAME=` for a
launcher-built game, `SELFTEST=1` unchanged), `tools/carry_content.py` (now writes `assets/` and
`data/` at the package root and parks the two code-recipe families in
`../_shared/evidence/fluxnetworks/outside/`), `notes.md`, `FINDINGS.md`, `VALIDATION.md` (marked
historical), `README.md` (rewritten for the flat layout), `LICENSE` and a new `LICENSE-NOTICE.md`,
both inside the package.

### File map (0.2.0)

| Path | What it is |
|---|---|
| `horizon-mod.toml` | the only manifest and identity: `id = "fluxnetworks"`, `version = "0.2.0"`, `api = "0.2"`, `side = "both"`, `capabilities = ["registries"]`, `[native]` pinned to `0b14f8a`/`e59da90` |
| `Cargo.toml` | one crate, `edition = "2024"`; its only dependency is `horizon-mod-api` from `.horizon/sdk/` |
| `src/lib.rs` | the one `#[mod_api::horizon_mod] impl Hooks` (`register`, `register_commands`, `run_mod_command`, `world_lifecycle`, `level_tick_post`), the `DevicePart`, the network pass and 5 glue tests |
| `src/runtime.rs` | the retired file, **byte-identical** (`0902dbea…8cdf`, `cmp` clean), 24 tests |
| `assets/`, `data/` | the carried content (90 + 28 files), moved from `content/` |
| `icon.png` | the jar's own `logoFile` (`assets/fluxnetworks/icon.png`), 256×256, 2,110 B, byte-identical |
| `LICENSE`, `LICENSE-NOTICE.md`, `README.md` | root documents, **inside** the package |
| `fluxnetworks.hmod` | the deliverable (125,597 B, sha256 `abf4aa1c…d399`) |
| `tools/e2e.sh`, `tools/carry_content.py` | the in-game scenario and the carry pipeline |

### Helper → hook table

Each former `[[injection]]` is now a typed method, a declaration, or a recorded gap; no hook was
dropped silently.

| 0.1.0 (hook, API, helper) | 0.2.0 | Notes |
|---|---|---|
| `embedded-content` (`content_root` raw target, HEAD, cancellable, `embedded_content.rs`) | **nothing** | the flat `.hmod` carries the content; the write-out, its static blob and its control leg are all gone |
| `block-entity` (`block_entity` 15, `block_entity.rs`) | five `Registrar::block_entity_type(block).blocks(&[block])` declarations + `BlockEntityPart::block_entity` → `Some(Compound::new())` | one type per block, so `entity.id()` is the block id as 0.1.0's `(id, compound)` pair made it |
| `block-entity-ticks` (15, `ticks.rs`) | `BlockEntityPart::block_entity_ticks` → `event.ticks = true` | the same five types |
| `block-entity-energy` (142, `energy.rs`) | `Registrar::block_energy(&block, buffer_of(block))`, five times | a **declaration**, not a hook: the game answers `horizon:energy` from it and keeps the amount in the entity's `energy` int |
| `block-entity-tick` (17, `tick.rs`) | `BlockEntityPart::block_entity_tick` → queue the position, `BlockEntityTickReply::Unchanged` | a ticked block entity is out of the world, so it only says where it is |
| `level-tick-post` (23, `level_tick_post.rs`) | `Hooks::level_tick_post` (`HostSide::Server`, `event.level`) | the whole cycle runs here, as in 0.1.0 |
| `register-commands` (8, `register.rs`) | `Hooks::register_commands` (four `Command::new` usages) | the four usages are unchanged |
| `perform-command` (8, `perform.rs`) | `Hooks::run_mod_command` + typed `CommandInvocation`, `Flow::Cancel` | readout text byte-identical (trap 7) |
| `world-lifecycle` (v1, `world.rs`) | `Hooks::world_lifecycle` | clears the queue and the last cycle's requests; `WorldPhase` is ignored, as before |
| `[[module]] runtime` | `pub mod runtime;` | the file itself is untouched |
| `[[module]] state` (`support.rs`) | `src/lib.rs` | no support module: the glue is one file and no `//!` header hides in it |
| `native_energy` / `native_move_energy` (142) | `LevelRef::capability(&ENERGY, …)` + `move_energy(host, from, to, max, tx)` inside `host.transaction` | see traps 2–5 |
| `native_with_mod_block_entity` | `host.block_entity(level, pos)` then `BlockEntityRef::data` / `merge_data` | the read and the two writes (`energy`, `network_id`/`priority`) |
| `game.native_dimension()` | `LevelRef` (the tick's `event.level`; `list_levels()` + `in_place` for a tick that is not told its level) | the queue and the limiter are keyed by `LevelRef`, not a dimension string |
| `game.world.game_time` | `LevelRef::game_time(host)` | the readout's `t=` |
| `game.native_feedback(Text::literal(..))` | `host.commands().feedback(Text::literal(..))` | same lines |
| `NativeCommand::new` / `::op` | `Command::new` / `Command::op` | same usages |

### The ladder (all in a `build-slot.sh` lease, `CARGO_INCREMENTAL=0`, no `--target-dir`)

`build-slot.sh` still speaks the retired tool, so it is the **slot guard**, not the driver: the SDK
crate is compiled in place against `.horizon/sdk/`, with no staging step and no staged tree.

| Step | Command | Result | Evidence |
|---|---|---|---|
| runtime only, before any glue | `cargo test --offline` | **24 passed**, 0 failed | `sdk-test-runtime.log` |
| glue red-first | `cargo test --offline` | **24 passed, 5 failed**, each `not yet implemented: …` | `sdk-glue-red.log` |
| tests | `cargo test --offline` | **29 passed**, 0 failed (24 `runtime::`, 5 `tests::`) | `sdk-test.log` |
| lints | `cargo clippy --all-targets --offline -- -D warnings` | exit 0, **0 warnings** | `sdk-clippy.log` |
| format | `cargo fmt --check` (**not** `--all`: that checks the fetched SDK) | clean | `sdk-fmt.log` |
| package | `rm -f Cargo.lock && cargo horizon-mod build` | `SDK 0b14f8a … in ./.horizon/sdk`, `Built ./target/horizon-mod/fluxnetworks.hmod` | `sdk-build.log` |
| registry check | `cargo horizon-mod check fluxnetworks.hmod` | **EXIT 0** (quoted below) | `sdk-check-fits.log` |
| fits the pinned build | `cargo horizon-mod fits fluxnetworks.hmod --build nightly` | `0b14f8a  made for`, **EXIT 0** | `sdk-check-fits.log` |
| second build | `rm -f Cargo.lock && cargo horizon-mod build` | **byte-identical**: 125,597 B, `abf4aa1c…d399` both times | `sdk-build-second.log`, `sdk-runtime-sha256.txt` |
| runtime untouched | `unzip -p fluxnetworks.hmod src/runtime.rs \| sha256sum` | `0902dbea…8cdf` = the retired file = `src/runtime.rs` | `sdk-runtime-sha256.txt` |
| content carry | `python3 tools/carry_content.py --check` | 0 diffs; `diff -r` against the retired `content/` clean | `sdk-content-carry.txt` |

`check` output, verbatim:

```
fluxnetworks 0.2.0 (native-mod, format native-1/0b14f8acfd97fa2c750e42795249dcc981178ca4/e59da9094f0c2fdda6c627fe9eff22dec6a906f0, for 26.3)
  125597 bytes, SHA-256 abf4aa1c21ac004147c08e0ba23d9779fc78690d0b7affd20f32416bdf6dd399, unsigned
  native source: unsandboxed; not executed by the registry check. Compile and test against the exact pinned Horizon and Mixins revisions before review.
```

`fits` output, verbatim:

```
Flux Networks:
0b14f8a  made for
```

The archive is **127 entries** (`sdk-package-entries.txt`): 7 root (`horizon-mod.toml`,
`Cargo.toml`, `LICENSE`, `LICENSE-NOTICE.md`, `README.md`, `icon.png`, `uses.json`), 2 `src/`
(`lib.rs`, `runtime.rs`), 90 `assets/` and 28 `data/`. **No `Cargo.lock`** (trap 15).

`uses.json` (`sdk-uses.json`, extracted from the package) is the migration's best summary: **244
surface entries** — 21 `hook:`, 29 `method:`, 53 `handle:`, 110 `type:`, 22 `trait:`, 3 `part:`, 2
`macro:`, 2 `fn:` and 2 `capability:`. The rows that name what this package really implements are
`hook:block_entity`, `hook:block_entity_ticks`, `hook:block_entity_tick`, `hook:level_tick_post`,
`hook:register_commands`, `hook:run_mod_command`, `hook:world_lifecycle`;
`method:BlockEntityPart::{block_entity,block_entity_ticks,block_entity_tick}`,
`method:DefaultCapabilitiesRegistrar::block_energy`, `method:Registrar::block_entity_type`/
`handle:Registrar::block_entity_part`, `method:EnergyHandler::{amount,capacity,insert,extract}`,
`fn:move_energy`, `handle:LevelRef::{capability,block,game_time,in_place}`,
`handle:BlockEntityRef::{data,merge_data}`, and the two capabilities `horizon:energy` +
`registries`. The tool's name matching also lists `hook:blocks` and `hook:command` (the words
`blocks` and `command` appear in `.blocks(&[…])` and `event.command`); neither hook is implemented
in `src/lib.rs`.

### Deviations the SDK forces (recorded, not hidden)

- **The energy capability reports no per-call insert/extract cap.** API 142's `EnergyView` exposed
  `max_insert`/`max_extract`, so 0.1.0 capped a point's `desired` and a plug's `offer` by the
  neighbour's per-call limit before planning. The SDK's `EnergyHandler` exposes only
  `amount`/`capacity` (and `insert`/`extract`, which clamp themselves), so the port plans from what
  the neighbour *holds* and *can hold*, and the destination's own handler decides how much of one
  call it takes. What a player sees is unchanged — the throughput stays the destination's per-call
  limit (the e2e's 120,000 FE/t measurement is the herculean storage's `max_insert`, and the move
  result feeds `pushed`/`pulled` so a partial move is accounted for) — but a point may now buffer up
  to its own limit for a cycle instead of only what one insert call can pass on. Recorded as an API
  gap: "the capability should report its per-call caps".
- **`block_entity` is a Part method, not a global hook.** The initial compound is unchanged (empty),
  and the five types are declared per block so a device's id is its block id, as the retired hook's
  `(id, compound)` pair made it.
- **A drain-only neighbour is counted as a consumer.** The capability answers for a side that reaches
  either a fill or a drain and reports `capacity`/`amount` but no `can_insert`/`can_extract`, so a
  point beside a block that only *gives* FE plans for its room, while the insert itself moves 0.
  Consequence: up to one point buffer (800,000 FE) can be pulled into the point and held there until
  a real consumer appears; nothing is created or destroyed, and a full point asks for nothing, so the
  source stops draining. 0.1.0 tested `can_insert` first and would not have pulled that energy at
  all. Recorded as an API gap: "a capability should say which way a side reaches" (`can_insert`).

### Gaps after the migration

1. **No in-game run at `0b14f8a`.** The scenario needs a game the **launcher built with this
   package** (`horizon-launcher native-build ports/fluxnetworks --instance <instance>
   --trust-native-code`); `horizon-launcher` is not installed and no game build exists at this pin
   (the shared binaries are at `1d63e0a5`/`41f822b6`). `tools/e2e.sh` was updated for that launch
   path (`GAME=`, optional `CONTROL_GAME=`, `SELFTEST=1` kept) and no 0.2.0 transcript is claimed.
   The 0.1.0 runs remain the behaviour's evidence, for a `src/runtime.rs` that did not change.
2. **No logger.** The SDK has no log service and a native mod may not add a crate, so the retired
   embedded-content helper's `log::warn!` fallback is gone; it had no successor need (content now
   ships in the package) and the rest of the slice never logged.
3. **Five block entity types, not one.** A declared type's id is what `entity.id()` answers, and the
   devices are told apart by that id, so one type per block keeps the 0.1.0 mapping exact; the price
   is five declarations instead of one.
4. Everything the 0.1.0 gap list records is unchanged: no configurator, GUI, wireless charging,
   controller behaviour, chunk loading, network security, statistics, colours or connection
   rendering; breaking a device still loses its FE and settings; no block blacklist; the two code
   recipe families are still parked (`../_shared/evidence/fluxnetworks/outside/`); `flux_dust` has
   no source recipe.
5. **`/fluxnetworks energy` is still a harness control**, as 0.1.0 documented (same precedent as
   `/powah set`).

### Upstream defects fixed

None found in this migration. The 0.1.0 repair (the energy helper overwrote every block's declared
buffer, review PR #599) was a **port** defect, fixed then; the format removes its cause entirely,
because `block_energy` is declared per block and no hook can write another block's answer. One
documentation correction, not a behaviour change: the example readout in this file claimed
`comparator 15` for `15880000/16000000`, while vanilla's formula (`1 + floor(14 * amount /
capacity)`) gives **14**; the new glue test pins 14.

### Traps this migration cost (new to this port)

1. **A retired path refuses the package, and `tools/audit/` is a crate.** The first attempt to
   build with the old tree present would have failed on `unexpected package path:
   tools/audit/src/main.rs`; `mixins/`, `content/` and the old tools were moved to the evidence
   folder **before** the first `cargo horizon-mod build`, as this file records above.
2. **Energy is a declaration, not a hook.** `Registrar::block_energy(&block, ModEnergyBuffer)`
   answers `horizon:energy`; the amount is not in the declaration, it lives in the block entity's
   saved `energy` int, so the runtime's `Device.buffer` read is unchanged and the engine keeps the
   books.
3. **`move_energy` replaces `native_move_energy`, and it takes two handles.** Read both
   `Capability<dyn EnergyHandler>`s through `LevelRef::capability(host, &ENERGY, pos, Some(side))`
   and call `move_energy(host, &from, &to, max, tx)` inside `host.transaction(…)`; that needs
   `HostTransaction` in scope, and it retries a partial insert with the accepted amount (a plugin
   answering more than asked aborts the move as `Overanswered`).
4. **`energy_amount`/`energy_capacity` are backend methods, not host methods.** A mod must go
   through the capability handler (`EnergyHandler::amount/capacity/insert/extract`); `&mut dyn
   ServerHost` exposes none of them.
5. **A capability asked through `None` is not "any face".** Pass `Some(side)`; all six faces are
   declared for both a fill and a drain, and the internal plug→point move asks through
   `Direction::Up` as 0.1.0 did. `energy_amount` also answers `None` for a block entity ticking
   right now, which is why every read and move runs at `level_tick_post`.
6. **`merge_data` answers `Ok(false)` for "no change".** That is a successful write of the value it
   already had, not a missing device: `/fluxnetworks energy 0` on an empty storage must still print
   `Energy set to 0 FE.` or the e2e's `load_energy` never sees its reply.
7. **The readout text is a contract.** The e2e parses `at X,Y,Z:` and `network …`; the SDK's typed
   `RunModCommandEvent` changes the *routing* (a parsed command with typed arguments) but not the
   line, which is produced by a pure `status_line` the glue test pins byte for byte.
8. **A command the dispatcher refuses never reaches the hook.** A bad `<pos:block_pos>` or
   `<amount:integer>` is refused while parsing, so 0.1.0's "must be a whole number" answers are
   only reachable for a wrong field type; they are kept for that case, and the *missing* argument
   case still prints the usage line.
9. **`BlockEntityTickReply::Unchanged` keeps a passive tick cheap.** The dispatch defaults the reply
   to `Changed`; a device that only queues its position answers `Unchanged` so it does not dirty the
   chunk every tick.
10. **`block_entity_part` on five types.** Bind each `BlockEntityTypeKey` in a local first (a
    `Registrar` call inside the argument list is a second mutable borrow), and `expect` the
    `Result`: the type was declared on the line above, so a refusal is a programming error.
11. **`register` is where the identity is decided.** The type id is the block id, so
    `device_kind(block)` and `device_kind(entity.id())` agree; declaring one shared type would have
    broken the readout's kind lookup.
12. **No `rust-toolchain.toml`, no `rust-version`.** The crate is edition 2024 and was compiled here
    with rustc 1.97.0 while `sdk.json` declares 1.99.1; the launcher's toolchain builds the package,
    so pinning either would only break a box.
13. **`cargo horizon-mod check` with no file checks host *and* wasm.** Use `check <file.hmod>` for
    the package and `cargo test`/`clippy` for the native build.
14. **A packed `Cargo.lock` changes the artifact.** `cargo test`/`clippy` write one; `rm -f
    Cargo.lock` immediately before `cargo horizon-mod build` and keep it gitignored. Two builds
    then agree byte for byte (125,597 B, `abf4aa1c…d399`).
15. **`cargo fmt --check`, never `--all`.** `.horizon/sdk` is a path dependency, so `--all` reports
    the fetched SDK's own files.
16. **The remote runner has no copy of the port.** `build-slot.sh run` forwarded the first test to
    the second machine and answered `cd: ports/fluxnetworks: No such file or directory`; every step
    here ran with `BUILD_LOCAL=1` in a local lease.

### Changelog for the resubmission (supersedes #853)

> Supersedes **#853**, closed because Horizon retired the `.hnmod` container (#16941 / #17992).
> Changes for `api = "0.2"`: one flat SDK package (`horizon-mod.toml` + `Cargo.toml` + `src/lib.rs`
> + `assets/` + `data/`), `[native]` pinned to nightly
> `0b14f8acfd97fa2c750e42795249dcc981178ca4` and mixins
> `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, launcher 0.2.78, for 26.3. The nine retired
> injections became five typed hooks (`register`, `register_commands`, `run_mod_command`,
> `world_lifecycle`, `level_tick_post`), three `BlockEntityPart` methods, five
> `Registrar::block_entity_type` and five `Registrar::block_energy` declarations, and an energy
> flow through `LevelRef::capability(&ENERGY, …)` + `move_energy` in a transaction instead of the
> old `native_energy`/`native_move_energy` pair; the content half now travels in the same package,
> so the embedded `.hmod`, its `content_root` write-out and the cache control leg are gone;
> `src/runtime.rs` and its 24 tests are unchanged, byte for byte. Package: 127 entries,
> **125,597 B**, sha256 `abf4aa1c21ac004147c08e0ba23d9779fc78690d0b7affd20f32416bdf6dd399`,
> `check` and `fits` exit 0, built twice with identical bytes. No 0.2.0 in-game run: the launcher
> is not installed here (gap 1).

### Migration verification

| Check | Command | Result |
|---|---|---|
| Runtime carried | `cmp` retired vs `src/runtime.rs`, and `unzip -p … src/runtime.rs \| sha256sum` | `0902dbea…8cdf` three times (`sdk-runtime-sha256.txt`) |
| Runtime tests in the new crate, before the glue | `cargo test --offline` | **24 passed**, 0 failed (`sdk-test-runtime.log`) |
| Glue red first | the same, with `todo!()` bodies | **5 failed** of 29 (`sdk-glue-red.log`) |
| Tests with the glue | `cargo test --offline` | **29 passed**, 0 failed (`sdk-test.log`) |
| Lints | `cargo clippy --all-targets --offline -- -D warnings` | exit 0, 0 warnings (`sdk-clippy.log`) |
| Formatting | `cargo fmt --check` | clean (`sdk-fmt.log`) |
| Package | `rm -f Cargo.lock && cargo horizon-mod build` | exit 0 (`sdk-build.log`) |
| Registry check | `cargo horizon-mod check fluxnetworks.hmod` | exit 0, 125,597 B, `abf4aa1c…d399` (`sdk-check-fits.log`) |
| Fits the pinned build | `cargo horizon-mod fits … --build nightly` | `0b14f8a  made for`, exit 0 |
| Same package twice | `cargo horizon-mod build` again | 125,597 B, `abf4aa1c…d399` both times (`sdk-build-second.log`) |
| Content carry unchanged | `carry_content.py --check`, `diff -r` | 0 diffs, 118 files byte-identical (`sdk-content-carry.txt`) |
| Entries | `unzip -l fluxnetworks.hmod` | 127, no `Cargo.lock` (`sdk-package-entries.txt`) |
| Uses | `unzip -p fluxnetworks.hmod uses.json` | 244 entries (`sdk-uses.json`) |

Skills: superpowers/using-superpowers, brainstorming, writing-plans, test-driven-development, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices, rust-port

## Review fixes (2026-11-18)

Source review of PR **#1239** (`var/review/1239-fluxnetworks.md`). Both findings are fixed in
**0.2.1**, and the first in-game run of the 0.2.x package (which this job also completed) found and
fixed a carried-data defect that stopped the world from loading at all.

| # | Finding (`file:line`) | What a player could do | Fix | Test that pins it |
|---|---|---|---|---|
| 1 | `src/lib.rs:281` — level-0 energy/network/priority on any loaded device | a non-operator created or erased FE and reassigned or redirected another player's network at coordinates they chose, with no distance or ownership check | `declare_commands` registers all four usages (`info`, `energy`, `network`, `priority`) with `Command::op(…, 2)`; the dispatcher refuses them below game-master permission before any listener runs. All four are the harness surface for upstream's configurator GUI, which is not ported, so they are operator diagnostics | `tests::the_device_commands_are_operator_only` — RED on the 0.2.0 registration (`left: 0, right: 2`, `evidence/fluxnetworks/review-test-red.log`), GREEN after |
| 2 | `src/lib.rs:240` — a limiter entry per network id, only cleared at a world lifecycle event | reassigning devices to new ids through the operator command grew the table (and the linear search per tick) without bound | `BoundedLimiter`: at most **256** networks (`LIMITER_CAP`), oldest first out, one value per `(level, network)`; a forgotten network simply starts from a full pull again next cycle, so no energy is lost. `world_lifecycle` still clears it | `tests::the_limiter_table_is_bounded` — RED on the unbounded table (1024 entries kept vs the 256 cap), GREEN after; it also pins that updating a remembered id neither grows the table nor moves it |

### Carried-data defect the first run exposed (fixed, not reproduced)

The first `launcher-game.sh run fluxnetworks` (0.2.1, before this fix) ended `EXIT:1` with the
server refusing to load: *"Couldn't parse data file {fluxnetworks:blocks/…@minecraft:loot_table}
.pools[0].condition: No key type in predicate {\"condition\":\"minecraft:survives_explosion\"}"*
(`evidence/fluxnetworks/first-run-loot.log`). All **seven** carried block loot tables still had the
1.21.1 wrapper `{"condition": {"condition": "minecraft:survives_explosion"}}`. The 26.3 shape is the
typed condition `{"condition": {"type": "minecraft:survives_explosion"}}`
(`ports/_shared/AGENT_MOD_STRUCTURE.md` §6), which is what powah's carried tables already use.
Each file is now `{"condition": {"type": "minecraft:survives_explosion"}}` and the world loads.
This is **fail-capable evidence by itself**: with the old bytes the server stops at the first tick,
so the e2e cannot run at all. (The carry came from the 1.21.1 jar; the upgrade was missed, not an
upstream bug.)

### Verification (0.2.1)

| Step | Command | Result |
|---|---|---|
| unit tests | `cargo test` | **31 passed**, 0 failed (`evidence/fluxnetworks/sdk-test.log`) |
| lints | `cargo clippy --all-targets -- -D warnings` | exit 0, no warning (`evidence/fluxnetworks/sdk-clippy.log`) |
| formatting | `cargo fmt --check` | exit 0, empty (`evidence/fluxnetworks/sdk-fmt.log`) |
| package, twice | `rm -f Cargo.lock; cargo horizon-mod build` ×2, `cmp` | **byte-identical**, 126,922 B (`evidence/fluxnetworks/sdk-build{,-second}.log`) |
| registry check | `cargo horizon-mod check fluxnetworks.hmod` | **exit 0**, 126,922 B, SHA-256 `9e4c845c…c65d06` (`evidence/fluxnetworks/sdk-check.log`) |
| fits the build | `cargo horizon-mod fits fluxnetworks.hmod --build nightly` | `Flux Networks: / 0b14f8a  made for`, **exit 0** (`evidence/fluxnetworks/sdk-fits.log`) |
| in-game | `ports/tools/launcher-game.sh run fluxnetworks` | `PASS: …` + `EXIT:0` (`evidence/fluxnetworks-e2e/e2e-launcher.log`) |
| control (same Nightly, no package) | `GAME=$CONTROL_GAME … bash ports/fluxnetworks/tools/e2e.sh` | `FAIL: expected fluxnetworks:herculean_flux_storage at 0,100,0, got 'minecraft:air'` + `EXIT:1` |
| selftest | `SELFTEST=1 …` | `FAIL: SELFTEST flipped: an unlinked network moved no energy, as it should` + `EXIT:1` |
| log gate | `python3 ports/tools/check-e2e-logs.py fluxnetworks-e2e --strict` | `logs=3 ok=3 missing-exit=0 bad-exit=0`, exit 0 |

Harness-only change: the scenario's scratch and socket paths now honour `E2E_TMP`
(`${E2E_TMP:-/tmp/opencode}`), because this box's `/tmp` is a full tmpfs (the launcher wrapper
exports `E2E_TMP` under `~/.cache/horizon-e2e`). No assertion changed.

**A rebuild is needed after a data-only change.** `launcher-game.sh run` reuses an existing export,
and `built_tag` matches a built version by a byte-identical `horizon-mod.toml`; a changed `data/`
file does not invalidate either, so the first run after the loot fix still loaded the old content.
Removing that mod's `versions/native-*` folder and its export forced the rebuild. Recorded so the
next data-only fix does not lose a run.

Package: `fluxnetworks.hmod`, **126,922 B**, sha256
`9e4c845c1931599da64e82ae678a51ada7f715ef458057feb88b63b578c65d06`.

## Slice 2 — creating flux: the flux-dust source, and the seven carried recipes proven (2026-11-21)

**Pin:** Horizon `0a875da91dbb0fdcb7ba8c42c11341aa229af51a` (Nightly 2026-11-20) + horizon-mixins
`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, `package_format` 6, `content_api` 0.2. Version
**0.2.2**. The manifest moved with the campaign pin this wave; `cargo horizon-mod update` reported
"none of the 346 items it uses changed", so no source had to change for the repin.

### The gap this slice closes

`## Deviations and gaps (slice 1)` item 10:

> 10. **`flux_dust` has no source recipe**: upstream's "creating flux" is a code
>     recipe with no data file, so the crafting chain starts at `flux_core`.

Measured in the shipped 0.2.1 data: `flux_core`, `flux_block`, `flux_plug`, `flux_point`,
`flux_controller`, `basic_flux_storage` and `flux_configurator` **all need
`fluxnetworks:flux_dust`**, and nothing produced it. All seven carried recipes were dead data: a
player could not obtain a single Flux Networks item in survival, however long they played. This is
the highest-value remaining family, and it is a **recipe** family, which the parity rule names as
the sharpest case.

### What upstream does (analysis; clean-room rewrite)

`SonarSonic/Flux-Networks` branch `1.21`, commit `ea8b6e0cf708098ab2f02bf5c3d5c1f0ed2a1f5f`
(shallow clone at `ports/_shared/upstream/fluxnetworks`, deleted after these findings were recorded).

| File | What it fixes |
|---|---|
| `register/EventHandler.onPlayerInteract` (`LeftClickBlock`, `receiveCanceled`) | **the mechanic.** The clicked block must be **obsidian**; the block **two below** must be **bedrock** or **a flux block**. Every **redstone** item entity inside `new AABB(pos.below())` — the one-block gap between them — is counted and discarded, stopping at `itemCount >= 512`. That many **flux dust** item entities spawn at the click (`.setNoPickUpDelay()`, motion `(0, 0.2, 0)`), the obsidian is removed, and one `nextDouble() > Math.pow(0.9, itemCount >> 3)` roll decides whether the gap becomes **cobblestone** (the press collapses) or **the obsidian** (it slid down one). Sounds: `entity.dragon_fireball.explode` for cobblestone, `entity.generic.explode` otherwise; `ParticleTypes.LAVA` count `Mth.clamp(itemCount >> 2, 4, 64)`. The click is cancelled. |
| `FluxConfig` (`general`) | `enableFluxRecipe` default **true** ("redstone being compressed with the bedrock and obsidian to get flux dusts"). |
| `client/jei/CreatingFluxRecipeCategory.getRecipes` | the JEI family is exactly two recipes — bedrock base and flux-block base, obsidian crusher, redstone in, flux dust out — which is the upstream statement of the two bases. |
| `register/RegistryItems`, `common/item/FluxDustItem` | `flux_dust` is a plain item with a tooltip (already carried by the port). |
| `EventHandler.onEntityAdded` (`FireItemEntity`, "old recipe") | **commented out upstream** — dead code, not ported. |

**Tier:** class `content`, tier `machines`, unchanged. This is a block-interaction rule with item
entities, block state and a level random — below the tier's ceiling (no block entity, no menu, no
renderer), so the row is not re-tiered and nothing above `machines` is attempted.

### What changed

| File | Change |
|---|---|
| `src/runtime.rs` | `CREATION_ENABLED`, `CREATION_CAP` (512), `creation_base_ok`, `is_crusher`, `created_dust`, `GapAfter`, `gap_after`, `creation_particles` — the whole rule, pure, with 5 new tests |
| `src/lib.rs` | the `left_click_block` hook (the slice's one new hook), `flux_creation_site`, `take_redstone`, the dust spawn, and a `CreationKeys` cache resolved in `register` (`item_key`, `block_key`, `particle_key`) — **2 new glue tests**, 37 tests in total |
| `tools/craft_check.py` | **new**: reads each carried `data/fluxnetworks/recipe/*.json`, gives exactly the pattern's ingredients, places them with right-clicks (grid 100–108, result 200), asserts the declared id and count, that the result reached the inventory, that the grid is empty and that **every ingredient was consumed by its declared count**; two wrong patterns must leave the result empty. Adapted from the proven `bigreactors/tools/craft_check.py`, but the patterns are read from the package instead of transcribed. |
| `tools/e2e.sh` | five creation phases + the crafting phase, before slice 1's network phases (which are unchanged); launch moved `--headless` → `--offscreen`, because `click-slot` needs a renderer |
| `horizon-mod.toml`, `Cargo.toml` | 0.2.1 → **0.2.2**; `[native] horizon` → `0a875da…` |

The mechanic's site, as the hook sees it:

```rust
fn flux_creation_site(clicked: &str, base: &str) -> bool {
    runtime::is_crusher(clicked) && runtime::creation_base_ok(base)
}
```
with `is_crusher` = `minecraft:obsidian` and `creation_base_ok` = `minecraft:bedrock` or
`fluxnetworks:flux_block`, both pinned by tests.

### TDD: red, then green

The family's rule first, as runtime tests, before any implementation:

```
error[E0425]: cannot find function `creation_base_ok` in this scope
error[E0433]: cannot find type `GapAfter` in this scope
error[E0425]: cannot find function `creation_particles` in this scope
error: could not compile `fluxnetworks` (lib test) due to 33 previous errors
```

(`ports/_shared/evidence/fluxnetworks/slice2-red.log`.) Then the glue's own red:

```
error[E0425]: cannot find function `flux_creation_site` in this scope
error: could not compile `fluxnetworks` (lib test) due to 5 previous errors
```

(`slice2-glue-red.log`.) Green: **37 passed, 0 failed** (`sdk-test.log`), clippy 0 warnings
(`sdk-clippy.log`), `cargo fmt --check` clean (`sdk-fmt.log`).

### The in-game leg (launcher 0.2.92, Nightly `nightly-2026-11-20-0a875da`)

`ports/_shared/evidence/fluxnetworks-e2e/e2e-launcher.log` — `PASS` + `EXIT:0`. The creation
phases, verbatim:

```
== creating flux: a stone base is refused (the differential) ==
  redstone in the gap: 8 in 1 stack(s); dust 0
  after the click: obsidian at 6,102,6 is 'minecraft:air', gap 'minecraft:air', redstone left 8, dust 0
== creating flux: 8 redstone between bedrock and obsidian ==
  redstone left 0; obsidian at 8,102,8 is 'minecraft:air'; gap 'minecraft:cobblestone'
  dust 0 -> 8
== creating flux: no redstone, nothing happens ==
  dust 8 -> 8; gap 'minecraft:air'; redstone 0
== creating flux: a flux block is the other base ==
  dust 8 -> 16; gap 'minecraft:obsidian'
== creating flux: 576 redstone gives 512 dust and leaves a stack ==
  redstone in the gap: 576 in 9 stacks
  redstone left 64 in 1 stack(s)
  dust 16 -> 528
== the seven carried shaped recipes, in a real crafting table ==
  block(17,100,10) = minecraft:crafting_table
flux_core -> {"count": 4, "item": "fluxnetworks:flux_core", "slot": 200}
basic_flux_storage -> {"count": 1, "item": "fluxnetworks:basic_flux_storage", "slot": 200}
flux_block -> {"count": 1, "item": "fluxnetworks:flux_block", "slot": 200}
flux_configurator -> {"count": 1, "item": "fluxnetworks:flux_configurator", "slot": 200}
flux_controller -> {"count": 1, "item": "fluxnetworks:flux_controller", "slot": 200}
flux_plug -> {"count": 1, "item": "fluxnetworks:flux_plug", "slot": 200}
flux_point -> {"count": 1, "item": "fluxnetworks:flux_point", "slot": 200}
flux_core with minecraft:redstone (control) -> null
flux_plug with minecraft:iron_ingot (control) -> null
CRAFTED: 7 carried shaped recipes, each with its declared id and count, its ingredients consumed, and two wrong patterns left empty
```

The `flux_core` row consumed **4 of the flux dust the mechanic had just produced** (its dust is
read from the inventory, not given), so the chain redstone → dust → core is one continuous run.
The two wrong-pattern controls are in the log with a `null` result slot, and one differential is a
*block*: obsidian on **stone** keeps its redstone and makes no dust (the same shape that works on
bedrock two blocks above), so the base test cannot pass vacuously.

Three legs, each fail-capable (`ports/_shared/evidence/fluxnetworks-e2e/`):

| Leg | Result | Log |
|---|---|---|
| the same command on the package | `PASS` + `EXIT:0` | `e2e-launcher.log` |
| the same command on the **same Nightly without the package** (`GAME=$CONTROL_GAME`) | `FAIL: the redstone was not consumed (8 left)` + `EXIT:1` | `e2e-launcher-control.log` |
| `SELFTEST=1` (flips the stone-base assertion) | `FAIL: SELFTEST flipped: the redstone survived a stone base, as it should` + `EXIT:1` | `e2e-launcher-selftest.log` |

`python3 ports/tools/check-e2e-logs.py fluxnetworks-e2e --strict` → `logs=4 ok=4
missing-exit=0 bad-exit=0`.

### Coverage (parity rule)

| Family (the jar's) | State | Where |
|---|---|---|
| creating flux — bedrock base | **tested** | e2e `creating flux: 8 redstone between bedrock and obsidian` (8 redstone → 8 dust, obsidian consumed, gap obsidian or cobble) |
| creating flux — flux-block base | **tested** | e2e `a flux block is the other base` (8 → 8 dust) |
| creating flux — base test | **tested** | e2e `a stone base is refused` (differential: redstone left) |
| creating flux — nothing to press | **tested** | e2e `no redstone, nothing happens` |
| creating flux — 512 cap | **tested** | e2e `576 redstone gives 512 dust and leaves a stack` (512 dust, exactly one 64 stack left) |
| creating flux — cobblestone roll / obsidian roll | **tested** | both outcomes observed across the runs (`gap 'minecraft:obsidian'` and `'minecraft:cobblestone'`); the rule is pinned by `a_bigger_batch_turns_the_gap_to_cobblestone_more_often` |
| creating flux — dust into a recipe | **tested** | the `flux_core` row consumes 4 of the dust the mechanic made |
| recipe `flux_core` (4) | **tested** | craft_check, declared id/count, 4 dust + 4 obsidian + 1 ender eye consumed |
| recipe `flux_block` | **tested** | craft_check (5 dust + 4 cores) |
| recipe `flux_plug` | **tested** | craft_check (4 cores + 1 flux block) |
| recipe `flux_point` | **tested** | craft_check (4 cores + 1 redstone block) |
| recipe `flux_controller` | **tested** | craft_check (5 flux blocks + 1 core + 2 dust) |
| recipe `basic_flux_storage` | **tested** | craft_check (6 flux blocks + 2 glass panes) |
| recipe `flux_configurator` | **tested** | craft_check (2 dust + 1 ender eye + 2 obsidian) |
| recipe type `fluxnetworks:flux_storage_recipe` (2) | **not ported** | a custom recipe type; the data files are parked in `../_shared/evidence/fluxnetworks/outside/`. **Now expressible**: the pin has `Registrar::register_recipe_type` + `add_recipe_serializer`, so this is a candidate slice, not a gap in the pin. |
| recipe type `fluxnetworks:config_wipe_recipe` (6) | **not ported** | same (custom recipe type, same `register_recipe_type` route) |
| JEI "creating flux" page | **not ported** | client half; JEI is not a Horizon surface |
| lava particles at the click | **requested, not measured** | `send_particles` with `particle_key("minecraft:lava")` and `Mth.clamp(count >> 2, 4, 64)`; the leg is `--offscreen` and no screenshot is claimed, so this is "code path exercised", not visual evidence |
| both explosion sounds | **requested, not measured** | `play_sound` with the two vanilla events (`HORIZON_NO_AUDIO` is set in the leg) |

### Deviations and API gaps (slice 2)

1. **The dust ships as 64-stacks, not one entity.** Upstream spawns one item entity holding the
   whole batch (`new ItemStack(FLUX_DUST, itemCount)`), up to 512. `LevelRef::spawn_item_with_motion`
   refuses a stack above the item's limit — a 512-dust stack spawned **nothing** (measured: the
   first cap run produced no dust entity at all), so the port spawns `ceil(dust / 64)` entities of at
   most 64. The total, the count and the pickup are identical; recorded as an API difference.
2. **A stack that crosses the 512 cap is consumed whole.** Upstream counts a stack and discards it,
   then breaks at `itemCount >= 512`, so 9 stacks of 64 give 512 dust and the 9th stack survives only
   because the break runs *before* it. That is upstream's own arithmetic and is kept (the e2e pins
   it: 576 redstone → 512 dust, 64 left).
3. **The config option is not ported** (`enableFluxRecipe`, default true). There is no config
   surface in this package, so the mechanic is always on, which is what a default player gets.
   Recorded rather than invented.
4. **The removed obsidian is written with `NEIGHBORS | CLIENTS`.** `ctl block` reads the client's
   level, and the first version wrote the removal with no flags (`NONE`), so the *server* had air
   while the client still showed obsidian and the e2e read obsidian back — a harness-visible
   difference that also matters to a player whose client would keep a ghost block. Vanilla's
   `removeBlock` uses the same update flags.
5. **No logger.** The SDK has none (recorded since the migration); a spawn the game refuses is
   skipped silently, which is why the split in deviation 1 is a rule, not a fallback.
6. **No JEI/EMI page and no client half.** Out of tier.

No upstream *defect* was found in this slice; the 512 rounding above is upstream's design, recorded
as a deviation.

### Gaps after slice 2

Everything the migration's gap list records still stands, except:

- **closed:** `flux_dust` has no source recipe (this slice);
- **closed by measurement:** the seven carried shaped recipes are no longer dead data;
- **new, and now expressible:** the two custom recipe types (`flux_storage_recipe`,
  `config_wipe_recipe`) can be registered at this pin (`register_recipe_type` +
  `add_recipe_serializer`) — a later slice, not a blocker;
- still open: no configurator GUI, no wireless charging, no chunk loading, network security,
  statistics, connection rendering, waterlogging, breaking a device losing its FE/settings, the
  block blacklist, and the client colour/tint half.

### Ladder (0.2.2, `build-slot.sh` leases, `CARGO_INCREMENTAL=0`, no `--target-dir`)

| Step | Command | Result | Evidence |
|---|---|---|---|
| tests | `cargo test --offline` | **37 passed**, 0 failed | `sdk-test.log` |
| lints | `cargo clippy --all-targets --offline -- -D warnings` | exit 0, **0 warnings** | `sdk-clippy.log` |
| format | `cargo fmt --check` | clean | `sdk-fmt.log` |
| package ×2 | `rm -f Cargo.lock; cargo horizon-mod build`, twice, `cmp` | **byte-identical**, 133,581 B | `sdk-build.log`, `sdk-build-second.log` |
| registry check | `cargo horizon-mod check fluxnetworks.hmod` | **exit 0**, 133,581 B, SHA-256 `a6d7c93e…f38b4` | `sdk-check.log` |
| fits the pin | `cargo horizon-mod fits fluxnetworks.hmod --build 0a875da` | `0a875da  made for`, exit 0 | `sdk-fits.log` |
| fits a **different** build | `cargo horizon-mod fits fluxnetworks.hmod --build 0b14f8a` | `0b14f8a  fits`, exit 0 | `sdk-fits.log` |
| package gate | `python3 ports/tools/verify-sdk-package.py fluxnetworks` | **1/1** | this session |
| entries | `unzip -l fluxnetworks.hmod` | **127 entries**, no `Cargo.lock` | `slice2-package-entries.txt` |
| uses | `unzip -p fluxnetworks.hmod uses.json` | `made_for 0a875da…`, **345 entries** | `slice2-uses-summary.txt` |
| runtime in the package | `unzip -p … src/runtime.rs \| sha256sum` | `8cbbdb78…a06fe4f`, same as the tree | this session |

`check` output, verbatim:

```
fluxnetworks 0.2.2 (native-mod, format native-1/0a875da91dbb0fdcb7ba8c42c11341aa229af51a/e59da9094f0c2fdda6c627fe9eff22dec6a906f0, for 26.3)
  133581 bytes, SHA-256 a6d7c93ecf4b9db2dcd81669bae99dc1194889a71ccbf79b24b94bd58e5f38b4, unsigned
  native source: unsandboxed; not executed by the registry check. Compile and test against the exact pinned Horizon and Mixins revisions before review.
```

`fits`, both lines in one log (`sdk-fits.log`):

```
== fits --build 0a875da (the pin: made for) ==
Flux Networks:
0a875da  made for
FITS_PIN_EXIT:0
== fits --build 0b14f8a (the previous build: the real check) ==
Flux Networks:
0b14f8a  fits
FITS_OLD_EXIT:0
```

### Traps this slice cost

1. **`ctl block` reads the client level.** A server-side write without `CLIENTS` leaves a ghost block
   the harness reads back (see deviation 4). Use `NEIGHBORS | CLIENTS`, as vanilla's `UPDATE_ALL`.
2. **A player one block from the press eats the redstone.** The first run measured 0 redstone because
   the player stood adjacent and picked the pile up before the click; the press is now worked from two
   blocks away and the pile is collected afterwards.
3. **A leftover pile poisons the next assertion.** The stone-base control must *keep* its redstone, so
   it is killed (`execute positioned … run kill @e[type=minecraft:item,distance=..2]`) before the
   later phases; counting by radius around the player alone is not enough.
4. **Spawn a batch as stacks, never as one oversized stack** (deviation 1) — the refused spawn is
   silent and the redstone is already gone.
5. **`click-slot` needs a renderer**: the leg launches `--offscreen`, not `--headless`.
6. **The launcher reuses a build whose `horizon-mod.toml` is unchanged.** A source-only edit inside
   the same version is invisible until the `versions/native-*` folder carrying the mod *and* the
   export under `~/.cache/horizon-e2e/<pin>/<mod>` are removed (this cost one run).
7. **The launcher reads the port's root `.hmod` for its compatibility check.** A stale package there
   (0.2.1, old pin) made `native-build` refuse the mod with *"made by an older tool … works on the
   Nov 20 update only"*; replacing it with the current build fixed it.
8. **The `e2e` instance was bound to a 0b14f8a build.** `native-build` reuses the *instance's* base
   version, so a package pinned to `0a875da` is left out on it; the build now runs with
   `HORIZON_E2E_INSTANCE=fluxnetworks-e2e`, a fresh instance on the new Nightly (the other agents'
   per-mod instance names are the same convention).

Skills: superpowers/using-superpowers, brainstorming, writing-plans, test-driven-development, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices, rust-review

Package: `fluxnetworks.hmod`, **133,581 B**, sha256
`a6d7c93ecf4b9db2dcd81669bae99dc1194889a71ccbf79b24b94bd58e5f38b4`.
