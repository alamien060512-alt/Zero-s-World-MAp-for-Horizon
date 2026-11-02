// The fullscreen map, drawn over the world while it is open.
fn worldmap_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use crate::game::worldmap as m;
    let _ = corners;
    if !game.native_on(m::ID) {
        return;
    }
    let game = &*game;
    let size = game.gui_size();
    let cursor = game.native_cursor();
    let width = |text: &str| game.native_text_width(text);
    let shapes = m::with(|k| game.native_setting(m::ID, k), |s| {
        if s.view.open {
            s.screen(size, (cursor[0], cursor[1]), &width)
        } else {
            Vec::new()
        }
    });
    for shape in shapes {
        match shape {
            m::Shape::Fill(rect, argb) => game.native_fill(batch, rect, argb),
            m::Shape::Text(text, at, rgb, shadow) => game.native_text(batch, &text, at, rgb, shadow),
        }
    }
}
