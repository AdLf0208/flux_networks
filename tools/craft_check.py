#!/usr/bin/env python3
"""Craft every shaped recipe the package carries, in a real crafting table.

Called by `tools/e2e.sh` with the open game's control socket. The harness's
crafting table exposes the 3x3 grid as slots 100-108 (row-major) and the result
as 200 (`ctl screen`). The recipe is read from the package's own data file, so
the pattern and the counts below are the shipped ones, not a transcription:

1. a fresh table and a cleared inventory;
2. exactly the pattern's ingredients are given;
3. the pattern is placed with right-clicks (one item per cell);
4. the result's **id and count** are asserted;
5. the result is shift-clicked into the inventory and its arrival asserted;
6. the grid is empty and every ingredient was consumed by its declared count.

`fluxnetworks:flux_core` is the one recipe whose flux dust comes from the
inventory instead of `give`: the e2e has just made that dust with the
"creating flux" mechanic, so the chain redstone -> dust -> flux core is one
run. Two deliberately wrong patterns must leave the result empty.

    python3 tools/craft_check.py SOCK CTL X Y Z
"""
import json
import subprocess
import sys
from pathlib import Path

# The crafting table's grid, row-major, and its result.
GRID = [100, 101, 102, 103, 104, 105, 106, 107, 108]
RESULT = 200

# The recipe whose dust the mechanic just made: do not give it and do not clear
# the inventory before it, so the dust that is consumed is the dust that was
# created.
KEEP_INVENTORY = {
    "flux_core": ["fluxnetworks:flux_dust"],
}


def shaped(port):
    """Every carried shaped recipe: (name, pattern, key, result id, result count)."""
    out = []
    for path in sorted((port / "data/fluxnetworks/recipe").glob("*.json")):
        data = json.loads(path.read_text())
        if data.get("type") != "minecraft:crafting_shaped":
            continue
        out.append(
            {
                "name": path.stem,
                "pattern": data["pattern"],
                "key": data["key"],
                "id": data["result"]["id"],
                "count": data["result"].get("count", 1),
            }
        )
    if not out:
        raise SystemExit(f"no shaped recipes under {port}/data/fluxnetworks/recipe")
    return out


def cells(recipe):
    """{grid slot: item id} for the pattern, in row-major order."""
    placed = {}
    for row, line in enumerate(recipe["pattern"]):
        for col, symbol in enumerate(line):
            if symbol == " ":
                continue
            placed[GRID[row * 3 + col]] = recipe["key"][symbol]
    return placed


def needs(recipe):
    """{item id: how many the pattern needs}."""
    counted = {}
    for item in cells(recipe).values():
        counted[item] = counted.get(item, 0) + 1
    return counted


def main():
    sock, ctl, x, y, z = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
    port = Path(sys.argv[6]) if len(sys.argv) > 6 else Path(__file__).resolve().parents[1]

    def run(*args):
        out = subprocess.run([ctl, "--socket", sock, *args], text=True, capture_output=True)
        try:
            return json.loads(out.stdout)
        except json.JSONDecodeError:
            raise SystemExit(f"ctl {' '.join(args)}: {out.stdout}{out.stderr}")

    def wait(ticks=4):
        run("wait", str(ticks))

    def slots():
        screen = run("screen")
        if "slots" not in screen:
            raise SystemExit(f"no menu is open: {screen}")
        return screen["slots"]

    def find(item):
        for slot in slots():
            if slot.get("item") == item:
                return slot["slot"]
        raise SystemExit(f"missing ingredient {item} in the inventory")

    def result():
        stack = [s for s in slots() if s["slot"] == RESULT]
        if not stack or not stack[0].get("item"):
            return None
        return stack[0]

    def grid_is_empty():
        return all(not s.get("item") for s in slots() if s["slot"] in GRID)

    def count(item):
        return run("inventory").get("counts", {}).get(item, 0)

    def place(item, targets):
        # One pickup per target: the held stack goes back to its slot after each
        # single item is placed, so a stack of four fills four cells.
        for target in targets:
            slot = find(item)
            run("click-slot", str(slot))
            wait(1)
            run("click-slot", str(target), "right")
            wait(1)
            run("click-slot", str(slot))
            wait(1)

    def setup(clear):
        try:
            run("close")
        except SystemExit:
            pass
        wait(4)
        # Replace the table so its block entity, and therefore the grid, is fresh.
        run("setblock", x, y, z, "minecraft:air")
        run("setblock", x, y, z, "minecraft:crafting_table")
        wait(2)
        run("command", f"tp @s {float(x) - 0.5} {y} {float(z) + 0.5}")
        wait(3)
        if clear:
            run("command", "clear @s")
            wait(2)
        run("interact", x, y, z)
        wait(5)

    failures = []

    def check(recipe):
        name = recipe["name"]
        needed = needs(recipe)
        held = KEEP_INVENTORY.get(name, [])
        setup(clear=not held)
        for item, amount in needed.items():
            if item in held:
                continue
            run("give", item, str(amount))
        wait(3)
        before = {item: count(item) for item in needed}
        # The result is not an ingredient, so its count is read on its own.
        made_before = count(recipe["id"])
        for item in held:
            if before[item] < needed[item]:
                failures.append(f"{name}: the inventory holds {before[item]} {item}, the pattern needs {needed[item]}")
                return
        by_item = {}
        for slot, item in cells(recipe).items():
            by_item.setdefault(item, []).append(slot)
        for item, targets in by_item.items():
            place(item, targets)
        got = result()
        print(f"{name} -> {json.dumps(got)}")
        if not got or got.get("item") != recipe["id"] or got.get("count") != recipe["count"]:
            print(f"  grid: {json.dumps([s for s in slots() if s['slot'] in GRID])}")
            failures.append(f"{name}: expected {recipe['count']} {recipe['id']}, got {got}")
            return
        took = run("click-slot", str(RESULT), "shift")
        wait(10)
        after = {item: count(item) for item in needed}
        made = count(recipe["id"]) - made_before
        if made < recipe["count"]:
            # One retry: a click that lands while the previous one is still settling is dropped.
            again = run("click-slot", str(RESULT), "shift")
            wait(10)
            after = {item: count(item) for item in needed}
            made = count(recipe["id"]) - made_before
            print(f"  take: {json.dumps(took)} then {json.dumps(again)}")
            print(f"  inventory: {json.dumps(run('inventory').get('counts', {}))}")
            print(f"  result slot: {json.dumps(result())}")
        if made < recipe["count"]:
            failures.append(f"{name}: the crafted stack reached the inventory as {made}")
        if not grid_is_empty():
            failures.append(f"{name}: the grid still holds ingredients after the craft")
        for item, amount in needed.items():
            used = before[item] - after[item]
            if used != amount:
                failures.append(f"{name}: {item} used {used} of the pattern's {amount}")

    def wrong_pattern(recipe, substitute):
        """The same shape with `substitute` where the pattern's first ingredient goes: no result."""
        name = recipe["name"]
        original = next(iter(cells(recipe).values()))  # row-major, so the first cell's item
        layout = {
            slot: (substitute if item == original else item)
            for slot, item in cells(recipe).items()
        }
        needed = {}
        for item in layout.values():
            needed[item] = needed.get(item, 0) + 1
        setup(clear=True)
        for item, amount in needed.items():
            run("give", item, str(amount))
        wait(3)
        by_item = {}
        for slot, item in layout.items():
            by_item.setdefault(item, []).append(slot)
        for item, targets in by_item.items():
            place(item, targets)
        got = result()
        print(f"{name} with {substitute} (control) -> {json.dumps(got)}")
        if got is not None:
            failures.append(
                f"{name} control: crafted {got['item']} x{got['count']} from a wrong pattern"
            )

    recipes = shaped(port)
    # The recipes that use what the game already holds go first: a later row clears
    # the inventory, and the dust the mechanic made must still be there for its core.
    recipes.sort(key=lambda recipe: recipe["name"] not in KEEP_INVENTORY)
    for recipe in recipes:
        check(recipe)
    # A wrong pattern must not craft: obsidian is the ingredient that makes a flux core.
    wrong_pattern(next(r for r in recipes if r["name"] == "flux_core"), "minecraft:redstone")
    # ... and the flux block's shape is not a flux plug.
    wrong_pattern(next(r for r in recipes if r["name"] == "flux_plug"), "minecraft:iron_ingot")

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}")
        return 1
    print(f"CRAFTED: {len(recipes)} carried shaped recipes, each with its declared id and count, its ingredients consumed, and two wrong patterns left empty")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
