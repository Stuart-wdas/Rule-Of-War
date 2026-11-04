//! Reads the original ROWTERR1 assets directly; no conversion runtime required.
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const MAX_SIZE: i32 = 2048;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub elevation: f32,
    pub surface: f32,
    pub moisture: f32,
    pub kind: u8,
}
impl Cell {
    pub fn water(self) -> bool {
        self.kind <= 2
    }
    pub fn cost(self) -> f32 {
        [f32::INFINITY, 5., 2.5, 1.2, 1., 1.8, 4.][self.kind.min(6) as usize]
    }
}
#[derive(Debug)]
pub struct Data {
    pub width: usize,
    pub height: usize,
    pub bounds: [f32; 4],
    pub scale: f32,
    pub cells: Vec<Cell>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Terrain {
    #[serde(default = "default_water_level")]
    pub water_level: f32,
    #[serde(default)]
    pub authored: Vec<f32>,
    pub id: String,
    pub cols: i32,
    pub rows: i32,
    pub seed: u32,
    #[serde(default)]
    pub procedural_seed: Option<String>,
    #[serde(default = "default_authored_scale")]
    pub authored_scale: f32,
    pub edits: BTreeMap<String, Cell>,
    pub revision: u64,
    #[serde(skip)]
    pub data: Option<Arc<Data>>,
}

/// Stable while the live map receives local edits, different for newly imported
/// authored/source allocations even when id, dimensions and revision are reused.
pub fn render_source_key(t: &Terrain) -> String {
    format!(
        "{}:{}:{}:{}:{:p}:{}:{:p}:{}:{}",
        t.id,
        t.cols,
        t.rows,
        t.seed,
        t.authored.as_ptr(),
        t.authored.len(),
        t.data.as_ref().map_or(std::ptr::null(), Arc::as_ptr),
        t.authored_scale.to_bits(),
        t.water_level.to_bits()
    )
}

pub type EditDelta = (String, Option<Cell>);

struct EffectiveEdits<'a> {
    base: std::iter::Peekable<std::collections::btree_map::Iter<'a, String, Cell>>,
    pending: std::iter::Peekable<std::collections::btree_map::Iter<'a, String, Option<Cell>>>,
}
impl<'a> Iterator for EffectiveEdits<'a> {
    type Item = (&'a String, &'a Cell);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match (self.base.peek(), self.pending.peek()) {
                (Some((a, _)), Some((b, _))) if a < b => return self.base.next(),
                (Some((a, _)), Some((b, _))) if a == b => {
                    self.base.next();
                    let (key, value) = self.pending.next().unwrap();
                    if let Some(cell) = value.as_ref() {
                        return Some((key, cell));
                    }
                }
                (_, Some(_)) => {
                    let (key, value) = self.pending.next().unwrap();
                    if let Some(cell) = value.as_ref() {
                        return Some((key, cell));
                    }
                }
                (Some(_), None) => return self.base.next(),
                (None, None) => return None,
            }
        }
    }
}

/// Merge sorted entries once. Only changed keys allocate; unchanged foundation
/// grading never causes logarithmic lookups, key clones or an authored-array copy.
pub fn edit_delta(
    before: &BTreeMap<String, Cell>,
    pending: &BTreeMap<String, Option<Cell>>,
    after: &BTreeMap<String, Cell>,
) -> Vec<EditDelta> {
    let mut a = EffectiveEdits {
        base: before.iter().peekable(),
        pending: pending.iter().peekable(),
    }
    .peekable();
    let mut b = after.iter().peekable();
    let mut changes = vec![];
    loop {
        match (a.peek(), b.peek()) {
            (Some((ka, _)), Some((kb, _))) if ka < kb => {
                let (key, _) = a.next().unwrap();
                changes.push((key.clone(), None));
            }
            (Some((ka, _)), Some((kb, _))) if ka == kb => {
                let (_, old) = a.next().unwrap();
                let (key, new) = b.next().unwrap();
                if old != new {
                    changes.push((key.clone(), Some(*new)));
                }
            }
            (_, Some(_)) => {
                let (key, value) = b.next().unwrap();
                changes.push((key.clone(), Some(*value)));
            }
            (Some(_), None) => {
                let (key, _) = a.next().unwrap();
                changes.push((key.clone(), None));
            }
            (None, None) => break,
        }
    }
    changes
}

/// Workers retain immutable snapshots. Defer a patch while one is in use instead
/// of letting Arc::make_mut deep-clone a large foundation map on the UI thread.
pub fn patch_snapshot(
    snapshot: &mut Arc<Terrain>,
    pending: &mut BTreeMap<String, Option<Cell>>,
    revision: u64,
) -> bool {
    let Some(terrain) = Arc::get_mut(snapshot) else {
        return false;
    };
    for (key, value) in std::mem::take(pending) {
        if let Some(cell) = value {
            terrain.edits.insert(key, cell);
        } else {
            terrain.edits.remove(&key);
        }
    }
    terrain.revision = revision;
    true
}
impl Default for Terrain {
    fn default() -> Self {
        Self::new("river-crossing", 160, 160)
    }
}
impl Terrain {
    pub fn new(id: &str, cols: i32, rows: i32) -> Self {
        Self::new_with_seed(id, cols, rows, Some(id.into()))
    }
    pub fn new_with_seed(id: &str, cols: i32, rows: i32, procedural_seed: Option<String>) -> Self {
        let mut t = Self {
            water_level: 0.33,
            authored: vec![],
            id: id.into(),
            cols: cols.clamp(10, MAX_SIZE),
            rows: rows.clamp(10, MAX_SIZE),
            seed: procedural_seed
                .as_deref()
                .map(crate::generation::seed_from)
                .unwrap_or(719381),
            procedural_seed,
            authored_scale: 160.,
            edits: BTreeMap::new(),
            revision: 0,
            data: None,
        };
        t.hydrate();
        t
    }
    pub fn hydrate(&mut self) {
        if !self.authored.is_empty() {
            self.data = None;
            return;
        }
        if self.id == crate::gis_prototype::ID {
            self.data = Some(crate::gis_prototype::data());
            return;
        }
        if self.id == "forward-base" || self.id == crate::training::ID {
            self.authored_scale = 48.;
            let training = self.id == crate::training::ID;
            let (cols, rows) = (self.cols, self.rows);
            self.authored = (0..self.rows)
                .flat_map(|z| {
                    (0..cols).map(move |x| {
                        if training {
                            crate::training::elevation(x as f32, z as f32)
                        } else {
                            crate::battlefield::elevation(x as f32, z as f32, cols, rows)
                        }
                    })
                })
                .collect();
            self.data = None;
            return;
        }
        static GRAND: OnceLock<Arc<Data>> = OnceLock::new();
        static WORLD: OnceLock<Arc<Data>> = OnceLock::new();
        static GALAXY: OnceLock<Arc<Data>> = OnceLock::new();
        self.data = match self.id.as_str() {
            "grand-front" => Some(
                GRAND
                    .get_or_init(|| {
                        Arc::new(
                            decode(include_bytes!(
                                "../../assets/terrain/grand-front-v3.terrain"
                            ))
                            .expect("grand terrain"),
                        )
                    })
                    .clone(),
            ),
            "world-war" => Some(
                WORLD
                    .get_or_init(|| {
                        Arc::new(
                            decode(include_bytes!("../../assets/terrain/world-war-v3.terrain"))
                                .expect("world terrain"),
                        )
                    })
                    .clone(),
            ),
            "galaxy-front" => Some(
                GALAXY
                    .get_or_init(|| {
                        Arc::new(
                            decode(include_bytes!(
                                "../../assets/terrain/galaxy-front-v3.terrain"
                            ))
                            .expect("galaxy terrain"),
                        )
                    })
                    .clone(),
            ),
            "river-crossing" => None,
            _ => self.procedural_seed.as_deref().map(|seed| {
                Arc::new(crate::generation::generate(
                    self.cols as usize,
                    self.rows as usize,
                    seed,
                ))
            }),
        };
    }
    pub fn cell(&self, x: f32, z: f32) -> Cell {
        if !self.edits.is_empty() {
            let key = format!("{},{}", x.round() as i32, z.round() as i32);
            if let Some(c) = self.edits.get(&key) {
                return *c;
            }
        }
        if let Some(d) = &self.data {
            let gx = ((x - d.bounds[0]) / (d.bounds[1] - d.bounds[0]).max(1.)
                * (d.width - 1) as f32)
                .clamp(0., (d.width - 1) as f32);
            let gz = ((z - d.bounds[2]) / (d.bounds[3] - d.bounds[2]).max(1.)
                * (d.height - 1) as f32)
                .clamp(0., (d.height - 1) as f32);
            let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
            let (x1, z1) = ((x0 + 1).min(d.width - 1), (z0 + 1).min(d.height - 1));
            let a = d.cells[z0 * d.width + x0];
            let b = d.cells[z0 * d.width + x1];
            let c = d.cells[z1 * d.width + x0];
            let e = d.cells[z1 * d.width + x1];
            let lerp = |a: f32, b: f32, c: f32, e: f32| {
                let top = a + (b - a) * (gx - x0 as f32);
                let bottom = c + (e - c) * (gx - x0 as f32);
                top + (bottom - top) * (gz - z0 as f32)
            };
            return Cell {
                elevation: lerp(a.elevation, b.elevation, c.elevation, e.elevation),
                surface: lerp(a.surface, b.surface, c.surface, e.surface),
                moisture: lerp(a.moisture, b.moisture, c.moisture, e.moisture),
                kind: d.cells[gz.round() as usize * d.width + gx.round() as usize].kind,
            };
        }
        if self.authored.len() == (self.cols * self.rows) as usize {
            let (x, z) = (
                x.clamp(0., self.cols as f32 - 1.),
                z.clamp(0., self.rows as f32 - 1.),
            );
            let (x0, z0) = (x.floor() as usize, z.floor() as usize);
            let (x1, z1) = (
                (x0 + 1).min(self.cols as usize - 1),
                (z0 + 1).min(self.rows as usize - 1),
            );
            let h = |x, z| self.authored[z * self.cols as usize + x];
            let top = h(x0, z0) + (h(x1, z0) - h(x0, z0)) * x.fract();
            let bottom = h(x0, z1) + (h(x1, z1) - h(x0, z1)) * x.fract();
            let elevation = top + (bottom - top) * z.fract();
            let kind = if elevation < self.water_level * 0.68 {
                0
            } else if elevation < self.water_level {
                1
            } else {
                classify(elevation).max(3)
            };
            return Cell {
                elevation,
                surface: if kind <= 2 {
                    self.water_level
                } else {
                    elevation
                },
                moisture: 0.62,
                kind,
            };
        }
        if self.id == "river-crossing" {
            let water = crate::simulation::water(x, z);
            return Cell {
                elevation: 0.,
                surface: 0.,
                moisture: 0.6,
                kind: if water { 2 } else { 4 },
            };
        }
        let s = self.seed as f32 * 0.0001;
        let h = (0.5
            + 0.21 * (x * 0.019 + s).sin() * (z * 0.023 - s).cos()
            + 0.12 * (x * 0.067 + z * 0.041).sin())
        .clamp(0., 1.);
        let kind = classify(h);
        Cell {
            elevation: h,
            surface: if kind <= 2 { 0.33 } else { h },
            moisture: 0.6,
            kind,
        }
    }
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let c = self.cell(x, z);
        c.surface
            * self
                .data
                .as_ref()
                .map(|d| d.scale)
                .unwrap_or(if !self.authored.is_empty() {
                    self.authored_scale
                } else if self.id == "river-crossing" {
                    0.
                } else {
                    12.
                })
    }
    pub fn apply_modifications(&mut self, modifications: &[serde_json::Value]) {
        for m in modifications {
            let (Some(x), Some(z)) = (m["x"].as_i64(), m["y"].as_i64()) else {
                continue;
            };
            if x < 0 || z < 0 || x >= self.cols as i64 || z >= self.rows as i64 {
                continue;
            }
            let mut c = self.cell(x as f32, z as f32);
            let p = &m["patch"];
            for (key, value) in [
                ("elevation", &mut c.elevation),
                ("surfaceElevation", &mut c.surface),
                ("moisture", &mut c.moisture),
            ] {
                if let Some(n) = p[key].as_f64().filter(|n| n.is_finite()) {
                    *value = n as f32;
                }
            }
            if let Some(kind) = p["terrainType"].as_str().and_then(|s| {
                [
                    "deep_water",
                    "shallow_water",
                    "river",
                    "lowland",
                    "plains",
                    "hill",
                    "mountain",
                ]
                .iter()
                .position(|k| *k == s)
            }) {
                c.kind = kind as u8;
            }
            self.edits.insert(format!("{x},{z}"), c);
        }
        self.revision += 1;
    }
    pub fn modifications(&self) -> Vec<serde_json::Value> {
        self.edits.iter().filter_map(|(key, c)| {
            let (x, z) = key.split_once(',')?;
            Some(serde_json::json!({"x":x.parse::<i32>().ok()?, "y":z.parse::<i32>().ok()?, "patch":{
                "elevation":c.elevation, "surfaceElevation":c.surface, "moisture":c.moisture,
                "terrainType":(["deep_water", "shallow_water", "river", "lowland", "plains", "hill", "mountain"].get(c.kind as usize)?)
            }}))
        }).collect()
    }
    pub fn brush(&mut self, x: f32, z: f32, radius: i32, delta: f32, water: bool) {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dz * dz > radius * radius {
                    continue;
                }
                let (x, z) = (x.round() as i32 + dx, z.round() as i32 + dz);
                if x < 0 || z < 0 || x >= self.cols || z >= self.rows {
                    continue;
                }
                let mut c = self.cell(x as f32, z as f32);
                c.elevation = if water {
                    self.water_level * 0.6
                } else {
                    (c.elevation + delta).clamp(0., 1.)
                };
                c.kind = if c.elevation < self.water_level * 0.68 {
                    0
                } else if c.elevation < self.water_level {
                    1
                } else {
                    classify(c.elevation).max(3)
                };
                c.surface = if c.kind <= 2 {
                    self.water_level
                } else {
                    c.elevation
                };
                self.edits.insert(format!("{x},{z}"), c);
            }
        }
        self.revision += 1;
    }
    pub fn crater(&mut self, x: f32, z: f32, radius: i32) {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let d = ((dx * dx + dz * dz) as f32).sqrt();
                let (cx, cz) = (x.round() as i32 + dx, z.round() as i32 + dz);
                if d > radius as f32 || cx < 0 || cz < 0 || cx >= self.cols || cz >= self.rows {
                    continue;
                }
                let mut c = self.cell(cx as f32, cz as f32);
                if c.water() {
                    continue;
                }
                c.elevation = (c.elevation - 0.007 * (1. - d / (radius as f32 + 0.5)))
                    .max((self.water_level + 0.005).min(c.elevation));
                c.surface = c.elevation;
                self.edits.insert(format!("{cx},{cz}"), c);
            }
        }
        self.revision += 1;
    }
}
fn default_water_level() -> f32 {
    0.33
}
fn default_authored_scale() -> f32 {
    160.
}
pub fn classify(h: f32) -> u8 {
    if h < 0.2 {
        0
    } else if h < 0.33 {
        1
    } else if h < 0.42 {
        3
    } else if h < 0.65 {
        4
    } else if h < 0.82 {
        5
    } else {
        6
    }
}
/// Writes the four fields used by the native terrain pipeline in ROWTERR1 format.
pub fn encode(data: &Data, seed: &str) -> Result<Vec<u8>, String> {
    let count = data
        .width
        .checked_mul(data.height)
        .filter(|count| *count > 0 && *count <= 16_000_000)
        .ok_or("Invalid terrain dimensions")?;
    if data.cells.len() != count
        || !data.scale.is_finite()
        || data.scale <= 0.
        || data.bounds.iter().any(|n| !n.is_finite())
        || data.bounds[0] >= data.bounds[1]
        || data.bounds[2] >= data.bounds[3]
        || data.cells.iter().any(|c| {
            !c.elevation.is_finite()
                || !c.surface.is_finite()
                || !c.moisture.is_finite()
                || c.kind > 6
        })
    {
        return Err("Invalid terrain data".into());
    }
    let header = serde_json::to_vec(&serde_json::json!({
        "version": 1, "seed": seed, "width": data.width, "height": data.height,
        "bounds": {"minX": data.bounds[0], "maxX": data.bounds[1],
                   "minY": data.bounds[2], "maxY": data.bounds[3]},
        "config": {"maxElevation": data.scale},
        "sections": [
            {"name": "elevation", "byteLength": count * 4},
            {"name": "surfaceElevation", "byteLength": count * 4},
            {"name": "moisture", "byteLength": count * 4},
            {"name": "terrainTypeCodes", "byteLength": count},
        ]
    }))
    .map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(12 + header.len() + count * 13);
    bytes.extend_from_slice(b"ROWTERR1");
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&header);
    for field in [
        |cell: &Cell| cell.elevation,
        |cell: &Cell| cell.surface,
        |cell: &Cell| cell.moisture,
    ] {
        for cell in &data.cells {
            bytes.extend_from_slice(&field(cell).to_le_bytes());
        }
    }
    bytes.extend(data.cells.iter().map(|cell| cell.kind));
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Data, String> {
    if bytes.len() < 12 || &bytes[..8] != b"ROWTERR1" {
        return Err("Unsupported terrain binary".into());
    }
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let head = bytes.get(12..12 + n).ok_or("Truncated terrain header")?;
    let h: Value = serde_json::from_slice(head).map_err(|e| e.to_string())?;
    let width = h["width"].as_u64().ok_or("Missing width")? as usize;
    let height = h["height"].as_u64().ok_or("Missing height")? as usize;
    let count = width
        .checked_mul(height)
        .filter(|n| *n <= 16_000_000)
        .ok_or("Invalid terrain dimensions")?;
    let mut sections = BTreeMap::new();
    let mut offset = 12 + n;
    for s in h["sections"].as_array().ok_or("Missing sections")? {
        let len = s["byteLength"].as_u64().ok_or("Invalid section length")? as usize;
        let end = offset.checked_add(len).ok_or("Section overflow")?;
        sections.insert(
            s["name"].as_str().ok_or("Missing section name")?,
            bytes.get(offset..end).ok_or("Truncated section")?,
        );
        offset = end;
    }
    let floats = |name: &str| -> Result<Vec<f32>, String> {
        let b = sections.get(name).ok_or(format!("Missing {name}"))?;
        if b.len() != count * 4 {
            return Err(format!("Invalid {name} length"));
        }
        Ok(b.chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect())
    };
    let elevation = floats("elevation")?;
    let surface = floats("surfaceElevation")?;
    let moisture = floats("moisture")?;
    let kinds = sections
        .get("terrainTypeCodes")
        .ok_or("Missing terrain types")?;
    if kinds.len() != count {
        return Err("Invalid terrain types".into());
    }
    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        if !elevation[i].is_finite()
            || !surface[i].is_finite()
            || !moisture[i].is_finite()
            || kinds[i] > 6
        {
            return Err("Invalid terrain sample".into());
        }
        cells.push(Cell {
            elevation: elevation[i],
            surface: surface[i],
            moisture: moisture[i],
            kind: kinds[i],
        });
    }
    Ok(Data {
        width,
        height,
        bounds: [
            h["bounds"]["minX"].as_f64().unwrap_or(0.) as f32,
            h["bounds"]["maxX"].as_f64().unwrap_or(width as f64) as f32,
            h["bounds"]["minY"].as_f64().unwrap_or(0.) as f32,
            h["bounds"]["maxY"].as_f64().unwrap_or(height as f64) as f32,
        ],
        scale: h["config"]["maxElevation"].as_f64().unwrap_or(12.) as f32,
        cells,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn edit_cell(elevation: f32) -> Cell {
        Cell {
            elevation,
            surface: elevation,
            moisture: 0.5,
            kind: 4,
        }
    }
    #[test]
    fn sorted_edit_delta_handles_overlay_insertions_changes_and_removals() {
        let base = BTreeMap::from([
            ("100,10".into(), edit_cell(0.2)),
            ("11,10".into(), edit_cell(0.3)),
            ("9,10".into(), edit_cell(0.4)),
        ]);
        let pending = BTreeMap::from([
            ("0,0".into(), Some(edit_cell(0.1))),
            ("100,10".into(), None),
            ("11,10".into(), Some(edit_cell(0.6))),
            ("8,10".into(), None),
        ]);
        let after = BTreeMap::from([
            ("100,10".into(), edit_cell(0.2)),
            ("11,10".into(), edit_cell(0.6)),
            ("20,10".into(), edit_cell(0.7)),
        ]);
        assert_eq!(
            edit_delta(&base, &pending, &after),
            vec![
                ("0,0".into(), None),
                ("100,10".into(), Some(edit_cell(0.2))),
                ("20,10".into(), Some(edit_cell(0.7))),
                ("9,10".into(), None),
            ]
        );
    }
    #[test]
    fn rapid_craters_coalesce_until_worker_releases_snapshot_without_copying_foundation() {
        let mut original = Terrain::new("snapshot-edits", 256, 256);
        original.authored = vec![0.5; 256 * 256];
        original.edits = (0..10_000)
            .map(|n| (format!("{},{}", n % 100, n / 100), edit_cell(0.5)))
            .collect();
        let mut live = original.clone();
        let mut snapshot = Arc::new(original);
        let snapshot_ptr = Arc::as_ptr(&snapshot);
        let authored_ptr = snapshot.authored.as_ptr();
        let unchanged_cell_ptr = snapshot.edits.get("99,99").unwrap() as *const Cell;
        let worker = snapshot.clone();
        let mut pending = BTreeMap::new();
        for revision in 1..=3 {
            live.revision = revision;
            match revision {
                1 => {
                    live.edits.insert("10,10".into(), edit_cell(0.2));
                    live.edits.insert("200,200".into(), edit_cell(0.3));
                }
                2 => {
                    live.edits.remove("10,10");
                    live.edits.insert("200,200".into(), edit_cell(0.1));
                }
                _ => {
                    live.edits.insert("10,10".into(), edit_cell(0.5));
                    live.edits.remove("200,200");
                    live.edits.insert("20,20".into(), edit_cell(0.4));
                }
            }
            let changes = edit_delta(&snapshot.edits, &pending, &live.edits);
            assert!(!changes.is_empty());
            pending.extend(changes);
            assert!(edit_delta(&snapshot.edits, &pending, &live.edits).is_empty());
            assert!(!patch_snapshot(&mut snapshot, &mut pending, live.revision));
            assert_eq!(worker.revision, 0);
            assert_eq!(worker.edits["10,10"], edit_cell(0.5));
            assert!(!worker.edits.contains_key("200,200"));
            assert_eq!(Arc::as_ptr(&snapshot), snapshot_ptr);
            assert_eq!(snapshot.authored.as_ptr(), authored_ptr);
        }
        assert_eq!(pending.len(), 3);
        drop(worker);
        assert!(patch_snapshot(&mut snapshot, &mut pending, live.revision));
        assert!(pending.is_empty());
        assert_eq!(snapshot.revision, 3);
        assert_eq!(snapshot.edits, live.edits);
        assert_eq!(Arc::as_ptr(&snapshot), snapshot_ptr);
        assert_eq!(snapshot.authored.as_ptr(), authored_ptr);
        assert_eq!(
            snapshot.edits.get("99,99").unwrap() as *const Cell,
            unchanged_cell_ptr,
            "unmodified foundation B-tree entries must stay in the original map"
        );
    }
    #[test]
    fn render_source_identity_preserves_local_edits_and_detects_same_id_replacement() {
        let mut terrain = Terrain::new("same-id", 64, 64);
        terrain.authored = vec![0.5; 64 * 64];
        let initial = render_source_key(&terrain);
        terrain.revision += 1;
        terrain.edits.insert("10,10".into(), edit_cell(0.2));
        assert_eq!(initial, render_source_key(&terrain));
        let mut replacement = terrain.clone();
        replacement.authored[0] = 0.6;
        assert_eq!(terrain.revision, replacement.revision);
        assert_eq!(terrain.id, replacement.id);
        assert_ne!(initial, render_source_key(&replacement));
    }
    #[test]
    fn native_export_preserves_every_sample() {
        let original = crate::generation::generate(25, 19, "native-export-🌍");
        let decoded = decode(&encode(&original, "native-export-🌍").unwrap()).unwrap();
        assert_eq!((decoded.width, decoded.height), (25, 19));
        assert_eq!(decoded.bounds, original.bounds);
        assert_eq!(decoded.scale, original.scale);
        assert_eq!(decoded.cells, original.cells);
    }
    #[test]
    fn native_export_rejects_invalid_fields() {
        let mut data = crate::generation::generate(10, 10, "invalid-export");
        data.cells[0].moisture = f32::NAN;
        assert!(encode(&data, "invalid-export").is_err());
        data.cells[0].moisture = 0.5;
        data.cells[0].kind = 7;
        assert!(encode(&data, "invalid-export").is_err());
        data.cells[0].kind = 3;
        data.cells.pop();
        assert!(encode(&data, "invalid-export").is_err());
    }
    #[test]
    fn samples_match_web_hydration() {
        let samples: Value =
            serde_json::from_str(include_str!("../tests/web-terrain-samples.json")).unwrap();
        for v in samples.as_array().unwrap() {
            let size = v["size"].as_i64().unwrap() as i32;
            let t = Terrain::new(v["id"].as_str().unwrap(), size, size);
            let c = t.cell(
                v["x"].as_f64().unwrap() as f32,
                v["y"].as_f64().unwrap() as f32,
            );
            assert!((c.elevation - v["elevation"].as_f64().unwrap() as f32).abs() < 0.00002);
            assert!((c.surface - v["surface"].as_f64().unwrap() as f32).abs() < 0.00002);
            assert_eq!(
                [
                    "deep_water",
                    "shallow_water",
                    "river",
                    "lowland",
                    "plains",
                    "hill",
                    "mountain"
                ][c.kind as usize],
                v["kind"].as_str().unwrap()
            );
        }
    }
    #[test]
    fn loads_original_assets() {
        for (id, size) in [
            ("grand-front", 128),
            ("world-war", 512),
            ("galaxy-front", 1024),
        ] {
            let t = Terrain::new(id, size, size);
            let d = t.data.unwrap();
            assert_eq!(d.cells.len(), d.width * d.height);
            assert!(d.cells.iter().any(|c| c.water()));
        }
    }
    #[test]
    fn rejects_truncated_data() {
        assert!(decode(b"ROWTERR1").is_err());
        let b = include_bytes!("../../assets/terrain/grand-front-v3.terrain");
        assert!(decode(&b[..b.len() - 1]).is_err());
    }
    #[test]
    fn editing_survives_serialization() {
        let mut t = Terrain::new("custom", 2000, 2000);
        t.brush(1800., 1800., 3, 0.1, true);
        let b: Terrain = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert!(b.cell(1800., 1800.).water());
        assert_eq!(b.cols, 2000);
    }
}
