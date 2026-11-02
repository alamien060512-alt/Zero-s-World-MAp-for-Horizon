// The wheel over the map zooms it, or scrolls the waypoint list.
fn worldmap_scroll(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    scroll: [f64; 2],
    canceled: &mut bool,
) {
    use crate::game::worldmap as m;
    if screen.id != "debug_options" || !game.native_on(m::ID) {
        return;
    }
    let at = (screen.mouse[0], screen.mouse[1]);
    let (taken, stored) = m::with(|k| game.native_setting(m::ID, k), |s| {
        if s.view.open {
            (s.scrolled(scroll[1], at), s.changed())
        } else {
            (false, Vec::new())
        }
    });
    if taken {
        *canceled = true;
    }
    for (key, value) in stored {
        game.native_set_setting(m::ID, key, &value);
    }
}
