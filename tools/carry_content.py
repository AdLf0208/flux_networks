#!/usr/bin/env python3
"""Carry Flux Networks' assets and data from the pack jar into `content/`.

The jar is the ATM10 one (`FluxNetworks-1.21.1-8.0.0.jar`, MIT), kept as
evidence in `ports/_shared/licence-cache/`. This script is the reproducible
pipeline `AGENT_MOD_STRUCTURE.md` asks for: it copies what a 26.3 content mod
can hold, rewrites what 1.21.1 spells differently, and parks what a content mod
cannot express in `content/outside/` with the reason.

Run it from anywhere:

    python3 ports/fluxnetworks/tools/carry_content.py [--check]

`--check` re-reads the jar and reports differences without writing.
"""

from __future__ import annotations

import argparse
import io
import json
import sys
import zipfile
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parents[3]
PORT = CAMPAIGN / "ports" / "fluxnetworks"
JAR = CAMPAIGN / "ports" / "_shared" / "licence-cache" / "FluxNetworks-1.21.1-8.0.0.jar"
CONTENT = PORT / "content"
NS = "fluxnetworks"

# The blocks the jar registers, and the block entity each one gets in the port.
BLOCKS = [
    "flux_block",
    "flux_plug",
    "flux_point",
    "flux_controller",
    "basic_flux_storage",
    "herculean_flux_storage",
    "gargantuan_flux_storage",
]

# Upstream's four items (RegistryItems): flux_dust, flux_core and the two
# configurators. The block items come from `"item": true`.
ITEMS = {
    "flux_dust": {"template": "minecraft:redstone"},
    "flux_core": {"template": "minecraft:diamond"},
    "flux_configurator": {"template": "minecraft:clock", "rarity": "rare"},
    "admin_configurator": {"template": "minecraft:clock", "rarity": "epic"},
}

# `RegistryBlocks.register`: every block is metal, strength(1.0, 1000); every
# device (all but the Flux Block here) is `noOcclusion`.
COMMON = {
    "hardness": 1.0,
    "explosion_resistance": 1000.0,
    "sounds": "minecraft:iron_block",
    "requires_tool": True,
}

# `FluxShapes`: the connector collisions. The connection arms move with the
# block states, which a static shape cannot follow, so the slice declares the
# centre box alone and records the difference.
CENTRE_BOXES = {
    "flux_plug": [0.25, 0.25, 0.25, 0.75, 0.75, 0.75],
    "flux_point": [0.34375, 0.34375, 0.34375, 0.65625, 0.65625, 0.65625],
}

CONNECTOR_STATES = {
    "down": {"type": "boolean"},
    "up": {"type": "boolean"},
    "north": {"type": "boolean"},
    "south": {"type": "boolean"},
    "west": {"type": "boolean"},
    "east": {"type": "boolean"},
}

# `#c:glass_panes` is NeoForge's convention tag; vanilla 26.3 has no such tag,
# so the two recipes that use it are pointed at the vanilla item instead.
ITEM_SUBSTITUTIONS = {"#c:glass_panes": "minecraft:glass_pane"}


def read(jar: zipfile.ZipFile, name: str) -> bytes:
    return jar.read(name)


def json_of(jar: zipfile.ZipFile, name: str):
    return json.loads(read(jar, name))


def block_declaration(name: str) -> dict:
    decl: dict = {"template": "minecraft:iron_block", "properties": dict(COMMON)}
    if name != "flux_block":
        decl["properties"]["occludes"] = False
    if name in CENTRE_BOXES:
        decl["state_properties"] = dict(CONNECTOR_STATES)
        decl["collision_shapes"] = {"default": [CENTRE_BOXES[name]]}
    decl["item"] = True
    return decl


def item_definition(name: str) -> dict:
    return {"model": {"type": "minecraft:model", "model": f"{NS}:item/{name}"}}


def loot_table(name: str, carried: dict) -> dict:
    """The jar's loot table in 26.3's singular-condition shape.

    The jar's `copy_components` function copies `fluxnetworks:device_config`
    and `fluxnetworks:stored_energy`, two components this port does not
    register; it is dropped, so a broken device loses its energy and settings.
    """
    out = {"type": "minecraft:block", "pools": []}
    for pool in carried.get("pools", []):
        conditions = pool.get("conditions", [])
        single = conditions[0] if len(conditions) == 1 else None
        entry = dict(pool["entries"][0])
        entry.pop("functions", None)  # copy_components: see the docstring
        new_pool: dict = {"rolls": 1, "entries": [entry]}
        if single is not None:
            new_pool["condition"] = single
        out["pools"].append(new_pool)
    return out


def recipe(recipe: dict) -> dict | None:
    """A 1.21.1 recipe in 26.3's shape, or None when it is a custom type."""
    kind = recipe.get("type", "")
    if kind.startswith(f"{NS}:"):
        return None  # flux_storage_recipe / config_wipe_recipe: code recipes
    out = json.loads(json.dumps(recipe))
    for key, value in list(out.get("key", {}).items()):
        if isinstance(value, dict) and "item" in value:
            out["key"][key] = value["item"]
        elif isinstance(value, dict) and "tag" in value:
            out["key"][key] = "#" + value["tag"]
    for key, value in list(out.get("key", {}).items()):
        if value in ITEM_SUBSTITUTIONS:
            out["key"][key] = ITEM_SUBSTITUTIONS[value]
    out.pop("group", None)
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    if not JAR.exists():
        print(f"missing jar: {JAR}", file=sys.stderr)
        return 1

    written: list[str] = []
    with zipfile.ZipFile(JAR) as jar:
        names = set(jar.namelist())

        def put(relative: str, data: bytes) -> None:
            target = CONTENT / relative
            written.append(relative)
            if args.check:
                if not target.exists() or target.read_bytes() != data:
                    print(f"DIFF {relative}")
                return
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)

        def put_json(relative: str, value) -> None:
            put(relative, (json.dumps(value, indent=2, sort_keys=True) + "\n").encode())

        # --- assets, carried byte for byte -------------------------------
        for prefix in (
            f"assets/{NS}/blockstates/",
            f"assets/{NS}/models/block/",
            f"assets/{NS}/models/item/",
            f"assets/{NS}/textures/block/",
            f"assets/{NS}/textures/item/",
            f"assets/{NS}/textures/gui/",
            f"assets/{NS}/lang/",
        ):
            for name in sorted(names):
                if not name.startswith(prefix) or name.endswith("/"):
                    continue
                stem = name.rsplit("/", 1)[-1]
                if prefix.endswith("models/block/") and stem in (
                    "flux_plug_inventory.json",
                    "flux_point_inventory.json",
                ):
                    # NeoForge's `neoforge:composite` loader has no 26.3 form;
                    # the item is drawn as the plain block model instead.
                    continue
                if prefix.endswith("models/item/") and stem.split(".")[0] in BLOCKS:
                    continue  # rewritten below (builtin/entity, composite parents)
                put(name, read(jar, name))

        # --- 26.3 item models and definitions ----------------------------
        for name in BLOCKS:
            put_json(
                f"assets/{NS}/models/item/{name}.json",
                {"parent": f"{NS}:block/{name}"},
            )
        for name in list(BLOCKS) + list(ITEMS):
            put_json(f"assets/{NS}/items/{name}.json", item_definition(name))

        # --- block and item declarations ---------------------------------
        for name in BLOCKS:
            put_json(f"data/{NS}/horizon/block/{name}.json", block_declaration(name))
        for name, decl in ITEMS.items():
            put_json(f"data/{NS}/horizon/item/{name}.json", decl)

        # --- loot tables --------------------------------------------------
        for name in BLOCKS:
            source = f"data/{NS}/loot_table/blocks/{name}.json"
            put_json(
                f"data/{NS}/loot_table/blocks/{name}.json",
                loot_table(name, json_of(jar, source)),
            )

        # --- recipes ------------------------------------------------------
        for name in sorted(names):
            if not name.startswith(f"data/{NS}/recipe/") or not name.endswith(".json"):
                continue
            stem = name.rsplit("/", 1)[-1]
            converted = recipe(json_of(jar, name))
            if converted is None:
                put(f"outside/{stem}", read(jar, name))
                continue
            put_json(f"data/{NS}/recipe/{stem}", converted)

        # --- tags ---------------------------------------------------------
        put(
            f"data/{NS}/tags/item/storage.json",
            read(jar, f"data/{NS}/tags/item/storage.json"),
        )
        tools = sorted(BLOCKS)
        put_json(
            "data/minecraft/tags/block/mineable/pickaxe.json", {"values": tools}
        )
        put_json(
            "data/minecraft/tags/block/needs_iron_tool.json", {"values": tools}
        )

    print(f"{len(written)} files")
    for relative in sorted(written):
        print(f"  {relative}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
