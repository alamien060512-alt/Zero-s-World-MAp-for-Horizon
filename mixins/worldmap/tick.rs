// Once a tick: scans the blocks around the player into the map and saves it.
fn worldmap_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::worldmap as m;
    if !game.native_window() || !game.native_on(m::ID) || !game.native_playing() {
        return;
    }
    let Some(p) = game.native_player() else {
        return;
    };
    let pos = m::Pos {
        x: p.x,
        y: p.y,
        z: p.z,
        yaw: p.yaw,
        dim: p.dimension.to_owned(),
    };
    let bounds = game.native_world_height();
    let root = game.native_data_dir(m::ID);
    let game = &*game;
    m::with(
        |k| game.native_setting(m::ID, k),
        |s| s.tick(&pos, bounds, &root, &|at| game.native_block(at)),
    );
}
