// Text typed over the map goes to the waypoint name box, and nowhere else.
fn worldmap_screen_char(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    c: char,
    canceled: &mut bool,
) {
    use crate::game::worldmap as m;
    if screen.id != "debug_options" || !game.native_on(m::ID) {
        return;
    }
    let taken = m::with(|k| game.native_setting(m::ID, k), |s| s.view.open && s.screen_char(c));
    if taken {
        *canceled = true;
    }
}
