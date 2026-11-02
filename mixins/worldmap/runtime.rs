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
/// `name|dimension|x|y|z|RRGGBB` and, for a hidden one, `|1` after that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waypoint {
    pub name: String,
    pub dim: String,
    pub pos: [i32; 3],
    pub rgb: u32,
    /// Kept in the list but not drawn on the map or in the world.
    pub hidden: bool,
}

const WAYPOINT_COLORS: [u32; 6] = [0xFF5555, 0x55FF55, 0x5555FF, 0xFFFF55, 0xFF55FF, 0x55FFFF];

impl Waypoint {
    pub fn line(&self) -> String {
        let mut line = format!(
            "{}|{}|{}|{}|{}|{:06X}",
            self.name.replace(['|', '\n', '\r'], " "),
            self.dim,
            self.pos[0],
            self.pos[1],
            self.pos[2],
            self.rgb
        );
        if self.hidden {
            line.push_str("|1");
        }
        line
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut parts = line.trim().split('|');
        let name = parts.next()?.to_owned();
        let dim = parts.next()?.to_owned();
        let mut coord = || parts.next()?.trim().parse::<i32>().ok();
        let pos = [coord()?, coord()?, coord()?];
        let rgb = u32::from_str_radix(parts.next()?.trim(), 16).ok()?;
        let hidden = parts.next().is_some_and(|f| f.trim() == "1");
        (!name.is_empty() && !dim.is_empty() && rgb <= 0xFF_FFFF).then_some(Self {
            name,
            dim,
            pos,
            rgb,
            hidden,
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
    let mut out =
        String::from("# World Map waypoints: name|dimension|x|y|z|RRGGBB[|1 when hidden]\n");
    for w in list {
        out.push_str(&w.line());
        out.push('\n');
    }
    out
}

/// `minecraft:the_nether` as `Nether`.
pub fn short_dim(dim: &str) -> String {
    let name = dim.rsplit(':').next().unwrap_or(dim);
    let name = name.strip_prefix("the_").unwrap_or(name).replace('_', " ");
    let mut chars = name.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

// ------------------------------------------------------------------- view

/// Pixels per block at each zoom step.
pub const ZOOMS: [f32; 8] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
const DEFAULT_ZOOM: usize = 4;

/// The fullscreen map's window on the world.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub open: bool,
    pub zoom: usize,
    /// Block the map is centred on; none follows the player.
    pub centre: Option<(f64, f64)>,
    /// The dimension shown; none is the player's own.
    pub dim: Option<String>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            open: false,
            zoom: DEFAULT_ZOOM,
            centre: None,
            dim: None,
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

/// The map's blocks as fills, runs of one colour merged along a row. A cell
/// is exactly one block once a block is a pixel or wider, and the fewest
/// blocks that fill a pixel when it is smaller, so no detail is thrown away
/// that the screen can show.
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
    let step = if scale >= 1.0 {
        1
    } else {
        (1.0 / scale).ceil() as i32
    };
    let cell = step as f32 * scale;
    let (left, top) = to_world((0.0, 0.0), centred, size, scale);
    let (right, bottom) = to_world(size, centred, size, scale);
    let x0 = floor_div(left.floor() as i32, step) * step;
    let mut out = Vec::new();
    let mut z = floor_div(top.floor() as i32, step) * step;
    while f64::from(z) < bottom {
        let y = to_screen((0.0, f64::from(z)), centred, size, scale).1;
        let mut run: Option<(i32, u32)> = None;
        let mut x = x0;
        loop {
            let past = f64::from(x) >= right;
            let pixel = if past {
                0
            } else {
                tile_pixel(tiles, x + step / 2, z + step / 2)
            };
            if run.is_none_or(|(_, p)| p != pixel) {
                if let Some((start, p)) = run.filter(|(_, p)| *p != 0) {
                    let sx = to_screen((f64::from(start), 0.0), centred, size, scale).0;
                    out.push(Shape::Fill([sx, y, (x - start) as f32 * scale, cell], p));
                }
                run = Some((x, pixel));
            }
            if past {
                break;
            }
            x += step;
        }
        z += step;
    }
    out
}

// ------------------------------------------------------- drawing helpers

/// A filled triangle as one-pixel-high rows.
fn triangle(p: [(f32, f32); 3], argb: u32) -> Vec<Shape> {
    let top = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i32;
    let bottom = p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i32;
    let mut out = Vec::new();
    for row in top..bottom {
        let yc = row as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..3 {
            let (a, b) = (p[i], p[(i + 1) % 3]);
            if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                xs.push(a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        if xs.len() >= 2 {
            let (lo, hi) = (
                xs.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
            );
            out.push(Shape::Fill([lo, row as f32, (hi - lo).max(1.0), 1.0], argb));
        }
    }
    out
}

/// An arrow at `at` pointing where a player facing `yaw` looks, on a map
/// with north up: yaw 0 faces south (down), 90 west (left).
pub fn arrow(at: (f32, f32), yaw: f32, len: f32, rgb: u32) -> Vec<Shape> {
    let (s, c) = yaw.to_radians().sin_cos();
    let dir = (-s, c);
    let perp = (dir.1, -dir.0);
    let corners = |k: f32| {
        let tip = (at.0 + dir.0 * len * k, at.1 + dir.1 * len * k);
        let back = (at.0 - dir.0 * len * 0.7 * k, at.1 - dir.1 * len * 0.7 * k);
        [
            tip,
            (
                back.0 + perp.0 * len * 0.6 * k,
                back.1 + perp.1 * len * 0.6 * k,
            ),
            (
                back.0 - perp.0 * len * 0.6 * k,
                back.1 - perp.1 * len * 0.6 * k,
            ),
        ]
    };
    let mut out = triangle(corners(1.35), 0xFF00_0000);
    out.extend(triangle(corners(1.0), 0xFF00_0000 | rgb));
    out
}

/// A filled diamond `r` pixels from its middle to a corner.
fn diamond(at: (f32, f32), r: i32, argb: u32) -> Vec<Shape> {
    let (x, y) = (at.0.round(), at.1.round());
    (-r..=r)
        .map(|dy| {
            let half = (r - dy.abs()) as f32;
            Shape::Fill([x - half, y + dy as f32, half * 2.0 + 1.0, 1.0], argb)
        })
        .collect()
}

/// The player's eye, as the world draws from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub eye: [f64; 3],
    /// Degrees; 0 faces south, 90 west.
    pub yaw: f32,
    /// Degrees; positive looks down.
    pub pitch: f32,
    /// The vertical field of view in degrees.
    pub fov: f32,
}

/// Where world point `target` appears on a `size` screen (GUI pixels), and how
/// far it is; none when it is behind the camera.
pub fn project(cam: &Camera, size: (f32, f32), target: [f64; 3]) -> Option<(f32, f32, f64)> {
    let (sy, cy) = f64::from(cam.yaw).to_radians().sin_cos();
    let (sp, cp) = f64::from(cam.pitch).to_radians().sin_cos();
    let fwd = [-sy * cp, -sp, cy * cp];
    let right = [-cy, 0.0, -sy];
    let up = [
        right[1] * fwd[2] - right[2] * fwd[1],
        right[2] * fwd[0] - right[0] * fwd[2],
        right[0] * fwd[1] - right[1] * fwd[0],
    ];
    let d = [
        target[0] - cam.eye[0],
        target[1] - cam.eye[1],
        target[2] - cam.eye[2],
    ];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    let depth = dot(fwd);
    if depth < 0.05 {
        return None;
    }
    let t = (f64::from(cam.fov).to_radians() / 2.0).tan();
    let aspect = f64::from(size.0) / f64::from(size.1);
    let nx = dot(right) / (depth * t * aspect);
    let ny = dot(up) / (depth * t);
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Some((
        (w / 2.0 * (1.0 + nx)) as f32,
        (h / 2.0 * (1.0 - ny)) as f32,
        dist,
    ))
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
    /// Waypoints drawn on the map.
    pub waypoints: bool,
    /// Waypoints drawn in the world, over the game's own view.
    pub in_world: bool,
    pub radius: i32,
}

const RADII: [i32; 3] = [4, 8, 12];

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan: true,
            waypoints: true,
            in_world: true,
            radius: 8,
        }
    }
}

// ------------------------------------------------------------- the screen

const BAR: f32 = 20.0;
const BUTTON: f32 = 16.0;
const PANEL: f32 = 170.0;
const ROW: f32 = 20.0;
const MAX_NAME: usize = 32;
/// Waypoints drawn in the world at once.
const MARKS: usize = 16;

/// What a button of the map screen does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    TogglePanel,
    AddHere,
    Follow,
    CycleDim,
    ToggleOptions,
    Close,
    ZoomIn,
    ZoomOut,
    Setting(usize),
    Goto(usize),
    Eye(usize),
    Remove(usize),
    MenuAdd,
    MenuCentre,
    TextOk,
    TextCancel,
}

/// A button: where it is, what it says and does.
#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub rect: [f32; 4],
    pub action: Action,
    pub label: String,
    /// Drawn pressed in (a toggle that is on).
    pub lit: bool,
    /// A colour chip drawn beside the label (a waypoint's).
    pub swatch: Option<u32>,
}

impl Widget {
    fn contains(&self, at: (f32, f32)) -> bool {
        let [x, y, w, h] = self.rect;
        at.0 >= x && at.0 < x + w && at.1 >= y && at.1 < y + h
    }
}

/// The name box for a new waypoint: where it goes (none is the player's feet)
/// and what has been typed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextInput {
    pub text: String,
    pub at: Option<(i32, i32)>,
}

/// The right-click menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Menu {
    pub at: (f32, f32),
    pub world: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    from: (f32, f32),
    centre: (f64, f64),
}

/// The map screen's own state.
#[derive(Debug, Default)]
pub struct Ui {
    pub panel: bool,
    pub options: bool,
    pub text: Option<TextInput>,
    pub menu: Option<Menu>,
    drag: Option<Drag>,
    pub scroll: usize,
    /// The size and pointer of the last frame.
    pub size: (f32, f32),
    pub mouse: (f32, f32),
    /// Asked for: the screen to open or close, done where the game can.
    pub want_open: bool,
    pub close: bool,
}

/// What to do to the game's screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub open: bool,
    pub close: bool,
}

pub struct State {
    pub map: WorldMap,
    pub view: View,
    pub settings: Settings,
    pub list: Vec<Waypoint>,
    pub player: Option<Pos>,
    pub ui: Ui,
    ticks: u64,
    root: Option<PathBuf>,
    loaded: BTreeSet<String>,
    waypoints_dirty: bool,
    settings_dirty: bool,
    last_save: u64,
    pub last_error: Option<String>,
}

fn on(value: Option<String>, default: bool) -> bool {
    value.map_or(default, |v| v != "false")
}

const DIMENSIONS: [&str; 3] = [
    "minecraft:overworld",
    "minecraft:the_nether",
    "minecraft:the_end",
];

impl State {
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Settings::default();
        let settings = Settings {
            scan: on(get("scan"), d.scan),
            waypoints: on(get("waypoints"), d.waypoints),
            in_world: on(get("in_world"), d.in_world),
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
            ui: Ui {
                panel: on(get("panel"), false),
                ..Ui::default()
            },
            ticks: 0,
            root: None,
            loaded: BTreeSet::new(),
            waypoints_dirty: false,
            settings_dirty: false,
            last_save: 0,
            last_error: None,
        }
    }

    /// Settings to keep: `(key, value)`.
    pub fn stored(&self) -> Vec<(&'static str, String)> {
        vec![
            ("scan", self.settings.scan.to_string()),
            ("waypoints", self.settings.waypoints.to_string()),
            ("in_world", self.settings.in_world.to_string()),
            ("radius", self.settings.radius.to_string()),
            ("zoom", self.view.zoom.to_string()),
            ("panel", self.ui.panel.to_string()),
        ]
    }

    /// The settings to write, once, after something changed them.
    pub fn changed(&mut self) -> Vec<(&'static str, String)> {
        if std::mem::take(&mut self.settings_dirty) {
            self.stored()
        } else {
            Vec::new()
        }
    }

    pub fn labels(&self) -> Vec<String> {
        let yes = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Explore as you move: {}", yes(self.settings.scan)),
            format!("Waypoints on the map: {}", yes(self.settings.waypoints)),
            format!("Waypoints in the world: {}", yes(self.settings.in_world)),
            format!("Explore radius: {} chunks", self.settings.radius),
        ]
    }

    pub fn click(&mut self, button: usize) {
        match button {
            0 => self.settings.scan = !self.settings.scan,
            1 => self.settings.waypoints = !self.settings.waypoints,
            2 => self.settings.in_world = !self.settings.in_world,
            3 => {
                let at = RADII
                    .iter()
                    .position(|r| *r == self.settings.radius)
                    .unwrap_or(0);
                self.settings.radius = RADII[(at + 1) % RADII.len()];
            }
            _ => return,
        }
        self.settings_dirty = true;
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
        self.load_dim(&pos.dim);
        if self.list.is_empty() && !self.waypoints_dirty && self.ticks == 1 {
            self.list = std::fs::read_to_string(root.join("waypoints.txt"))
                .map(|t| parse_waypoints(&t))
                .unwrap_or_default();
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

    /// Reads a dimension's saved map the first time it is wanted.
    fn load_dim(&mut self, dim: &str) {
        if let Some(root) = self.root.clone()
            && self.loaded.insert(dim.to_owned())
        {
            self.map.load(&root.join("map"), dim);
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

    /// The dimension the map shows.
    pub fn viewed_dim(&self) -> String {
        self.view
            .dim
            .clone()
            .or_else(|| self.player.as_ref().map(|p| p.dim.clone()))
            .unwrap_or_else(|| DIMENSIONS[0].to_owned())
    }

    /// Adds a waypoint at the player's feet. Returns its number (from 1).
    pub fn add_waypoint(&mut self, name: &str) -> Option<usize> {
        let p = self.player.as_ref()?;
        let at = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
        let dim = p.dim.clone();
        Some(self.add_waypoint_at(name, &dim, at))
    }

    /// Adds a waypoint anywhere. Returns its number (from 1).
    pub fn add_waypoint_at(&mut self, name: &str, dim: &str, pos: [i32; 3]) -> usize {
        let name = name.trim();
        let name = if name.is_empty() {
            format!("Waypoint {}", self.list.len() + 1)
        } else {
            name.replace(['|', '\n', '\r'], " ")
        };
        let rgb = WAYPOINT_COLORS[self.list.len() % WAYPOINT_COLORS.len()];
        self.list.push(Waypoint {
            name,
            dim: dim.to_owned(),
            pos,
            rgb,
            hidden: false,
        });
        self.waypoints_dirty = true;
        self.list.len()
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
            "open" => self.ui.want_open = !self.view.open,
            "zoom_in" => self.zoom(1, None),
            "zoom_out" => self.zoom(-1, None),
            "pan_left" => shift(-pan, 0.0),
            "pan_right" => shift(pan, 0.0),
            "pan_up" => shift(0.0, -pan),
            "pan_down" => shift(0.0, pan),
            "recentre" => {
                self.view.centre = None;
                self.view.dim = None;
            }
            "waypoint" => {
                self.add_waypoint("");
            }
            _ => return false,
        }
        true
    }

    /// Zooms `steps`, keeping the block under `anchor` (a screen position)
    /// where it is.
    fn zoom(&mut self, steps: i32, anchor: Option<(f32, f32)>) {
        let size = self.ui.size;
        let before = anchor.filter(|_| size.0 > 0.0).map(|at| {
            let world = to_world(at, self.centre(), size, self.view.scale());
            (at, world)
        });
        self.view.zoom_by(steps);
        self.settings_dirty = true;
        if let Some((at, world)) = before {
            let scale = f64::from(self.view.scale());
            self.view.centre = Some((
                world.0 - f64::from(at.0 - size.0 / 2.0) / scale,
                world.1 - f64::from(at.1 - size.1 / 2.0) / scale,
            ));
        }
    }

    // ------------------------------------------------- screen and the game

    /// Reconciles the map with the game's screen. `on_map` says the game's
    /// screen is the one the map borrows, `playing` that there is no screen;
    /// what comes back is for the game to do.
    pub fn plan(&mut self, on_map: bool, playing: bool) -> Plan {
        if self.view.open && !on_map {
            self.view.open = false;
            self.ui.close = false;
        }
        let close = std::mem::take(&mut self.ui.close) && on_map;
        let open = std::mem::take(&mut self.ui.want_open) && playing && !self.view.open;
        Plan { open, close }
    }

    /// The map's screen came up.
    pub fn opened(&mut self) {
        self.view.open = true;
        self.view.dim = None;
        self.view.centre = None;
        self.ui = Ui {
            panel: self.ui.panel,
            ..Ui::default()
        };
    }

    /// The map's screen went away.
    pub fn closed(&mut self) {
        self.view.open = false;
        self.ui.text = None;
        self.ui.menu = None;
        self.ui.drag = None;
        self.ui.options = false;
    }

    fn known_dims(&self) -> Vec<String> {
        let mut dims: BTreeSet<String> = DIMENSIONS.iter().map(|d| (*d).to_owned()).collect();
        dims.extend(self.loaded.iter().cloned());
        if let Some(p) = &self.player {
            dims.insert(p.dim.clone());
        }
        dims.into_iter().collect()
    }

    fn cycle_dim(&mut self) {
        let dims = self.known_dims();
        let now = self.viewed_dim();
        let at = dims.iter().position(|d| *d == now).unwrap_or(0);
        let next = dims[(at + 1) % dims.len()].clone();
        self.load_dim(&next);
        let own = self.player.as_ref().map(|p| p.dim.as_str());
        self.view.dim = (own != Some(next.as_str())).then_some(next);
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::TogglePanel => {
                self.ui.panel = !self.ui.panel;
                self.ui.scroll = 0;
                self.settings_dirty = true;
            }
            Action::AddHere => {
                if self.player.is_some() {
                    self.ui.text = Some(TextInput::default());
                }
            }
            Action::Follow => {
                self.view.centre = None;
                self.view.dim = None;
            }
            Action::CycleDim => self.cycle_dim(),
            Action::ToggleOptions => self.ui.options = !self.ui.options,
            Action::Close => self.ui.close = true,
            Action::ZoomIn => self.zoom(1, None),
            Action::ZoomOut => self.zoom(-1, None),
            Action::Setting(i) => self.click(i),
            Action::Goto(i) => {
                if let Some(w) = self.list.get(i) {
                    self.view.centre = Some((f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5));
                }
            }
            Action::Eye(i) => {
                if let Some(w) = self.list.get_mut(i) {
                    w.hidden = !w.hidden;
                    self.waypoints_dirty = true;
                }
            }
            Action::Remove(i) => {
                self.remove_waypoint(i + 1);
            }
            Action::MenuAdd => {
                let at = self.ui.menu.map(|m| m.world);
                self.ui.text = Some(TextInput {
                    text: String::new(),
                    at,
                });
            }
            Action::MenuCentre => {
                if let Some(m) = self.ui.menu {
                    self.view.centre =
                        Some((f64::from(m.world.0) + 0.5, f64::from(m.world.1) + 0.5));
                }
            }
            Action::TextOk => self.finish_text(),
            Action::TextCancel => self.ui.text = None,
        }
        self.ui.menu = None;
        if !matches!(action, Action::ToggleOptions | Action::Setting(_)) {
            self.ui.options = false;
        }
    }

    fn finish_text(&mut self) {
        let Some(input) = self.ui.text.take() else {
            return;
        };
        let dim = self.viewed_dim();
        let own_dim = self.player.as_ref().is_some_and(|p| p.dim == dim);
        let y = self
            .player
            .as_ref()
            .filter(|_| own_dim)
            .map_or(64, |p| p.y.floor() as i32);
        let pos = match (input.at, self.player.as_ref()) {
            (Some((x, z)), _) => [x, y, z],
            (None, Some(p)) => [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32],
            (None, None) => return,
        };
        self.add_waypoint_at(&input.text, &dim, pos);
    }

    /// The buttons now on screen, lowest layer first.
    pub fn widgets(&self, size: (f32, f32)) -> Vec<Widget> {
        let mut out = Vec::new();
        let width = |label: &str| label.chars().count() as f32 * 6.0 + 12.0;
        let put = |out: &mut Vec<Widget>, x: &mut f32, label: String, action, lit| {
            let w = width(&label);
            out.push(Widget {
                rect: [*x, 2.0, w, BUTTON],
                action,
                label,
                lit,
                swatch: None,
            });
            *x += w + 3.0;
        };
        let mut x = 4.0;
        put(
            &mut out,
            &mut x,
            "Waypoints".into(),
            Action::TogglePanel,
            self.ui.panel,
        );
        put(
            &mut out,
            &mut x,
            "+ Waypoint".into(),
            Action::AddHere,
            false,
        );
        put(&mut out, &mut x, "Follow".into(), Action::Follow, false);
        let dim = format!("Dim: {}", short_dim(&self.viewed_dim()));
        put(&mut out, &mut x, dim, Action::CycleDim, false);
        let options_x = x;
        put(
            &mut out,
            &mut x,
            "Options".into(),
            Action::ToggleOptions,
            self.ui.options,
        );
        let close_w = width("Close");
        out.push(Widget {
            rect: [size.0 - close_w - 4.0, 2.0, close_w, BUTTON],
            action: Action::Close,
            label: "Close".into(),
            lit: false,
            swatch: None,
        });
        let side = if self.ui.panel { PANEL } else { 0.0 };
        for (i, (label, action)) in [("+", Action::ZoomIn), ("-", Action::ZoomOut)]
            .into_iter()
            .enumerate()
        {
            out.push(Widget {
                rect: [
                    size.0 - side - 22.0,
                    size.1 - 60.0 + i as f32 * 20.0,
                    BUTTON,
                    BUTTON,
                ],
                action,
                label: label.into(),
                lit: false,
                swatch: None,
            });
        }
        if self.ui.panel {
            let dim = self.viewed_dim();
            let left = size.0 - PANEL;
            let mut y = BAR + 16.0;
            let rows = self
                .list
                .iter()
                .enumerate()
                .filter(|(_, w)| w.dim == dim)
                .skip(self.ui.scroll);
            for (i, w) in rows {
                if y + ROW > size.1 - 14.0 {
                    break;
                }
                let widget = |rect, action, label: String, lit, swatch| Widget {
                    rect,
                    action,
                    label,
                    lit,
                    swatch,
                };
                out.push(widget(
                    [left + 3.0, y + 3.0, 14.0, 14.0],
                    Action::Eye(i),
                    "o".into(),
                    !w.hidden,
                    None,
                ));
                let name: String = w.name.chars().take(16).collect();
                out.push(widget(
                    [left + 20.0, y + 1.0, PANEL - 42.0, ROW - 2.0],
                    Action::Goto(i),
                    name,
                    false,
                    Some(w.rgb),
                ));
                out.push(widget(
                    [left + PANEL - 20.0, y + 3.0, 14.0, 14.0],
                    Action::Remove(i),
                    "x".into(),
                    false,
                    None,
                ));
                y += ROW;
            }
        }
        if self.ui.options {
            for (i, label) in self.labels().into_iter().enumerate() {
                out.push(Widget {
                    rect: [options_x, BAR + i as f32 * (BUTTON + 1.0), 190.0, BUTTON],
                    action: Action::Setting(i),
                    label,
                    lit: false,
                    swatch: None,
                });
            }
        }
        if let Some(m) = self.ui.menu {
            let (w, x) = (130.0, m.at.0.min(size.0 - 130.0));
            for (i, (label, action)) in [
                ("Add waypoint here", Action::MenuAdd),
                ("Centre map here", Action::MenuCentre),
            ]
            .into_iter()
            .enumerate()
            {
                out.push(Widget {
                    rect: [x, m.at.1.min(size.1 - 34.0) + i as f32 * BUTTON, w, BUTTON],
                    action,
                    label: label.into(),
                    lit: false,
                    swatch: None,
                });
            }
        }
        if self.ui.text.is_some() {
            let (cx, cy) = (size.0 / 2.0, size.1 / 2.0);
            for (label, action, dx) in [
                ("OK", Action::TextOk, -60.0),
                ("Cancel", Action::TextCancel, 10.0),
            ] {
                out.push(Widget {
                    rect: [cx + dx, cy + 14.0, 50.0, BUTTON],
                    action,
                    label: label.into(),
                    lit: false,
                    swatch: None,
                });
            }
        }
        out
    }

    /// The button under `at`, if the layer that is up lets it be clicked.
    fn hit(&self, widgets: &[Widget], at: (f32, f32)) -> Option<Action> {
        let modal = |a: Action| {
            if self.ui.text.is_some() {
                matches!(a, Action::TextOk | Action::TextCancel)
            } else if self.ui.menu.is_some() {
                matches!(a, Action::MenuAdd | Action::MenuCentre)
            } else {
                true
            }
        };
        widgets
            .iter()
            .rev()
            .find(|w| modal(w.action) && w.contains(at))
            .map(|w| w.action)
    }

    fn over_chrome(&self, at: (f32, f32)) -> bool {
        at.1 < BAR || (self.ui.panel && at.0 >= self.ui.size.0 - PANEL)
    }

    /// A mouse button went down (`down`) or up over the map; true when the
    /// game must not act on it, which is always while the map is up.
    pub fn mouse(&mut self, button: i32, down: bool, at: (f32, f32)) -> bool {
        self.ui.mouse = at;
        if !down {
            if button == 0 {
                self.ui.drag = None;
            }
            return true;
        }
        let size = self.ui.size;
        if size.0 <= 0.0 {
            return true;
        }
        if button == 0 {
            let widgets = self.widgets(size);
            if let Some(action) = self.hit(&widgets, at) {
                self.apply(action);
            } else if self.ui.text.is_none() {
                self.ui.menu = None;
                self.ui.options = false;
                if !self.over_chrome(at) {
                    self.ui.drag = Some(Drag {
                        from: at,
                        centre: self.centre(),
                    });
                }
            }
        } else if button == 1 && self.ui.text.is_none() && !self.over_chrome(at) {
            let world = to_world(at, self.centre(), size, self.view.scale());
            self.ui.options = false;
            self.ui.menu = Some(Menu {
                at,
                world: (world.0.floor() as i32, world.1.floor() as i32),
            });
        }
        true
    }

    /// The wheel turned by `dy` (up is positive) over the map.
    pub fn scrolled(&mut self, dy: f64, at: (f32, f32)) -> bool {
        let steps = if dy > 0.0 {
            1
        } else if dy < 0.0 {
            -1
        } else {
            0
        };
        if self.ui.panel && at.0 >= self.ui.size.0 - PANEL {
            let dim = self.viewed_dim();
            let rows = self.list.iter().filter(|w| w.dim == dim).count();
            let scroll = self.ui.scroll as i32 - steps;
            self.ui.scroll = scroll.clamp(0, rows.saturating_sub(1) as i32) as usize;
        } else if steps != 0 {
            self.zoom(steps, Some(at));
        }
        true
    }

    /// A key went down over the map. `action` is the name of the map key it
    /// is bound to. True when the game must not act on it; false lets
    /// Escape close the screen as the game does.
    pub fn screen_key(&mut self, key: &str, action: Option<&str>) -> bool {
        const ESCAPE: &str = "key.keyboard.escape";
        if let Some(input) = self.ui.text.as_mut() {
            match key {
                ESCAPE => self.ui.text = None,
                "key.keyboard.enter" | "key.keyboard.keypad.enter" => self.apply(Action::TextOk),
                "key.keyboard.backspace" => {
                    input.text.pop();
                }
                _ => {}
            }
            return true;
        }
        if key == ESCAPE {
            if self.ui.menu.is_some() || self.ui.options {
                self.ui.menu = None;
                self.ui.options = false;
                return true;
            }
            return false;
        }
        match action {
            Some("open") => self.ui.close = true,
            Some("waypoint") => self.apply(Action::AddHere),
            Some(name) => {
                self.press(name);
            }
            None => {}
        }
        true
    }

    /// A character typed over the map; always taken.
    pub fn screen_char(&mut self, c: char) -> bool {
        if let Some(input) = self.ui.text.as_mut()
            && !c.is_control()
            && input.text.chars().count() < MAX_NAME
        {
            input.text.push(c);
        }
        true
    }

    /// Runs `/worldmap <args>`; the lines to show.
    pub fn command(&mut self, args: &str) -> Vec<String> {
        let mut words = args.split_whitespace();
        let rest = |w: std::str::SplitWhitespace<'_>| w.collect::<Vec<_>>().join(" ");
        match words.next().unwrap_or("status") {
            "open" => {
                self.ui.want_open = !self.view.open;
                vec!["Opening the map. Esc or the map key closes it.".into()]
            }
            "close" => {
                self.ui.close = self.view.open;
                vec!["Map closed.".into()]
            }
            "zoom" => match words.next().map(str::parse::<usize>) {
                Some(Ok(n)) if (1..=ZOOMS.len()).contains(&n) => {
                    self.view.zoom = n - 1;
                    self.settings_dirty = true;
                    vec![format!("Zoom {} of {}.", n, ZOOMS.len())]
                }
                _ => vec![format!("Usage: /worldmap zoom <1-{}>", ZOOMS.len())],
            },
            "centre" | "center" => {
                self.view.centre = None;
                self.view.dim = None;
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
                    self.view.centre = Some((f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5));
                    self.view.dim = Some(w.dim.clone());
                    self.ui.want_open = !self.view.open;
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
                    .map(|(i, w)| {
                        let hidden = if w.hidden { " (hidden)" } else { "" };
                        format!(
                            "{}. {} ({}, {}, {}) {}{hidden}",
                            i + 1,
                            w.name,
                            w.pos[0],
                            w.pos[1],
                            w.pos[2],
                            w.dim
                        )
                    })
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

    // ---------------------------------------------------------- drawing

    /// One frame of the map screen: `size` is the window in GUI pixels and
    /// `mouse` the pointer. A drag in progress moves the map first.
    pub fn frame(
        &mut self,
        size: (f32, f32),
        mouse: (f32, f32),
        width: &dyn Fn(&str) -> f32,
    ) -> Vec<Shape> {
        self.ui.size = size;
        self.ui.mouse = mouse;
        if let Some(d) = self.ui.drag {
            let scale = f64::from(self.view.scale());
            self.view.centre = Some((
                d.centre.0 - f64::from(mouse.0 - d.from.0) / scale,
                d.centre.1 - f64::from(mouse.1 - d.from.1) / scale,
            ));
        }
        let scale = self.view.scale();
        let centred = self.centre();
        let dim = self.viewed_dim();
        let mut out = vec![Shape::Fill([0.0, 0.0, size.0, size.1], 0xFF0E_1014)];
        out.extend(map_fills(&self.map, &dim, centred, size, scale));
        if self.settings.waypoints {
            for w in self.list.iter().filter(|w| w.dim == dim && !w.hidden) {
                let (x, y) = to_screen(
                    (f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5),
                    centred,
                    size,
                    scale,
                );
                if x < -60.0 || y < -10.0 || x > size.0 + 60.0 || y > size.1 + 10.0 {
                    continue;
                }
                out.extend(diamond((x, y), 5, 0xFF00_0000));
                out.extend(diamond((x, y), 4, 0xFF00_0000 | w.rgb));
                out.push(Shape::Text(
                    w.name.clone(),
                    [x - width(&w.name) / 2.0, y - 17.0],
                    0xFF_FFFF,
                    true,
                ));
            }
        }
        if let Some(p) = self.player.as_ref().filter(|p| p.dim == dim) {
            let at = to_screen((p.x, p.z), centred, size, scale);
            out.extend(arrow(at, p.yaw, 7.0, 0xFFFFFF));
        }
        // Chrome.
        out.push(Shape::Fill([0.0, 0.0, size.0, BAR], 0xE010_1418));
        if self.ui.panel {
            let left = size.0 - PANEL;
            out.push(Shape::Fill([left, BAR, PANEL, size.1 - BAR], 0xE010_1418));
            let count = self.list.iter().filter(|w| w.dim == dim).count();
            out.push(Shape::Text(
                format!("Waypoints ({count})"),
                [left + 6.0, BAR + 4.0],
                0xFF_FFFF,
                true,
            ));
            if count == 0 {
                out.push(Shape::Text(
                    "None here yet".into(),
                    [left + 6.0, BAR + 20.0],
                    0xB3_B3B3,
                    false,
                ));
            }
        }
        let widgets = self.widgets(size);
        let at = self.hit(&widgets, mouse);
        let dialog = |w: &Widget| matches!(w.action, Action::TextOk | Action::TextCancel);
        let draw = |out: &mut Vec<Shape>, w: &Widget| {
            let [x, y, bw, bh] = w.rect;
            let hot = at == Some(w.action);
            let fill = match (hot, w.lit) {
                (true, _) => 0xFF5A_5A6E,
                (false, true) => 0xFF2F_5C94,
                (false, false) => 0xFF2A_2D35,
            };
            out.push(Shape::Fill(
                [x - 1.0, y - 1.0, bw + 2.0, bh + 2.0],
                0xFF00_0000,
            ));
            out.push(Shape::Fill([x, y, bw, bh], fill));
            let mut tx = x + (bw - width(&w.label)) / 2.0;
            if let Some(rgb) = w.swatch {
                out.push(Shape::Fill(
                    [x + 3.0, y + 4.0, 8.0, bh - 8.0],
                    0xFF00_0000 | rgb,
                ));
                tx = x + 15.0;
            }
            out.push(Shape::Text(
                w.label.clone(),
                [tx, y + (bh - 8.0) / 2.0],
                0xFF_FFFF,
                true,
            ));
        };
        for w in widgets.iter().filter(|w| !dialog(w)) {
            draw(&mut out, w);
        }
        // Status line.
        let cursor = to_world(mouse, centred, size, scale);
        let status = format!(
            "X {} Z {}   Zoom {}/{}   Drag: move   Wheel: zoom   Right-click: menu   Esc: close",
            cursor.0.floor() as i32,
            cursor.1.floor() as i32,
            self.view.zoom + 1,
            ZOOMS.len()
        );
        out.push(Shape::Fill(
            [0.0, size.1 - 14.0, width(&status) + 8.0, 14.0],
            0xB000_0000,
        ));
        out.push(Shape::Text(status, [4.0, size.1 - 11.0], 0xFF_FFFF, false));
        if let Some(input) = &self.ui.text {
            let (cx, cy) = (size.0 / 2.0, size.1 / 2.0);
            out.push(Shape::Fill([0.0, 0.0, size.0, size.1], 0x8000_0000));
            out.push(Shape::Fill(
                [cx - 111.0, cy - 41.0, 222.0, 82.0],
                0xFF00_0000,
            ));
            out.push(Shape::Fill(
                [cx - 110.0, cy - 40.0, 220.0, 80.0],
                0xFF1C_1F26,
            ));
            out.push(Shape::Text(
                "Name the waypoint".into(),
                [cx - 100.0, cy - 33.0],
                0xFF_FFFF,
                true,
            ));
            out.push(Shape::Fill(
                [cx - 101.0, cy - 21.0, 202.0, 16.0],
                0xFF00_0000,
            ));
            let line = format!("{}_", input.text);
            out.push(Shape::Text(line, [cx - 98.0, cy - 17.0], 0xFF_FFFF, false));
            for w in widgets.iter().filter(|w| dialog(w)) {
                draw(&mut out, w);
            }
        }
        out
    }

    /// Waypoints seen in the world, over the game's own view: a diamond
    /// where each one is, with its name and distance. Nothing while the map
    /// shows another dimension's or the setting is off.
    pub fn world_marks(
        &self,
        cam: &Camera,
        size: (f32, f32),
        width: &dyn Fn(&str) -> f32,
    ) -> Vec<Shape> {
        let Some(p) = self.player.as_ref().filter(|_| self.settings.in_world) else {
            return Vec::new();
        };
        let mut seen: Vec<(f64, f32, f32, &Waypoint)> = self
            .list
            .iter()
            .filter(|w| w.dim == p.dim && !w.hidden)
            .filter_map(|w| {
                let target = [
                    f64::from(w.pos[0]) + 0.5,
                    f64::from(w.pos[1]) + 1.0,
                    f64::from(w.pos[2]) + 0.5,
                ];
                let (x, y, dist) = project(cam, size, target)?;
                (x >= 0.0 && x < size.0 && y >= 0.0 && y < size.1).then_some((dist, x, y, w))
            })
            .collect();
        seen.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = Vec::new();
        for (dist, x, y, w) in seen.into_iter().take(MARKS) {
            out.extend(diamond((x, y), 5, 0xFF00_0000));
            out.extend(diamond((x, y), 4, 0xFF00_0000 | w.rgb));
            let label = format!("{} {}m", w.name, dist.round() as i64);
            out.push(Shape::Text(
                label.clone(),
                [
                    (x - width(&label) / 2.0).clamp(1.0, size.0 - width(&label) - 1.0),
                    y - 17.0,
                ],
                0xFF_FFFF,
                true,
            ));
        }
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
            hidden: false,
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
        let hidden = Waypoint {
            hidden: true,
            ..w.clone()
        };
        assert!(hidden.line().ends_with("|1"));
        assert_eq!(Waypoint::parse(&hidden.line()), Some(hidden));
        assert!(!Waypoint::parse("n|d|1|2|3|FFFFFF").unwrap().hidden);
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
    fn fills_are_one_block_a_cell_and_merge_runs() {
        let mut map = WorldMap::default();
        let mut tile = Tile::default();
        tile.pixels.iter_mut().for_each(|p| *p = 0xFF22_4466);
        tile.pixels[5] = 0xFF99_0000;
        map.insert("d", 0, 0, &tile);
        // Scale 1: a block is a pixel, so the 16 x 16 chunk is 16 rows.
        let fills = map_fills(&map, "d", (8.0, 8.0), (16.0, 16.0), 1.0);
        assert_eq!(fills.len(), 16 + 2, "row 0 is split by the odd block");
        assert_eq!(fills[0], Shape::Fill([0.0, 0.0, 5.0, 1.0], 0xFF22_4466));
        assert_eq!(fills[1], Shape::Fill([5.0, 0.0, 1.0, 1.0], 0xFF99_0000));
        assert_eq!(fills[2], Shape::Fill([6.0, 0.0, 10.0, 1.0], 0xFF22_4466));
        // Zoomed in, each block is wider: still one fill per run.
        let big = map_fills(&map, "d", (8.0, 8.0), (64.0, 64.0), 4.0);
        assert!(
            big.iter()
                .all(|s| matches!(s, Shape::Fill([_, _, w, h], _) if *h == 4.0 && *w >= 4.0))
        );
        // Zoomed out, a pixel covers several blocks.
        let far = map_fills(&map, "d", (8.0, 8.0), (4.0, 4.0), 0.25);
        assert!(
            far.iter()
                .all(|s| matches!(s, Shape::Fill([_, _, _, h], _) if *h == 1.0))
        );
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
        assert!(s.press("open") && s.ui.want_open);
        s.press("zoom_in");
        assert_eq!(s.view.zoom, DEFAULT_ZOOM + 1);
        for _ in 0..20 {
            s.press("zoom_out");
        }
        assert_eq!(s.view.zoom, 0);
        s.view.zoom = DEFAULT_ZOOM;
        s.press("pan_right");
        assert_eq!(s.view.centre, Some((3.5 + 32.0, 3.5)), "a pan is 64 pixels");
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
        assert_eq!(s.command("zoom 2"), vec!["Zoom 2 of 8."]);
        assert_eq!(s.view.zoom, 1);
        assert!(s.command("zoom 99")[0].starts_with("Usage"));
        s.command("add Mine");
        assert_eq!(s.command("goto 1"), vec!["Map centred on Mine."]);
        assert!(s.ui.want_open && s.view.centre == Some((3.5, 3.5)));
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
        s.click(3);
        s.click(7);
        assert_eq!(
            s.settings,
            Settings {
                scan: false,
                waypoints: false,
                in_world: false,
                radius: 12
            }
        );
        assert!(s.labels()[0].ends_with("OFF") && s.labels()[3].contains("12"));
        let stored: BTreeMap<_, _> = s.stored().into_iter().collect();
        assert_eq!(stored["scan"], "false");
        let back = State::read(|k| stored.get(k).cloned());
        assert_eq!(back.settings, s.settings);
    }

    #[test]
    fn the_screen_draws_map_player_waypoints_and_buttons() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("screen"), &world);
        s.add_waypoint("Spot");
        s.map.insert(
            "minecraft:overworld",
            0,
            0,
            &scan_chunk(&env(&world), 0, 0).unwrap(),
        );
        let shapes = s.frame((400.0, 225.0), (200.0, 112.0), &|t| t.len() as f32 * 6.0);
        let texts = texts(&shapes);
        for want in [
            "Spot",
            "Waypoints",
            "+ Waypoint",
            "Follow",
            "Options",
            "Close",
            "+",
            "-",
        ] {
            assert!(texts.iter().any(|t| t == want), "{want}");
        }
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("X ") && t.contains("Esc: close"))
        );
        assert!(shapes.len() > 40, "the map itself is drawn");
        s.settings.waypoints = false;
        let hidden = s.frame((400.0, 225.0), (0.0, 0.0), &|t| t.len() as f32 * 6.0);
        assert!(!texts_of(&hidden).iter().any(|t| t == "Spot"));
    }

    fn texts(shapes: &[Shape]) -> Vec<String> {
        texts_of(shapes)
    }

    fn texts_of(shapes: &[Shape]) -> Vec<String> {
        shapes
            .iter()
            .filter_map(|sh| match sh {
                Shape::Text(t, ..) => Some(t.clone()),
                Shape::Fill(..) => None,
            })
            .collect()
    }

    const SIZE: (f32, f32) = (427.0, 240.0);

    /// A state with a player, a frame drawn (so it knows the screen) and the map open.
    fn open_state(name: &str) -> State {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp(name), &world);
        s.opened();
        s.frame(SIZE, (0.0, 0.0), &|t| t.len() as f32 * 6.0);
        s
    }

    fn centre_of(s: &State, action: Action) -> (f32, f32) {
        let w = s
            .widgets(SIZE)
            .into_iter()
            .find(|w| w.action == action)
            .unwrap();
        (w.rect[0] + w.rect[2] / 2.0, w.rect[1] + w.rect[3] / 2.0)
    }

    fn click(s: &mut State, action: Action) {
        let at = centre_of(s, action);
        assert!(s.mouse(0, true, at));
        assert!(s.mouse(0, false, at));
        s.frame(SIZE, at, &|t| t.len() as f32 * 6.0);
    }

    #[test]
    fn the_arrow_points_the_way_the_player_faces() {
        // Which side of the middle the white triangle reaches farther on:
        // its tip is longer than its base is deep.
        let tip = |yaw: f32| {
            let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for s in arrow((50.0, 50.0), yaw, 10.0, 0xFFFFFF) {
                if let Shape::Fill([x, y, w, h], 0xFFFF_FFFF) = s {
                    (x0, x1) = (x0.min(x), x1.max(x + w));
                    (y0, y1) = (y0.min(y), y1.max(y + h));
                }
            }
            ((x1 - 50.0) - (50.0 - x0), (y1 - 50.0) - (50.0 - y0))
        };
        // South faces down the map, west left, north up, east right.
        assert!(tip(0.0).1 > 0.0, "south is down");
        assert!(tip(180.0).1 < 0.0, "north is up");
        assert!(tip(90.0).0 < 0.0, "west is left");
        assert!(tip(270.0).0 > 0.0, "east is right");
        assert!(!arrow((0.0, 0.0), 0.0, 7.0, 0xFFFFFF).is_empty());
    }

    #[test]
    fn projection_puts_things_where_the_camera_looks() {
        let cam = Camera {
            eye: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            fov: 70.0,
        };
        let size = (400.0, 200.0);
        // Straight ahead (south, +z) is the middle of the screen.
        let (x, y, d) = project(&cam, size, [0.0, 64.0, 10.0]).unwrap();
        assert!((x - 200.0).abs() < 0.01 && (y - 100.0).abs() < 0.01 && (d - 10.0).abs() < 1e-9);
        // West (-x) is to the right of a player facing south... no: their right hand is west.
        let (right, _, _) = project(&cam, size, [-5.0, 64.0, 10.0]).unwrap();
        assert!(right > 200.0, "west is on the right when facing south");
        let (left, _, _) = project(&cam, size, [5.0, 64.0, 10.0]).unwrap();
        assert!(left < 200.0);
        // Above is higher on screen (smaller y); behind is hidden.
        assert!(project(&cam, size, [0.0, 70.0, 10.0]).unwrap().1 < 100.0);
        assert!(project(&cam, size, [0.0, 64.0, -10.0]).is_none());
        // Turning to face west (yaw 90) puts a western point ahead.
        let west = Camera { yaw: 90.0, ..cam };
        let (x, _, _) = project(&west, size, [-10.0, 64.0, 0.0]).unwrap();
        assert!((x - 200.0).abs() < 0.01);
        // Looking down (pitch 90) puts a point below in front.
        let down = Camera { pitch: 45.0, ..cam };
        assert!(project(&down, size, [0.0, 54.0, 10.0]).unwrap().1 < 100.5);
        // The field of view sets the spread: the vertical edge is fov/2 off the axis.
        let edge = (35.0_f64).to_radians().tan() * 10.0;
        let (_, y, _) = project(&cam, size, [0.0, 64.0 + edge, 10.0]).unwrap();
        assert!(y.abs() < 0.01);
    }

    #[test]
    fn waypoints_show_in_the_world_nearest_first_and_can_be_switched_off() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("marks"), &world);
        s.add_waypoint_at("Far", "minecraft:overworld", [3, 65, 60]);
        s.add_waypoint_at("Near", "minecraft:overworld", [3, 65, 13]);
        s.add_waypoint_at("Behind", "minecraft:overworld", [3, 65, -30]);
        s.add_waypoint_at("Elsewhere", "minecraft:the_nether", [3, 65, 20]);
        let cam = Camera {
            eye: [3.5, 66.6, 3.5],
            yaw: 0.0,
            pitch: 0.0,
            fov: 70.0,
        };
        let marks = s.world_marks(&cam, (400.0, 225.0), &|t| t.len() as f32 * 6.0);
        let labels = texts_of(&marks);
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert!(labels[0].starts_with("Near ") && labels[0].ends_with("m"));
        assert!(labels[1].starts_with("Far "));
        s.list[1].hidden = true;
        assert_eq!(
            texts_of(&s.world_marks(&cam, (400.0, 225.0), &|t| t.len() as f32 * 6.0)).len(),
            1
        );
        s.settings.in_world = false;
        assert!(
            s.world_marks(&cam, (400.0, 225.0), &|t| t.len() as f32 * 6.0)
                .is_empty()
        );
    }

    #[test]
    fn the_map_opens_and_closes_with_the_game_screen() {
        let mut s = State::read(|_| None);
        s.tick(&pos(), (-64, 384), &temp("plan"), &world);
        s.press("open");
        assert_eq!(
            s.plan(false, true),
            Plan {
                open: true,
                close: false
            }
        );
        s.opened();
        assert!(s.view.open);
        // Asked to open while open does nothing; the key then closes.
        s.press("open");
        assert!(!s.ui.want_open);
        assert!(s.screen_key("key.keyboard.m", Some("open")));
        assert_eq!(
            s.plan(true, false),
            Plan {
                open: false,
                close: true
            }
        );
        s.closed();
        assert!(!s.view.open);
        // Another screen taking over (death, a menu) drops the map's state.
        s.opened();
        assert_eq!(
            s.plan(false, false),
            Plan {
                open: false,
                close: false
            }
        );
        assert!(!s.view.open);
        // The Close button asks the same way.
        s.opened();
        s.frame(SIZE, (0.0, 0.0), &|t| t.len() as f32 * 6.0);
        click(&mut s, Action::Close);
        assert!(s.plan(true, false).close);
    }

    #[test]
    fn toolbar_buttons_work_by_click() {
        let mut s = open_state("toolbar");
        assert!(!s.ui.panel);
        click(&mut s, Action::TogglePanel);
        assert!(
            s.ui.panel
                && s.changed()
                    .iter()
                    .any(|(k, v)| *k == "panel" && v == "true")
        );
        assert!(s.changed().is_empty(), "written once");
        click(&mut s, Action::ZoomIn);
        assert_eq!(s.view.zoom, DEFAULT_ZOOM + 1);
        click(&mut s, Action::ZoomOut);
        click(&mut s, Action::ZoomOut);
        assert_eq!(s.view.zoom, DEFAULT_ZOOM - 1);
        s.view.centre = Some((500.0, 500.0));
        click(&mut s, Action::Follow);
        assert_eq!(s.view.centre, None);
        click(&mut s, Action::ToggleOptions);
        assert!(
            s.widgets(SIZE)
                .iter()
                .any(|w| w.action == Action::Setting(3))
        );
        click(&mut s, Action::Setting(0));
        assert!(
            !s.settings.scan && s.ui.options,
            "the list stays up for more"
        );
        click(&mut s, Action::ToggleOptions);
        assert!(
            !s.widgets(SIZE)
                .iter()
                .any(|w| matches!(w.action, Action::Setting(_)))
        );
    }

    #[test]
    fn dimensions_cycle_and_come_back() {
        let mut s = open_state("dims");
        assert_eq!(s.viewed_dim(), "minecraft:overworld");
        click(&mut s, Action::CycleDim);
        assert_eq!(s.viewed_dim(), "minecraft:the_end");
        click(&mut s, Action::CycleDim);
        assert_eq!(s.viewed_dim(), "minecraft:the_nether");
        click(&mut s, Action::CycleDim);
        assert_eq!(s.viewed_dim(), "minecraft:overworld");
        assert_eq!(
            s.view.dim, None,
            "the player's own dimension is the default"
        );
        assert_eq!(short_dim("minecraft:the_nether"), "Nether");
        assert_eq!(short_dim("minecraft:overworld"), "Overworld");
        assert_eq!(short_dim("mymod:deep_dark"), "Deep dark");
    }

    #[test]
    fn a_new_waypoint_is_named_in_a_box() {
        let mut s = open_state("name");
        click(&mut s, Action::AddHere);
        assert!(s.ui.text.is_some());
        // Typing goes to the box and the game never sees it.
        for c in "Base".chars() {
            assert!(s.screen_char(c));
        }
        assert!(
            s.screen_char('\n'),
            "control characters are taken and dropped"
        );
        assert!(s.screen_key("key.keyboard.backspace", None));
        for c in "e2".chars() {
            s.screen_char(c);
        }
        assert_eq!(s.ui.text.as_ref().unwrap().text, "Base2");
        // Buttons behind the box are dead while it is up.
        let follow = centre_of(&s, Action::Follow);
        s.view.centre = Some((9.0, 9.0));
        s.mouse(0, true, follow);
        assert_eq!(s.view.centre, Some((9.0, 9.0)));
        assert!(s.screen_key("key.keyboard.enter", None));
        assert!(s.ui.text.is_none());
        assert_eq!(s.list.len(), 1);
        assert_eq!(
            (s.list[0].name.as_str(), s.list[0].pos),
            ("Base2", [3, 65, 3])
        );
        // Escape cancels; an empty name gets a default.
        click(&mut s, Action::AddHere);
        assert!(s.screen_key("key.keyboard.escape", None));
        assert!(s.ui.text.is_none() && s.list.len() == 1);
        click(&mut s, Action::AddHere);
        click(&mut s, Action::TextOk);
        assert_eq!(s.list[1].name, "Waypoint 2");
        // The box is capped.
        click(&mut s, Action::AddHere);
        for _ in 0..80 {
            s.screen_char('x');
        }
        assert_eq!(s.ui.text.as_ref().unwrap().text.len(), MAX_NAME);
    }

    #[test]
    fn right_click_menu_adds_a_waypoint_where_you_clicked() {
        let mut s = open_state("menu");
        // Scale 2 with the player at (3.5, 3.5): the screen's middle is that block.
        let at = (SIZE.0 / 2.0 + 20.0, SIZE.1 / 2.0 + 30.0);
        s.mouse(1, true, at);
        let menu = s.ui.menu.expect("menu");
        assert_eq!(menu.world, (13, 18));
        s.frame(SIZE, at, &|t| t.len() as f32 * 6.0);
        let labels = texts_of(&s.frame(SIZE, at, &|t| t.len() as f32 * 6.0));
        assert!(
            labels.contains(&"Add waypoint here".to_owned())
                && labels.contains(&"Centre map here".to_owned())
        );
        click(&mut s, Action::MenuAdd);
        assert!(s.ui.menu.is_none() && s.ui.text.as_ref().is_some_and(|t| t.at == Some((13, 18))));
        for c in "Mine".chars() {
            s.screen_char(c);
        }
        click(&mut s, Action::TextOk);
        assert_eq!(
            (s.list[0].name.as_str(), s.list[0].pos[0], s.list[0].pos[2]),
            ("Mine", 13, 18)
        );
        // Centre map here; and a click elsewhere just closes the menu.
        s.mouse(1, true, at);
        click(&mut s, Action::MenuCentre);
        assert_eq!(s.view.centre, Some((13.5, 18.5)));
        s.mouse(1, true, at);
        s.mouse(0, true, (100.0, 150.0));
        s.mouse(0, false, (100.0, 150.0));
        assert!(s.ui.menu.is_none());
        // Right-clicking on the toolbar does nothing.
        s.mouse(1, true, (50.0, 5.0));
        assert!(s.ui.menu.is_none());
    }

    #[test]
    fn dragging_pans_and_the_wheel_zooms_under_the_pointer() {
        let mut s = open_state("drag");
        let scale = f64::from(s.view.scale());
        s.mouse(0, true, (200.0, 120.0));
        s.frame(SIZE, (240.0, 120.0), &|t| t.len() as f32 * 6.0);
        let (x, _) = s.view.centre.unwrap();
        assert!(
            (x - (3.5 - 40.0 / scale)).abs() < 1e-6,
            "dragging right moves the map right"
        );
        s.mouse(0, false, (240.0, 120.0));
        s.frame(SIZE, (300.0, 120.0), &|t| t.len() as f32 * 6.0);
        assert_eq!(
            s.view.centre.unwrap().0,
            x,
            "it stops when the button is up"
        );
        // The block under the pointer stays put while zooming.
        let at = (300.0, 60.0);
        let before = to_world(at, s.centre(), SIZE, s.view.scale());
        s.scrolled(1.0, at);
        let after = to_world(at, s.centre(), SIZE, s.view.scale());
        assert!(s.view.zoom == DEFAULT_ZOOM + 1);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
        s.scrolled(-1.0, at);
        s.scrolled(-1.0, at);
        assert_eq!(s.view.zoom, DEFAULT_ZOOM - 1);
        // A drag that starts on the toolbar does not move the map.
        s.view.centre = None;
        s.mouse(0, true, (390.0, 18.0));
        s.frame(SIZE, (300.0, 100.0), &|t| t.len() as f32 * 6.0);
        assert_eq!(s.view.centre, None);
    }

    #[test]
    fn the_waypoint_list_selects_hides_and_deletes() {
        let mut s = open_state("panel");
        s.add_waypoint_at("Alpha", "minecraft:overworld", [100, 70, 200]);
        s.add_waypoint_at("Beta", "minecraft:overworld", [-50, 40, 0]);
        s.add_waypoint_at("Nether gate", "minecraft:the_nether", [1, 1, 1]);
        click(&mut s, Action::TogglePanel);
        let shown = |s: &State| {
            s.widgets(SIZE)
                .iter()
                .filter(|w| matches!(w.action, Action::Goto(_)))
                .count()
        };
        assert_eq!(shown(&s), 2, "only this dimension's");
        let labels = texts_of(&s.frame(SIZE, (0.0, 0.0), &|t| t.len() as f32 * 6.0));
        assert!(labels.contains(&"Waypoints (2)".to_owned()));
        click(&mut s, Action::Goto(1));
        assert_eq!(s.view.centre, Some((-49.5, 0.5)));
        click(&mut s, Action::Eye(0));
        assert!(s.list[0].hidden);
        let on_map = texts_of(&s.frame(SIZE, (0.0, 0.0), &|t| t.len() as f32 * 6.0));
        assert!(
            on_map.iter().filter(|t| *t == "Alpha").count() == 1,
            "listed, not drawn on the map"
        );
        click(&mut s, Action::Remove(0));
        assert_eq!(
            s.list.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            ["Beta", "Nether gate"]
        );
        // The wheel over the list scrolls it, not the map.
        let zoom = s.view.zoom;
        s.scrolled(-1.0, (SIZE.0 - 10.0, 100.0));
        assert_eq!(s.view.zoom, zoom);
        // The list's width moves the zoom buttons out of its way.
        let plus = centre_of(&s, Action::ZoomIn);
        assert!(plus.0 < SIZE.0 - PANEL);
    }

    #[test]
    fn keys_over_the_map_are_taken_except_escape() {
        let mut s = open_state("screenkeys");
        assert!(
            !s.screen_key("key.keyboard.escape", None),
            "the game closes the screen"
        );
        assert!(
            s.screen_key("key.keyboard.e", None),
            "other keys never reach the screen"
        );
        let zoom = s.view.zoom;
        assert!(s.screen_key("key.keyboard.equal", Some("zoom_in")));
        assert_eq!(s.view.zoom, zoom + 1);
        assert!(s.screen_key("key.keyboard.b", Some("waypoint")));
        assert!(s.ui.text.is_some(), "B names a waypoint over the map");
        s.screen_key("key.keyboard.escape", None);
        // Escape first shuts a menu or the options list.
        s.mouse(1, true, (200.0, 100.0));
        assert!(s.screen_key("key.keyboard.escape", None) && s.ui.menu.is_none());
        s.apply(Action::ToggleOptions);
        assert!(s.screen_key("key.keyboard.escape", None) && !s.ui.options);
        assert!(!s.screen_key("key.keyboard.escape", None));
        assert!(s.mouse(0, true, (0.0, 0.0)) && s.scrolled(1.0, (0.0, 0.0)) && s.screen_char('a'));
        // The map key closes it.
        s.screen_key("key.keyboard.m", Some("open"));
        assert!(s.ui.close);
    }
}
