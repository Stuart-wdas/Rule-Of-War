//! Persistent, terrain-conforming disturbed soil. Albedo contains no lighting.
use crate::{simulation::BattlefieldProp, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

/// Read the installed terrain, including coarse/detail feathering and tiles
/// still waiting for a crater update. Never guess which LOD is on the GPU.
#[derive(Default)]
pub struct Projection {
    tiles: HashMap<(i32, i32), Grid>,
}
struct Grid {
    mesh: Handle<Mesh>,
    low: Vec2,
    step: f32,
    stride: usize,
}
impl Projection {
    pub fn new<'a>(ground: impl Iterator<Item = &'a Handle<Mesh>>, meshes: &Assets<Mesh>) -> Self {
        let mut result = Self::default();
        for handle in ground {
            let Some(mesh) = meshes.get(handle) else {
                continue;
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            if p.len() < 4 {
                continue;
            }
            let (first, last) = (p[0], p[p.len() - 1]);
            let step = p[1][0] - first[0];
            if step <= 0. {
                continue;
            }
            let low = Vec2::new(first[0], first[2]);
            let high = Vec2::new(last[0], last[2]);
            let stride = ((high.x - low.x) / step).ceil() as usize + 1;
            if stride < 2 || p.len() / stride < 2 {
                continue;
            }
            result.tiles.insert(
                ((low.x / 64.).round() as i32, (low.y / 64.).round() as i32),
                Grid {
                    mesh: handle.clone(),
                    low,
                    step,
                    stride,
                },
            );
        }
        result
    }
    fn signature(&self, at: [f32; 2], radius: f32) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let lo = ((Vec2::from_array(at) - Vec2::splat(radius * 1.3)) / 64.)
            .floor()
            .as_ivec2();
        let hi = ((Vec2::from_array(at) + Vec2::splat(radius * 1.3)) / 64.)
            .floor()
            .as_ivec2();
        for z in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                self.tiles.get(&(x, z)).map(|g| g.mesh.id()).hash(&mut hash);
            }
        }
        hash.finish()
    }
    fn height(&self, t: &Terrain, p: Vec2, meshes: &Assets<Mesh>) -> f32 {
        let key = (p / 64.).floor().as_ivec2();
        let grid = self.tiles.get(&(key.x, key.y));
        let height = grid
            .and_then(|g| {
                let mesh = meshes.get(&g.mesh)?;
                let bevy::mesh::VertexAttributeValues::Float32x3(v) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
                else {
                    return None;
                };
                let cell = ((p - g.low) / g.step).floor().max(Vec2::ZERO).as_uvec2();
                let x = (cell.x as usize).min(g.stride.saturating_sub(2));
                let z = (cell.y as usize).min(v.len() / g.stride - 2);
                let i = z * g.stride + x;
                let (a, b, c, d) = (v[i], v[i + 1], v[i + g.stride], v[i + g.stride + 1]);
                let u = ((p.x - a[0]) / (b[0] - a[0]).max(0.001)).clamp(0., 1.);
                let w = ((p.y - a[2]) / (c[2] - a[2]).max(0.001)).clamp(0., 1.);
                Some(if u + w <= 1. {
                    a[1] + (b[1] - a[1]) * u + (c[1] - a[1]) * w
                } else {
                    d[1] + (c[1] - d[1]) * (1. - u) + (b[1] - d[1]) * (1. - w)
                })
            })
            .unwrap_or_else(|| ground(t, p));
        // Road ribbons are raised above either terrain level of detail.
        height.max(road_height(t, p).unwrap_or(f32::NEG_INFINITY))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mark {
    /// Outer extent of the loose ejecta, in metres (not the damage radius).
    pub radius: f32,
    pub excavated: bool,
    pub seed: u32,
}
impl Mark {
    pub fn for_weapon(weapon: &str, terrain_radius: f32, seed: u32) -> Option<Self> {
        let radius = match weapon {
            "medium" => 1.65,
            "c130-30" => 2.4,
            "c130-105" => 4.8,
            "heli-rocket" => 2.3,
            "long" => 3.25,
            "xtralong" => 3.9,
            _ => return None,
        };
        Some(Self {
            radius,
            excavated: terrain_radius > 0.,
            seed,
        })
    }
    pub fn from_prop(p: &BattlefieldProp) -> Self {
        // Older saves only store the crater prop and the terrain deformation.
        let seed =
            p.id.bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let mut mark = p.scar.unwrap_or(Self {
            radius: 3.25,
            excavated: true,
            seed,
        });
        mark.radius = if mark.radius.is_finite() {
            mark.radius.clamp(0.5, 5.)
        } else {
            3.25
        };
        mark
    }
}

pub fn material(images: &mut Assets<Image>) -> StandardMaterial {
    let mut material = crate::surfaces::material("impact_scar", Color::WHITE, images);
    material.base_color = Color::srgb(0.76, 0.74, 0.68);
    material.alpha_mode = AlphaMode::Blend;
    material.reflectance = 0.12;
    material.metallic = 0.;
    material.perceptual_roughness = 1.;
    material.depth_bias = 1.;
    material
}

/// Only changed terrain cells underneath a mark invalidate its mesh.
#[derive(Component)]
pub struct Scar {
    pub mesh: Handle<Mesh>,
    at: [f32; 2],
    heading: f32,
    mark: Mark,
    revision: u64,
    source: String,
    heights: u64,
    projection: u64,
}
impl Scar {
    pub fn new(
        p: &BattlefieldProp,
        t: &Terrain,
        mesh: Handle<Mesh>,
        projection: &Projection,
    ) -> Self {
        let mut scar = Self {
            mesh,
            at: p.at,
            heading: p.heading,
            mark: Mark::from_prop(p),
            revision: t.revision,
            source: crate::terrain::render_source_key(t),
            heights: 0,
            projection: projection.signature(p.at, Mark::from_prop(p).radius),
        };
        scar.heights = scar.ground_signature(t);
        scar
    }
    fn ground_signature(&self, t: &Terrain) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let radius = (self.mark.radius * 1.25).ceil() as i32 + 2;
        for z in -radius..=radius {
            for x in -radius..=radius {
                let (x, z) = (self.at[0].floor() + x as f32, self.at[1].floor() + z as f32);
                t.height(x, z).to_bits().hash(&mut hash);
                t.cell(x, z).kind.hash(&mut hash);
            }
        }
        hash.finish()
    }
    pub fn refresh(
        &mut self,
        t: &Terrain,
        source: &str,
        projection: &Projection,
        meshes: &mut Assets<Mesh>,
    ) -> bool {
        let next_projection = projection.signature(self.at, self.mark.radius);
        if self.revision == t.revision
            && self.source == source
            && self.projection == next_projection
        {
            return false;
        }
        let next = self.ground_signature(t);
        let changed =
            next != self.heights || self.source != source || self.projection != next_projection;
        self.projection = next_projection;
        self.revision = t.revision;
        self.source = source.into();
        self.heights = next;
        if changed {
            let next = projected(t, self.at, self.heading, self.mark, projection, meshes);
            if let Some(mesh) = meshes.get_mut(&self.mesh) {
                *mesh = next;
            }
        }
        changed
    }
}

/// Match the visible terrain triangles, rather than the rounded edit-cell lookup.
/// Road surfaces sit a few centimetres above the ground; clear their actual LOD.
fn sample(t: &Terrain, p: Vec2, step: f32) -> f32 {
    let lo = (p / step).floor() * step;
    let hi = (lo + Vec2::splat(step)).min(Vec2::new((t.cols - 1) as f32, (t.rows - 1) as f32));
    let uv = (p - lo) / (hi - lo).max(Vec2::splat(0.001));
    let h = |x, z| crate::landscape::visual_height(t, x, z);
    let (a, b, c, d) = (h(lo.x, lo.y), h(hi.x, lo.y), h(lo.x, hi.y), h(hi.x, hi.y));
    if uv.x + uv.y <= 1. {
        a + (b - a) * uv.x + (c - a) * uv.y
    } else {
        d + (c - d) * (1. - uv.x) + (b - d) * (1. - uv.y)
    }
}
fn road_height(t: &Terrain, p: Vec2) -> Option<f32> {
    if t.id == crate::battlefield::ID {
        let map = crate::rural_layout::map_point(p.x, p.y, t.cols, t.rows);
        if crate::rural_layout::surface_distance_for(map, t.cols, t.rows) < 0.5 {
            return Some(
                sample(t, p, 1.).max(sample(
                    t,
                    p,
                    if t.cols.max(t.rows) > 1536 { 8. } else { 4. },
                )) + 0.095,
            );
        }
    }
    None
}
fn ground(t: &Terrain, p: Vec2) -> f32 {
    sample(t, p, 1.).max(road_height(t, p).unwrap_or(f32::NEG_INFINITY))
}

pub fn projected(
    t: &Terrain,
    at: [f32; 2],
    heading: f32,
    mark: Mark,
    projection: &Projection,
    meshes: &Assets<Mesh>,
) -> Mesh {
    build_with(t, at, heading, mark, |p| projection.height(t, p, meshes))
}

#[cfg(test)]
pub fn build(t: &Terrain, at: [f32; 2], heading: f32, mark: Mark) -> Mesh {
    build_with(t, at, heading, mark, |p| ground(t, p))
}
fn build_with(
    t: &Terrain,
    at: [f32; 2],
    heading: f32,
    mark: Mark,
    height: impl Fn(Vec2) -> f32,
) -> Mesh {
    let mut positions = Vec::new();
    let mut uv = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let segments = (mark.radius * 5.).ceil().clamp(12., 24.) as u32;
    let origin = Vec2::from_array(at);
    let rotation = Quat::from_rotation_y(heading);
    let base = t.height(at[0], at[1]);
    let variant = mark.seed % 4;
    let atlas = Vec2::new((variant % 2) as f32, (variant / 2) as f32) * 0.5;
    let aspect = 0.83 + random(mark.seed, 31) * 0.14;
    let local_world = |q: Vec2| {
        let p = rotation * Vec3::new(q.x * mark.radius, 0., q.y * mark.radius * aspect);
        origin + Vec2::new(p.x, p.z)
    };
    let visible = |p: Vec2| {
        p.min_element() >= 0.
            && p.x <= (t.cols - 1) as f32
            && p.y <= (t.rows - 1) as f32
            && !t.cell(p.x, p.y).water()
    };
    for z in 0..=segments {
        for x in 0..=segments {
            let q = Vec2::new(x as f32, z as f32) / segments as f32 * 2. - Vec2::ONE;
            let world = local_world(q);
            let angle = q.y.atan2(q.x);
            let boundary =
                1. + 0.07 * (angle * 3. + mark.seed as f32).sin() + 0.04 * (angle * 7.).sin();
            let r = q.length() / boundary;
            let broken = crate::natural_noise::value(q.x * 4. + 13., q.y * 4. + 9., mark.seed);
            let lip = (-((r - 0.44) / 0.085).powi(2)).exp()
                * (mark.radius * 0.065).min(0.22)
                * (0.10 + 0.90 * smooth(0.30, 0.72, broken))
                * if mark.excavated { 1. } else { 0.22 };
            let y = height(world) + 0.045 + lip;
            positions.push([q.x * mark.radius, y - base, q.y * mark.radius * aspect]);
            uv.push((atlas + (q * 0.5 + Vec2::splat(0.5)) * 0.5).to_array());
            colors.push([1., 1., 1., if visible(world) { 1. } else { 0. }]);
            if x < segments && z < segments {
                let a = z * (segments + 1) + x;
                indices.extend([
                    a,
                    a + segments + 1,
                    a + 1,
                    a + 1,
                    a + segments + 1,
                    a + segments + 2,
                ]);
            }
        }
    }
    // A few angular soil clods, grouped in broken fans. One shared mesh and
    // material per impact; no twelve-sphere rim and no individual rock entities.
    let count = if mark.excavated { 19 } else { 7 };
    for i in 0..count {
        let a = random(mark.seed, i * 7) * std::f32::consts::TAU;
        let r = 0.37 + random(mark.seed, i * 7 + 1) * 0.40;
        let q = Vec2::new(a.cos(), a.sin()) * r;
        let world = local_world(q);
        if !visible(world) {
            continue;
        }
        let size = mark.radius * (0.018 + random(mark.seed, i * 7 + 2) * 0.026);
        let centre = Vec3::new(
            q.x * mark.radius,
            height(world) - base + 0.035,
            q.y * mark.radius * aspect,
        );
        let points = [
            centre + Vec3::new(-size, 0., -size * 0.7),
            centre + Vec3::new(size * 0.7, 0., -size * 0.5),
            centre + Vec3::new(size * 0.2, 0., size),
            centre
                + Vec3::new(
                    -size * 0.1,
                    size * (0.65 + random(mark.seed, i * 7 + 3)),
                    0.,
                ),
        ];
        for face in [[0, 3, 1], [1, 3, 2], [2, 3, 0]] {
            for (j, p) in face.into_iter().enumerate() {
                indices.push(positions.len() as u32);
                positions.push(points[p].to_array());
                let tex = [
                    Vec2::new(0.45, 0.56),
                    Vec2::new(0.52, 0.49),
                    Vec2::new(0.56, 0.57),
                ][j];
                uv.push((atlas + tex * 0.5).to_array());
                let tint = 0.76 + random(mark.seed, i + 103) * 0.30;
                colors.push([tint, tint, tint, 1.]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    let _ = mesh.generate_tangents();
    mesh
}
fn random(seed: u32, i: u32) -> f32 {
    let mut h = seed ^ i.wrapping_mul(374761393);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let u = ((x - a) / (b - a)).clamp(0., 1.);
    u * u * (3. - 2. * u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing ground positions");
        };
        p
    }
    #[test]
    fn scars_follow_sloping_triangles_with_complete_bounded_pbr_geometry() {
        let terrain = Terrain::new(crate::training::ID, 384, 384);
        let at = [292.3, 299.7];
        for (seed, weapon) in ["medium", "long", "xtralong", "heli-rocket"]
            .iter()
            .enumerate()
        {
            let mark = Mark::for_weapon(weapon, 2., seed as u32).unwrap();
            let heading = 0.37 + seed as f32;
            let mesh = build(&terrain, at, heading, mark);
            assert!(mesh.count_vertices() <= 850);
            let grid = ((mark.radius * 5.).ceil().clamp(12., 24.) as usize + 1).pow(2);
            for p in positions(&mesh).iter().take(grid) {
                let world = Quat::from_rotation_y(heading) * Vec3::from_array(*p)
                    + Vec3::new(at[0], terrain.height(at[0], at[1]), at[1]);
                let distance = world.y - ground(&terrain, Vec2::new(world.x, world.z));
                assert!(
                    (0.044..0.266).contains(&distance),
                    "ground clearance: {distance}"
                );
            }
            for attr in [
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_TANGENT,
                Mesh::ATTRIBUTE_UV_0,
                Mesh::ATTRIBUTE_COLOR,
            ] {
                assert_eq!(mesh.attribute(attr).unwrap().len(), mesh.count_vertices());
            }
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!()
            };
            assert!(normals.iter().all(|n| Vec3::from_array(*n).is_finite()
                && (Vec3::from_array(*n).length() - 1.).abs() < 0.001));
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!()
            };
            let lo = Vec2::new((mark.seed % 2) as f32, (mark.seed / 2 % 2) as f32) * 0.5;
            assert!(uv.iter().all(|v| Vec2::from_array(*v).cmpge(lo).all()
                && Vec2::from_array(*v).cmple(lo + Vec2::splat(0.5)).all()));
            assert!(
                mesh.indices()
                    .unwrap()
                    .iter()
                    .all(|i| i < mesh.count_vertices())
            );
        }
    }
    #[test]
    fn adjacent_craters_reproject_in_place_but_distant_edits_leave_meshes_alone() {
        let mut terrain = Terrain::new(crate::training::ID, 384, 384);
        let prop: BattlefieldProp = serde_json::from_value(serde_json::json!({
            "id":"legacy-impact", "kind":"crater", "at":[190.,190.], "hp":0., "heading":0.7
        }))
        .unwrap();
        let mark = Mark::from_prop(&prop);
        assert!(mark.excavated && mark.radius == 3.25);
        let mut meshes = Assets::<Mesh>::default();
        let handle = meshes.add(build(&terrain, prop.at, prop.heading, mark));
        let original = positions(meshes.get(&handle).unwrap()).to_vec();
        let projection = Projection::default();
        let mut scar = Scar::new(&prop, &terrain, handle.clone(), &projection);
        let source = crate::terrain::render_source_key(&terrain);
        terrain.crater(270., 280., 2);
        assert!(!scar.refresh(&terrain, &source, &projection, &mut meshes));
        assert_eq!(positions(meshes.get(&handle).unwrap()), original);
        terrain.crater(191., 190., 2);
        assert!(scar.refresh(&terrain, &source, &projection, &mut meshes));
        assert_ne!(positions(meshes.get(&handle).unwrap()), original);
        assert_eq!(scar.mesh.id(), handle.id());
        assert!(!scar.refresh(&terrain, &source, &projection, &mut meshes));
    }
    #[test]
    fn scars_track_the_installed_lod_and_refresh_when_a_streamed_tile_is_replaced() {
        let terrain = Terrain::new(crate::training::ID, 384, 384);
        let prop: BattlefieldProp = serde_json::from_value(serde_json::json!({
            "id":"streamed", "kind":"crater", "at":[30.,30.], "hp":0., "heading":1.2
        }))
        .unwrap();
        let tile = |height| {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vec![
                    [0., height, 0.],
                    [64., height + 2., 0.],
                    [0., height + 1., 64.],
                    [64., height + 3., 64.],
                ],
            )
        };
        let mut meshes = Assets::<Mesh>::default();
        let first = meshes.add(tile(26.));
        let projection = Projection::new([&first].into_iter(), &meshes);
        let mark = Mark::from_prop(&prop);
        let mesh = projected(&terrain, prop.at, prop.heading, mark, &projection, &meshes);
        let grid = ((mark.radius * 5.).ceil() as usize + 1).pow(2);
        for p in positions(&mesh).iter().take(grid) {
            let world = Quat::from_rotation_y(prop.heading) * Vec3::from_array(*p)
                + Vec3::new(30., terrain.height(30., 30.), 30.);
            let expected = 26. + world.x / 32. + world.z / 64.;
            assert!((0.044..0.266).contains(&(world.y - expected)));
        }
        let handle = meshes.add(mesh);
        let original = positions(meshes.get(&handle).unwrap()).to_vec();
        let mut scar = Scar::new(&prop, &terrain, handle.clone(), &projection);
        let second = meshes.add(tile(27.));
        let replacement = Projection::new([&second].into_iter(), &meshes);
        assert!(scar.refresh(
            &terrain,
            &crate::terrain::render_source_key(&terrain),
            &replacement,
            &mut meshes
        ));
        for (before, after) in original.iter().zip(positions(meshes.get(&handle).unwrap())) {
            assert!((after[1] - before[1] - 1.).abs() < 0.001);
        }
    }
    #[test]
    fn water_and_map_edges_have_no_visible_soil_or_floating_clods() {
        let mut terrain = Terrain::new(crate::training::ID, 384, 384);
        for z in 12..=28 {
            for x in 12..=28 {
                terrain.edits.insert(
                    format!("{x},{z}"),
                    crate::terrain::Cell {
                        elevation: 0.3,
                        surface: 0.33,
                        moisture: 1.,
                        kind: 2,
                    },
                );
            }
        }
        let mark = Mark::for_weapon("long", 2., 7).unwrap();
        let mesh = build(&terrain, [20., 20.], 0.3, mark);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        assert!(colors.iter().all(|c| c[3] == 0.));
        let edge = build(&terrain, [0., 0.], 0., mark);
        let Some(VertexAttributeValues::Float32x4(colors)) = edge.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        for (p, c) in positions(&edge).iter().zip(colors) {
            if p[0] < 0. || p[2] < 0. {
                assert_eq!(c[3], 0.);
            }
        }
    }
}
