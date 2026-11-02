# Validation

Linux, Rust 1.95.0, Horizon `c1f4a7c1968b035596d0cae9985fa154cb5bda41`,
horizon-mixins `d9c83d7096b7ea5b83678da2efdfd6161f74997e`.

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Pass |
| `cargo test --workspace --locked --offline` | 20 runtime tests pass |
| `python3 tools/test_template.py` | Pass (stand-in fixture: seven hooks compile, original body preserved, AST audit) |
| `python3 tools/test_stage.py` on the pinned export | Pass (plan/apply of all seven hooks, checkout unchanged) |
| `python3 tools/horizon.py check` on the pinned export | Pass (`cargo check` of the modded horizon-client) |
| `python3 tools/horizon.py test` / `build` | **Not run** |

No game was launched. Untested in game: block lookups on a real client world,
the colours, the HUD overlay's size and look, the rebinding screen itself (the default key names `m`, `equal`, `minus`,
`left`, `right`, `up`, `down`, `home` and `b` were checked to exist in Horizon's
key table at the pinned revision, but pressing them in game was not),
saving and loading under the real mod data folder, and whether a second bundle
can name `crate::game::worldmap` (the minimap Rust API in docs/FORMAT.md).
Scanning cost per tick (up to one chunk, a few thousand block reads) is
unmeasured.
