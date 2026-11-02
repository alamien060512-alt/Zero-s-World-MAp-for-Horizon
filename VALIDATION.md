# Validation

Linux, Rust 1.95.0, Horizon `c1f4a7c1968b035596d0cae9985fa154cb5bda41`,
horizon-mixins `d9c83d7096b7ea5b83678da2efdfd6161f74997e`.

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Pass |
| `cargo test --workspace --locked --offline` | 31 runtime tests pass |
| `python3 tools/test_template.py` | Pass (stand-in fixture: thirteen hooks compile, original body preserved, AST audit) |
| `python3 tools/test_stage.py` on the pinned export | Pass (plan/apply of all thirteen hooks, checkout unchanged) |
| `python3 tools/horizon.py check` on the pinned export | Pass (`cargo check` of the modded horizon-client) |
| `python3 tools/horizon.py test` / `build` | **Not run** |

No game was launched. Untested in game, and the places most likely to need a
fix:

- **The borrowed screen.** That the Debug Options screen opens and closes through
  `toggle_debug_options` / `close_debug_options`, that the render, mouse, wheel,
  key and character hooks fire for it as the hook documentation says, and that
  cancelling them keeps the screen's own rows and search box from reacting.
  Only the hook call sites in the pinned source were read, not run.
- **The pointer and menus**: hit boxes and text positions were laid out from
  character counts (6 GUI pixels each), not from the game's real font widths,
  so a label may sit a pixel or two off its button.
- **In-world waypoints**: the projection is unit-tested against a hand-written
  camera, not against the game's real view (bobbing and third person are not
  modelled).
- **Map pixels**: block lookups on a real client world, the colours, and the
  frame cost of up to one fill per run of equal-coloured blocks (tens of
  thousands of fills at the finest zoom on a large screen).
- Pressing the default keys and the rebinding screen (every key name used,
  `m`, `equal`, `minus`, `left`, `right`, `up`, `down`, `home`, `b`, `escape`,
  `enter`, `keypad.enter` and `backspace`, was looked up in Horizon's key
  table, but pressing them in game was not tried).
- Saving and loading under the real mod data folder, and whether a second
  bundle can name `crate::game::worldmap` (the minimap Rust API in
  docs/FORMAT.md).
- Scanning cost per tick (up to one chunk, a few thousand block reads).
