# `content/` — what is carried, and every patch

Rebuilt from the jar by [`../tools/carry_content.py`](../tools/carry_content.py)
(`--check` re-reads the jar and reports differences). The jar is
`FluxNetworks-1.21.1-8.0.0.jar` (MIT), kept at
`ports/_shared/licence-cache/`; nothing here was written by hand except the
manifest and this file.

**119 files** (the `.hmod` entry count): 7 blockstates, 8 block models, 11 item
models, 11 26.3 item definitions, 44 textures (30 block, 6 item, 8 gui,
including the 3 `.mcmeta` files), 9 languages, 7 block declarations, 4 item
declarations, 7 loot tables, 7 recipes, 1 mod tag, 2 vanilla block tags.

## Carried byte for byte

Every blockstate, every block model except the two inventory models, every
texture, every language file, the `fluxnetworks:storage` item tag.

## Rewritten, with the reason

| What | Why | What it is now |
|---|---|---|
| `models/item/flux_plug.json`, `flux_point.json` | parent `fluxnetworks:block/*_inventory`, which is a `neoforge:composite` model — NeoForge's loader has no 26.3 form | parent the plain block model |
| `models/item/basic/herculean/gargantuan_flux_storage.json` | parent `builtin/entity`, an entity renderer this port does not ship | parent the plain block model |
| `models/item/<block>.json` × 7 | their 1.21.1 parents are the two cases above | one model per block item, parenting `fluxnetworks:block/<name>` |
| `assets/fluxnetworks/items/<name>.json` × 11 | 26.3 item definitions, which 1.21.1 does not have | `{"model": {"type": "minecraft:model", "model": "fluxnetworks:item/<name>"}}` |
| `data/fluxnetworks/horizon/block/<name>.json` × 7 | the jar registers blocks in code; a content mod declares them | template `minecraft:iron_block` with upstream's numbers (metal, 1.0/1000, metal sound); the six devices `occludes: false` (`noOcclusion`); the plug and the point declare the six connection booleans and a centre collision box |
| `data/fluxnetworks/horizon/item/<name>.json` × 4 | the four items are code-registered upstream | templates `minecraft:redstone` (dust), `minecraft:diamond` (core), `minecraft:clock` (both configurators) |
| `data/.../loot_table/blocks/*.json` × 7 | 1.21.1's pool form is plural; 26.3 takes one singular `condition` | the 26.3 shape, **without** the jar's `copy_components` function (it copies `fluxnetworks:device_config` and `fluxnetworks:stored_energy`, components this port does not register) |
| `data/.../recipe/*.json` × 7 | 26.3 recipe keys are `"namespace:item"` / `"#tag"` strings | upgraded; `#c:glass_panes` became `minecraft:glass_pane`, because NeoForge's `c:` convention tags do not exist in vanilla 26.3 |
| `data/minecraft/tags/block/{mineable/pickaxe,needs_iron_tool}.json` | upstream tags the blocks in code | the seven block ids |

## Parked in `outside/` (not packed)

| File | Why |
|---|---|
| `herculean_flux_storage.json`, `gargantuan_flux_storage.json` | type `fluxnetworks:flux_storage_recipe`: a code recipe that keeps the stored energy across a tier upgrade. A 26.3 content recipe cannot express it, and the port has no such recipe type. **Gap**: the upgrades are not craftable. |
| `wipe_*.json` × 6 | type `fluxnetworks:config_wipe_recipe`: a code recipe that clears a device's settings. Same reason. **Gap**. |

## Not carried

- `assets/fluxnetworks/textures/gui/gui_creating_flux_recipe.png` — carried but
  unused: the `CreatingFluxRecipe` category is a JEI plugin.
- The `flux_dust` source recipe: upstream's "creating flux" is a code recipe
  (drop redstone on obsidian); the jar has no data file for it. **Gap**: the
  crafting chain starts at `flux_core`, which needs `flux_dust`.
