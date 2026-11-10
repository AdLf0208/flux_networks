# fluxnetworks (Horizon port)

A Horizon port of [fluxnetworks](SonarSonic/Flux-Networks) — MIT.

**Status: first slices.** What is below is what the port actually does today.

## Implemented so far

- One network, five blocks, one readout:
- `fluxnetworks:flux_plug`** — pulls FE from the blocks beside it (up to its
- transfer limit and what the network asked for last cycle), buffers it, and
- gives it to the network.
- `fluxnetworks:flux_point`** — takes FE from the network's plugs by
- priority, up to what the blocks beside it can accept, and pushes it there.
- `fluxnetworks:basic_flux_storage` / `herculean_…` / `gargantuan_…`** —
- network buffers of 2,000,000 / 16,000,000 / 128,000,000 FE moving

## Not yet ported

- A device of another network is an outside energy block.** Upstream refuses
- every* `IFluxDevice` as an outside block. A one-bundle world has no other
- block with an FE buffer, so the network id is the boundary this slice
- models: a plug or point ignores the devices of its *own* network and treats a
- device of another network as an ordinary FE block. Consequence: two networks
- placed side by side exchange FE through their plugs and points. `FINDINGS.md`
- and the support module say so; the e2e's source and receiver are devices on
- networks 10 and 20.

## Install

Build the bundle with a native-mod capable launcher, or add the file from the
registry once it is listed. Pinned to Horizon `1d63e0a` and horizon-mixins
`5a64af93` for game 26.3.

## Build from source

```sh
cargo test
python3 tools/horizon.py check --horizon <horizon checkout>
python3 tools/horizon.py build --horizon <horizon checkout>
python3 tools/package.py
```

## Credits and licence

Original mod: SonarSonic/Flux-Networks. Port: AdLf0208. Licence: MIT — see [LICENSE](LICENSE);
the original notices are preserved.
