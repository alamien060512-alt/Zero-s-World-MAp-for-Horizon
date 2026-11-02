# Map data for a minimap (or any other mod)

World Map keeps its data under the game's native-mod folder for the id
`worldmap`: `horizon-mods/.horizon-mod-data/worldmap/`. Both ways in below read
the same data; the formats are versioned and stay stable within `HWM1`.

## 1. In the same game build (Rust)

A minimap bundled into the same custom Horizon executable can call the public
API of the `worldmap` module that `runtime.rs` becomes (`crate::game::worldmap`):

| Item | Use |
|---|---|
| `with(get, \|s\| ...)` | The shared `State` (the same one the fullscreen map draws). |
| `State::map.sample(dim, x, z)` | `0xAARRGGBB` of one block column, `0` if never seen. |
| `State::map.image(dim, centre, (w, h), blocks_per_pixel)` | A ready minimap image, row by row, `0` for unseen. |
| `State::list` | Every `Waypoint { name, dim, pos: [x, y, z], rgb, hidden }`; skip the `hidden` ones. |
| `State::player` | The player's last `Pos { x, y, z, yaw, dim }`. |
| `block_color`, `dir_name`, `ZOOMS`, `to_screen`, `to_world`, `project`, `arrow` | Shared helpers (`project` puts a world point on the screen given a `Camera`). |

Whether a second bundle may name `crate::game::worldmap` depends on how
horizon-mixins orders two bundles' modules; this has not been tried (see
VALIDATION.md). A minimap that cannot reach the module can use the files.

## 2. Files (any language, no shared build)

All numbers are little-endian. Dimension ids become folder names by replacing
every character other than letters, digits, `_` and `-` with `_`
(`minecraft:the_nether` -> `minecraft_the_nether`).

### `map/<dimension>/r.<rx>.<rz>.bin`

A region of 8 x 8 chunks; region `(rx, rz)` holds chunks `rx*8 .. rx*8+7` and
`rz*8 .. rz*8+7`. Layout:

```text
"HWM1"                      4 bytes
64 entries, row by row (chunk z outer, chunk x inner):
  flag                      1 byte: 0 absent, 1 present
  if present: 256 x u32     pixels, row z*16+x, north first, 0xAARRGGBB
                            (0 = column never seen)
```

A file that is truncated, has extra bytes or another magic is not valid; skip it.
The files are rewritten whole (temp file, then rename) about every 30 seconds and
when `/worldmap save` runs, so a reader sees either the old or the new region.

### `waypoints.txt`

UTF-8, `#` starts a comment line, one waypoint per line:

```text
name|dimension|x|y|z|RRGGBB
```

`|` and line breaks in names are written as spaces. A line that does not parse
is ignored. Writers other than World Map should rewrite the file whole and keep
the lines of other dimensions.

## Compatibility promise

`HWM1` pixels and `waypoints.txt` lines will not change meaning. A new layout
gets a new magic (`HWM2`) and a new file name, and World Map will keep reading
`HWM1`.
