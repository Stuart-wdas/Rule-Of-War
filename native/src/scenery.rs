//! World-anchored vegetation, retained independently of terrain detail meshes.
use crate::{
    Controls, RtsCamera,
    landscape::{Environment, Nature},
    runtime::Battle,
    simulation::NativeGame,
    terrain::Terrain,
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const CELL: i32 = 6;
pub(crate) const CHUNK: i32 = 96;
const MAX_PART_VERTICES: usize = 8192;
const UPLOAD_BUDGET: usize = 1024 * 1024;
const CACHE_BUDGET: usize = 64 * 1024 * 1024;
const RETIRE_BUDGET: usize = 4 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Tier {
    Coarse,
    Medium,
    Fine,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VariantKey {
    coord: IVec2,
    tier: Tier,
    quality: u8,
    version: u64,
    epoch: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub cell: IVec2,
    pub position: Vec3,
    pub scale: f32,
    pub tree: bool,
    /// 0: tree/stone, 1: low shrub, 2: groundcover, 3: persistent planted hedge.
    pub understory: u8,
    broadleaf: bool,
}
#[derive(Component)]
pub struct Scenery {
    pub coord: IVec2,
    pub placements: Vec<Placement>,
    key: VariantKey,
}
#[derive(Resource, Default)]
pub struct Stats {
    worker_requests: u64,
    completed: u64,
    discarded: u64,
    uploaded_bytes: u64,
    peak_upload_bytes: usize,
    cache_hits: u64,
    active_variants: usize,
    active_bytes: usize,
    resident_bytes: usize,
    inactive_bytes: usize,
    retiring_bytes: usize,
    queued_bytes: usize,
    pending_variants: usize,
}
impl Stats {
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"workerRequests":self.worker_requests,"completedVariants":self.completed,
            "discardedVariants":self.discarded,"uploadedBytes":self.uploaded_bytes,
            "peakUploadBytesPerFrame":self.peak_upload_bytes,"uploadBudgetBytes":UPLOAD_BUDGET,
            "cacheHits":self.cache_hits,"activeVariants":self.active_variants,
            "activeMeshBytes":self.active_bytes,"residentMeshBytes":self.resident_bytes,
            "inactiveCacheBytes":self.inactive_bytes,"inactiveCacheBudgetBytes":CACHE_BUDGET,
            "retiringBytes":self.retiring_bytes,"queuedUploadBytes":self.queued_bytes,
            "pendingVariants":self.pending_variants,"maxPartVertices":MAX_PART_VERTICES})
    }
}
#[derive(Component)]
pub struct GrowthShadow(Vec2);
/// Grass is thin enough that distant sun shadows alias and waste cascade work.
/// Shadow quality owns this range independently of the visible vegetation LOD.
/// Nearby turf retains sun shadows; SSAO handles contact at visible distances.
pub fn growth_shadows(
    mut commands: Commands,
    controls: Res<Controls>,
    graphics: Res<crate::graphics::Graphics>,
    parts: Query<(Entity, &GrowthShadow, Option<&bevy::light::NotShadowCaster>)>,
) {
    let radius = graphics.settings.shadows.grass_shadow_radius();
    let at = Vec2::new(controls.center.x, controls.center.z);
    for (entity, shadow, disabled) in &parts {
        let cast = radius > 0. && shadow.0.distance_squared(at) < radius * radius;
        if cast && disabled.is_some() {
            commands
                .entity(entity)
                .remove::<bevy::light::NotShadowCaster>();
        } else if !cast && disabled.is_none() {
            commands.entity(entity).insert(bevy::light::NotShadowCaster);
        }
    }
}
fn hash(x: u32, z: u32, seed: u32) -> u32 {
    let mut n = x.wrapping_mul(374761393) ^ z.wrapping_mul(668265263) ^ seed;
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^ (n >> 16)
}
fn candidate(t: &Terrain, x: i32, z: i32) -> Option<Placement> {
    let n = hash(x as u32, z as u32, t.seed);
    let px = (x * CELL) as f32 + (hash(n, 7, 91) as f64 / u32::MAX as f64) as f32 * CELL as f32;
    let pz = (z * CELL) as f32 + (hash(n, 23, 17) as f64 / u32::MAX as f64) as f32 * CELL as f32;
    if px < 0. || pz < 0. || px >= t.cols as f32 || pz >= t.rows as f32 {
        return None;
    }
    if t.id == crate::battlefield::ID {
        // Cultivation, hedgerows and woodland now cover the whole operation.
        // Keep random stones/scrub out of the authored fields and access lanes.
        return None;
    }
    let training_range = t.id == crate::training::ID && (64. ..330.).contains(&px) && pz < 278.;
    if training_range
        && (((76. ..112.).contains(&px) && (46. ..266.).contains(&pz))
            || ((108. ..316.).contains(&px)
                && [72., 120., 168., 216.].iter().any(|z| (pz - z).abs() < 5.)))
    {
        return None;
    }
    let c = t.cell(px, pz);
    if c.water() {
        return None;
    }
    let gis = t.id == crate::gis_prototype::ID;
    let gis_mask = if gis {
        crate::gis_prototype::mask(px, pz)
    } else {
        [0; 3]
    };
    if gis && (gis_mask[1] > 0 || gis_mask[2] > 0) {
        return None;
    }
    let mut near_road = false;
    let base_density = if gis {
        if gis_mask[0] == 4 { 95. } else { 0. }
    } else if t.id == crate::battlefield::ID {
        let (mx, mz) = (px * 384. / t.cols as f32, pz * 384. / t.rows as f32);
        let distance = crate::battlefield::road_distance(mx, mz);
        if distance < 4.6 {
            return None;
        }
        near_road = distance < 7.;
        56. - crate::battlefield::field_weight(mx, mz) * 49.
    } else {
        42.
    };
    let clusters = crate::natural_noise::value(px / 28., pz / 28., t.seed);
    let grove = ((clusters - 0.24) / 0.52).clamp(0., 1.);
    let density = base_density * (0.12 + grove * grove * 1.6);
    let outcrop = crate::natural_noise::value(px / 24., pz / 24., t.seed ^ 371);
    let stone = !gis
        && !training_range
        && !near_road
        && hash(n, 311, 71) % 100 < if outcrop > 0.70 { 24 } else { 1 };
    let tree = !stone
        && !training_range
        && !near_road
        && c.moisture > 0.47
        && ((n % 100) as f32) < density
        && c.elevation < 0.75;
    let roll = (hash(n, 619, 913) % 100) as f32;
    if gis && gis_mask[0] != 4 && !tree && roll > 3. {
        return None;
    }
    let understory = if tree || stone {
        0
    } else if roll < 8. + grove * 22. {
        1
    } else if roll < 26. + grove * 35. {
        2
    } else {
        return None;
    };
    Some(Placement {
        cell: IVec2::new(x, z),
        position: Vec3::new(px, crate::landscape::visual_height(t, px, pz), pz),
        scale: 0.8 + (n % 50) as f32 * 0.012,
        tree,
        understory,
        broadleaf: hash(x as u32, z as u32, 93).is_multiple_of(3),
    })
}
fn placements(t: &Terrain, coord: IVec2) -> Vec<Placement> {
    let mut result = vec![];
    for z in coord.y * CHUNK / CELL..(coord.y + 1) * CHUNK / CELL {
        for x in coord.x * CHUNK / CELL..(coord.x + 1) * CHUNK / CELL {
            let Some(p) = candidate(t, x, z) else {
                continue;
            };
            let priority = (hash(x as u32, z as u32, t.seed), z, x);
            let radius = |p: &Placement| {
                if p.tree {
                    1.63 * p.scale
                } else if p.understory == 0 {
                    1.8 * p.scale
                } else {
                    0.91 * p.scale
                }
            };
            // Neighbours compete by a stable world priority, including across chunk edges.
            let crowded = (-1..=1).any(|dz| {
                (-1..=1).any(|dx| {
                    if dx == 0 && dz == 0 {
                        return false;
                    }
                    let other_priority = (
                        hash((x + dx) as u32, (z + dz) as u32, t.seed),
                        z + dz,
                        x + dx,
                    );
                    if other_priority >= priority {
                        return false;
                    }
                    candidate(t, x + dx, z + dz).is_some_and(|other| {
                        Vec2::new(
                            p.position.x - other.position.x,
                            p.position.z - other.position.z,
                        )
                        .length()
                            < radius(&p) + radius(&other) + 0.15
                    })
                })
            });
            if !crowded {
                result.push(p);
            }
        }
    }
    if t.id == crate::gis_prototype::ID {
        for (i, plant) in crate::gis_prototype::hedge_plants().iter().enumerate() {
            let [px, pz] = plant.at;
            if (px / CHUNK as f32).floor() as i32 != coord.x
                || (pz / CHUNK as f32).floor() as i32 != coord.y
                || t.cell(px, pz).water()
            {
                continue;
            }
            result.push(Placement {
                cell: IVec2::new(i as i32, -2),
                position: Vec3::new(px, crate::landscape::visual_height(t, px, pz), pz),
                scale: plant.scale,
                tree: false,
                understory: 3,
                broadleaf: true,
            });
        }
    }
    if t.id == crate::battlefield::ID {
        static PLANTS: std::sync::OnceLock<Vec<crate::rural_layout::Plant>> =
            std::sync::OnceLock::new();
        static WIDE_PLANTS: std::sync::OnceLock<Vec<crate::rural_layout::Plant>> =
            std::sync::OnceLock::new();
        let plants = if crate::battlefield::expanded(t.cols, t.rows) {
            WIDE_PLANTS.get_or_init(|| {
                crate::rural_layout::plants_for(crate::battlefield::COLS, crate::battlefield::ROWS)
            })
        } else {
            PLANTS.get_or_init(crate::rural_layout::plants)
        };
        for (i, plant) in plants.iter().enumerate() {
            let px = t.cols as f32 * 0.5 + plant.at[0];
            let pz = t.rows as f32 * 0.5 + plant.at[1];
            if px < 0.
                || pz < 0.
                || px >= t.cols as f32
                || pz >= t.rows as f32
                || (px / CHUNK as f32).floor() as i32 != coord.x
                || (pz / CHUNK as f32).floor() as i32 != coord.y
                || t.cell(px, pz).water()
            {
                continue;
            }
            result.push(Placement {
                cell: IVec2::new(i as i32, -1),
                position: Vec3::new(px, crate::landscape::visual_height(t, px, pz), pz),
                scale: plant.scale,
                tree: plant.tree,
                understory: if plant.tree { 0 } else { 3 },
                broadleaf: true,
            });
        }
    }
    result
}
#[derive(Clone, PartialEq)]
pub(crate) enum Clearing {
    Circle(Vec2, f32),
    Lane(Vec2),
    Rectangle(Vec2, Vec2, f32),
}
impl Clearing {
    pub(crate) fn contains(&self, point: Vec2) -> bool {
        match self {
            Self::Circle(at, radius) => at.distance_squared(point) < radius * radius,
            Self::Rectangle(at, half, yaw) => {
                let p = point - *at;
                let (s, c) = yaw.sin_cos();
                let p = Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y);
                p.x.abs() < half.x && p.y.abs() < half.y
            }
            Self::Lane(at) => {
                let p = point - *at;
                p.x.abs() < 23. && (p.y - (p.x * 0.12).sin() * 3.5).abs() < 3.5
            }
        }
    }
}
fn clearings(battle: &Battle) -> Vec<Clearing> {
    scene_clearings(
        battle.snapshot.entities.iter(),
        &battle.snapshot.bases,
        &battle.snapshot.props,
    )
}
fn scene_clearings<'a>(
    pieces: impl Iterator<Item = &'a crate::runtime::Piece>,
    bases: &[crate::simulation::BasePlan],
    props: &[crate::simulation::BattlefieldProp],
) -> Vec<Clearing> {
    let mut result = vec![];
    for p in pieces.filter(|p| p.building) {
        let at = Vec2::new(p.x, p.z);
        if p.lots.is_empty() {
            let spec = crate::simulation::catalog()["buildings"][&p.kind].clone();
            let center = if p.kind == "airfield" {
                Vec2::from_array(crate::airfield_layout::FOUNDATION_CENTER)
            } else {
                Vec2::ZERO
            };
            let (s, c) = p.heading.sin_cos();
            result.push(Clearing::Rectangle(
                at + Vec2::new(c * center.x + s * center.y, -s * center.x + c * center.y)
                    * p.model_scale,
                Vec2::new(
                    spec["footprintWidth"].as_f64().unwrap_or(8.) as f32,
                    spec["footprintDepth"].as_f64().unwrap_or(8.) as f32,
                ) * p.model_scale
                    * 0.5
                    + Vec2::splat(2.),
                p.heading,
            ));
        } else {
            if p.label == "PRACTICE HOUSES" {
                result.push(Clearing::Lane(at));
            } else {
                for plaza in p.town_plan.plazas() {
                    result.push(Clearing::Rectangle(
                        at + Vec2::from_array(plaza),
                        Vec2::splat(8.),
                        0.,
                    ));
                }
                for road in crate::settlement_layout::roads_for(&p.kind, p.town_plan) {
                    let a = Vec2::from_array(road.a);
                    let b = Vec2::from_array(road.b);
                    let d = b - a;
                    result.push(Clearing::Rectangle(
                        at + (a + b) * 0.5,
                        Vec2::new(d.length() * 0.5 + 1.5, road.width * 0.5 + 1.5),
                        -d.y.atan2(d.x),
                    ));
                }
            }
            for lot in &p.lots {
                // Clear real wings, rather than a large disc that stripped all
                // garden growth from the space between neighbouring buildings.
                for wing in lot.wings() {
                    let (s, c) = lot.yaw.sin_cos();
                    let center = Vec2::new(
                        c * wing.at[0] + s * wing.at[1],
                        -s * wing.at[0] + c * wing.at[1],
                    );
                    result.push(Clearing::Rectangle(
                        at + Vec2::from_array(lot.at) + center,
                        Vec2::from_array(wing.size) * 0.5 + Vec2::splat(1.25),
                        lot.yaw,
                    ));
                }
                let door = Vec2::from_array(lot.door().0);
                result.push(Clearing::Circle(at + door, 2.2));
            }
        }
    }
    for base in bases {
        let (sn, c) = base.yaw.sin_cos();
        let local = Vec2::new(57., 0.);
        result.push(Clearing::Rectangle(
            Vec2::from(base.origin)
                + Vec2::new(c * local.x + sn * local.y, -sn * local.x + c * local.y) * base.scale,
            Vec2::new(77., 181.) * base.scale,
            base.yaw,
        ));
    }
    result.extend(props.iter().filter(|p| p.grounded()).map(|p| {
        if let Some(w) = &p.wreck {
            Clearing::Rectangle(
                Vec2::from(p.at),
                Vec2::from(w.half_extents) + Vec2::splat(2.),
                p.heading,
            )
        } else {
            Clearing::Circle(Vec2::from(p.at), 5.5)
        }
    }));
    result
}
/// The simulation uses the same clearings as the renderer, at every detail setting.
pub(crate) fn spotting_clearings(game: &NativeGame) -> Vec<Clearing> {
    scene_clearings(
        game.entities.iter().map(|e| &e.view),
        &game.bases,
        &game.props,
    )
}
/// Stable seeded placements, independent of the camera and vegetation LOD.
pub(crate) fn spotting_placements(t: &Terrain) -> Vec<Placement> {
    let mut result = vec![];
    for z in 0..(t.rows + CHUNK - 1) / CHUNK {
        for x in 0..(t.cols + CHUNK - 1) / CHUNK {
            result.extend(
                placements(t, IVec2::new(x, z))
                    .into_iter()
                    .filter(|p| p.tree || p.understory == 1 || p.understory == 3),
            );
        }
    }
    result
}
fn visible_placements(t: &Terrain, coord: IVec2, clearings: &[Clearing]) -> Vec<Placement> {
    placements(t, coord)
        .into_iter()
        .filter(|p| {
            let at = Vec2::new(p.position.x, p.position.z);
            !clearings.iter().any(|c| c.contains(at))
        })
        .collect()
}
/// Include all projected screen corners, plus a full chunk of prefetch on every side.
pub(crate) fn view_bounds(
    controls: &Controls,
    camera: Option<(&Camera, &GlobalTransform)>,
) -> [f32; 4] {
    if let Some((camera, transform)) = camera
        && let Some(size) = camera.logical_viewport_size()
    {
        let mut bounds = [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut valid = true;
        for corner in [
            Vec2::ZERO,
            Vec2::new(size.x, 0.),
            size,
            Vec2::new(0., size.y),
        ] {
            if let Ok(ray) = camera.viewport_to_world(transform, corner)
                && ray.direction.y < -0.001
            {
                // The terrain base is below the ordinary battlefield's hills and water.
                let distance = -ray.origin.y / ray.direction.y;
                let p = ray.get_point(distance.max(0.));
                bounds[0] = bounds[0].min(p.x);
                bounds[1] = bounds[1].max(p.x);
                bounds[2] = bounds[2].min(p.z);
                bounds[3] = bounds[3].max(p.z);
            } else {
                valid = false;
            }
        }
        if valid {
            return bounds;
        }
    }
    // Covers initialization before the camera's computed viewport becomes available.
    let radius = controls.zoom * 2.2;
    [
        controls.center.x - radius,
        controls.center.x + radius,
        controls.center.z - radius,
        controls.center.z + radius,
    ]
}
pub(crate) fn chunk_bounds(t: &Terrain, view: [f32; 4], padding: f32) -> [i32; 4] {
    [
        ((view[0] - padding).max(0.) as i32 / CHUNK).min((t.cols - 1) / CHUNK),
        ((view[1] + padding).max(0.) as i32 / CHUNK).min((t.cols - 1) / CHUNK),
        ((view[2] - padding).max(0.) as i32 / CHUNK).min((t.rows - 1) / CHUNK),
        ((view[3] + padding).max(0.) as i32 / CHUNK).min((t.rows - 1) / CHUNK),
    ]
}
fn contains(bounds: [i32; 4], at: IVec2) -> bool {
    at.x >= bounds[0] && at.x <= bounds[1] && at.y >= bounds[2] && at.y <= bounds[3]
}
#[derive(Clone, Default)]
pub(crate) struct Batch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    uv: Vec<[f32; 2]>,
    growth_base: Vec<[f32; 2]>,
}
impl Batch {
    fn add_tinted(&mut self, mesh: &Mesh, transform: Transform, tint: Vec3) {
        let start = self.colors.len();
        self.add(mesh, transform);
        for color in &mut self.colors[start..] {
            color[0] *= tint.x;
            color[1] *= tint.y;
            color[2] *= tint.z;
        }
    }
    pub(crate) fn add(&mut self, mesh: &Mesh, transform: Transform) {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return;
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            return;
        };
        if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            self.uv.extend(uv);
        } else {
            self.uv
                .extend(positions.iter().map(|p| [p[0] * 0.5, p[2] * 0.5]));
        }
        let offset = self.positions.len() as u32;
        self.growth_base
            .extend(positions.iter().map(|_| [transform.translation.y, 0.]));
        let affine = transform.compute_affine();
        self.positions.extend(
            positions
                .iter()
                .map(|p| affine.transform_point3(Vec3::from_array(*p)).to_array()),
        );
        self.normals.extend(normals.iter().map(|n| {
            (transform.rotation * (Vec3::from_array(*n) / transform.scale))
                .normalize_or_zero()
                .to_array()
        }));
        if let Some(VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        {
            self.colors.extend(colors);
        } else {
            self.colors
                .extend(positions.iter().map(|_| [1., 1., 1., 1.]));
        }
        if let Some(indices) = mesh.indices() {
            self.indices
                .extend(indices.iter().map(|i| i as u32 + offset));
        } else {
            self.indices
                .extend((0..positions.len() as u32).map(|i| i + offset));
        }
    }
    pub(crate) fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.growth_base)
        .with_inserted_indices(Indices::U32(self.indices))
    }
    fn parts(self) -> Vec<Mesh> {
        if self.indices.is_empty() {
            return vec![];
        }
        if self.positions.len() <= MAX_PART_VERTICES {
            return vec![self.mesh()];
        }
        let mut result = vec![];
        let mut part = Batch::default();
        let mut remap = HashMap::<u32, u32>::new();
        for triangle in self.indices.chunks_exact(3) {
            let missing = triangle.iter().filter(|i| !remap.contains_key(i)).count();
            if part.positions.len() + missing > MAX_PART_VERTICES && !part.indices.is_empty() {
                result.push(std::mem::take(&mut part).mesh());
                remap.clear();
            }
            for &source in triangle {
                let target = *remap.entry(source).or_insert_with(|| {
                    let target = part.positions.len() as u32;
                    let i = source as usize;
                    part.positions.push(self.positions[i]);
                    part.normals.push(self.normals[i]);
                    part.colors.push(self.colors[i]);
                    part.uv.push(self.uv[i]);
                    part.growth_base.push(self.growth_base[i]);
                    target
                });
                part.indices.push(target);
            }
        }
        if !part.indices.is_empty() {
            result.push(part.mesh());
        }
        result
    }
}
/// Existing scatter slots cover a broken patch of ground rather than a ring or
/// a tight ornamental island. The same deterministic prefix serves all LODs.
fn understory_offset(seed: u32, index: u32) -> Vec3 {
    let n = hash(seed, index, 891);
    let u = (n & 1023) as f32 / 511.5 - 1.;
    let v = ((n >> 10) & 1023) as f32 / 511.5 - 1.;
    Vec3::new(
        u * 3.15 + (v * 2.8).sin() * 0.22,
        0.018,
        v * 3.0 + (u * 2.4).sin() * 0.24,
    )
}
fn clear_ground(terrain: &Terrain, clearings: &[Clearing], point: Vec3, radius: f32) -> bool {
    [
        Vec2::ZERO,
        Vec2::X * radius,
        -Vec2::X * radius,
        Vec2::Y * radius,
        -Vec2::Y * radius,
    ]
    .into_iter()
    .all(|offset| {
        let at = Vec2::new(point.x, point.z) + offset;
        let training_lane = terrain.id == crate::training::ID
            && (64. ..330.).contains(&at.x)
            && at.y < 278.
            && (((76. ..112.).contains(&at.x) && (46. ..266.).contains(&at.y))
                || ((108. ..316.).contains(&at.x)
                    && [72., 120., 168., 216.]
                        .iter()
                        .any(|z| (at.y - z).abs() < 5.)));
        at.x >= 0.
            && at.y >= 0.
            && at.x < terrain.cols as f32
            && at.y < terrain.rows as f32
            && !training_lane
            && !terrain.cell(at.x, at.y).water()
            && !clearings.iter().any(|c| c.contains(at))
    })
}
fn batches(
    props: &[Placement],
    shapes: &[Mesh; 7],
    tier: Tier,
    detail: crate::graphics::Detail,
    terrain: &Terrain,
    clearings: &[Clearing],
    cancel: &AtomicBool,
) -> [Batch; 5] {
    let overview = tier == Tier::Coarse;
    let scatter = if tier == Tier::Medium {
        detail.scatter_count().min(2)
    } else {
        detail.scatter_count()
    };
    let mut result: [Batch; 5] = std::array::from_fn(|_| Batch::default());
    for p in props {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let seed = hash(p.cell.x as u32, p.cell.y as u32, 731);
        // Broken patches retain independent orientations; a shared direction
        // across the placement grid would read as planted rows.
        let angle = (seed % 6283) as f32 * 0.001;
        let yaw = Quat::from_rotation_y(angle);
        let height = 0.82 + ((seed >> 12) % 100) as f32 * 0.004;
        let start = [result[1].colors.len(), result[2].colors.len()];
        let transform = |at: Vec3, scale: Vec3| {
            let stretch = Vec3::new(1., if p.tree { height } else { 1. }, 1.);
            let mut position = p.position + yaw * (at * stretch * p.scale);
            position.y +=
                crate::landscape::visual_height(terrain, position.x, position.z) - p.position.y;
            Transform::from_translation(position)
                .with_rotation(yaw)
                .with_scale(scale * stretch * p.scale)
        };
        if p.understory == 3 {
            // Keep the authored hedge silhouette even at overview distance,
            // using the same low shrub mesh and leaf material as ordinary scrub.
            result[1].add_tinted(
                &shapes[4],
                transform(Vec3::new(0., 0.65, 0.), Vec3::new(1.18, 0.66, 1.08)),
                Vec3::new(0.76, 0.88, 0.67),
            );
            continue;
        }
        if p.understory != 0 {
            if overview {
                continue;
            }
            // Redistribute the existing scatter count into connected scrub and
            // mixed-size stone patches. No placement/collision seeds change.
            for j in 0..scatter * 3 {
                let n = hash(seed, j as u32, 891);
                let at = understory_offset(seed, j as u32);
                let world = p.position + yaw * (at * p.scale);
                if !clear_ground(terrain, clearings, world, 0.55 * p.scale) {
                    continue;
                }
                let middle_stone = j % 5 == 0;
                let size = if middle_stone {
                    0.40 + ((n >> 20) % 100) as f32 * 0.0032
                } else {
                    0.085 + ((n >> 20) % 100) as f32 * 0.0025
                };
                let mut stone = transform(
                    at,
                    Vec3::new(size, size * 0.70, size * (0.72 + (n % 31) as f32 * 0.018)),
                );
                stone.rotation *= Quat::from_rotation_y((n % 6283) as f32 * 0.001);
                let mineral = 0.78 + ((n >> 13) % 100) as f32 * 0.0025;
                result[3].add_tinted(
                    &shapes[6],
                    stone,
                    Vec3::new(mineral, mineral * 0.99, mineral * 0.94),
                );
                let turf = 0.85 + ((n >> 7) % 100) as f32 * 0.0105;
                let dry = (n >> 18) % 5 < 2;
                let tint = if dry {
                    Vec3::new(1.16, 1.02, 1.08)
                } else {
                    Vec3::new(0.87, 0.94, 0.83)
                };
                let turf_at = if middle_stone {
                    at + Vec3::new(0.20, 0., -0.16)
                } else {
                    at
                };
                result[4].add_tinted(
                    &shapes[5],
                    transform(turf_at, Vec3::new(turf, 0.42 + turf * 0.27, turf * 0.85)),
                    tint,
                );
            }
            if p.understory == 1 {
                let spread = 0.65 + ((seed >> 7) % 100) as f32 * 0.014;
                for (at, scale, material) in [
                    (Vec3::new(-0.22, 0.33, 0.1), Vec3::new(0.55, 0.44, 0.55), 1),
                    (Vec3::new(0.22, 0.42, -0.13), Vec3::new(0.58, 0.53, 0.56), 2),
                ] {
                    result[material].add(
                        &shapes[4],
                        transform(
                            at * spread,
                            scale * Vec3::new(spread, 0.75 + spread * 0.35, spread),
                        ),
                    );
                }
                for j in 0..scatter / 2 {
                    let n = hash(seed, j as u32, 773);
                    let mut at = understory_offset(seed, (j + scatter * 3) as u32) * 0.82;
                    at.y = 0.13;
                    let world = p.position + yaw * (at * p.scale);
                    if !clear_ground(terrain, clearings, world, 0.5 * p.scale) {
                        continue;
                    }
                    let leaf = 0.86 + (n % 41) as f32 * 0.003;
                    result[1 + j % 2].add_tinted(
                        &shapes[4],
                        transform(at, Vec3::new(0.45, 0.24 + (n % 50) as f32 * 0.002, 0.60)),
                        Vec3::new(leaf, leaf * 0.99, leaf * 0.92),
                    );
                }
            } else {
                for j in 0..3 {
                    let a = j as f32 * 2.4 + seed as f32;
                    let at = Vec3::new(a.cos() * 0.28, 0.015, a.sin() * 0.28);
                    result[4].add(
                        &shapes[5],
                        transform(at, Vec3::new(0.72, 0.8 + (j as f32) * 0.13, 0.72)),
                    );
                }
            }
            continue;
        }
        if p.tree {
            if !overview {
                result[0].add(
                    &shapes[0],
                    transform(Vec3::new(0., 1.3, 0.), Vec3::new(1., 1.45, 1.)),
                );
                for side in [-1., 1.] {
                    let fork = 0.82 + ((seed >> 8) % 40) as f32 * 0.009;
                    let mut branch = transform(
                        Vec3::new(side * 0.29, 2.52 + side * 0.12, 0.09 + side * 0.12),
                        Vec3::new(0.39, 0.65 * fork, 0.39),
                    );
                    branch.rotation =
                        yaw * Quat::from_euler(EulerRot::YXZ, side * 0.23, 0., side * -0.51);
                    result[0].add(&shapes[0], branch);
                    result[4].add_tinted(
                        &shapes[5],
                        transform(
                            Vec3::new(side * 0.45, 0.015, -0.17 + side * 0.15),
                            Vec3::new(1.26, 0.68, 1.07),
                        ),
                        Vec3::new(0.77, 0.83, 0.73),
                    );
                }
            }
            if p.broadleaf {
                for (at, scale, material) in [
                    (Vec3::new(-0.46, 2.35, 0.07), 1.1, 1),
                    (Vec3::new(0.48, 2.75, 0.24), 1., 2),
                    (Vec3::new(-0.06, 3.4, -0.17), 0.9, 1),
                ] {
                    let crown = 1.05 + ((seed >> 13) % 41) as f32 * 0.006;
                    let offset = Vec3::new(
                        ((seed >> 5) % 31) as f32 * 0.007 - 0.1,
                        ((seed >> 17) % 31) as f32 * 0.009 - 0.13,
                        ((seed >> 23) % 31) as f32 * 0.006 - 0.09,
                    );
                    result[material].add(
                        &shapes[2],
                        transform(
                            at * Vec3::new(1., 1.13, 1.) + offset,
                            Vec3::new(scale, scale * crown, scale),
                        ),
                    );
                }
            } else {
                for (y, scale, material) in [(2., 1., 1), (2.9, 0.76, 2), (3.6, 0.48, 1)] {
                    if overview && y == 3.6 {
                        continue;
                    }
                    result[material].add(
                        &shapes[1],
                        transform(
                            Vec3::new(0., y * 1.45, 0.),
                            Vec3::new(scale, scale * 1.45, scale),
                        ),
                    );
                }
            }
            let warmth = ((seed >> 19) % 100) as f32 / 100.;
            let tint = Vec3::new(
                0.78 + warmth * 0.21,
                0.84 + warmth * 0.12,
                0.72 + warmth * 0.19,
            );
            for (j, begin) in start.into_iter().enumerate() {
                for c in &mut result[j + 1].colors[begin..] {
                    c[0] *= tint.x;
                    c[1] *= tint.y;
                    c[2] *= tint.z;
                }
            }
        } else {
            let begin = result[3].colors.len();
            let size = 0.85 + (seed % 100) as f32 * 0.006;
            let mut rock = transform(
                Vec3::new(-0.25, 0.35, 0.),
                Vec3::new(
                    1.33 + (seed % 13) as f32 * 0.01,
                    0.94 + ((seed >> 6) % 21) as f32 * 0.013,
                    1.12,
                ) * size,
            );
            rock.rotation *= Quat::from_euler(
                EulerRot::YXZ,
                0.,
                ((seed >> 9) % 21) as f32 * 0.008 - 0.08,
                ((seed >> 15) % 21) as f32 * 0.007 - 0.07,
            );
            result[3].add(&shapes[3], rock);
            if seed.is_multiple_of(3) {
                result[3].add(
                    &shapes[3],
                    transform(Vec3::new(0.89, 0.12, 0.35), Vec3::new(0.65, 0.45, 0.48)),
                );
                result[3].add(
                    &shapes[6],
                    transform(Vec3::new(-0.66, 0.075, -0.74), Vec3::new(0.48, 0.35, 0.56)),
                );
            }
            let warmth = ((seed >> 9) % 100) as f32 / 100.;
            for color in &mut result[3].colors[begin..] {
                color[0] *= 0.78 + warmth * 0.27;
                color[1] *= 0.79 + warmth * 0.18;
                color[2] *= 0.80 + warmth * 0.09;
            }
        }
    }
    result
}
struct PreparedPart {
    material: usize,
    mesh: Mesh,
    bytes: usize,
}
#[derive(Clone)]
struct ResidentPart {
    handle: Handle<Mesh>,
    bytes: usize,
}
struct PreparedScenery {
    key: VariantKey,
    placements: Arc<Vec<Placement>>,
    geometry: VecDeque<PreparedPart>,
}
struct UploadScenery {
    key: VariantKey,
    placements: Arc<Vec<Placement>>,
    geometry: VecDeque<PreparedPart>,
    root: Entity,
    loaded: Vec<ResidentPart>,
}
struct CachedScenery {
    root: Entity,
    placements: Arc<Vec<Placement>>,
    parts: Vec<ResidentPart>,
    last_used: u64,
}
impl CachedScenery {
    fn bytes(&self) -> usize {
        self.parts.iter().map(|p| p.bytes).sum()
    }
}
struct SceneryJob {
    receiver: std::sync::mpsc::Receiver<PreparedScenery>,
    keys: Vec<VariantKey>,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct StreamState {
    map: String,
    revision: u64,
    generation: u64,
    frame: u64,
    layout_signature: u64,
    clearings: Arc<Vec<Clearing>>,
    versions: HashMap<IVec2, u64>,
    cache: HashMap<VariantKey, CachedScenery>,
    pending: std::sync::Mutex<Option<SceneryJob>>,
    staged: VecDeque<UploadScenery>,
    retiring: VecDeque<ResidentPart>,
    terrain: Option<Arc<Terrain>>,
    pending_edits: BTreeMap<String, Option<crate::terrain::Cell>>,
    previous_zoom: f32,
    last_scroll: Option<Instant>,
    overview: bool,
}
fn mesh_bytes(mesh: &Mesh) -> usize {
    mesh.count_vertices() * 56 + mesh.indices().map_or(0, |i| i.len() * 4)
}
fn quality_code(detail: crate::graphics::Detail) -> u8 {
    match detail {
        crate::graphics::Detail::Low => 0,
        crate::graphics::Detail::Balanced => 1,
        crate::graphics::Detail::High => 2,
    }
}
fn compact_groundcover(mut mesh: Mesh) -> Mesh {
    // Six complete original blades, retaining their authored colour and shape.
    for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
        if let Some(VertexAttributeValues::Float32x3(v)) = mesh.attribute_mut(attribute) {
            v.truncate(36);
        }
    }
    if let Some(VertexAttributeValues::Float32x4(v)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        v.truncate(36);
    }
    if let Some(VertexAttributeValues::Float32x2(v)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        v.truncate(36);
    }
    let indices: Vec<u32> = mesh
        .indices()
        .into_iter()
        .flat_map(|i| i.iter())
        .take(72)
        .map(|i| i as u32)
        .collect();
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
fn layout_signature(battle: &Battle) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for p in battle.snapshot.entities.iter().filter(|p| p.building) {
        p.id.hash(&mut hash);
        p.kind.hash(&mut hash);
        (p.label == "PRACTICE HOUSES").hash(&mut hash);
        for value in [p.x, p.z, p.heading, p.model_scale] {
            value.to_bits().hash(&mut hash);
        }
        (p.town_plan as u8).hash(&mut hash);
        p.lots.len().hash(&mut hash);
        for lot in &p.lots {
            for value in [lot.at[0], lot.at[1], lot.width, lot.depth, lot.yaw] {
                value.to_bits().hash(&mut hash);
            }
            lot.style.hash(&mut hash);
        }
    }
    hash.finish()
}
fn clearing_bounds(c: &Clearing) -> [f32; 4] {
    let (at, half) = match c {
        Clearing::Circle(at, r) => (*at, Vec2::splat(*r)),
        Clearing::Lane(at) => (*at, Vec2::new(23., 7.)),
        Clearing::Rectangle(at, half, yaw) => {
            let (s, c) = yaw.sin_cos();
            (
                *at,
                Vec2::new(
                    c.abs() * half.x + s.abs() * half.y,
                    s.abs() * half.x + c.abs() * half.y,
                ),
            )
        }
    };
    [at.x - half.x, at.x + half.x, at.y - half.y, at.y + half.y]
}
fn add_affected(t: &Terrain, bounds: [f32; 4], affected: &mut HashSet<IVec2>) {
    let b = chunk_bounds(t, bounds, 8.);
    for z in b[2]..=b[3] {
        for x in b[0]..=b[1] {
            affected.insert(IVec2::new(x, z));
        }
    }
}
#[cfg(test)]
fn edited_chunks(before: &Terrain, after: &Terrain, affected: &mut HashSet<IVec2>) {
    if before.authored != after.authored
        || before.water_level != after.water_level
        || before.authored_scale != after.authored_scale
    {
        add_affected(
            after,
            [0., after.cols as f32, 0., after.rows as f32],
            affected,
        );
        return;
    }
    changed_chunks(
        &crate::terrain::edit_delta(&before.edits, &BTreeMap::new(), &after.edits),
        after,
        affected,
    );
}
fn changed_chunks(
    changes: &[crate::terrain::EditDelta],
    after: &Terrain,
    affected: &mut HashSet<IVec2>,
) {
    for (key, _) in changes {
        if let Some((x, z)) = key.split_once(',')
            && let (Ok(x), Ok(z)) = (x.parse::<f32>(), z.parse::<f32>())
        {
            add_affected(after, [x, x, z, z], affected);
        }
    }
}
fn tier_for(
    coord: IVec2,
    view: [f32; 4],
    controls: &Controls,
    overview: bool,
    detail: crate::graphics::Detail,
) -> Tier {
    let low = coord.as_vec2() * CHUNK as f32;
    let high = low + Vec2::splat(CHUNK as f32);
    let intersects = high.x >= view[0] && low.x <= view[1] && high.y >= view[2] && low.y <= view[3];
    if overview || !intersects {
        return Tier::Coarse;
    }
    let center = Vec2::new(controls.center.x, controls.center.z);
    let distance = (center - center.clamp(low, high)).length();
    // Carry full grass, stones and canopy detail farther across the visible
    // ground. Low retains its original range; mesh admission stays budgeted.
    let (zoom, scale, minimum, maximum) = match detail {
        crate::graphics::Detail::Low => (110., 0.7, 24., 64.),
        crate::graphics::Detail::Balanced => (140., 0.875, 30., 80.),
        crate::graphics::Detail::High => (165., 1.05, 36., 96.),
    };
    if controls.zoom < zoom && distance < (controls.zoom * scale).clamp(minimum, maximum) {
        Tier::Fine
    } else {
        Tier::Medium
    }
}
fn wanted_key(key: VariantKey, targets: &HashMap<IVec2, VariantKey>) -> bool {
    targets.get(&key.coord).is_some_and(|target| {
        key.epoch == target.epoch
            && key.version == target.version
            && key.quality == target.quality
            && key.tier <= target.tier
    })
}
fn can_activate(current: Option<VariantKey>, chosen: VariantKey, target: VariantKey) -> bool {
    // An incomplete Medium replacement must not flash the cached Coarse mesh
    // while its Fine predecessor is still a perfectly valid visible variant.
    chosen == target
        || current.is_none_or(|old| {
            old.epoch != chosen.epoch
                || old.version != chosen.version
                || old.quality != chosen.quality
                || old.tier <= chosen.tier
        })
}
fn hide(commands: &mut Commands, root: Entity) {
    commands
        .entity(root)
        .remove::<Scenery>()
        .remove::<Environment>()
        .insert(Visibility::Hidden);
}
fn upload_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    nature: &Nature,
    root: Entity,
    coord: IVec2,
    part: PreparedPart,
) -> ResidentPart {
    let mut mesh = part.mesh;
    // Static baked geometry has no main-world consumers after upload.
    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    let handle = meshes.add(mesh);
    commands.entity(root).with_children(|parent| {
        let mut child = parent.spawn((Mesh3d(handle.clone()), Transform::default()));
        match part.material {
            0 => {
                child.insert(MeshMaterial3d(nature.bark.clone()));
            }
            1 => {
                child.insert(MeshMaterial3d(nature.leaves.clone()));
            }
            2 => {
                child.insert(MeshMaterial3d(nature.leaves_light.clone()));
            }
            3 => {
                child.insert(MeshMaterial3d(nature.stone.clone()));
            }
            _ => {
                child.insert((
                    MeshMaterial3d(nature.low_growth.clone()),
                    bevy::light::NotShadowCaster,
                    GrowthShadow(coord.as_vec2() * CHUNK as f32 + Vec2::splat(CHUNK as f32 * 0.5)),
                ));
            }
        }
    });
    ResidentPart {
        handle,
        bytes: part.bytes,
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects streaming and rendering resources"
)]
pub fn stream(
    mut commands: Commands,
    game: Res<NativeGame>,
    battle: Res<Battle>,
    controls: Res<Controls>,
    graphics: Option<Res<crate::graphics::Graphics>>,
    nature: Res<Nature>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(Entity, &Scenery)>,
    camera: Option<Single<(&Camera, &GlobalTransform), With<RtsCamera>>>,
    front: Res<crate::frontend::Frontend>,
    mut state: Local<StreamState>,
    mut stats: Option<ResMut<Stats>>,
) {
    let _timing = crate::performance::span("scenery.stream");
    if front.world {
        return;
    }
    state.frame += 1;
    let frame = state.frame;
    let t = &game.terrain;
    let map = crate::terrain::render_source_key(t);
    let changed_map = state.map != map;
    let showcase = std::env::args().any(|a| a == "--models");
    if changed_map {
        state.generation += 1;
        state.versions.clear();
        if let Some(job) = state.pending.lock().unwrap().take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let old = std::mem::take(&mut state.cache);
        for (_, entry) in old {
            commands.entity(entry.root).despawn();
            state.retiring.extend(entry.parts);
        }
        let old = std::mem::take(&mut state.staged);
        for entry in old {
            commands.entity(entry.root).despawn();
            state.retiring.extend(entry.loaded);
        }
        state.terrain = Some(Arc::new(t.clone()));
        state.pending_edits.clear();
        state.map = map;
        state.revision = t.revision;
    }
    let signature = layout_signature(&battle);
    let next_clearings = if changed_map || state.layout_signature != signature {
        Some(Arc::new(clearings(&battle)))
    } else {
        None
    };
    if !changed_map && (state.revision != t.revision || next_clearings.is_some()) {
        let mut affected = HashSet::new();
        if state.revision != t.revision {
            let changes = crate::terrain::edit_delta(
                &state.terrain.as_ref().unwrap().edits,
                &state.pending_edits,
                &t.edits,
            );
            changed_chunks(&changes, t, &mut affected);
            state.pending_edits.extend(changes);
            state.revision = t.revision;
            if !state.pending_edits.is_empty()
                && let Some(job) = state.pending.lock().unwrap().take()
            {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
        if let Some(next) = &next_clearings {
            for c in state.clearings.iter().filter(|c| !next.contains(c)) {
                add_affected(t, clearing_bounds(c), &mut affected);
            }
            for c in next.iter().filter(|c| !state.clearings.contains(c)) {
                add_affected(t, clearing_bounds(c), &mut affected);
            }
        }
        for coord in affected {
            *state.versions.entry(coord).or_default() += 1;
        }
    }
    if let Some(next) = next_clearings {
        state.clearings = next;
        state.layout_signature = signature;
    }
    if let Some(mut snapshot) = state.terrain.take() {
        crate::terrain::patch_snapshot(&mut snapshot, &mut state.pending_edits, t.revision);
        state.terrain = Some(snapshot);
    }
    if (state.previous_zoom - controls.zoom).abs() > 0.01 {
        state.last_scroll = Some(Instant::now());
        state.previous_zoom = controls.zoom;
    }
    let refining = state
        .last_scroll
        .is_none_or(|last| last.elapsed() >= Duration::from_millis(150));
    let detail = graphics
        .as_ref()
        .map_or(crate::graphics::Detail::Balanced, |g| {
            g.settings.environment
        });
    let (near, far) = match detail {
        crate::graphics::Detail::Low => (220., 260.),
        crate::graphics::Detail::Balanced => (250., 290.),
        crate::graphics::Detail::High => (280., 320.),
    };
    state.overview = controls.zoom > if state.overview { near } else { far };
    let view = view_bounds(&controls, camera.as_ref().map(|c| **c));
    let wanted = chunk_bounds(t, view, CHUNK as f32);
    let retained = chunk_bounds(t, view, CHUNK as f32 * 2.);
    let targets: HashMap<_, _> = if showcase {
        HashMap::new()
    } else {
        (wanted[2]..=wanted[3])
            .flat_map(|z| (wanted[0]..=wanted[1]).map(move |x| IVec2::new(x, z)))
            .map(|coord| {
                (
                    coord,
                    VariantKey {
                        coord,
                        tier: tier_for(coord, view, &controls, state.overview, detail),
                        quality: quality_code(detail),
                        version: *state.versions.get(&coord).unwrap_or(&0),
                        epoch: state.generation,
                    },
                )
            })
            .collect()
    };
    let mut active = HashMap::new();
    if !changed_map {
        for (entity, chunk) in &chunks {
            if !targets.contains_key(&chunk.coord) {
                hide(&mut commands, entity);
            } else {
                active.insert(chunk.coord, (entity, chunk.key));
            }
        }
    }
    if let Some(job) = state.pending.lock().unwrap().as_ref()
        && job.keys.iter().all(|key| !wanted_key(*key, &targets))
    {
        job.cancel.store(true, Ordering::Relaxed);
    }
    let completed = {
        let mut pending = state.pending.lock().unwrap();
        let mut completed = vec![];
        if let Some(job) = pending.as_ref() {
            loop {
                match job.receiver.try_recv() {
                    Ok(p) => completed.push(p),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        *pending = None;
                        break;
                    }
                }
            }
        }
        completed
    };
    for prepared in completed {
        if let Some(stats) = stats.as_deref_mut() {
            stats.completed += 1;
        }
        if !wanted_key(prepared.key, &targets) || state.cache.contains_key(&prepared.key) {
            if let Some(stats) = stats.as_deref_mut() {
                stats.discarded += 1;
            }
            continue;
        }
        let root = commands
            .spawn((Transform::default(), Visibility::Hidden))
            .id();
        state.staged.push_back(UploadScenery {
            key: prepared.key,
            placements: prepared.placements,
            geometry: prepared.geometry,
            root,
            loaded: vec![],
        });
    }
    let mut index = 0;
    while index < state.staged.len() {
        if wanted_key(state.staged[index].key, &targets) {
            index += 1;
            continue;
        }
        let stale = state.staged.remove(index).unwrap();
        commands.entity(stale.root).despawn();
        state.retiring.extend(stale.loaded);
        if let Some(stats) = stats.as_deref_mut() {
            stats.discarded += 1;
        }
    }
    let center = Vec2::new(controls.center.x, controls.center.z);
    let distance = |coord: IVec2| {
        (coord.as_vec2() * CHUNK as f32 + Vec2::splat(CHUNK as f32 * 0.5) - center).length_squared()
    };
    let mut budget = UPLOAD_BUDGET;
    let mut fresh = HashSet::new();
    {
        let _upload = crate::performance::span("scenery.upload");
        while let Some(index) = state
            .staged
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                (s.key.tier == Tier::Coarse || refining)
                    && s.geometry.front().is_none_or(|p| p.bytes <= budget)
            })
            .min_by(|(_, a), (_, b)| {
                a.key
                    .tier
                    .cmp(&b.key.tier)
                    .then_with(|| distance(a.key.coord).total_cmp(&distance(b.key.coord)))
            })
            .map(|(index, _)| index)
        {
            let mut entry = state.staged.remove(index).unwrap();
            if let Some(part) = entry.geometry.pop_front() {
                budget -= part.bytes;
                entry.loaded.push(upload_part(
                    &mut commands,
                    &mut meshes,
                    &nature,
                    entry.root,
                    entry.key.coord,
                    part,
                ));
            }
            if entry.geometry.is_empty() {
                fresh.insert(entry.key);
                state.cache.insert(
                    entry.key,
                    CachedScenery {
                        root: entry.root,
                        placements: entry.placements,
                        parts: entry.loaded,
                        last_used: frame,
                    },
                );
            } else {
                state.staged.push_back(entry);
            }
        }
    }
    for &coord in targets.keys() {
        if let Some((_, key)) = active.get(&coord)
            && let Some(entry) = state.cache.get_mut(key)
        {
            entry.last_used = frame;
        }
        let chosen = state
            .cache
            .keys()
            .copied()
            .filter(|key| key.coord == coord && wanted_key(*key, &targets))
            .max_by_key(|key| key.tier);
        let Some(chosen) = chosen else {
            continue;
        };
        if !can_activate(
            active.get(&coord).map(|(_, key)| *key),
            chosen,
            targets[&coord],
        ) {
            continue;
        }
        if active.get(&coord).is_none_or(|(_, key)| *key != chosen) {
            if let Some((root, _)) = active.get(&coord) {
                hide(&mut commands, *root);
            }
            let entry = state.cache.get_mut(&chosen).unwrap();
            commands.entity(entry.root).insert((
                Environment,
                Visibility::Inherited,
                Scenery {
                    coord,
                    placements: entry.placements.as_ref().clone(),
                    key: chosen,
                },
            ));
            active.insert(coord, (entry.root, chosen));
            if !fresh.contains(&chosen)
                && let Some(stats) = stats.as_deref_mut()
            {
                stats.cache_hits += 1;
            }
        }
        if let Some((_, key)) = active.get(&coord)
            && let Some(entry) = state.cache.get_mut(key)
        {
            entry.last_used = frame;
        }
    }
    let active_keys: HashSet<_> = active.values().map(|(_, key)| *key).collect();
    let mut inactive_bytes: usize = state
        .cache
        .iter()
        .filter(|(key, _)| !active_keys.contains(*key))
        .map(|(_, v)| v.bytes())
        .sum();
    loop {
        let candidate = state
            .cache
            .iter()
            .filter(|(key, _)| !active_keys.contains(*key))
            .filter(|(key, _)| {
                inactive_bytes > CACHE_BUDGET
                    || key.version != *state.versions.get(&key.coord).unwrap_or(&0)
            })
            .min_by_key(|(key, v)| (contains(retained, key.coord), v.last_used))
            .map(|(key, _)| *key);
        let Some(key) = candidate else {
            break;
        };
        let entry = state.cache.remove(&key).unwrap();
        inactive_bytes = inactive_bytes.saturating_sub(entry.bytes());
        commands.entity(entry.root).despawn();
        state.retiring.extend(entry.parts);
    }
    let mut retirement = RETIRE_BUDGET;
    while state
        .retiring
        .front()
        .is_some_and(|p| p.bytes <= retirement)
    {
        let part = state.retiring.pop_front().unwrap();
        retirement -= part.bytes;
        meshes.remove(part.handle.id());
    }
    let queued_bytes: usize = state
        .staged
        .iter()
        .flat_map(|s| &s.geometry)
        .map(|p| p.bytes)
        .sum();
    if state.pending_edits.is_empty()
        && state.pending.lock().unwrap().is_none()
        && queued_bytes < 16 * 1024 * 1024
    {
        let staged: HashSet<_> = state.staged.iter().map(|s| s.key).collect();
        let mut requests: Vec<_> = targets
            .values()
            .filter_map(|target| {
                let target = if !active.contains_key(&target.coord) {
                    VariantKey {
                        tier: Tier::Coarse,
                        ..*target
                    }
                } else if refining {
                    *target
                } else {
                    return None;
                };
                (!state.cache.contains_key(&target) && !staged.contains(&target)).then_some(target)
            })
            .collect();
        requests.sort_by(|a, b| {
            active
                .contains_key(&a.coord)
                .cmp(&active.contains_key(&b.coord))
                .then_with(|| distance(a.coord).total_cmp(&distance(b.coord)))
        });
        requests.truncate(if requests.first().is_some_and(|r| r.tier == Tier::Fine) {
            1
        } else {
            3
        });
        if !requests.is_empty() {
            let inputs: Vec<_> = requests
                .iter()
                .map(|key| {
                    (
                        *key,
                        state
                            .cache
                            .iter()
                            .find(|(k, _)| {
                                k.coord == key.coord
                                    && k.version == key.version
                                    && k.epoch == key.epoch
                            })
                            .map(|(_, v)| v.placements.clone()),
                    )
                })
                .collect();
            let shapes = [
                &nature.trunk,
                &nature.canopy,
                &nature.crown,
                &nature.rock,
                &nature.shrub,
                &nature.groundcover,
                &nature.pebble,
            ]
            .map(|h| meshes.get(h).unwrap().clone());
            let low_canopy = meshes.get(&nature.canopy_low).unwrap().clone();
            let low_crown = meshes.get(&nature.crown_low).unwrap().clone();
            let terrain = state.terrain.as_ref().unwrap().clone();
            let clearings = state.clearings.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let (sender, receiver) = std::sync::mpsc::channel();
            let job = std::thread::Builder::new()
                .name("vegetation-mesh".into())
                .spawn(move || {
                    let _timing = crate::performance::span("scenery.prepare.worker");
                    for (key, placements) in inputs {
                        if worker_cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let placements = placements.unwrap_or_else(|| {
                            Arc::new(visible_placements(&terrain, key.coord, &clearings))
                        });
                        let mut source = shapes.clone();
                        if key.tier != Tier::Fine {
                            source[1] = low_canopy.clone();
                            source[2] = low_crown.clone();
                        }
                        if key.tier == Tier::Medium {
                            source[5] = compact_groundcover(source[5].clone());
                        }
                        let batches = batches(
                            &placements,
                            &source,
                            key.tier,
                            detail,
                            &terrain,
                            &clearings,
                            &worker_cancel,
                        );
                        if worker_cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let geometry = batches
                            .into_iter()
                            .enumerate()
                            .flat_map(|(material, b)| {
                                b.parts().into_iter().map(move |mesh| PreparedPart {
                                    material,
                                    bytes: mesh_bytes(&mesh),
                                    mesh,
                                })
                            })
                            .collect();
                        if sender
                            .send(PreparedScenery {
                                key,
                                placements,
                                geometry,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            if job.is_ok() {
                *state.pending.lock().unwrap() = Some(SceneryJob {
                    receiver,
                    keys: requests,
                    cancel,
                });
                if let Some(stats) = stats.as_deref_mut() {
                    stats.worker_requests += 1;
                }
            }
        }
    }
    if let Some(stats) = stats.as_deref_mut() {
        let uploaded = UPLOAD_BUDGET - budget;
        stats.uploaded_bytes += uploaded as u64;
        stats.peak_upload_bytes = stats.peak_upload_bytes.max(uploaded);
        stats.active_variants = active.len();
        stats.active_bytes = state
            .cache
            .iter()
            .filter(|(key, _)| active_keys.contains(*key))
            .map(|(_, v)| v.bytes())
            .sum();
        stats.inactive_bytes = inactive_bytes;
        stats.retiring_bytes = state.retiring.iter().map(|p| p.bytes).sum();
        stats.resident_bytes = stats.active_bytes
            + inactive_bytes
            + stats.retiring_bytes
            + state
                .staged
                .iter()
                .flat_map(|s| &s.loaded)
                .map(|p| p.bytes)
                .sum::<usize>();
        stats.queued_bytes = queued_bytes;
        stats.pending_variants = state.staged.len()
            + state
                .pending
                .lock()
                .unwrap()
                .as_ref()
                .map_or(0, |j| j.keys.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grass_shadow_lod_follows_shadow_quality_instead_of_environment_detail() {
        use crate::graphics::Detail;
        let mut app = App::new();
        let mut graphics = crate::graphics::Graphics::load();
        graphics.settings.environment = Detail::High;
        app.add_plugins(MinimalPlugins)
            .insert_resource(graphics)
            .init_resource::<Controls>()
            .add_systems(Update, growth_shadows);
        let center = app.world().resource::<Controls>().center;
        let casters = [16., 48., 96.].map(|distance| {
            app.world_mut()
                .spawn(GrowthShadow(Vec2::new(center.x + distance, center.z)))
                .id()
        });
        for (detail, expected) in [
            (Detail::Low, [false, false, false]),
            (Detail::Balanced, [true, false, false]),
            (Detail::High, [true, true, false]),
        ] {
            app.world_mut()
                .resource_mut::<crate::graphics::Graphics>()
                .settings
                .shadows = detail;
            app.update();
            assert_eq!(
                casters.map(|id| !app
                    .world()
                    .entity(id)
                    .contains::<bevy::light::NotShadowCaster>()),
                expected
            );
        }
    }
    use std::collections::HashMap;

    #[test]
    fn upload_parts_preserve_every_triangle_and_all_five_vertex_streams() {
        let source = crate::surfaces::foliage(false, false);
        let mut batch = Batch::default();
        for i in 0..400 {
            batch.add(&source, Transform::from_xyz(i as f32 * 0.2, 2., 4.));
        }
        let original = batch.clone().mesh();
        let parts = batch.parts();
        assert!(parts.len() > 1);
        fn triangles(mesh: &Mesh) -> Vec<Vec<u32>> {
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            indices
                .chunks_exact(3)
                .map(|tri| {
                    let mut value = vec![];
                    for &i in tri {
                        for attribute in [
                            Mesh::ATTRIBUTE_POSITION,
                            Mesh::ATTRIBUTE_NORMAL,
                            Mesh::ATTRIBUTE_COLOR,
                            Mesh::ATTRIBUTE_UV_0,
                            Mesh::ATTRIBUTE_UV_1,
                        ] {
                            match mesh.attribute(attribute).unwrap() {
                                VertexAttributeValues::Float32x2(v) => {
                                    value.extend(v[i].map(f32::to_bits))
                                }
                                VertexAttributeValues::Float32x3(v) => {
                                    value.extend(v[i].map(f32::to_bits))
                                }
                                VertexAttributeValues::Float32x4(v) => {
                                    value.extend(v[i].map(f32::to_bits))
                                }
                                _ => panic!("unexpected scenery attribute"),
                            }
                        }
                    }
                    value
                })
                .collect()
        }
        assert_eq!(
            triangles(&original),
            parts.iter().flat_map(triangles).collect::<Vec<_>>()
        );
        for part in parts {
            assert!(part.count_vertices() <= MAX_PART_VERTICES);
            assert!(mesh_bytes(&part) <= UPLOAD_BUDGET);
        }
    }
    #[test]
    fn medium_turf_uses_six_complete_original_blades() {
        let original = crate::surfaces::groundcover();
        let medium = compact_groundcover(original.clone());
        assert_eq!(medium.count_vertices(), 36);
        assert_eq!(medium.indices().unwrap().len(), 72);
        assert_eq!(
            medium.indices().unwrap().iter().collect::<Vec<_>>(),
            original
                .indices()
                .unwrap()
                .iter()
                .take(72)
                .collect::<Vec<_>>()
        );
        for attr in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
            let Some(VertexAttributeValues::Float32x3(a)) = medium.attribute(attr) else {
                panic!()
            };
            let Some(VertexAttributeValues::Float32x3(b)) = original.attribute(attr) else {
                panic!()
            };
            assert_eq!(a, &b[..36]);
        }
    }
    #[test]
    fn refinement_keeps_the_old_variant_until_its_exact_replacement_is_ready() {
        let fine = VariantKey {
            coord: IVec2::ZERO,
            tier: Tier::Fine,
            quality: 2,
            version: 0,
            epoch: 1,
        };
        let medium = VariantKey {
            tier: Tier::Medium,
            ..fine
        };
        let coarse = VariantKey {
            tier: Tier::Coarse,
            ..fine
        };
        assert!(!can_activate(Some(fine), coarse, medium));
        assert!(can_activate(Some(fine), medium, medium));
        assert!(can_activate(None, coarse, medium));
        assert!(can_activate(Some(fine), coarse, coarse));
        assert!(can_activate(
            Some(fine),
            VariantKey {
                version: 1,
                ..coarse
            },
            VariantKey {
                version: 1,
                ..medium
            }
        ));
    }
    #[test]
    fn extended_detail_covers_far_visible_ground_without_promoting_the_whole_view() {
        use crate::graphics::Detail;
        let mut controls = Controls {
            center: Vec3::new(300., 0., 300.),
            zoom: 100.,
            ..default()
        };
        let view = [0., 768., 0., 768.];
        let farther = IVec2::new(4, 3); // 84 m beyond the focus, formerly Medium.
        assert_eq!(
            tier_for(farther, view, &controls, false, Detail::High),
            Tier::Fine
        );
        assert_eq!(
            tier_for(farther, view, &controls, false, Detail::Low),
            Tier::Medium
        );
        assert_eq!(
            tier_for(IVec2::new(6, 3), view, &controls, false, Detail::High),
            Tier::Medium
        );
        assert_eq!(
            tier_for(IVec2::new(9, 3), view, &controls, false, Detail::High),
            Tier::Coarse
        );
        controls.zoom = 130.;
        assert_eq!(
            tier_for(IVec2::new(3, 3), view, &controls, false, Detail::Balanced),
            Tier::Fine
        );
        assert_eq!(
            tier_for(IVec2::new(3, 3), view, &controls, false, Detail::Low),
            Tier::Medium
        );
        assert_eq!(
            tier_for(farther, view, &controls, true, Detail::High),
            Tier::Coarse
        );
    }
    #[test]
    fn camera_footprint_limits_fine_geometry_and_far_edits_do_not_invalidate_near_chunks() {
        let controls = Controls {
            center: Vec3::new(200., 0., 200.),
            zoom: 36.,
            ..default()
        };
        let view = [180., 220., 180., 220.];
        assert_eq!(
            tier_for(
                IVec2::new(2, 2),
                view,
                &controls,
                false,
                crate::graphics::Detail::Balanced
            ),
            Tier::Fine
        );
        assert_eq!(
            tier_for(
                IVec2::ZERO,
                view,
                &controls,
                false,
                crate::graphics::Detail::Balanced
            ),
            Tier::Coarse
        );
        assert_eq!(
            tier_for(
                IVec2::new(2, 2),
                view,
                &controls,
                true,
                crate::graphics::Detail::Balanced
            ),
            Tier::Coarse
        );
        let before = flat_terrain(512);
        let mut after = before.clone();
        after.edits.insert(
            "500,500".into(),
            crate::terrain::Cell {
                elevation: 0.7,
                surface: 0.7,
                moisture: 0.6,
                kind: 4,
            },
        );
        let mut affected = HashSet::new();
        edited_chunks(&before, &after, &mut affected);
        assert!(affected.contains(&IVec2::new(5, 5)));
        assert!(!affected.contains(&IVec2::new(1, 1)));
        assert!(affected.len() <= 4);
    }

    fn await_chunks(app: &mut App, mut done: impl FnMut(&mut App) -> bool) {
        for _ in 0..2000 {
            app.update();
            if done(app) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(done(app), "vegetation preparation did not finish");
    }
    #[test]
    fn trees_use_the_full_scatter_cell_instead_of_lining_up_on_grid_rows() {
        let t = flat_terrain(512);
        let mut trees = vec![];
        for z in 0..5 {
            for x in 0..5 {
                trees.extend(
                    placements(&t, IVec2::new(x, z))
                        .into_iter()
                        .filter(|p| p.tree),
                );
            }
        }
        for axis in [0, 2] {
            let min = trees
                .iter()
                .map(|p| p.position[axis].rem_euclid(CELL as f32))
                .fold(f32::INFINITY, f32::min);
            let max = trees
                .iter()
                .map(|p| p.position[axis].rem_euclid(CELL as f32))
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(max - min > 5.5);
        }
    }
    #[test]
    fn batching_preserves_foliage_atlas_coordinates() {
        let foliage = crate::surfaces::foliage(false, false);
        let mut batch = Batch::default();
        batch.add(
            &foliage,
            Transform::from_xyz(20., 4., 30.).with_scale(Vec3::splat(2.)),
        );
        let merged = batch.mesh();
        assert_eq!(
            merged.attribute(Mesh::ATTRIBUTE_UV_0),
            foliage.attribute(Mesh::ATTRIBUTE_UV_0)
        );
        assert_eq!(merged.count_vertices(), foliage.count_vertices());
    }

    fn flat_terrain(size: i32) -> Terrain {
        let mut t = Terrain::new("scenery-test", size, size);
        t.authored = vec![0.52; (size * size) as usize];
        t.authored_scale = 12.;
        t.hydrate();
        t
    }
    #[test]
    fn resized_base_clearings_cover_both_runway_ends_and_the_outer_apron() {
        let game = NativeGame::default();
        let battle = Battle {
            snapshot: game.snapshot(),
            ..default()
        };
        let cleared = clearings(&battle);
        for field in game.entities.iter().filter(|e| e.view.kind == "airfield") {
            for local in [[81., -171.], [81., 171.], [-33., -171.], [-33., 171.]] {
                let point = Vec2::from_array(crate::airfield_layout::world(&field.view, local));
                assert!(
                    cleared.iter().any(|c| c.contains(point)),
                    "vegetation in apron at {point:?}"
                );
            }
        }
    }
    #[test]
    fn low_growth_is_stable_and_respects_training_lanes_and_building_clearings() {
        let mut t = flat_terrain(384);
        t.id = crate::training::ID.into();
        let clear = [
            Clearing::Circle(Vec2::new(185., 190.), 8.),
            Clearing::Rectangle(Vec2::new(250., 250.), Vec2::new(12., 8.), 0.4),
        ];
        let all: Vec<_> = (0..4)
            .flat_map(|z| (0..4).flat_map(move |x| [(x, z)]))
            .flat_map(|(x, z)| visible_placements(&t, IVec2::new(x, z), &clear))
            .collect();
        assert!(all.iter().filter(|p| p.understory == 1).count() > 30);
        assert!(all.iter().filter(|p| p.understory == 2).count() > 30);
        for p in &all {
            let at = Vec2::new(p.position.x, p.position.z);
            assert!(!clear.iter().any(|c| c.contains(at)));
            if (108. ..316.).contains(&at.x) && at.y < 278. {
                assert!(
                    [72., 120., 168., 216.]
                        .iter()
                        .all(|z| (at.y - z).abs() >= 5.)
                );
            }
        }
        assert_eq!(
            visible_placements(&t, IVec2::new(1, 1), &clear),
            visible_placements(&t, IVec2::new(1, 1), &clear)
        );
    }
    #[test]
    fn world_grid_is_repeatable_unique_and_canopies_do_not_overlap() {
        let t = flat_terrain(384);
        let mut all = HashMap::new();
        for z in 0..4 {
            for x in 0..4 {
                let at = IVec2::new(x, z);
                let first = placements(&t, at);
                assert_eq!(first, placements(&t, at));
                for p in first {
                    assert!(all.insert(p.cell, p).is_none());
                }
            }
        }
        assert!(all.len() > 450, "coverage must not have the old global cap");
        for p in all.values() {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    if let Some(other) = all.get(&(p.cell + IVec2::new(dx, dz))) {
                        let radius = |p: &Placement| {
                            if p.tree {
                                1.63 * p.scale
                            } else if p.understory == 0 {
                                1.8 * p.scale
                            } else {
                                0.91 * p.scale
                            }
                        };
                        let distance = Vec2::new(
                            p.position.x - other.position.x,
                            p.position.z - other.position.z,
                        )
                        .length();
                        assert!(distance > radius(p) + radius(other));
                    }
                }
            }
        }
        for (x, z) in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            assert!(
                all.values()
                    .filter(|p| p.cell.x / 32 == x / 2 && p.cell.y / 32 == z / 2)
                    .count()
                    > 100
            );
        }
    }
    #[test]
    fn coverage_includes_the_view_buffer_and_all_map_edges() {
        let t = flat_terrain(512);
        let near = chunk_bounds(&t, [140., 260., 120., 280.], CHUNK as f32);
        assert_eq!(near, [0, 3, 0, 3]);
        let wide = chunk_bounds(&t, [-300., 900., -700., 1200.], CHUNK as f32);
        assert_eq!(wide, [0, 5, 0, 5]);
        let edge = placements(&t, IVec2::new(5, 5));
        assert!(!edge.is_empty());
        assert!(
            edge.iter()
                .all(|p| p.position.x < 512. && p.position.z < 512.)
        );
    }
    fn chunks(app: &mut App) -> HashMap<IVec2, (Entity, Vec<Placement>)> {
        app.world_mut()
            .query::<(Entity, &Scenery)>()
            .iter(app.world())
            .map(|(e, p)| (p.coord, (e, p.placements.clone())))
            .collect()
    }
    #[test]
    fn camera_pan_and_unrelated_terrain_updates_keep_resident_entities() {
        let mut game = NativeGame::new();
        game.entities.clear();
        game.props.clear();
        game.terrain = flat_terrain(512);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<crate::grass_material::GrassMaterial>>()
            .init_resource::<Assets<crate::grass_material::GrowthMaterial>>()
            .init_resource::<Battle>()
            .init_resource::<Stats>()
            .init_resource::<crate::frontend::Frontend>()
            .insert_resource(game)
            .insert_resource(Controls {
                center: Vec3::new(200., 0., 200.),
                zoom: 36.,
                ..default()
            })
            .insert_resource(crate::graphics::Graphics::load())
            .add_systems(
                Startup,
                (crate::ground_detail::setup, crate::landscape::setup).chain(),
            )
            .add_systems(Update, stream);
        for _ in 0..600 {
            app.update();
            if app
                .world_mut()
                .query::<&Scenery>()
                .iter(app.world())
                .count()
                >= 16
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        await_chunks(&mut app, |app| {
            let (center, zoom) = {
                let c = app.world().resource::<Controls>();
                (c.center, c.zoom)
            };
            let controls = Controls {
                center,
                zoom,
                ..default()
            };
            let view = view_bounds(&controls, None);
            let detail = app
                .world()
                .resource::<crate::graphics::Graphics>()
                .settings
                .environment;
            let mut query = app.world_mut().query::<&Scenery>();
            query
                .iter(app.world())
                .all(|s| s.key.tier == tier_for(s.coord, view, &controls, false, detail))
        });
        let before = chunks(&mut app);
        assert!(before.values().map(|(_, p)| p.len()).sum::<usize>() > 450);
        // Stay within the same Fine footprint while changing an unrelated cell.
        app.world_mut().resource_mut::<Controls>().center.x += 5.;
        app.world_mut()
            .resource_mut::<NativeGame>()
            .terrain
            .revision += 1;
        app.world_mut()
            .resource_mut::<NativeGame>()
            .terrain
            .edits
            .insert(
                "500,500".into(),
                crate::terrain::Cell {
                    elevation: 0.7,
                    surface: 0.7,
                    moisture: 0.6,
                    kind: 4,
                },
            );
        for _ in 0..12 {
            app.update();
        }
        let after = chunks(&mut app);
        for (at, (entity, props)) in &before {
            assert_eq!(after.get(at), Some(&(*entity, props.clone())));
        }
        // Repeated movement across boundaries must never create duplicate chunks/cells.
        for x in [350., 160., 360., 200.] {
            app.world_mut().resource_mut::<Controls>().center.x = x;
            for _ in 0..12 {
                app.update();
            }
            let mut coords = HashSet::new();
            let mut cells = HashSet::new();
            for chunk in app.world_mut().query::<&Scenery>().iter(app.world()) {
                assert!(coords.insert(chunk.coord));
                for p in &chunk.placements {
                    assert!(cells.insert(p.cell));
                }
            }
        }
        app.world_mut().resource_mut::<Controls>().center = Vec3::new(500., 0., 500.);
        app.world_mut().resource_mut::<Controls>().zoom = 16.;
        for _ in 0..12 {
            app.update();
        }
        assert!(!chunks(&mut app).contains_key(&IVec2::new(1, 1)));
        app.world_mut().resource_mut::<Controls>().center = Vec3::new(200., 0., 200.);
        app.world_mut().resource_mut::<Controls>().zoom = 36.;
        for _ in 0..12 {
            app.update();
        }
        await_chunks(&mut app, |app| chunks(app).contains_key(&IVec2::new(1, 1)));
        let restored = chunks(&mut app);
        assert_eq!(before[&IVec2::new(1, 1)].1, restored[&IVec2::new(1, 1)].1);
        assert!(
            app.world().resource::<Stats>().cache_hits > 0,
            "a pan revisit should reuse resident geometry"
        );
        let at = restored[&IVec2::new(1, 1)].1[0].position;
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .push(crate::runtime::Piece {
                building: true,
                kind: "barracks".into(),
                x: at.x,
                z: at.z,
                ..default()
            });
        await_chunks(&mut app, |app| {
            let exclusions = clearings(app.world().resource::<Battle>());
            chunks(app).values().flat_map(|(_, props)| props).all(|p| {
                !exclusions
                    .iter()
                    .any(|c| c.contains(Vec2::new(p.position.x, p.position.z)))
            })
        });
        let cleared = chunks(&mut app);
        let exclusions = clearings(app.world().resource::<Battle>());
        assert!(cleared.values().flat_map(|(_, p)| p).all(|p| {
            !exclusions
                .iter()
                .any(|c| c.contains(Vec2::new(p.position.x, p.position.z)))
        }));
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .clear();
        await_chunks(&mut app, |app| {
            chunks(app)
                .get(&IVec2::new(1, 1))
                .is_some_and(|(_, props)| *props == restored[&IVec2::new(1, 1)].1)
        });
        assert_eq!(
            restored[&IVec2::new(1, 1)].1,
            chunks(&mut app)[&IVec2::new(1, 1)].1
        );
        let stats = app.world().resource::<Stats>();
        assert!(stats.inactive_bytes <= CACHE_BUDGET);
        assert!(stats.peak_upload_bytes <= UPLOAD_BUDGET);
        // Live variants are split into bounded parts; retained inactive data is
        // capped by bytes rather than an obsolete five-handles-per-chunk count.
        let resident: usize = app
            .world()
            .resource::<Assets<Mesh>>()
            .iter()
            .map(|(_, m)| mesh_bytes(m))
            .sum();
        assert!(resident <= stats.resident_bytes + 1024 * 1024);
        // Importing a fresh authored buffer with the same map id/revision must
        // invalidate old scenery instead of keeping positions from the old map.
        let previous_entities: HashSet<_> = chunks(&mut app)
            .values()
            .map(|(entity, _)| *entity)
            .collect();
        let replacement = {
            let game = app.world().resource::<NativeGame>();
            let mut terrain = game.terrain.clone();
            terrain.authored.fill(0.6);
            terrain
        };
        app.world_mut().resource_mut::<NativeGame>().terrain = replacement;
        app.update();
        assert!(
            previous_entities
                .iter()
                .all(|entity| app.world().get::<Scenery>(*entity).is_none())
        );
        await_chunks(&mut app, |app| !chunks(app).is_empty());
        assert!(
            chunks(&mut app)
                .values()
                .all(|(entity, _)| !previous_entities.contains(entity))
        );
    }
}
