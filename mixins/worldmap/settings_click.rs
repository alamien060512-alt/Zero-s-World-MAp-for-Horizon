// A click on the settings page: change the setting and keep it.
fn worldmap_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::worldmap as m;
    if mod_id != m::ID {
        return;
    }
    let stored = m::with(|k| game.native_setting(m::ID, k), |s| {
        s.click(button);
        s.stored()
    });
    for (key, value) in stored {
        game.native_set_setting(m::ID, key, &value);
    }
}
