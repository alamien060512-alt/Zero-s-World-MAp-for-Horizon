#!/usr/bin/env python3
"""Compile the real bundle on a tiny source fixture; no Horizon/assets required."""
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
import horizon


def run(command, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return result


def snapshot(root):
    return {str(p.relative_to(root)): (horizon.digest(p.read_bytes()), p.stat().st_mtime_ns)
            for p in root.rglob('*') if p.is_file()}


def audit(original, stage, plan):
    run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'native-mod-source-audit',
         '--', str(original), str(stage), str(plan)], cwd=horizon.ROOT)



# A small stand-in for the parts of Horizon the hooks touch: the hook points
# and helpers of native_hooks.rs and the audio/HUD types the adapters name.
FIXTURE = {
    'lib.rs': 'extern crate self as horizon_client;\npub mod extras;\npub mod extras_hud;\npub mod game;\npub mod hud;\npub mod music;\npub mod sound_engine;\n',
    'extras.rs': '#[derive(Clone, Copy)]\npub enum Corner { BottomRight }\n',
    'extras_hud.rs': 'pub struct Corners;\nimpl Corners { pub fn place(&mut self, _c: crate::extras::Corner, _w: f32, _h: f32) -> (f32, f32) { (0.0, 0.0) } }\n',
    'hud.rs': 'pub struct HudBatch;\n',
    'music.rs': """use crate::sound_engine::Sounds;
#[derive(Default)]
pub struct MusicManager { pub toast_requests: Vec<bool> }
impl MusicManager {
    pub fn current_instance(&self) -> Option<u64> { None }
    pub fn stop_playing(&mut self, _situational: Option<&u8>, _sounds: &mut impl Sounds) {}
}
""",
    'sound_engine.rs': """#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayResult { Started, StartedSilently, NotStarted }
pub const MUSIC: usize = 1;
pub struct SoundEngine;
impl SoundEngine { pub fn unseeded() -> u64 { 0 } }
pub struct SoundInstance;
impl SoundInstance { pub fn for_music(_id: &str, _random: u64) -> Self { Self } }
pub trait Sounds {
    fn is_active(&mut self, id: u64) -> bool;
    fn stop_instance(&mut self, id: u64);
    fn release(&mut self, id: u64);
    fn update_category_volume(&mut self, source: usize, gain: f32);
    fn play_instance(&mut self, instance: SoundInstance, retain: bool) -> (u64, PlayResult);
}
#[derive(Default)]
pub struct Audio;
impl Sounds for Audio {
    fn is_active(&mut self, _id: u64) -> bool { false }
    fn stop_instance(&mut self, _id: u64) {}
    fn release(&mut self, _id: u64) {}
    fn update_category_volume(&mut self, _source: usize, _gain: f32) {}
    fn play_instance(&mut self, _instance: SoundInstance, _retain: bool) -> (u64, PlayResult) { (1, PlayResult::Started) }
}
""",
    'game.rs': """pub mod native_hooks;
pub struct Game<'a> {
    pub ticks: &'a mut u32,
    audio: crate::sound_engine::Audio,
    music: crate::music::MusicManager,
}
impl Game<'_> { pub(super) fn gui_size(&self) -> (f32, f32) { (480.0, 270.0) } }
#[test] fn original_body_runs_once_per_call() {
    let mut ticks = 0;
    let mut game = Game { ticks: &mut ticks, audio: Default::default(), music: Default::default() };
    game.native_tick();
    game.native_tick();
    assert_eq!(ticks, 2);
}
""",
    'game/native_hooks.rs': """use super::Game;
use crate::{extras::Corner, hud::HudBatch};
pub struct NativePlayer { pub x: f64, pub y: f64, pub z: f64, pub yaw: f32, pub pitch: f32, pub dimension: &'static str }
pub struct NativeMod { pub id: String, pub title: String, pub description: String, pub keys: Vec<[String; 3]> }
impl Game<'_> {
    pub(super) fn native_tick(&mut self) { *self.ticks += 1; }
    pub(super) fn native_hud(&mut self, batch: &mut HudBatch, corners: &mut crate::extras_hud::Corners) { let _ = (batch, corners); }
    pub(super) fn native_key(&mut self, key: &str, pressed: bool) { let _ = (key, pressed); }
    pub(super) fn native_command(&mut self, command: &str, ran: &mut bool) { let _ = (command, ran); }
    pub(super) fn native_mods(&mut self, mods: &mut Vec<NativeMod>) { let _ = mods; }
    pub(super) fn native_settings(&mut self, mod_id: &str, buttons: &mut Vec<String>) { let _ = (mod_id, buttons); }
    pub(super) fn native_settings_click(&mut self, mod_id: &str, button: usize) { let _ = (mod_id, button); }
    pub(super) fn native_window(&self) -> bool { true }
    pub(super) fn native_on(&self, _mod_id: &str) -> bool { true }
    pub(super) fn native_playing(&self) -> bool { true }
    pub(super) fn native_setting(&self, _mod_id: &str, _key: &str) -> Option<String> { None }
    pub(super) fn native_set_setting(&self, _mod_id: &str, _key: &str, _value: &str) {}
    pub(super) fn native_mod_key(&self, _mod_id: &str, _name: &str, default: &str) -> String { default.to_owned() }
    pub(super) fn native_chat(&mut self, _text: &str) {}
    pub(super) fn native_player(&self) -> Option<NativePlayer> { Some(NativePlayer { x: 0.0, y: 64.0, z: 0.0, yaw: 0.0, pitch: 0.0, dimension: "minecraft:overworld" }) }
    pub(super) fn native_world_height(&self) -> (i32, i32) { (-64, 384) }
    pub(super) fn native_block(&self, _pos: [i32; 3]) -> String { "minecraft:air".to_owned() }
    pub(super) fn native_cursor(&self) -> [f32; 2] { [0.0, 0.0] }
    pub(super) fn native_data_dir(&self, mod_id: &str) -> std::path::PathBuf { std::path::PathBuf::from(mod_id) }
    pub(super) fn native_corner_shown(&self, _corner: Corner) -> bool { true }
    pub(super) fn native_text_width(&self, text: &str) -> f32 { text.len() as f32 }
    pub(super) fn native_fill(&self, _batch: &mut HudBatch, _rect: [f32; 4], _argb: u32) {}
    pub(super) fn native_text(&self, _batch: &mut HudBatch, _text: &str, _at: [f32; 2], _rgb: u32, _shadow: bool) {}
}
""",
}


def main():
    root = horizon.ROOT
    (root/'target').mkdir(exist_ok=True)
    run(['cargo', 'build', '--locked', '--offline', '-p', 'horizon-mixins-cli'], cwd=horizon.MIXINS)
    cli = horizon.MIXINS/'target/debug/horizon-mixins'
    manifests = list((root/'mixins').glob('*/horizon-mixin.toml'))
    assert len(manifests) == 1, 'this test expects one bundle'
    manifest = tomllib.loads(manifests[0].read_text())
    with tempfile.TemporaryDirectory(prefix='template-test-', dir=root/'target') as scratch:
        scratch = Path(scratch)
        original, stage = scratch/'original', scratch/'stage'
        (original/'crates/horizon-client/src').mkdir(parents=True)
        (original/'Cargo.toml').write_text('[workspace]\nmembers=["crates/horizon-client"]\nresolver="2"\n')
        (original/'crates/horizon-client/Cargo.toml').write_text('[package]\nname="horizon-client"\nversion="0.1.0"\nedition="2024"\n')
        src = original/'crates/horizon-client/src'
        (src/'game').mkdir()
        for name, text in FIXTURE.items():
            (src/name).write_text(text)
        (original/'rust-toolchain.toml').write_bytes((root/'rust-toolchain.toml').read_bytes())
        (original/'.horizon-revision').write_text(horizon.PIN+'\n')
        run(['cargo', 'generate-lockfile', '--offline'], cwd=original)
        before = snapshot(original)
        base = ['--workspace', str(original), '--mods', str(root/'mixins')]
        report = json.loads(run([str(cli), 'check', *base, '--out', str(stage), '--trust-native-mods']).stdout)
        tests = (root/'mixins/worldmap/runtime.rs').read_text().count('#[test]') + 1
        assert report['injections'] == len(manifest['injection']) == 7 and report['modules'] == 1
        assert f'{tests} passed' in report['cargo_stdout'], report['cargo_stdout']
        first = snapshot(stage)
        second = json.loads(run([str(cli), 'check', *base, '--out', str(stage), '--trust-native-mods']).stdout)
        assert not second['stage_changed'] and snapshot(stage) == first
        assert snapshot(original) == before
        plan_path = scratch/'plan.json'
        run([str(cli), 'plan', *base, '--out', str(plan_path)])
        plan = json.loads(plan_path.read_text())
        assert plan['schema'] == horizon.COMPAT['mixin_schema']
        assert plan['injections'][0]['mod_id'] == manifest['mod']['id']
        audit(original, stage, plan_path)
        (original/'.horizon-revision').write_text('incompatible\n')
        bad = subprocess.run([str(cli), 'apply', *base, '--out', str(stage)], capture_output=True, text=True)
        assert bad.returncode and json.loads(bad.stderr)['code'] == 'E_REVISION'
        assert snapshot(stage) == first
        (original/'.horizon-revision').write_text(horizon.PIN+'\n')
        source = original/'crates/horizon-client/src/game/native_hooks.rs'
        source.write_text(source.read_text().replace('native_tick', 'renamed_native_tick'))
        bad = subprocess.run([str(cli), 'apply', *base, '--out', str(stage)], capture_output=True, text=True)
        assert bad.returncode and json.loads(bad.stderr)['code'] == 'E_TARGET_NOT_FOUND'
        assert snapshot(stage) == first
    print(json.dumps({'status': 'ok', 'compiled_runtime_and_original_body': True,
                      'only_planned_asts_changed': True, 'deterministic_bytes_and_mtimes': True,
                      'incompatible_revision_and_target_rejected': True}, indent=2))


if __name__ == '__main__':
    main()
