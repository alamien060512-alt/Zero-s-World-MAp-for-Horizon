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
| Fullscreen map screen with a free mouse pointer | Yes: press **M**; Esc, M or the Close button leaves |
| Map detail | One map pixel per block: top-down colours by block, height shading, water depth |
| Explores as you move | Yes: one chunk every 2 ticks within 4 / 8 / 12 chunks, nearest first, re-scanned as blocks change |
| Remembered between sessions | Yes: saved per dimension in `map/<dimension>/r.<rx>.<rz>.bin` |
| Overworld, Nether, End | Yes, and a **Dim:** button views any of them; the Nether shows the floor you stand on |
| Pan, zoom, follow | Drag to pan, wheel zooms toward the pointer, +/- buttons, 8 zoom steps, **Follow** returns to you |
| Your pointer | A solid arrow that turns to the way you face |
| Menu and toolbar | **Waypoints** list, **+ Waypoint**, **Follow**, **Dim:**, **Options** (explore, waypoints on map / in world, radius), **Close** |
| Waypoints | Name box when adding (button, **B**, or right-click > Add waypoint here); colour chips; eye button hides one, x deletes, a click centres the map on it |
| Right-click menu | Add waypoint here, Centre map here |
| **Waypoints in the world** | Yes, without opening the map: a diamond with name and distance over the waypoint, nearest 16, when it is in view |
| Cursor coordinates | Yes (bottom bar) |
| Data for a minimap | Yes: Rust API and plain files, see [docs/FORMAT.md](docs/FORMAT.md) |
| Chunk grid, cave layers, biome colours, shared multiplayer maps, waypoint colour/icon picker, teleport | **Not ported** |

Honest limits:

- **The map borrows the game's Debug Options screen.** Horizon gives native mods no screen of their own, but it does give them hooks that draw over, and take the mouse and keys from, any open screen. World Map opens the Debug Options screen, covers it entirely, and cancels every click and key that screen would have used. In singleplayer that screen pauses the game while the map is open, as the game's menus do; in multiplayer the world keeps running around you, so close the map before you walk. F3+F6 still opens the real Debug Options screen, which World Map leaves alone.
- **In-world waypoints are placed from your eye position, facing and the game's field of view.** Camera effects (view bobbing, hurt shake) and third-person views are not modelled, so a marker can sit a few pixels off. They are drawn on the HUD layer, over everything, and not hidden by blocks in front of them.
- **Colours come from block names**, a fixed table with a steady fallback colour for unknown and modded blocks. No biome tinting: grass and leaves are one green.
- **Only what the client has loaded can be mapped**, around you, up to your explore radius.
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
| B | Waypoint here (a name box over the map, instantly while playing) |

On the map: left-drag pans, wheel zooms (over the Waypoints list it scrolls the list), right-click opens the menu, Esc closes the menu, the name box, or the map.

Chat: `/worldmap [open|close|zoom <1-8>|centre|add [name]|remove <n>|goto <n>|list|save|status]`.

## For a minimap

[docs/FORMAT.md](docs/FORMAT.md) describes both the Rust API (`WorldMap::sample`,
`WorldMap::image`, `Waypoint`) and the on-disk formats (`HWM1` regions and
`waypoints.txt`). The formats are versioned so a minimap built now keeps working.

## Layout

```text
Cargo.toml                          Standalone runtime library + source audit workspace
compatibility.json                  Exact Horizon / horizon-mixins revisions and source hashes
mixins/worldmap/
  horizon-mixin.toml                Module and thirteen HEAD hooks
  runtime.rs                        Scanner, tiles, storage, waypoints, view, commands + tests
  tick.rs      native_tick          Scanning and saving
  hud.rs       native_hud           Waypoints in the world; opens / closes the map screen
  key.rs       native_key           M and the other keys while playing
  screen.rs    native_screen_render_post   The map screen's drawing
  mouse_down.rs, mouse_up.rs, scroll.rs, screen_key.rs, screen_char.rs
               screen input hooks   Clicks, wheel, keys and typing over the map
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
