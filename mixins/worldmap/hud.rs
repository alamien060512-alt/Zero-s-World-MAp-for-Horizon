// Every frame: opens or closes the map's screen when asked, and draws the
// waypoints that are in view over the world.
fn worldmap_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use crate::game::Screen;
    use crate::game::worldmap as m;
    let _ = corners;
    if !game.native_on(m::ID) {
        return;
    }
    let on_map = game.screen == Screen::DebugOptions;
    let playing = game.screen == Screen::Playing;
    let plan = m::with(|k| game.native_setting(m::ID, k), |s| s.plan(on_map, playing));
    if plan.close {
        game.close_debug_options();
        m::with(|k| game.native_setting(m::ID, k), |s| s.closed());
    }
    if plan.open {
        game.toggle_debug_options();
        if game.screen == Screen::DebugOptions {
            m::with(|k| game.native_setting(m::ID, k), |s| s.opened());
        }
    }
    if game.screen != Screen::Playing {
        return;
    }
    let Some(p) = game.native_player() else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let cam = m::Camera {
        eye,
        yaw: p.yaw,
        pitch: p.pitch,
        fov: game.fov_degrees(0.0, eye),
    };
    let size = game.gui_size();
    let game = &*game;
    let width = |text: &str| game.native_text_width(text);
    let shapes = m::with(|k| game.native_setting(m::ID, k), |s| s.world_marks(&cam, size, &width));
    for shape in shapes {
        match shape {
            m::Shape::Fill(rect, argb) => game.native_fill(batch, rect, argb),
            m::Shape::Text(text, at, rgb, shadow) => game.native_text(batch, &text, at, rgb, shadow),
        }
    }
}
