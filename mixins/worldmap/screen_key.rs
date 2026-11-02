// A key over the map: Escape closes it as the game does, the rest are the
// map's own keys or nothing at all.
fn worldmap_screen_key(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    key: &str,
    canceled: &mut bool,
) {
    use crate::game::worldmap as m;
    if screen.id != "debug_options" || !game.native_on(m::ID) {
        return;
    }
    let action = m::keys()
        .into_iter()
        .find(|[name, _, default]| game.native_mod_key(m::ID, name, default) == key)
        .map(|[name, _, _]| name);
    let (taken, stored) = m::with(|k| game.native_setting(m::ID, k), |s| {
        if s.view.open {
            (s.screen_key(key, action.as_deref()), s.changed())
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
