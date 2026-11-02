# World Map for Horizon

A native [Horizon](https://github.com/horizon-rust/horizon) mod in the spirit of
[Xaero's World Map](https://www.curseforge.com/minecraft/mc-mods/xaeros-world-map):
a fullscreen map of the world you have explored, with zoom, panning and named
waypoints, and a documented data interface a future minimap can read.

**This is not a port of Xaero's World Map.** That mod is all-rights-reserved, so
none of its code, textures or file formats are used here; this is an independent
implementation of a similar feature set, and it is not affiliated with Xaero.
It is a native mod: it builds into a custom Horizon executable, not a `.hmod`.

Built from [horizon-native-mod-template](https://github.com/horizon-rust/horizon-native-mod-template).

## What works

| Feature | In this mod |
|---|---|
| Fullscreen map of explored terrain | Yes: top-down colours by block, height shading, water depth |
| Explores as you move | Yes: one chunk every 2 ticks within 4 / 8 / 12 chunks, nearest first, re-scanned as blocks change |
| Remembered between sessions | Yes: saved per dimension in `map/<dimension>/r.<rx>.<rz>.bin` |
| Overworld, Nether, End | Yes; the Nether shows the floor you stand on, not the roof |
| Zoom, pan, follow player | Yes: 7 zoom steps, keys or `/worldmap` |
| Waypoints | Yes: add at your feet, list, remove, centre the map on one; drawn with names |
| Cursor coordinates | Yes (top bar) |
| Data for a minimap | Yes: Rust API and plain files, see [docs/FORMAT.md](docs/FORMAT.md) |
| Mouse drag and scroll, waypoint editor screen, cave layers, biome colours, chunk grid, shared multiplayer maps | **Not ported.** Horizon's native hook points give a mod keys, chat commands and HUD drawing, but not a screen of its own or mouse-wheel events |

Honest limits:

- **The map is a HUD overlay, not a screen.** You keep control of the player while it is open (Horizon has no mod-screen hook for this). Close it with the map key to walk safely.
- **Colours come from block names**, a fixed table with a steady fallback colour for unknown and modded blocks. No biome tinting: grass and leaves are one green.
- **Only what the client has loaded can be mapped**, around you, up to your explore radius. Servers that send fewer chunks than your radius show less.
- **Surface view.** The Overworld shows the top block. Caves and underground levels are not mapped.
- **Nothing here has been run in a game.** See [VALIDATION.md](VALIDATION.md).

## Controls

Press **M** to open and close the map. Every key can be rebound in Options > Mods > World Map
(Horizon's own mod key list, the same place Overtune's keys live). Defaults:

| Key | Action |
|---|---|
| M | Open / close the map |
| `=` / `-` | Zoom in / out |
| Arrow keys | Pan |
| Home | Follow the player again |
| B | Waypoint at your feet |

Chat: `/worldmap [open|close|zoom <1-7>|centre|add [name]|remove <n>|goto <n>|list|save|status]`.

Options > Mods > World Map: explore on/off, waypoints shown on/off, explore radius.

## For a minimap

[docs/FORMAT.md](docs/FORMAT.md) describes both the Rust API (`WorldMap::sample`,
`WorldMap::image`, `Waypoint`) and the on-disk formats (`HWM1` regions and
`waypoints.txt`). The formats are versioned so a minimap built now keeps working.

## Layout

```text
Cargo.toml                          Standalone runtime library + source audit workspace
compatibility.json                  Exact Horizon / horizon-mixins revisions and source hashes
mixins/worldmap/
  horizon-mixin.toml                Module and seven HEAD hooks
  runtime.rs                        Scanner, tiles, storage, waypoints, view, commands + tests
  tick.rs      native_tick          Scanning and saving
  hud.rs       native_hud           The fullscreen map
  key.rs       native_key           Keys
  command.rs   native_command       /worldmap
  mods.rs      native_mods          Options > Mods row and keys
  settings.rs, settings_click.rs    Settings page
docs/FORMAT.md                      Data interface for a minimap
tools/                              Stage / check / build / test, packaging, audits
```

## Build

Requirements: Linux, Rust 1.95.0, Python 3.12+, Git and Cargo.

```sh
git clone https://github.com/horizon-rust/horizon ../horizon
git clone https://github.com/horizon-rust/horizon-mixins ../horizon-mixins
git -C ../horizon-mixins checkout "$(python3 -c 'import json; print(json.load(open("compatibility.json"))["horizon_mixins_revision"])')"

python3 tools/horizon.py stage --horizon ../horizon
cargo fetch --locked --manifest-path target/mixins/modded/Cargo.toml
python3 tools/horizon.py check --horizon ../horizon
python3 tools/horizon.py build --horizon ../horizon
./target/horizon-cargo/fast/horizon-client --resources /absolute/path/to/prepared/resources
```

Pinned to Horizon `c1f4a7c` (Nightly 2026-10-31) and horizon-mixins `d9c83d7`
(0.6.0). Package for the [horizon-mods](https://github.com/horizon-rust/horizon-mods)
registry with `python3 tools/package.py` (`target/worldmap-0.1.0.hnmod`).

## License

MIT, see [LICENSE](LICENSE). Build tooling is adapted from the MIT-licensed
Horizon native mod template.
