# Working on Flux Networks (native Horizon mod, SDK format)

This is a **flat SDK package** (mod API 0.2): `horizon-mod.toml` + `Cargo.toml` + `src/lib.rs`
(`impl mod_api::Hooks`) + `src/runtime.rs` (the pure rules and their tests) + `assets/` + `data/`.
Horizon retired the `.hnmod` container in the mod-API merge (#16941), so there is no
`compatibility.json`, `mixins/<id>/`, `horizon-mixin.toml`, plan/injection schema or
`rust-toolchain.toml` here, and none of them should be recreated. The retired tree is
`../_shared/evidence/fluxnetworks/retired-hnmod-0.1.0/`.

> **Build slots (v2):** every build/test goes through the campaign runner
> (`../tools/build-slot.sh` from the port, `ports/tools/build-slot.sh` from the campaign root). It
> **blocks in the kernel** until a slot is free, so **never poll and never background a build**; a
> timeout exits **75**. It does **not** `cd` into the port — wrap the command
> (`bash -c 'cd ports/fluxnetworks && …'`) and never pass `--target-dir`. Use `BUILD_LOCAL=1`: the
> far box has no copy of this port. Read `../_shared/build-slots-usage.md` before running anything.

```sh
BUILD_LOCAL=1 ports/tools/build-slot.sh run --job fluxnetworks-sdk --port ports/fluxnetworks --step test \
    -- bash -c 'cd ports/fluxnetworks && CARGO_INCREMENTAL=0 cargo test --offline'
BUILD_LOCAL=1 ports/tools/build-slot.sh run --job fluxnetworks-sdk --port ports/fluxnetworks --step build \
    -- bash -c 'cd ports/fluxnetworks && rm -f Cargo.lock && cargo horizon-mod build'
```

- `cargo horizon-mod` fetches the pinned SDK into `.horizon/sdk/` (gitignored, never packed).
  `build` packs the `.hmod`; it does **not** compile against the game — that is
  `horizon-launcher native-build . --instance <instance> --trust-native-code`, which is not
  installed in this environment (so no in-game claim is made for 0.2.0).
- **`src/runtime.rs` is byte-identical to the retired bundle's `runtime.rs` and must stay so**: the
  behaviour rules live there (`0902dbea…8cdf`); `src/lib.rs` only converts SDK types to theirs. New
  rule → a test in `runtime.rs`; new glue → a private fn and a test in `lib.rs`.
- Only the API the SDK publishes: `cargo horizon-mod hooks <text>` finds the exact method; never
  guess a name, and record an API gap in `notes.md` instead of faking one.
- The energy a device holds lives in its block entity's saved `energy` int
  (`Registrar::block_energy`); every read and move happens at `level_tick_post`, because a block
  entity is out of the world during its own tick.
- The delivered package is `fluxnetworks.hmod` **in the port root**, never in `target/`;
  `LICENSE`, `LICENSE-NOTICE.md` and `README.md` travel inside it. Remove `Cargo.lock` right before
  a build — it is packed and changes the artifact.
- `python3 ports/tools/check-skills.py --notes ports/fluxnetworks` must stay green, and `notes.md`
  records every claim with its command and output.
- Keep the port separate from Horizon and horizon-mixins; never modify the pin checkouts, never
  commit binaries, exported sources or Minecraft assets, and never put Mojang files in the tree.
- Licence: MIT, rank 0, **local-only** until the maintainer approves publishing (ARCHITECTURE.md
  §8). The carried assets and data come from the ATM10 jar (kept in `_shared/licence-cache/`); the
  carry pipeline is `tools/carry_content.py` (it writes `assets/` and `data/` here and parks the
  two code-recipe families in `../_shared/evidence/fluxnetworks/outside/`).
