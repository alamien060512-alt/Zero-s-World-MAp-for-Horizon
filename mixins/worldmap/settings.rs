// The buttons of the mod's settings page.
fn worldmap_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::worldmap as m;
    if mod_id != m::ID {
        return;
    }
    let game = &*game;
    buttons.extend(m::with(|k| game.native_setting(m::ID, k), |s| s.labels()));
}
