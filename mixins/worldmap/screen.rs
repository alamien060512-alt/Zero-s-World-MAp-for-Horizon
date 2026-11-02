// The map's screen: it borrows the game's debug options screen for a free
// mouse pointer, and draws over all of it.
fn worldmap_screen(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
) {
    use crate::game::worldmap as m;
    if screen.id != "debug_options" || !game.native_on(m::ID) {
        return;
    }
    let size = game.gui_size();
    let mouse = (screen.mouse[0], screen.mouse[1]);
    let width = |text: &str| game.native_text_width(text);
    let shapes = m::with(|k| game.native_setting(m::ID, k), |s| {
        if s.view.open {
            s.frame(size, mouse, &width)
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
