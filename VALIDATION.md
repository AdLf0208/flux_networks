# Flux Networks slice 1 — validation

> **Historical from 2026-11-18.** Everything below documents the **retired `.hnmod` bundle**
> (0.1.0, pin `1d63e0a5`) and is kept as history and as the behaviour's last in-game evidence. The
> current package is the flat SDK `.hmod` **0.2.0** at the SDK pin `0b14f8a`; its ladder lives in
> the `## Migration to the SDK format` section of [`notes.md`](notes.md) and in
> `ports/_shared/evidence/fluxnetworks/sdk-*.log`. The retired run commands here
> (`tools/horizon.py`, `tools/package.sh`, `mixins/…`) no longer exist in this port.
>
> A 0.2.0 in-game run is **not claimed**: it needs a game the launcher built with the package
> (`horizon-launcher native-build`), which is not installed in this environment (see the gaps).

Pin **`1d63e0a5a37a7a708eb0b32b49bf0dfeb28ed8a9`**, mixins
**`5a64af938ec86b00d53d16a408aabc0038fb3e57`** (schema 6); checkouts
`ports/_shared/pins/horizon-1d63e0a` and `.../horizon-mixins-5a64af9`. Every
build, test and game run went through a `build-slot.sh` lease (the compile steps
on the campaign's second build machine, the game legs the same way with this
port's prepared `resources/26.3`), never a bare `cargo`, never `--target-dir`,
`CARGO_INCREMENTAL=0`.

This file covers two slices: **slice 1** (the network, `--dev-mod content/`) and
its **embedding** into one `.hnmod` (the AE2 pattern). The pre-embedding logs are
kept as `e2e-pre-embedding{,-selftest}.log`; the current evidence is the
embedded pair.

## Ladder (re-taken after the embedding, 2026-11-10)

| Step | Command | Result | Log |
|---|---|---|---|
| format | `cargo fmt --check` | clean | `fmt.log` (`EXIT:0`) |
| format (helper) | `rustfmt --check --edition 2024 mixins/fluxnetworks/embedded_content.rs` | clean | `fmt-helper.log` (`EXIT:0`) |
| clippy | `cargo clippy --all-targets --locked --offline -- -D warnings` | clean, **0 warnings** | `clippy.log` (`EXIT:0`) |
| runtime tests | `cargo test --locked --offline` | **24 passed, 0 failed** | `test.log` (`EXIT:0`) |
| staged check | `python3 tools/horizon.py check` | `cargo_exit: 0`, 0 warnings, `stage_changed: true` (the new `embedded-content` injection) | `check.log`, `cargo-check.log` |
| embedded tests | `python3 tools/horizon.py test` | `cargo_exit: 0`, **24 passed; 0 failed; 1240 filtered out** | `test-embedded.log`, `cargo-test.log` |
| build | `python3 tools/horizon.py build` | `cargo_exit: 0`, `fast` profile | `build.log`, `cargo-build.log` |

All seven steps ran in **one lease** of `tools/evidence-ladder.sh` on the second
build machine (slot 6, `FORWARDED`); the `stage`/`target` were that slot's shared
caches, so the embedded tests ran inside the real game crate as
`game::fluxnetworks::tests`. The red-first run of the implementing session
(5 passed / 18 failed on stubs) left no log, and its tally is 23, not 24 (a test
was added after the red phase), so it is reported as history and is **not**
evidence. The green **24** is the measured count.

## Packages (rebuilt by `tools/package.sh`, 2026-11-10)

| Artifact | Size | SHA-256 | Entries |
|---|---|---|---|
| `content/fluxnetworks.hmod` | **98,176 B** | `0766d6f3933d6be77066aac2d759bb2339ba151f5aeae7200bec53ccdd333cac` | 119 |
| `fluxnetworks.hnmod` | **92,951 B** | `f749c92805e14fcd18815b36ef8a771ea7fa67f9762f0e1e0b5201d5f51d7e95` | 14 |
| `mixins/fluxnetworks/data/fluxnetworks-content.hmod` | 98,176 B | `0766d6f3…333cac` (= the built `.hmod`) | — |

- The `.hmod` rebuild is **byte-identical** to the one recorded before the
  embedding (`no code: content only`, API 0.2, `side = both`), so the carry is
  stable.
- The `.hnmod` gained two entries (14, not 12): `embedded_content.rs` and
  `data/fluxnetworks-content.hmod`. `cargo horizon-mod check
  ../fluxnetworks.hnmod --kind native-mod` exits **0**:
  `fluxnetworks 0.1.0 (native-mod, format native-1/1d63e0a5…/5a64af93…, for 26.3)`.
- `tools/package.py` refuses a bundle whose `content_root` injection has no
  `data/<id>-content.hmod` or a copy that differs from `content/<id>.hmod`; both
  guards were exercised (exit 1 each) before the good run.
- **Two machines, two hashes, same contents.** The same sources packed on the
  second build machine give **92,880 B / `8d1b2ed4…`** (`package-remote.log`).
  The deflate stream differs because this box's Python links **zlib-ng 1.3.1**
  and the far box links plain zlib 1.3.1; the entries and their bytes are the
  same. Each box reproduces its own hash on a second run (two consecutive local
  runs: `f749c928…` both), so the bundle has nothing non-deterministic in it. The
  delivered file is the local one, and `horizon-mod check --kind native-mod` was
  run on **that** file.

## In-game scenario, one file only (quoted from `e2e-embedded.log`)

Launched with **no `--dev-mod`**: the binary is the port's own build (the bundle
compiled in) and the content is the bundle's embedded `.hmod`. `CONTROL=1`
(default) runs first, with `.horizon-cache` replaced by a plain file.

```
== control: the same game with no embedded content file ==
  launch: no --dev-mod; the bundle carries its own content
  arena: y=100.0 floor(0,99,3)=minecraft:stone feet(0,100,3)=minecraft:air
  without the embedded file: no flux_plug item, the block stayed minecraft:air
  launch: no --dev-mod; the bundle carries its own content
  embedded content: 98176 B at /tmp/opencode/fluxnetworks-home…/.horizon-cache/fluxnetworks-embedded-content.hmod, identical to content/fluxnetworks.hmod
  block(0,100,0) = fluxnetworks:herculean_flux_storage
  block(1,100,0) = fluxnetworks:flux_plug
  block(3,100,0) = fluxnetworks:flux_point
  block(4,100,0) = fluxnetworks:herculean_flux_storage
  block(2,100,0) = minecraft:air (the plug and the point are not adjacent)

== control: no network id, nothing moves ==
  source:   Flux Storage at 0,100,0: 1000000/16000000 FE (… network none … t=186)
  plug:     Flux Plug at 1,100,0: 0/800000 FE (… network none … t=194)
  receiver: Flux Storage at 4,100,0: 0/16000000 FE (… network none … t=202)

== join the network and let it run ==
  info(1,100,0): Flux Plug at 1,100,0: 0/800000 FE (comparator 0, network 1, limit 800000 FE/t, priority 0, t=246)
  info(0,100,0): Flux Storage at 0,100,0: 0/16000000 FE (comparator 0, network 10, limit 120000 FE/t, priority -1000000, t=254)
  load(0,100,0) = 16000000 FE
  source before: Flux Storage at 0,100,0: 15040000/16000000 FE (… t=294)
  source after:  Flux Storage at 0,100,0: 14080000/16000000 FE (… t=302)
  receiver after: Flux Storage at 4,100,0: 3840000/16000000 FE (… t=318)
  source: -960000 FE in 8 ticks; receiver: +960000 FE in 8 ticks; network holds 16000000 FE

== control: a point on another network carries nothing ==
  disconnected: source 16000000 → 16000000 ; receiver 0 → 0

== a full receiver refuses more ==
  full receiver: Flux Storage at 4,100,0: 16000000/16000000 FE
  source: 16000000 → 16000000

PASS: an unlinked network moved nothing; joined, the plug drained the source at
120,000 FE/t, the network carried it two blocks to the point, the point filled
the receiver, FE was conserved, a point on another network carried nothing, and
a full receiver refused more
EXIT:0
```

What each line proves:

- **the content is the bundle's**: the block/item ids resolve with no
  `--dev-mod`, and the file the helper wrote is byte-identical to
  `content/fluxnetworks.hmod` (98,176 B);
- **the control cannot pass vacuously**: with `.horizon-cache` a plain file the
  helper logs
  `Horizon: Flux Networks' content could not be written to …` and the same
  binary reports `Unknown item 'fluxnetworks:flux_plug'` and
  `Unknown block type 'fluxnetworks:herculean_flux_storage'` — no item, air
  block. (The helper resolves
  `std::env::current_dir()/.horizon-cache`; the script never passes `--game-dir`,
  so the cache is the launch `cd`, `$SCRATCH`, and the control blocks exactly
  that path.)
- **the rate**: 960,000 FE over 8 game ticks is exactly the herculean storage's
  120,000 FE/t, on both the source's loss and the receiver's gain;
- **the network, not adjacency**: the plug (x=1) and the point (x=3) have air
  between them; the only path is the shared `network_id`;
- **conservation**: with the flow frozen (the point moved to network 99) the four
  buffers hold exactly 16,000,000 FE;
- **no network id does nothing**: all three devices read `network none` and
  1,000,000 FE stays where it was put;
- **a disconnected point does nothing**: 40 ticks with the point on another
  network move nothing;
- **a full storage refuses more**, and because nothing asks for energy the
  source stops draining (upstream's `mBufferLimiter`).

`SELFTEST=1` flips the "no network id" assertion and the run fails there
(`FAIL: SELFTEST flipped: an unlinked network moved no energy, as it should`,
`EXIT:1`, `e2e-embedded-selftest.log`), so the scenario can fail with the
embedded content too.

## Gates

```
$ python3 ports/tools/check-e2e-logs.py fluxnetworks --strict
ok  e2e-embedded-selftest.log            FAIL + EXIT:1
ok  e2e-embedded.log                     PASS + EXIT:0
ok  e2e-pre-embedding-selftest.log       FAIL + EXIT:1
ok  e2e-pre-embedding.log                PASS + EXIT:0
logs=4  ok=4  missing-exit=0  bad-exit=0        (exit 0)

$ python3 ports/tools/upstream_ready.py fluxnetworks
READY    fluxnetworks                 MIT
ready=1 blocked=0 of 1                          (exit 0)
```

`upstream_ready` is green because the bundle **embeds** its content
(`mixins/fluxnetworks/data/*.hmod`), which the gate accepts instead of calling
the port a split content+code port. Publishing still needs the maintainer's
approval (ARCHITECTURE.md §8).

## Evidence

`ports/_shared/evidence/fluxnetworks/`:

- `e2e-embedded.log` — the run above (`PASS`, `EXIT:0`), with the control leg;
- `e2e-embedded-selftest.log` — the flipped run (`FAIL`, `EXIT:1`);
- `e2e-pre-embedding.log`, `e2e-pre-embedding-selftest.log` — the slice-1 runs
  with `--dev-mod content/` (PASS `EXIT:0` / FAIL `EXIT:1`), renamed from
  `e2e.log`/`e2e-selftest.log` when the embedding landed;
- `package.log` — `tools/package.sh` on this box: `.hmod` 98,176 B
  `0766d6f3…`, `.hnmod` 92,951 B `f749c928…` 14 entries, embedded copy
  identical, `horizon-mod check --kind native-mod` exit 0, the 14 entry names;
- `package-remote.log` — the same step on the second build machine
  (92,880 B / `8d1b2ed4…`; zlib vs zlib-ng, see above);
- `ladder.log` — the `evidence-ladder.sh` run summary (all `EXIT:0`);
- `test.log`, `test-embedded.log` — 24 passed / 24 passed, 1240 filtered;
- `fmt.log`, `fmt-helper.log`, `clippy.log` — clean, `EXIT:0`;
- `check.log`, `cargo-check.log`, `build.log`, `cargo-build.log` — staged check
  and fast build, `cargo_exit: 0`, 0 warnings;
- `hmod-load.log` — the earlier load-check (7 blocks + 4 items from the
  `.hmod`; unknown without it), kept as history.

## Environment notes (paid for, worth keeping)

- **Call the game leg from the campaign root with a port-relative command.**
  `build-slot.sh e2e` mirrors the caller's directory on the far box *and* its
  wrapper `cd`s into `--port`. From inside the port, `--port ports/fluxnetworks`
  becomes `cd ports/fluxnetworks` under `…/ports/fluxnetworks` (`No such file or
  directory`, exit 1); with `--port .` the evidence pull is skipped. What works:
  from the campaign root,
  `ports/tools/build-slot.sh e2e --port ports/fluxnetworks -- tools/e2e-remote.sh`.
  Compile/package steps do **not** have this problem (`run` does not `cd`), so
  they are fine from inside the port with `--port ports/fluxnetworks`.
- The second build machine's pin checkouts had no `.git` (the payload was a tar
  without it), so `horizon.py` could not stage there until
  `ports/_shared/pins/horizon-1d63e0a/.git` and
  `.../horizon-mixins-5a64af9/.git` were rsynced to it.
- The far box had no `jq` in a system path: `tools/e2e-remote.sh` puts
  `~/jqbin` on `PATH`/`LD_LIBRARY_PATH`.
- Its published `shared-bin` game is linked against a newer glibc than it has
  (`GLIBC_2.44` not found), so game legs run the port's own remote build.
- `python3 tools/test_template.py` is the template's own fixture test (it
  asserts `injections == 1` on a fake workspace with no `native-hooks.json`);
  it cannot pass for a real bundle and is not part of this ladder.
