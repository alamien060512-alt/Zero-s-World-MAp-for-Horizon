// `/worldmap ...` in chat: the map, zoom, waypoints and status.
fn worldmap_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::worldmap as m;
    let Some(args) = command.strip_prefix(m::ID) else {
        return;
    };
    if !args.is_empty() && !args.starts_with(' ') || !game.native_window() || !game.native_on(m::ID) {
        return;
    }
    *ran = true;
    let (lines, stored) = m::with(|k| game.native_setting(m::ID, k), |s| (s.command(args), s.stored()));
    for (key, value) in stored {
        game.native_set_setting(m::ID, key, &value);
    }
    for line in lines {
        game.native_chat(&line);
    }
}
