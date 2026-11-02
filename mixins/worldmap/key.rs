// World Map's keys: open, zoom, pan, centre and waypoints.
fn worldmap_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::worldmap as m;
    if !pressed || !game.native_playing() || !game.native_on(m::ID) {
        return;
    }
    let Some([name, _, _]) = m::keys()
        .into_iter()
        .find(|[name, _, default]| game.native_mod_key(m::ID, name, default) == key)
    else {
        return;
    };
    let stored = m::with(|k| game.native_setting(m::ID, k), |s| {
        s.press(&name);
        s.stored()
    });
    for (key, value) in stored {
        game.native_set_setting(m::ID, key, &value);
    }
}
