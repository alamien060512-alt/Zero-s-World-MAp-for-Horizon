// World Map for Horizon: the map's logic. Nothing here touches the game, so
// Cargo tests it alone; the hook adapters in this folder carry it to Horizon.
//
// Other mods (a minimap, say) read the map through `WorldMap::sample`,
// `WorldMap::image` and `Waypoint`, or through the files described in
// docs/FORMAT.md.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const ID: &str = "worldmap";
pub const NAME: &str = "World Map";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str =
    "A fullscreen map of the blocks you have explored, with zoom, panning and named waypoints.";

/// Blocks along a chunk's side.
pub const CHUNK: i32 = 16;
/// Chunks along a saved region's side.
const REGION: i32 = 8;
const MAGIC: &[u8; 4] = b"HWM1";
/// Ticks between two chunk scans.
const SCAN_EVERY: u64 = 2;
/// Ticks between two saves of changed regions.
const SAVE_EVERY: u64 = 600;
/// How many blocks above the player a column's search starts.
const HEADROOM: i32 = 48;
/// How far a search climbs out of solid ground to find open air.
const CLIMB: i32 = 128;
/// How deep a column's water is measured.
const WATER_DEPTH: i32 = 12;

// ---------------------------------------------------------------- colours

/// Whether `name` is a block you see through to what lies below it.
pub fn is_air(name: &str) -> bool {
    matches!(
        name,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "minecraft:light"
    )
}

pub fn is_water(name: &str) -> bool {
    matches!(name, "minecraft:water" | "minecraft:bubble_column")
}

/// The map colour (0xRRGGBB) of a block: its look from above, as a map shows
/// it. Names match by their most telling part, so modded blocks fall into the
/// nearest kind; a name nothing fits gets a steady grey-brown of its own.
pub fn block_color(name: &str) -> u32 {
    let n = name.rsplit(':').next().unwrap_or(name);
    const TABLE: &[(&str, u32)] = &[
        ("water", 0x3F76E4),
        ("lava", 0xD96514),
        ("magma", 0x8A3A12),
        ("ice", 0xA5C8FF),
        ("snow", 0xF8FEFE),
        ("powder_snow", 0xF8FEFE),
        ("grass_block", 0x5E9D34),
        ("mycelium", 0x6F6370),
        ("podzol", 0x5A3F1F),
        ("moss", 0x596D2C),
        ("leaves", 0x2F7A2B),
        ("azalea", 0x4E7A2B),
        ("fern", 0x4E8A2E),
        ("grass", 0x5E9D34),
        ("vine", 0x2F7A2B),
        ("lily_pad", 0x2F7A2B),
        ("kelp", 0x2E6F43),
        ("seagrass", 0x2E6F43),
        ("sand", 0xDBCF9B),
        ("sandstone", 0xD9C98C),
        ("gravel", 0x837F7E),
        ("clay", 0xA0A6B8),
        ("mud", 0x3B3A3E),
        ("dirt", 0x866043),
        ("farmland", 0x6F4A2B),
        ("path", 0x93763F),
        ("terracotta", 0x985F45),
        ("netherrack", 0x6F3535),
        ("crimson", 0x8A2D4B),
        ("warped", 0x2A8C85),
        ("soul", 0x4C3B30),
        ("basalt", 0x4B4B52),
        ("blackstone", 0x2B2630),
        ("nether_brick", 0x2C161A),
        ("glowstone", 0xF5D27E),
        ("end_stone", 0xDDDFA5),
        ("purpur", 0xA77BA8),
        ("obsidian", 0x15101F),
        ("bedrock", 0x3A3A3A),
        ("deepslate", 0x4D4D50),
        ("cobblestone", 0x7F7F7F),
        ("stone", 0x7D7D7D),
        ("andesite", 0x888888),
        ("diorite", 0xB8B8B8),
        ("granite", 0x95675A),
        ("tuff", 0x6C6D65),
        ("calcite", 0xDFE0DC),
        ("dripstone", 0x7C6652),
        ("ore", 0x7D7D7D),
        ("planks", 0x9C7F4E),
        ("log", 0x6F5636),
        ("wood", 0x6F5636),
        ("bamboo", 0x7B9A2D),
        ("cactus", 0x2F6F2A),
        ("pumpkin", 0xC2761A),
        ("melon", 0x7D9A2B),
        ("hay", 0xC2A21B),
        ("wool", 0xDADADA),
        ("concrete", 0x9A9A9A),
        ("glass", 0xC8E2EE),
        ("brick", 0x965B4C),
        ("iron", 0xCFCFCF),
        ("gold", 0xF1D24A),
        ("copper", 0xC0714F),
        ("diamond", 0x5ED6D1),
        ("emerald", 0x3AC05E),
        ("quartz", 0xECE6DF),
    ];
    if let Some(&(_, rgb)) = TABLE.iter().find(|(key, _)| n.contains(key)) {
        return rgb;
    }
    // Unknown: a steady muted colour from the name.
    let h = n.bytes().fold(0x811C_9DC5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let (r, g, b) = (
        90 + (h & 0x3F),
        90 + ((h >> 8) & 0x3F),
        90 + ((h >> 16) & 0x3F),
    );
    (r << 16) | (g << 8) | b
}

fn scale(rgb: u32, num: i32, den: i32) -> u32 {
    let ch = |shift: u32| {
        let v = ((rgb >> shift) & 0xFF) as i32;
        (v * num / den).clamp(0, 255) as u32
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

fn shade(rgb: u32, delta: i32) -> u32 {
    scale(rgb, 100 + delta * 8, 100)
}

// ------------------------------------------------------------------ tiles

/// One chunk's top view: 16 x 16 pixels, `0xAARRGGBB`, 0 where unknown. Row
/// `z * 16 + x`, north (smaller z) first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub pixels: Vec<u32>,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            pixels: vec![0; (CHUNK * CHUNK) as usize],
        }
    }
}

impl Tile {
    pub fn known(&self) -> usize {
        self.pixels.iter().filter(|p| **p != 0).count()
    }
}

/// What a scan needs to know of the world.
pub struct Env<'a> {
    /// The block name at `[x, y, z]` (air where nothing is loaded).
    pub read: &'a dyn Fn([i32; 3]) -> String,
    /// Lowest block y and the world's height.
    pub min_y: i32,
    pub height: i32,
    /// The player's y, where a search starts.
    pub from_y: i32,
    /// A dimension with a roof (the Nether): map the floor the player stands
    /// on, not the roof.
    pub ceiling: bool,
}

/// What a column's search found.
struct Surface {
    y: i32,
    rgb: u32,
}

fn surface(env: &Env<'_>, x: i32, z: i32) -> Option<Surface> {
    let top = env.min_y + env.height - 1;
    let read = |y: i32| (env.read)([x, y, z]);
    let mut y = if env.ceiling {
        env.from_y.clamp(env.min_y, top)
    } else {
        (env.from_y + HEADROOM).clamp(env.min_y, top)
    };
    if !is_air(&read(y)) {
        // Inside something solid: climb to open air above it.
        let limit = if env.ceiling { 32 } else { CLIMB };
        let mut up = y;
        let mut found = false;
        for _ in 0..limit {
            if up >= top {
                break;
            }
            up += 1;
            if is_air(&read(up)) {
                found = true;
                break;
            }
        }
        if found {
            y = up;
        } else if env.ceiling {
            return None;
        }
    }
    while y >= env.min_y {
        let name = read(y);
        if !is_air(&name) {
            if is_water(&name) {
                let mut depth = 1;
                while depth < WATER_DEPTH && y - depth >= env.min_y && is_water(&read(y - depth)) {
                    depth += 1;
                }
                return Some(Surface {
                    y,
                    rgb: scale(block_color(&name), 100 - depth * 4, 100),
                });
            }
            return Some(Surface {
                y,
                rgb: block_color(&name),
            });
        }
        y -= 1;
    }
    None
}

/// Scans chunk `[cx, cz]` from above; none when nothing in it is loaded.
pub fn scan_chunk(env: &Env<'_>, cx: i32, cz: i32) -> Option<Tile> {
    let mut tile = Tile::default();
    let mut heights = [i32::MIN; (CHUNK * CHUNK) as usize];
    for lz in 0..CHUNK {
        for lx in 0..CHUNK {
            let i = (lz * CHUNK + lx) as usize;
            let Some(s) = surface(env, cx * CHUNK + lx, cz * CHUNK + lz) else {
                continue;
            };
            heights[i] = s.y;
            // Light from the north west, as a printed map is lit.
            let north = if lz > 0 {
                heights[i - CHUNK as usize]
            } else {
                i32::MIN
            };
            let delta = if north == i32::MIN {
                0
            } else {
                (s.y - north).clamp(-3, 3)
            };
            tile.pixels[i] = 0xFF00_0000 | shade(s.rgb, delta);
        }
    }
    (tile.known() > 0).then_some(tile)
}

// -------------------------------------------------------------- the world

type TileKey = (i32, i32);

/// Everything explored, by dimension (`minecraft:overworld`...).
#[derive(Default)]
pub struct WorldMap {
    tiles: BTreeMap<String, HashMap<TileKey, Tile>>,
    scanned: BTreeMap<String, HashMap<TileKey, u64>>,
    dirty: BTreeSet<(String, i32, i32)>,
}

fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

impl WorldMap {
    /// Puts a scanned chunk in; known pixels replace old ones, unknown ones
    /// leave them (a column that read as empty keeps what was seen before).
    pub fn insert(&mut self, dim: &str, cx: i32, cz: i32, tile: &Tile) {
        let slot = self
            .tiles
            .entry(dim.to_owned())
            .or_default()
            .entry((cx, cz))
            .or_default();
        let mut changed = false;
        for (old, new) in slot.pixels.iter_mut().zip(&tile.pixels) {
            if *new != 0 && *old != *new {
                *old = *new;
                changed = true;
            }
        }
        if changed {
            self.dirty.insert((dim.to_owned(), cx, cz));
        }
    }

    /// The pixel (`0xAARRGGBB`, 0 unknown) of the block column at `x, z`.
    pub fn sample(&self, dim: &str, x: i32, z: i32) -> u32 {
        let Some(tiles) = self.tiles.get(dim) else {
            return 0;
        };
        tile_pixel(tiles, x, z)
    }

    /// A `width` x `height` image (`0xAARRGGBB`, row by row) of the columns
    /// around block `centre`, `blocks_per_pixel` columns to a pixel: what a
    /// minimap draws. Unknown pixels are 0.
    pub fn image(
        &self,
        dim: &str,
        centre: (i32, i32),
        size: (usize, usize),
        blocks_per_pixel: i32,
    ) -> Vec<u32> {
        let step = blocks_per_pixel.max(1);
        let mut out = Vec::with_capacity(size.0 * size.1);
        let Some(tiles) = self.tiles.get(dim) else {
            out.resize(size.0 * size.1, 0);
            return out;
        };
        for py in 0..size.1 as i32 {
            for px in 0..size.0 as i32 {
                let x = centre.0 + (px - size.0 as i32 / 2) * step;
                let z = centre.1 + (py - size.1 as i32 / 2) * step;
                out.push(tile_pixel(tiles, x, z));
            }
        }
        out
    }

    /// How many chunks of `dim` have anything on the map.
    pub fn chunks(&self, dim: &str) -> usize {
        self.tiles.get(dim).map_or(0, HashMap::len)
    }

    /// The nearest chunk within `radius` of `at` that is due for a scan at
    /// tick `now`: near ones again and again (blocks change), far ones rarely.
    pub fn next_due(&self, dim: &str, at: (i32, i32), radius: i32, now: u64) -> Option<(i32, i32)> {
        let seen = self.scanned.get(dim);
        for ring in 0..=radius {
            let interval = match ring {
                0..=2 => 60,
                3..=5 => 400,
                _ => 1200,
            };
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring {
                        continue;
                    }
                    let key = (at.0 + dx, at.1 + dz);
                    let due = seen
                        .and_then(|s| s.get(&key))
                        .is_none_or(|last| now.saturating_sub(*last) >= interval);
                    if due {
                        return Some(key);
                    }
                }
            }
        }
        None
    }

    pub fn mark_scanned(&mut self, dim: &str, key: TileKey, now: u64) {
        self.scanned
            .entry(dim.to_owned())
            .or_default()
            .insert(key, now);
    }

    /// Writes every changed region under `root` (`<root>/<dimension>/`).
    pub fn save(&mut self, root: &Path) -> std::io::Result<usize> {
        let mut regions: BTreeSet<(String, i32, i32)> = BTreeSet::new();
        for (dim, cx, cz) in &self.dirty {
            regions.insert((dim.clone(), floor_div(*cx, REGION), floor_div(*cz, REGION)));
        }
        let mut written = 0;
        for (dim, rx, rz) in &regions {
            let Some(tiles) = self.tiles.get(dim) else {
                continue;
            };
            let dir = root.join(dir_name(dim));
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("r.{rx}.{rz}.bin"));
            let temp = dir.join(format!("r.{rx}.{rz}.tmp"));
            std::fs::write(&temp, encode_region(tiles, *rx, *rz))?;
            std::fs::rename(&temp, &path)?;
            written += 1;
        }
        self.dirty.clear();
        Ok(written)
    }

    /// Reads the saved regions of `dim` under `root`; files that don't parse
    /// are skipped. Returns how many regions were read.
    pub fn load(&mut self, root: &Path, dim: &str) -> usize {
        let Ok(entries) = std::fs::read_dir(root.join(dir_name(dim))) else {
            return 0;
        };
        let mut read = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((rx, rz)) = parse_region_name(&name) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            let Some(chunks) = decode_region(&bytes, rx, rz) else {
                continue;
            };
            let tiles = self.tiles.entry(dim.to_owned()).or_default();
            for (key, tile) in chunks {
                tiles.entry(key).or_insert(tile);
            }
            read += 1;
        }
        read
    }
}

fn tile_pixel(tiles: &HashMap<TileKey, Tile>, x: i32, z: i32) -> u32 {
    let key = (floor_div(x, CHUNK), floor_div(z, CHUNK));
    tiles.get(&key).map_or(0, |t| {
        let (lx, lz) = (x.rem_euclid(CHUNK), z.rem_euclid(CHUNK));
        t.pixels[(lz * CHUNK + lx) as usize]
    })
}

/// A dimension's folder name: `minecraft:the_nether` -> `minecraft_the_nether`.
pub fn dir_name(dim: &str) -> String {
    dim.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_region_name(name: &str) -> Option<(i32, i32)> {
    let rest = name.strip_prefix("r.")?.strip_suffix(".bin")?;
    let (a, b) = rest.split_once('.')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Region file: `HWM1`, then for each of the 8 x 8 chunks (row by row) one
/// byte, 0 absent or 1 present, and 256 little-endian `u32` pixels if present.
fn encode_region(tiles: &HashMap<TileKey, Tile>, rx: i32, rz: i32) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for lz in 0..REGION {
        for lx in 0..REGION {
            match tiles.get(&(rx * REGION + lx, rz * REGION + lz)) {
                Some(tile) => {
                    out.push(1);
                    for p in &tile.pixels {
                        out.extend_from_slice(&p.to_le_bytes());
                    }
                }
                None => out.push(0),
            }
        }
    }
    out
}

fn decode_region(bytes: &[u8], rx: i32, rz: i32) -> Option<Vec<(TileKey, Tile)>> {
    let mut at = bytes.strip_prefix(MAGIC.as_slice())?;
    let mut out = Vec::new();
    for lz in 0..REGION {
        for lx in 0..REGION {
            let (&flag, rest) = at.split_first()?;
            at = rest;
            if flag == 0 {
                continue;
            }
            let (data, rest) = at.split_at_checked((CHUNK * CHUNK) as usize * 4)?;
            at = rest;
            let pixels = data
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            out.push(((rx * REGION + lx, rz * REGION + lz), Tile { pixels }));
        }
    }
    at.is_empty().then_some(out)
}

// -------------------------------------------------------------- waypoints

/// A named place. Saved in `waypoints.txt`, one per line:
/// `name|dimension|x|y|z|RRGGBB`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waypoint {
    pub name: String,
    pub dim: String,
    pub pos: [i32; 3],
    pub rgb: u32,
}

const WAYPOINT_COLORS: [u32; 6] = [0xFF5555, 0x55FF55, 0x5555FF, 0xFFFF55, 0xFF55FF, 0x55FFFF];

impl Waypoint {
    pub fn line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{:06X}",
            self.name.replace(['|', '\n', '\r'], " "),
            self.dim,
            self.pos[0],
            self.pos[1],
            self.pos[2],
            self.rgb
        )
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut parts = line.trim().split('|');
        let name = parts.next()?.to_owned();
        let dim = parts.next()?.to_owned();
        let mut coord = || parts.next()?.trim().parse::<i32>().ok();
        let pos = [coord()?, coord()?, coord()?];
        let rgb = u32::from_str_radix(parts.next()?.trim(), 16).ok()?;
        (!name.is_empty() && !dim.is_empty() && rgb <= 0xFF_FFFF).then_some(Self {
            name,
            dim,
            pos,
            rgb,
        })
    }
}

pub fn parse_waypoints(text: &str) -> Vec<Waypoint> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(Waypoint::parse)
        .collect()
}

pub fn format_waypoints(list: &[Waypoint]) -> String {
    let mut out = String::from("# World Map waypoints: name|dimension|x|y|z|RRGGBB\n");
    for w in list {
        out.push_str(&w.line());
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------- view

/// Pixels per block at each zoom step.
pub const ZOOMS: [f32; 7] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
const DEFAULT_ZOOM: usize = 3;

/// The fullscreen map's window on the world.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub open: bool,
    pub zoom: usize,
    /// Block the map is centred on; none follows the player.
    pub centre: Option<(f64, f64)>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            open: false,
            zoom: DEFAULT_ZOOM,
            centre: None,
        }
    }
}

impl View {
    pub fn scale(&self) -> f32 {
        ZOOMS[self.zoom.min(ZOOMS.len() - 1)]
    }
    pub fn zoom_by(&mut self, steps: i32) {
        self.zoom = (self.zoom as i32 + steps).clamp(0, ZOOMS.len() as i32 - 1) as usize;
    }
}

/// Screen position (GUI pixels) of block `world`, for a map `centred` on a
/// block in a `size` screen at `scale` pixels per block.
pub fn to_screen(
    world: (f64, f64),
    centred: (f64, f64),
    size: (f32, f32),
    scale: f32,
) -> (f32, f32) {
    (
        size.0 / 2.0 + (world.0 - centred.0) as f32 * scale,
        size.1 / 2.0 + (world.1 - centred.1) as f32 * scale,
    )
}

/// The block under screen position `at`.
pub fn to_world(at: (f32, f32), centred: (f64, f64), size: (f32, f32), scale: f32) -> (f64, f64) {
    (
        centred.0 + f64::from((at.0 - size.0 / 2.0) / scale),
        centred.1 + f64::from((at.1 - size.1 / 2.0) / scale),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Fill([f32; 4], u32),
    Text(String, [f32; 2], u32, bool),
}

/// GUI pixels of one map cell: smaller cells cost more shapes.
const CELL: f32 = 4.0;

/// The map's cells as fills, runs of one colour merged along a row.
pub fn map_fills(
    map: &WorldMap,
    dim: &str,
    centred: (f64, f64),
    size: (f32, f32),
    scale: f32,
) -> Vec<Shape> {
    let Some(tiles) = map.tiles.get(dim) else {
        return Vec::new();
    };
    // A cell is one block or more; at high zoom a block is wider than a cell.
    let cell = CELL.max(scale);
    let mut out = Vec::new();
    let rows = (size.1 / cell).ceil() as i32;
    let cols = (size.0 / cell).ceil() as i32;
    for row in 0..rows {
        let y = row as f32 * cell;
        let mut run: Option<(i32, u32)> = None;
        for col in 0..=cols {
            let pixel = if col < cols {
                let w = to_world(
                    (col as f32 * cell + cell / 2.0, y + cell / 2.0),
                    centred,
                    size,
                    scale,
                );
                tile_pixel(tiles, w.0.floor() as i32, w.1.floor() as i32)
            } else {
                0
            };
            match run {
                Some((_, p)) if p == pixel => {}
                _ => {
                    if let Some((start, p)) = run.filter(|(_, p)| *p != 0) {
                        out.push(Shape::Fill(
                            [start as f32 * cell, y, (col - start) as f32 * cell, cell],
                            p,
                        ));
                    }
                    run = Some((col, pixel));
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------ state

/// The player, as the tick reports them.
#[derive(Clone, Debug, PartialEq)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Degrees; 0 faces south, 90 west.
    pub yaw: f32,
    pub dim: String,
}

/// The mod's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub scan: bool,
    pub waypoints: bool,
    pub radius: i32,
}

const RADII: [i32; 3] = [4, 8, 12];

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan: true,
            waypoints: true,
            radius: 8,
        }
    }
}

pub struct State {
    pub map: WorldMap,
    pub view: View,
    pub settings: Settings,
    pub list: Vec<Waypoint>,
    pub player: Option<Pos>,
    ticks: u64,
    root: Option<PathBuf>,
    loaded: BTreeSet<String>,
    waypoints_dirty: bool,
    last_save: u64,
    pub last_error: Option<String>,
}

fn on(value: Option<String>, default: bool) -> bool {
    value.map_or(default, |v| v != "false")
}

impl State {
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Settings::default();
        let settings = Settings {
            scan: on(get("scan"), d.scan),
            waypoints: on(get("waypoints"), d.waypoints),
            radius: get("radius")
                .and_then(|v| v.parse().ok())
                .filter(|r| RADII.contains(r))
                .unwrap_or(d.radius),
        };
        Self {
            map: WorldMap::default(),
            view: View {
                zoom: get("zoom")
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|z| *z < ZOOMS.len())
                    .unwrap_or(DEFAULT_ZOOM),
                ..View::default()
            },
            settings,
            list: Vec::new(),
            player: None,
            ticks: 0,
            root: None,
            loaded: BTreeSet::new(),
            waypoints_dirty: false,
            last_save: 0,
            last_error: None,
        }
    }

    /// Settings to keep: `(key, value)`.
    pub fn stored(&self) -> Vec<(&'static str, String)> {
        vec![
            ("scan", self.settings.scan.to_string()),
            ("waypoints", self.settings.waypoints.to_string()),
            ("radius", self.settings.radius.to_string()),
            ("zoom", self.view.zoom.to_string()),
        ]
    }

    pub fn labels(&self) -> Vec<String> {
        let yes = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Explore as you move: {}", yes(self.settings.scan)),
            format!("Waypoints on the map: {}", yes(self.settings.waypoints)),
            format!("Explore radius: {} chunks", self.settings.radius),
        ]
    }

    pub fn click(&mut self, button: usize) {
        match button {
            0 => self.settings.scan = !self.settings.scan,
            1 => self.settings.waypoints = !self.settings.waypoints,
            2 => {
                let at = RADII
                    .iter()
                    .position(|r| *r == self.settings.radius)
                    .unwrap_or(0);
                self.settings.radius = RADII[(at + 1) % RADII.len()];
            }
            _ => {}
        }
    }

    /// Once a game tick. `read` is the world's block lookup.
    pub fn tick(
        &mut self,
        pos: &Pos,
        bounds: (i32, i32),
        root: &Path,
        read: &dyn Fn([i32; 3]) -> String,
    ) {
        self.ticks += 1;
        self.player = Some(pos.clone());
        self.root = Some(root.to_path_buf());
        if self.loaded.insert(pos.dim.clone()) {
            self.map.load(&root.join("map"), &pos.dim);
            if self.list.is_empty() {
                self.list = std::fs::read_to_string(root.join("waypoints.txt"))
                    .map(|t| parse_waypoints(&t))
                    .unwrap_or_default();
            }
        }
        if self.settings.scan && self.ticks.is_multiple_of(SCAN_EVERY) {
            let at = (
                floor_div(pos.x.floor() as i32, CHUNK),
                floor_div(pos.z.floor() as i32, CHUNK),
            );
            if let Some(key) = self
                .map
                .next_due(&pos.dim, at, self.settings.radius, self.ticks)
            {
                let env = Env {
                    read,
                    min_y: bounds.0,
                    height: bounds.1,
                    from_y: pos.y.floor() as i32,
                    ceiling: pos.dim.contains("nether"),
                };
                if let Some(tile) = scan_chunk(&env, key.0, key.1) {
                    self.map.insert(&pos.dim, key.0, key.1, &tile);
                }
                self.map.mark_scanned(&pos.dim, key, self.ticks);
            }
        }
        if self.ticks - self.last_save >= SAVE_EVERY {
            self.flush();
        }
    }

    /// Writes the map and the waypoints now.
    pub fn flush(&mut self) {
        self.last_save = self.ticks;
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut error = self.map.save(&root.join("map")).err();
        if self.waypoints_dirty {
            let result = std::fs::create_dir_all(&root).and_then(|()| {
                std::fs::write(root.join("waypoints.txt"), format_waypoints(&self.list))
            });
            match result {
                Ok(()) => self.waypoints_dirty = false,
                Err(e) => error = Some(e),
            }
        }
        self.last_error = error.map(|e| e.to_string());
    }

    fn centre(&self) -> (f64, f64) {
        self.view
            .centre
            .or_else(|| self.player.as_ref().map(|p| (p.x, p.z)))
            .unwrap_or((0.0, 0.0))
    }

    /// Adds a waypoint at the player's feet. Returns its number (from 1).
    pub fn add_waypoint(&mut self, name: &str) -> Option<usize> {
        let p = self.player.as_ref()?;
        let name = name.trim();
        let name = if name.is_empty() {
            format!("Waypoint {}", self.list.len() + 1)
        } else {
            name.replace(['|', '\n', '\r'], " ")
        };
        let rgb = WAYPOINT_COLORS[self.list.len() % WAYPOINT_COLORS.len()];
        self.list.push(Waypoint {
            name,
            dim: p.dim.clone(),
            pos: [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32],
            rgb,
        });
        self.waypoints_dirty = true;
        Some(self.list.len())
    }

    /// Removes waypoint `n` (from 1).
    pub fn remove_waypoint(&mut self, n: usize) -> Option<Waypoint> {
        if n == 0 || n > self.list.len() {
            return None;
        }
        self.waypoints_dirty = true;
        Some(self.list.remove(n - 1))
    }

    /// Applies the key named `name`; false for a name that is not ours.
    pub fn press(&mut self, name: &str) -> bool {
        let pan = 64.0 / f64::from(self.view.scale());
        let mut shift = |dx: f64, dz: f64| {
            let (x, z) = self.centre();
            self.view.centre = Some((x + dx, z + dz));
        };
        match name {
            "open" => self.view.open = !self.view.open,
            "zoom_in" => self.view.zoom_by(1),
            "zoom_out" => self.view.zoom_by(-1),
            "pan_left" => shift(-pan, 0.0),
            "pan_right" => shift(pan, 0.0),
            "pan_up" => shift(0.0, -pan),
            "pan_down" => shift(0.0, pan),
            "recentre" => self.view.centre = None,
            "waypoint" => {
                self.add_waypoint("");
            }
            _ => return false,
        }
        true
    }

    /// Runs `/worldmap <args>`; the lines to show.
    pub fn command(&mut self, args: &str) -> Vec<String> {
        let mut words = args.split_whitespace();
        let rest = |w: std::str::SplitWhitespace<'_>| w.collect::<Vec<_>>().join(" ");
        match words.next().unwrap_or("status") {
            "open" => {
                self.view.open = true;
                vec!["Map open. Close it with the map key or /worldmap close.".into()]
            }
            "close" => {
                self.view.open = false;
                vec!["Map closed.".into()]
            }
            "zoom" => match words.next().map(str::parse::<usize>) {
                Some(Ok(n)) if (1..=ZOOMS.len()).contains(&n) => {
                    self.view.zoom = n - 1;
                    vec![format!("Zoom {} of {}.", n, ZOOMS.len())]
                }
                _ => vec![format!("Usage: /worldmap zoom <1-{}>", ZOOMS.len())],
            },
            "centre" | "center" => {
                self.view.centre = None;
                vec!["Map follows you.".into()]
            }
            "add" => match self.add_waypoint(&rest(words)) {
                Some(n) => vec![format!("Waypoint {n} added: {}", self.list[n - 1].name)],
                None => vec!["Not in a world.".into()],
            },
            "remove" => match words.next().map(str::parse::<usize>) {
                Some(Ok(n)) => match self.remove_waypoint(n) {
                    Some(w) => vec![format!("Removed {}.", w.name)],
                    None => vec![format!("No waypoint {n}.")],
                },
                _ => vec!["Usage: /worldmap remove <number>".into()],
            },
            "goto" => match words.next().map(str::parse::<usize>) {
                Some(Ok(n)) if n >= 1 && n <= self.list.len() => {
                    let w = &self.list[n - 1];
                    self.view.centre = Some((f64::from(w.pos[0]), f64::from(w.pos[2])));
                    self.view.open = true;
                    vec![format!("Map centred on {}.", w.name)]
                }
                _ => vec!["Usage: /worldmap goto <number>".into()],
            },
            "list" => {
                if self.list.is_empty() {
                    return vec!["No waypoints. /worldmap add [name] makes one here.".into()];
                }
                self.list
                    .iter()
                    .enumerate()
                    .map(|(i, w)| format!("{}. {} ({}, {}, {}) {}", i + 1, w.name, w.pos[0], w.pos[1], w.pos[2], w.dim))
                    .collect()
            }
            "save" => {
                self.flush();
                match &self.last_error {
                    Some(e) => vec![format!("Could not save: {e}")],
                    None => vec!["Map saved.".into()],
                }
            }
            "status" => {
                let dim = self.player.as_ref().map_or("none", |p| p.dim.as_str());
                vec![format!(
                    "{dim}: {} chunks mapped, {} waypoints, exploring {}, zoom {} of {}.",
                    self.map.chunks(dim),
                    self.list.len(),
                    if self.settings.scan { "on" } else { "off" },
                    self.view.zoom + 1,
                    ZOOMS.len()
                )]
            }
            _ => vec!["/worldmap [open|close|zoom <n>|centre|add [name]|remove <n>|goto <n>|list|save|status]".into()],
        }
    }

    /// The fullscreen map for a `size` screen with the mouse at `cursor`.
    pub fn screen(
        &self,
        size: (f32, f32),
        cursor: (f32, f32),
        width: &dyn Fn(&str) -> f32,
    ) -> Vec<Shape> {
        let scale = self.view.scale();
        let centred = self.centre();
        let dim = self
            .player
            .as_ref()
            .map_or("minecraft:overworld", |p| p.dim.as_str());
        let mut out = vec![Shape::Fill([0.0, 0.0, size.0, size.1], 0xE00E_1014)];
        out.extend(map_fills(&self.map, dim, centred, size, scale));
        if self.settings.waypoints {
            for w in self.list.iter().filter(|w| w.dim == dim) {
                let (x, y) = to_screen(
                    (f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5),
                    centred,
                    size,
                    scale,
                );
                if x < -40.0 || y < -10.0 || x > size.0 + 40.0 || y > size.1 + 10.0 {
                    continue;
                }
                out.push(Shape::Fill([x - 3.0, y - 3.0, 7.0, 7.0], 0xFF00_0000));
                out.push(Shape::Fill(
                    [x - 2.0, y - 2.0, 5.0, 5.0],
                    0xFF00_0000 | w.rgb,
                ));
                out.push(Shape::Text(
                    w.name.clone(),
                    [x - width(&w.name) / 2.0, y - 14.0],
                    0xFF_FFFF,
                    true,
                ));
            }
        }
        if let Some(p) = self.player.as_ref().filter(|p| p.dim == dim) {
            let (x, y) = to_screen((p.x, p.z), centred, size, scale);
            out.push(Shape::Fill([x - 3.0, y - 3.0, 7.0, 7.0], 0xFF00_0000));
            out.push(Shape::Fill([x - 2.0, y - 2.0, 5.0, 5.0], 0xFFFF_FFFF));
            // The facing: 0 degrees is south (down the screen), 90 west.
            let (s, c) = p.yaw.to_radians().sin_cos();
            out.push(Shape::Fill(
                [x - s * 7.0 - 1.0, y + c * 7.0 - 1.0, 3.0, 3.0],
                0xFFFF_3030,
            ));
        }
        let at = to_world(cursor, centred, size, scale);
        let head = format!(
            "World Map  {dim}  zoom {}/{}  X {} Z {}",
            self.view.zoom + 1,
            ZOOMS.len(),
            at.0.floor() as i32,
            at.1.floor() as i32
        );
        out.push(Shape::Fill(
            [0.0, 0.0, width(&head) + 8.0, 14.0],
            0xA000_0000,
        ));
        out.push(Shape::Text(head, [4.0, 3.0], 0xFF_FFFF, true));
        let hint = "Arrows: pan   +/-: zoom   Home: follow you   B: waypoint here   M: close";
        let (hw, y) = (width(hint), size.1 - 14.0);
        out.push(Shape::Fill([0.0, y, hw + 8.0, 14.0], 0xA000_0000));
        out.push(Shape::Text(hint.into(), [4.0, y + 3.0], 0xB3_B3B3, false));
        out
    }
}

/// The keys Options > Mods lists: name, label, default key.
pub fn keys() -> Vec<[String; 3]> {
    [
        ("open", "Open / close map", "key.keyboard.m"),
        ("zoom_in", "Zoom in", "key.keyboard.equal"),
        ("zoom_out", "Zoom out", "key.keyboard.minus"),
        ("pan_left", "Pan left", "key.keyboard.left"),
        ("pan_right", "Pan right", "key.keyboard.right"),
        ("pan_up", "Pan up", "key.keyboard.up"),
        ("pan_down", "Pan down", "key.keyboard.down"),
        ("recentre", "Follow player", "key.keyboard.home"),
        ("waypoint", "Waypoint here", "key.keyboard.b"),
    ]
    .map(|(a, b, c)| [a.to_owned(), b.to_owned(), c.to_owned()])
    .to_vec()
}

// The game's tick and its HUD may run on different threads.
static STATE: std::sync::Mutex<Option<State>> = std::sync::Mutex::new(None);

/// Runs `f` on the window's state, read from the stored settings the first
/// time.
pub fn with<R>(get: impl Fn(&str) -> Option<String>, f: impl FnOnce(&mut State) -> R) -> R {
    let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(slot.get_or_insert_with(|| State::read(get)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat ground at y 64 (stone, grass on top at x >= 0), water for z < 0.
    fn world(at: [i32; 3]) -> String {
        let [x, y, z] = at;
        match y {
            ..=62 => "minecraft:stone",
            63 => {
                if z < 0 {
                    "minecraft:water"
                } else {
                    "minecraft:dirt"
                }
            }
            64 if z >= 0 => {
                if x >= 0 {
                    "minecraft:grass_block"
                } else {
                    "minecraft:sand"
                }
            }
            _ => "minecraft:air",
        }
        .to_owned()
    }

    fn env(read: &dyn Fn([i32; 3]) -> String) -> Env<'_> {
        Env {
            read,
            min_y: -64,
            height: 384,
            from_y: 70,
            ceiling: false,
        }
    }

    fn pos() -> Pos {
        Pos {
            x: 3.5,
            y: 65.0,
            z: 3.5,
            yaw: 0.0,
            dim: "minecraft:overworld".into(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("worldmap-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn colours_by_block_name() {
        assert_eq!(block_color("minecraft:grass_block"), 0x5E9D34);
        assert_eq!(block_color("minecraft:oak_leaves"), 0x2F7A2B);
        assert_eq!(block_color("minecraft:deepslate_iron_ore"), 0x4D4D50);
        assert_eq!(
            block_color("somemod:mystery"),
            block_color("othermod:mystery")
        );
        assert!(is_air("minecraft:cave_air") && !is_air("minecraft:stone"));
    }

    #[test]
    fn scan_finds_the_top_block_and_its_colour() {
        let tile = scan_chunk(&env(&world), 0, 0).expect("tile");
        assert_eq!(tile.known(), 256);
        // Row 0 has no northern neighbour, so it is the plain colour.
        assert_eq!(tile.pixels[0], 0xFF00_0000 | 0x5E9D34);
    }

    #[test]
    fn water_is_blue_and_darker_when_deep() {
        let deep = |at: [i32; 3]| {
            if at[1] <= 55 {
                "minecraft:stone"
            } else if at[1] <= 63 {
                "minecraft:water"
            } else {
                "minecraft:air"
            }
            .to_owned()
        };
        let shallow = |at: [i32; 3]| {
            if at[1] <= 62 {
                "minecraft:stone"
            } else if at[1] == 63 {
                "minecraft:water"
            } else {
                "minecraft:air"
            }
            .to_owned()
        };
        let d = scan_chunk(&env(&deep), 0, 0).unwrap().pixels[0] & 0xFF;
        let s = scan_chunk(&env(&shallow), 0, 0).unwrap().pixels[0] & 0xFF;
        assert!(d < s, "deep {d:#x} should be darker than shallow {s:#x}");
    }

    #[test]
    fn slopes_are_shaded_against_the_north() {
        let hill = |at: [i32; 3]| {
            let top = if at[2] >= 8 { 66 } else { 64 };
            if at[1] <= top {
                "minecraft:stone"
            } else {
                "minecraft:air"
            }
            .to_owned()
        };
        let t = scan_chunk(&env(&hill), 0, 0).unwrap();
        let (flat, rise) = (t.pixels[4 * 16] & 0xFF, t.pixels[8 * 16] & 0xFF);
        assert!(rise > flat, "a south-facing rise is lighter");
    }

    #[test]
    fn unloaded_chunks_give_nothing_and_keep_old_pixels() {
        let air = |_: [i32; 3]| "minecraft:air".to_owned();
        assert!(scan_chunk(&env(&air), 0, 0).is_none());
        let mut map = WorldMap::default();
        map.insert("d", 0, 0, &scan_chunk(&env(&world), 0, 0).unwrap());
        let before = map.sample("d", 1, 1);
        map.insert("d", 0, 0, &Tile::default());
        assert_eq!(map.sample("d", 1, 1), before);
        assert_ne!(before, 0);
    }

    #[test]
    fn the_nether_maps_the_floor_under_the_roof() {
        let nether = |at: [i32; 3]| {
            match at[1] {
                ..=30 | 120.. => "minecraft:netherrack",
                _ => "minecraft:air",
            }
            .to_owned()
        };
        let mut e = env(&nether);
        e.ceiling = true;
        e.from_y = 60;
        e.min_y = 0;
        e.height = 128;
        let tile = scan_chunk(&e, 0, 0).unwrap();
        assert_eq!(
            tile.pixels[0] & 0xFF_FFFF,
            block_color("minecraft:netherrack")
        );
        // Standing inside rock with no air within reach maps nothing.
        let solid = |_: [i32; 3]| "minecraft:netherrack".to_owned();
        assert!(scan_chunk(&Env { read: &solid, ..e }, 0, 0).is_none());
    }

    #[test]
    fn sample_and_image_use_negative_coordinates() {
        let mut map = WorldMap::default();
        let mut tile = Tile::default();
        tile.pixels[15 * 16 + 15] = 0xFF11_2233;
        map.insert("d", -1, -1, &tile);
        assert_eq!(map.sample("d", -1, -1), 0xFF11_2233);
        assert_eq!(map.sample("d", 0, 0), 0);
        assert_eq!(map.sample("other", -1, -1), 0);
        let img = map.image("d", (-1, -1), (3, 3), 1);
        assert_eq!(img.len(), 9);
        assert_eq!(img[4], 0xFF11_2233);
        assert_eq!(map.image("none", (0, 0), (2, 2), 1), vec![0; 4]);
    }

    #[test]
    fn next_due_goes_outward_and_waits_between_scans() {
        let mut map = WorldMap::default();
        assert_eq!(map.next_due("d", (5, 5), 4, 0), Some((5, 5)));
        map.mark_scanned("d", (5, 5), 0);
        let next = map.next_due("d", (5, 5), 4, 1).unwrap();
        assert_eq!((next.0 - 5).abs().max((next.1 - 5).abs()), 1);
        for dx in -4..=4 {
            for dz in -4..=4 {
                map.mark_scanned("d", (5 + dx, 5 + dz), 10);
            }
        }
        assert_eq!(map.next_due("d", (5, 5), 4, 11), None);
        assert_eq!(map.next_due("d", (5, 5), 4, 70), Some((5, 5)));
    }

    #[test]
    fn regions_save_and_load_back_exactly() {
        let dir = temp("regions");
        let mut map = WorldMap::default();
        for (cx, cz) in [(0_i32, 0_i32), (-1, -9), (7, 7), (8, 0)] {
            let mut t = Tile::default();
            t.pixels[3] = 0xFF00_0000 | (cx.unsigned_abs() << 8) | cz.unsigned_abs();
            map.insert("minecraft:the_nether", cx, cz, &t);
        }
        assert_eq!(map.save(&dir).unwrap(), 3, "(0,0) and (7,7) share a region");
        assert_eq!(map.save(&dir).unwrap(), 0, "nothing changed since");
        let mut back = WorldMap::default();
        assert_eq!(back.load(&dir, "minecraft:the_nether"), 3);
        assert_eq!(back.chunks("minecraft:the_nether"), 4);
        assert_eq!(
            back.sample("minecraft:the_nether", -16 + 3, -144),
            map.sample("minecraft:the_nether", -16 + 3, -144)
        );
        assert_eq!(back.load(&dir, "minecraft:overworld"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_region_files_are_skipped() {
        let dir = temp("damaged");
        let sub = dir.join("d");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("r.0.0.bin"), b"nope").unwrap();
        std::fs::write(sub.join("r.1.0.bin"), [b'H', b'W', b'M', b'1', 1, 0]).unwrap();
        std::fs::write(sub.join("notes.txt"), b"x").unwrap();
        let mut map = WorldMap::default();
        assert_eq!(map.load(&dir, "d"), 0);
        assert_eq!(map.chunks("d"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waypoints_round_trip_and_reject_bad_lines() {
        let w = Waypoint {
            name: "Home base".into(),
            dim: "minecraft:overworld".into(),
            pos: [-10, 64, 200],
            rgb: 0xFF5555,
        };
        assert_eq!(Waypoint::parse(&w.line()), Some(w.clone()));
        let text = format_waypoints(&[
            w.clone(),
            Waypoint {
                name: "A|B".into(),
                ..w.clone()
            },
        ]);
        let back = parse_waypoints(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].name, "A B");
        for bad in [
            "",
            "x|y",
            "|d|1|2|3|FFFFFF",
            "n|d|a|2|3|FFFFFF",
            "n|d|1|2|3|1000000",
            "n|d|1|2|3|zz",
        ] {
            assert_eq!(Waypoint::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn screen_and_world_positions_agree() {
        let size = (800.0, 450.0);
        let at = to_screen((130.0, -40.0), (100.0, -50.0), size, 2.0);
        assert_eq!(at, (460.0, 245.0));
        let back = to_world(at, (100.0, -50.0), size, 2.0);
        assert!((back.0 - 130.0).abs() < 1e-6 && (back.1 + 40.0).abs() < 1e-6);
    }

    #[test]
    fn fills_merge_runs_and_skip_unknown_cells() {
        let mut map = WorldMap::default();
        let mut tile = Tile::default();
        tile.pixels.iter_mut().for_each(|p| *p = 0xFF22_4466);
        map.insert("d", 0, 0, &tile);
        // Scale 1: 4-pixel cells are 4 blocks; the 16 x 16 chunk is 4 x 4 cells.
        let fills = map_fills(&map, "d", (8.0, 8.0), (16.0, 16.0), 1.0);
        assert_eq!(fills.len(), 4, "one run per row");
        assert_eq!(fills[0], Shape::Fill([0.0, 0.0, 16.0, 4.0], 0xFF22_4466));
        assert!(map_fills(&map, "other", (8.0, 8.0), (16.0, 16.0), 1.0).is_empty());
    }

    #[test]
    fn ticking_explores_around_the_player_and_saves() {
        let dir = temp("tick");
        let mut s = State::read(|_| None);
        let p = pos();
        for _ in 0..40 {
            s.tick(&p, (-64, 384), &dir, &world);
        }
        assert!(s.map.chunks("minecraft:overworld") > 4);
        assert_ne!(s.map.sample("minecraft:overworld", 3, 3), 0);
        s.flush();
        assert!(dir.join("map/minecraft_overworld").is_dir());
        let mut again = State::read(|_| None);
        again.tick(&p, (-64, 384), &dir, &|_| "minecraft:air".into());
        assert_eq!(
            again.map.sample("minecraft:overworld", 3, 3),
            s.map.sample("minecraft:overworld", 3, 3)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scanning_can_be_turned_off() {
        let dir = temp("off");
        let mut s = State::read(|k| (k == "scan").then(|| "false".to_owned()));
        for _ in 0..10 {
            s.tick(&pos(), (-64, 384), &dir, &world);
        }
        assert_eq!(s.map.chunks("minecraft:overworld"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waypoints_are_added_listed_and_kept_between_runs() {
        let dir = temp("way");
        let mut s = State::read(|_| None);
        assert_eq!(s.command("add Spawn"), vec!["Not in a world."]);
        s.tick(&pos(), (-64, 384), &dir, &world);
        assert_eq!(
            s.command("add Spawn base"),
            vec!["Waypoint 1 added: Spawn base"]
        );
        s.press("waypoint");
        assert_eq!(s.list[1].name, "Waypoint 2");
        assert!(s.command("list")[0].starts_with("1. Spawn base (3, 65, 3)"));
        assert_eq!(s.command("remove 2"), vec!["Removed Waypoint 2."]);
        assert_eq!(s.command("remove 9"), vec!["No waypoint 9."]);
        assert_eq!(s.command("save"), vec!["Map saved."]);
        let mut next = State::read(|_| None);
        next.tick(&pos(), (-64, 384), &dir, &world);
        assert_eq!(next.list, s.list);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_zoom_pan_and_recentre() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("keys"), &world);
        assert!(s.press("open") && s.view.open);
        s.press("zoom_in");
        assert_eq!(s.view.zoom, DEFAULT_ZOOM + 1);
        for _ in 0..20 {
            s.press("zoom_out");
        }
        assert_eq!(s.view.zoom, 0);
        s.view.zoom = DEFAULT_ZOOM;
        s.press("pan_right");
        assert_eq!(s.view.centre, Some((3.5 + 64.0, 3.5)));
        s.press("recentre");
        assert_eq!(s.view.centre, None);
        assert!(!s.press("nonsense"));
        for [name, _, _] in keys() {
            assert!(s.press(&name), "{name}");
        }
    }

    #[test]
    fn commands_zoom_goto_and_status() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("cmd"), &world);
        assert_eq!(s.command("zoom 2"), vec!["Zoom 2 of 7."]);
        assert_eq!(s.view.zoom, 1);
        assert!(s.command("zoom 99")[0].starts_with("Usage"));
        s.command("add Mine");
        assert_eq!(s.command("goto 1"), vec!["Map centred on Mine."]);
        assert!(s.view.open && s.view.centre == Some((3.0, 3.0)));
        assert!(s.command("status")[0].contains("0 chunks mapped, 1 waypoints"));
        assert!(s.command("wat")[0].starts_with("/worldmap"));
    }

    #[test]
    fn settings_read_click_and_store() {
        let s = State::read(|k| match k {
            "radius" => Some("12".into()),
            "zoom" => Some("99".into()),
            "waypoints" => Some("false".into()),
            _ => None,
        });
        assert_eq!(
            (s.settings.radius, s.view.zoom, s.settings.waypoints),
            (12, DEFAULT_ZOOM, false)
        );
        let mut s = State::read(|_| None);
        s.click(0);
        s.click(1);
        s.click(2);
        s.click(7);
        assert_eq!(
            s.settings,
            Settings {
                scan: false,
                waypoints: false,
                radius: 12
            }
        );
        assert!(s.labels()[0].ends_with("OFF") && s.labels()[2].contains("12"));
        let stored: BTreeMap<_, _> = s.stored().into_iter().collect();
        assert_eq!(stored["scan"], "false");
        let back = State::read(|k| stored.get(k).cloned());
        assert_eq!(back.settings, s.settings);
    }

    #[test]
    fn the_screen_draws_map_player_waypoints_and_hints() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("screen"), &world);
        s.add_waypoint("Spot");
        s.map.insert(
            "minecraft:overworld",
            0,
            0,
            &scan_chunk(&env(&world), 0, 0).unwrap(),
        );
        let shapes = s.screen((400.0, 225.0), (200.0, 112.0), &|t| t.len() as f32 * 6.0);
        let texts: Vec<&str> = shapes
            .iter()
            .filter_map(|sh| {
                if let Shape::Text(t, ..) = sh {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("World Map  minecraft:overworld"))
        );
        assert!(texts.contains(&"Spot"));
        assert!(texts.iter().any(|t| t.contains("Arrows")));
        assert!(shapes.len() > 10);
        s.settings.waypoints = false;
        let hidden = s.screen((400.0, 225.0), (0.0, 0.0), &|t| t.len() as f32 * 6.0);
        assert!(
            !hidden
                .iter()
                .any(|sh| matches!(sh, Shape::Text(t, ..) if t == "Spot"))
        );
    }
}
