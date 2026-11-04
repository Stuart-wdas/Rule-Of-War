//! Indexed smooth terrain, clipped water and background mesh preparation.
use crate::{Controls, simulation::NativeGame, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use std::sync::mpsc;
#[derive(Component)]
pub struct Ground(pub Handle<Mesh>);
#[derive(Component)]
pub struct Environment;
#[derive(Resource, Default)]
pub struct Ready {
    pub ready: bool,
    pub error: Option<String>,
    pub builds: u32,
    pub vertices: usize,
    pub upload_bytes: usize,
    pub peak_upload_bytes: usize,
    pub cached_detail_tiles: usize,
    pub stale_tiles: u64,
    pub pending_tiles: usize,
}
#[derive(Resource)]
pub struct Nature {
    pub(crate) pebble: Handle<Mesh>,
    pub(crate) trunk: Handle<Mesh>,
    pub(crate) canopy: Handle<Mesh>,
    pub(crate) canopy_low: Handle<Mesh>,
    pub(crate) crown: Handle<Mesh>,
    pub(crate) crown_low: Handle<Mesh>,
    pub(crate) rock: Handle<Mesh>,
    pub(crate) shrub: Handle<Mesh>,
    pub(crate) groundcover: Handle<Mesh>,
    pub(crate) low_growth: Handle<crate::grass_material::GrowthMaterial>,
    pub(crate) bark: Handle<StandardMaterial>,
    pub(crate) leaves: Handle<StandardMaterial>,
    pub(crate) leaves_light: Handle<StandardMaterial>,
    pub(crate) stone: Handle<StandardMaterial>,
    pub(crate) grass: Handle<crate::grass_material::GrassMaterial>,
    water: Handle<StandardMaterial>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects terrain, growth and texture asset collections"
)]
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut grass_materials: ResMut<Assets<crate::grass_material::GrassMaterial>>,
    mut growth_materials: ResMut<Assets<crate::grass_material::GrowthMaterial>>,
    graphics: Res<crate::graphics::Graphics>,
    wear: Res<crate::ground_detail::GroundWear>,
) {
    let trunk = crate::natural_geometry::trunk();
    let rock = crate::natural_geometry::rock(73);
    let mut stone = crate::surfaces::material("stone", Color::srgb(0.42, 0.43, 0.36), &mut images);
    // Supplied textures default to white modulation; keep the outcrops weathered
    // grey instead of allowing the pale source texture to resemble white pebbles.
    stone.base_color = Color::srgb(0.59, 0.60, 0.55);
    stone.perceptual_roughness = 0.90;
    stone.reflectance = 0.22;
    let mut leaves =
        crate::surfaces::material("leaves", Color::srgb(0.14, 0.22, 0.095), &mut images);
    leaves.alpha_mode = AlphaMode::Mask(0.4);
    leaves.cull_mode = None;
    leaves.double_sided = true;
    leaves.perceptual_roughness = 0.88;
    leaves.diffuse_transmission = 0.12;
    if leaves.base_color == Color::WHITE {
        leaves.base_color = if crate::ground_demo::photographic() {
            Color::srgb(0.36, 0.38, 0.28)
        } else {
            Color::srgb(0.49, 0.62, 0.36)
        };
    }
    leaves.reflectance = 0.18;
    let mut leaves_light = leaves.clone();
    leaves_light.base_color = if leaves.base_color_texture.is_some() {
        if crate::ground_demo::photographic() {
            Color::srgb(0.42, 0.43, 0.30)
        } else {
            Color::srgb(0.65, 0.72, 0.43)
        }
    } else {
        Color::srgb(0.20, 0.28, 0.13)
    };
    let grass = StandardMaterial {
        metallic: 0.,
        reflectance: 0.2,
        perceptual_roughness: 0.96,
        ..default()
    };
    let size = graphics.settings.textures.texture_size();
    let texture = images.add(crate::surfaces::terrain_array("albedo", size));
    let (repeats, variation) = crate::surfaces::terrain_repeats();
    let grass = grass_materials.add(crate::grass_material::GrassMaterial {
        base: grass,
        extension: crate::grass_material::GrassBlend {
            rural: crate::grass_material::RuralPaint::new(),
            tracks: wear.image.clone(),
            track_world: Vec4::new(1. / wear.extent.x, 1. / wear.extent.y, 0., 0.),
            debug: Vec4::new(
                if std::env::args().any(|a| a == "--smoke-test") {
                    let args: Vec<_> = std::env::args().collect();
                    args.iter()
                        .position(|a| a == "--terrain-debug")
                        .and_then(|i| args.get(i + 1))
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(0.)
                } else {
                    0.
                },
                0.,
                if crate::ground_demo::photographic() {
                    1.
                } else {
                    0.
                },
                if std::env::args().any(|a| a == "--legacy-terrain-noise") {
                    1.
                } else {
                    0.
                },
            ),
            texture,
            normals: images.add(crate::surfaces::terrain_array("normal", size)),
            orm: images.add(crate::surfaces::terrain_array("orm", size)),
            repeats,
            variation,
            photo_means: crate::surfaces::terrain_means(),
        },
    });
    commands.insert_resource(Nature {
        pebble: meshes.add(crate::natural_geometry::pebble(31)),
        trunk: meshes.add(trunk),
        canopy: meshes.add(crate::surfaces::foliage(true, false)),
        canopy_low: meshes.add(crate::surfaces::foliage(true, true)),
        crown: meshes.add(crate::surfaces::foliage(false, false)),
        crown_low: meshes.add(crate::surfaces::foliage(false, true)),
        rock: meshes.add(rock),
        shrub: meshes.add(crate::surfaces::shrub()),
        groundcover: meshes.add(crate::surfaces::groundcover()),
        low_growth: growth_materials.add(crate::grass_material::GrowthMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.29, 0.37, 0.16),
                perceptual_roughness: 1.,
                reflectance: 0.1,
                cull_mode: None,
                double_sided: true,
                ..default()
            },
            extension: crate::grass_material::GrowthBlend {
                tracks: wear.image.clone(),
                track_world: Vec4::new(1. / wear.extent.x, 1. / wear.extent.y, 0., 0.),
            },
        }),
        bark: materials.add(crate::surfaces::material(
            "bark",
            Color::srgb(0.23, 0.20, 0.16),
            &mut images,
        )),
        leaves: materials.add(leaves),
        leaves_light: materials.add(leaves_light),
        stone: materials.add(stone),
        grass,
        water: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.24, 0.23),
            perceptual_roughness: 0.3,
            reflectance: 0.35,
            metallic: 0.0,
            ..default()
        }),
    });
}
fn wet(t: &Terrain, x: f32, z: f32) -> f32 {
    let (cx, cz) = (x.floor(), z.floor());
    let w = |x, z| if t.cell(x, z).water() { 1. } else { 0. };
    let a = w(cx, cz) + (w(cx + 1., cz) - w(cx, cz)) * (x - cx);
    let b = w(cx, cz + 1.) + (w(cx + 1., cz + 1.) - w(cx, cz + 1.)) * (x - cx);
    a + (b - a) * (z - cz)
}
fn color(t: &Terrain, x: f32, z: f32, water: f32) -> [f32; 4] {
    let mut base;
    let slope = Vec2::new(
        t.height(x - 1., z) - t.height(x + 1., z),
        t.height(x, z - 1.) - t.height(x, z + 1.),
    )
    .length()
        * 0.5;
    let soil = (slope * 0.6).clamp(0., 0.65);
    // The supplied grass/soil scans carry surface detail; authored parcels apply
    // the same restrained crop/pasture tint at every terrain level of detail.
    let broad = crate::natural_noise::value(x / 71., z / 71., 57);
    let fine = crate::natural_noise::value(x / 23., z / 23., 731);
    // Broad natural modulation remains in the world-space fragment shader.
    let light = 0.77 + broad * 0.025 + fine * 0.01;
    base =
        Vec3::new(light, light * 1.015, light * 0.97).lerp(Vec3::splat(light * 0.86), soil * 0.35);
    if t.id == crate::gis_prototype::ID {
        base *= crate::gis_prototype::tint(x, z);
    }
    // Agricultural colour is evaluated per pixel from the authored polygons.
    // Vertex colour remains neutral at both coarse and fine terrain detail.
    base = base.lerp(Vec3::new(0.18, 0.29, 0.27), water.clamp(0., 1.));
    Color::srgb(base.x, base.y, base.z)
        .to_linear()
        .to_f32_array()
}
fn clip_water(triangle: [(Vec3, f32); 3]) -> Vec<Vec3> {
    let mut output = vec![];
    let mut previous = triangle[2];
    for current in triangle {
        if (previous.1 >= 0.5) != (current.1 >= 0.5) {
            output.push(
                previous
                    .0
                    .lerp(current.0, (0.5 - previous.1) / (current.1 - previous.1)),
            );
        }
        if current.1 >= 0.5 {
            output.push(current.0);
        }
        previous = current;
    }
    output
}
include!("terrain_samples.rs");
fn mesh(
    t: &Terrain,
    b: [i32; 4],
    step: f32,
    exclude: Option<[i32; 4]>,
    blend: Option<(i32, [i32; 4])>,
) -> (Mesh, Mesh) {
    let nx = ((b[1] - b[0]) as f32 / step).ceil() as usize;
    let nz = ((b[3] - b[2]) as f32 / step).ceil() as usize;
    let mut positions = Vec::with_capacity((nx + 1) * (nz + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut uv = Vec::with_capacity(positions.capacity());
    let mut weights = Vec::with_capacity(positions.capacity());
    let coarse_samples = blend.map(|(coarse, bounds)| {
        SampleGrid::new(bounds, coarse as f32, |x, z| SurfaceSample::at(t, x, z))
    });
    for z in 0..=nz {
        for x in 0..=nx {
            let (x, z) = (
                (b[0] as f32 + x as f32 * step).min(b[1] as f32),
                (b[2] as f32 + z as f32 * step).min(b[3] as f32),
            );
            let mut sample = SurfaceSample::at(t, x, z);
            if let (Some((coarse, bounds)), Some(grid)) = (blend, coarse_samples.as_ref()) {
                let weight = edge_weight(x, z, coarse as f32, bounds);
                if weight < 1. {
                    sample = grid.sample(x, z, SurfaceSample::lerp).lerp(sample, weight);
                }
            }
            positions.push([x, sample.height, z]);
            normals.push(sample.normal.normalize_or(Vec3::Y).to_array());
            colors.push(sample.color.to_array());
            uv.push([x * 0.0625, z * 0.0625]);
            weights.push(sample.wet);
        }
    }
    let mut indices = vec![];
    let mut water_positions: Vec<[f32; 3]> = vec![];
    let mut water_normals: Vec<[f32; 3]> = vec![];
    for z in 0..nz {
        for x in 0..nx {
            let r = z * (nx + 1) + x;
            if exclude.is_some_and(|e| {
                positions[r][0] >= e[0] as f32
                    && positions[r + 1][0] <= e[1] as f32
                    && positions[r][2] >= e[2] as f32
                    && positions[r + nx + 1][2] <= e[3] as f32
            }) {
                continue;
            }
            for tri in [[r, r + nx + 1, r + 1], [r + 1, r + nx + 1, r + nx + 2]] {
                indices.extend(tri.map(|i| i as u32));
                let polygon = clip_water(tri.map(|i| (Vec3::from_array(positions[i]), weights[i])));
                for i in 1..polygon.len().saturating_sub(1) {
                    for mut p in [polygon[0], polygon[i], polygon[i + 1]] {
                        p.y += 0.035;
                        water_positions.push(p.to_array());
                        water_normals.push(
                            Vec3::new(0.035 * (p.x * 1.7).sin(), 1., 0.025 * (p.z * 1.9).cos())
                                .normalize()
                                .to_array(),
                        );
                    }
                }
            }
        }
    }
    (
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(indices)),
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, water_positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, water_normals),
    )
}
#[cfg(test)]
fn ground_material(t: &Terrain, x: f32, z: f32) -> usize {
    if t.cell(x, z).water() {
        return 0;
    }
    if t.id == crate::training::ID {
        return if ((78. ..110.).contains(&x) && (48. ..264.).contains(&z))
            || ((110. ..312.).contains(&x)
                && [72., 120., 168., 216.]
                    .iter()
                    .any(|lane| (z - lane).abs() < 3.))
        {
            2
        } else {
            1
        };
    }
    if t.id == "forward-base" {
        let (mx, mz) = (x * 384. / t.cols as f32, z * 384. / t.rows as f32);
        return if crate::battlefield::road_distance(mx, mz) < 3.5 {
            2
        } else if crate::battlefield::field(mx, mz) {
            0
        } else {
            1
        };
    }
    match t.cell(x, z).kind {
        3 | 5 | 6 => 1,
        _ => 0,
    }
}
fn feather(a: f32, b: f32, x: f32) -> f32 {
    let u = ((x - a) / (b - a)).clamp(0., 1.);
    u * u * (3. - 2. * u)
}
/// Centimetre-scale render relief, independent of collision and navigation.
pub(crate) fn visual_height(t: &Terrain, x: f32, z: f32) -> f32 {
    let base = t.height(x, z);
    if t.cell(x, z).water() {
        return base;
    }
    let coverage = surface_blend(t, x, z, &[]);
    let mut mask = (1. - coverage[1]).powi(3) * (1. - coverage[0]);
    if t.id == crate::battlefield::ID {
        let p = crate::rural_layout::map_point(x, z, t.cols, t.rows);
        mask *= feather(
            -0.1,
            1.4,
            crate::rural_layout::surface_distance_for(p, t.cols, t.rows),
        ) * (1. - crate::rural_layout::ground_for(p, t.cols, t.rows).1);
        // Keep centimetre-scale grass relief below the existing base aprons.
        // It previously poked through their low edges as small jagged teeth.
        let apron = x.min(t.cols as f32 - x) - 135.;
        let end = (z - t.rows as f32 * 0.5).abs() - 164.;
        mask *= feather(-1., 3., apron.max(end));
        mask *= feather(
            0.,
            12.,
            x.min(z)
                .min((t.cols - 1) as f32 - x)
                .min((t.rows - 1) as f32 - z),
        );
    }
    let height = crate::ground_detail::scan_height(x, z);
    base + (height - 0.5) * 0.24 * mask
}
/// UV1 stores continuous dirt/gravel coverage, shared by every terrain surface batch.
fn surface_blend(t: &Terrain, x: f32, z: f32, towns: &[TownPaint]) -> [f32; 2] {
    if t.id == crate::gis_prototype::ID {
        // Actual OSM roads have continuous ribbon meshes; never paint the
        // unrelated procedural town streets underneath imported houses.
        return match crate::gis_prototype::mask(x, z)[0] {
            1 => [0.28, 0.],
            2 => [0.48, 0.],
            _ => [0.035, 0.],
        };
    }
    let patch = crate::natural_noise::value(x / 37., z / 37., 91);
    let mut dirt = 0.025 + feather(0.36, 0.79, patch) * 0.16;
    let mut gravel = 0.;
    if t.id == crate::battlefield::ID {
        // Fields are shaded analytically; roads have continuous surface meshes.
        // Never bake narrow painted strips into the four-metre overview grid.
        return [0.035, 0.];
    } else if t.id == crate::training::ID {
        let apron = (1. - feather(15., 19., (x - 94.).abs()))
            * (1. - feather(107., 112., (z - 156.).abs()));
        let lane = [72., 120., 168., 216.]
            .into_iter()
            .map(|lane| 1. - feather(2., 4.5, (z - lane).abs()))
            .fold(0., f32::max)
            * feather(107., 112., x)
            * (1. - feather(310., 315., x));
        gravel = apron.max(lane);
        if crate::ground_demo::photographic() {
            // A gritty verge beyond the road core, retaining the road's flat
            // surface and the ordinary simulation/navigation terrain.
            let distance = [72_f32, 120., 168., 216.]
                .into_iter()
                .map(|lane| (z - lane).abs())
                .fold(f32::MAX, f32::min)
                + (crate::natural_noise::value(x / 3.1, z / 3.1, 319) - 0.5) * 0.8;
            let extent = feather(107., 112., x) * (1. - feather(310., 315., x));
            dirt = dirt.max((1. - feather(3., 8., distance)) * extent);
        }
    } else {
        match t.cell(x, z).kind {
            // Biome 4 is natural plains, not a road. Only authored road masks
            // above should select asphalt or suppress the meadow normal.
            3 => dirt = dirt.max(0.42),
            5 | 6 => dirt = dirt.max(0.32),
            0..=2 => dirt = 1.,
            _ => {}
        }
    }
    for town in towns {
        let local = Vec2::new(x, z) - town.at;
        let street = town
            .roads
            .iter()
            .map(|r| 1. - feather(0., 4.5, r.distance(local.to_array()) + (patch - 0.5) * 0.65))
            .fold(0., f32::max);
        gravel = gravel.max(street);
        let yard = town
            .lots
            .iter()
            .map(|at| {
                let q = (local - *at).abs() - Vec2::splat(4.5);
                1. - feather(0., 5.5, q.max(Vec2::ZERO).length() + (patch - 0.5) * 1.2)
            })
            .fold(0., f32::max);
        dirt = dirt.max(yard * 0.38);
    }
    [dirt.clamp(0., 1.), gravel.clamp(0., 1.)]
}
#[cfg(test)]
fn ground_layers(land: Mesh, t: &Terrain) -> [Mesh; 3] {
    ground_layers_with_towns(land, t, &[])
}
#[derive(Clone, Debug)]
struct TownPaint {
    at: Vec2,
    roads: Vec<crate::settlement_layout::Road>,
    lots: Vec<Vec2>,
}
#[cfg(test)]
impl TownPaint {
    fn contains(&self, p: Vec2) -> bool {
        let p = p - self.at;
        self.roads.iter().any(|r| r.distance(p.to_array()) < 4.)
            || self
                .lots
                .iter()
                .any(|at| (p.x - at.x).abs() < 7. && (p.y - at.y).abs() < 7.)
    }
}
#[cfg(test)]
fn ground_layers_with_towns(mut land: Mesh, t: &Terrain, towns: &[TownPaint]) -> [Mesh; 3] {
    use bevy::mesh::VertexAttributeValues;
    let Some(VertexAttributeValues::Float32x3(positions)) =
        land.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return std::array::from_fn(|_| land.clone());
    };
    let mut groups: [Vec<u32>; 3] = std::array::from_fn(|_| vec![]);
    if let Some(indices) = land.indices() {
        let indices: Vec<_> = indices.iter().collect();
        for tri in indices.chunks_exact(3) {
            let p = (Vec3::from_array(positions[tri[0]])
                + Vec3::from_array(positions[tri[1]])
                + Vec3::from_array(positions[tri[2]]))
                / 3.;
            let layer = if towns.iter().any(|town| town.contains(Vec2::new(p.x, p.z))) {
                1
            } else {
                ground_material(t, p.x, p.z)
            };
            groups[layer].extend(tri.iter().map(|i| *i as u32));
        }
    }
    let blends: Vec<_> = positions
        .iter()
        .map(|p| surface_blend(t, p[0], p[2], towns))
        .collect();
    land.insert_attribute(Mesh::ATTRIBUTE_UV_1, blends);
    if let Some(VertexAttributeValues::Float32x2(uv)) = land.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for p in uv {
            p[0] *= 4.;
            p[1] *= 4.;
        }
    }
    std::array::from_fn(|i| {
        let mut mesh = land.clone();
        mesh.insert_indices(Indices::U32(std::mem::take(&mut groups[i])));
        mesh
    })
}

include!("terrain_stream.rs");
fn ray_interval(t: &Terrain, ray: Ray3d) -> Option<(f32, f32)> {
    // Picking rays come from the pinned downward-looking lens. Reject sky and
    // horizon rays before sampling any terrain; the render far plane is 8192m.
    if ray.direction.y >= -0.00001 {
        return None;
    }
    let scale = t.data.as_ref().map(|data| data.scale).unwrap_or_else(|| {
        if t.authored.is_empty() {
            160.
        } else {
            t.authored_scale
        }
    });
    let lower = Vec3::new(0., -8., 0.);
    let upper = Vec3::new((t.cols - 1) as f32, scale.max(0.) + 8., (t.rows - 1) as f32);
    let (mut near, mut far) = (0_f32, 8192_f32);
    for axis in 0..3 {
        let direction = ray.direction[axis];
        let origin = ray.origin[axis];
        if direction.abs() < 0.000001 {
            if origin < lower[axis] || origin > upper[axis] {
                return None;
            }
        } else {
            let a = (lower[axis] - origin) / direction;
            let b = (upper[axis] - origin) / direction;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if far < near {
                return None;
            }
        }
    }
    (far >= near && near.is_finite() && far.is_finite()).then_some((near, far))
}
pub fn intersect(t: &Terrain, ray: Ray3d) -> Option<Vec3> {
    let _timing = crate::performance::span("terrain.pick");
    let (near, far) = ray_interval(t, ray)?;
    let first = ray.get_point(near);
    if first.y <= t.height(first.x, first.z) {
        return Some(first);
    }
    // The air above the height slab and portions outside the map have already
    // been removed. Most camera rays now need tens of samples, not thousands.
    // Half-metre horizontal samples preserve the nearest foreground crossing;
    // the upper bound also keeps a pathological shallow ray's work finite.
    let horizontal = Vec2::new(ray.direction.x, ray.direction.z).length();
    let spacing = (0.5 / horizontal.max(0.01)).clamp(0.5, 4.);
    let steps = ((far - near) / spacing).ceil().clamp(1., 2048.) as usize;
    let mut previous = near;
    for i in 1..=steps {
        let d = near + (far - near) * i as f32 / steps as f32;
        let p = ray.get_point(d);
        if p.y <= t.height(p.x, p.z) {
            let (mut lo, mut hi) = (previous, d);
            for _ in 0..12 {
                let mid = (lo + hi) * 0.5;
                let p = ray.get_point(mid);
                if p.y > t.height(p.x, p.z) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return Some(ray.get_point(hi));
        }
        previous = d;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    fn picking_terrain(size: i32) -> Terrain {
        let mut t = Terrain::new("pick-test", size, size);
        t.authored = vec![0.56; (size * size) as usize];
        t.authored_scale = 32.;
        t.hydrate();
        t
    }
    fn pick_ray(from: Vec3, to: Vec3) -> Ray3d {
        Ray3d::new(from, Dir3::new(to - from).unwrap())
    }
    #[test]
    fn maximum_zoom_picking_reaches_the_center_beyond_the_old_3200m_limit() {
        let t = picking_terrain(1024);
        let center = Vec3::new(512., t.height(512., 512.), 512.);
        let eye = crate::fixed_view::transform(center, 1536.).translation;
        assert!(eye.distance(center) > 3200.);
        let ray = pick_ray(eye, center);
        let (near, far) = ray_interval(&t, ray).unwrap();
        assert!(near > 3200.);
        assert!(
            far - near < 80.,
            "only the bounded ground-height slab should be sampled"
        );
        let point = intersect(&t, ray).unwrap();
        assert!(point.distance(center) < 0.01, "{point:?} != {center:?}");
    }
    #[test]
    fn vertical_picks_are_exact_and_sky_horizon_and_off_map_rays_return_none() {
        let t = picking_terrain(128);
        let point = intersect(&t, Ray3d::new(Vec3::new(40., 100., 70.), Dir3::NEG_Y)).unwrap();
        assert!(point.distance(Vec3::new(40., t.height(40., 70.), 70.)) < 0.001);
        for ray in [
            Ray3d::new(Vec3::new(-1., 100., 70.), Dir3::NEG_Y),
            Ray3d::new(Vec3::new(128., 100., 70.), Dir3::NEG_Y),
            Ray3d::new(Vec3::new(40., 100., 70.), Dir3::Y),
            Ray3d::new(Vec3::new(40., 100., 70.), Dir3::X),
        ] {
            assert!(intersect(&t, ray).is_none());
        }
    }
    #[test]
    fn clipped_picks_match_a_dense_nearest_surface_reference_on_slopes() {
        let mut t = picking_terrain(256);
        for z in 0..256 {
            for x in 0..256 {
                t.authored[(z * 256 + x) as usize] =
                    0.52 + x as f32 * 0.0004 + z as f32 * 0.0003 + (x as f32 * 0.09).sin() * 0.025;
            }
        }
        for (from, to) in [
            (Vec3::new(220., 100., 20.), Vec3::new(30., 0., 190.)),
            (Vec3::new(-20., 120., 80.), Vec3::new(200., 0., 160.)),
            (Vec3::new(128., 300., 128.), Vec3::new(160., 0., 160.)),
        ] {
            let ray = pick_ray(from, to);
            let expected = (0..32768)
                .map(|i| ray.get_point(i as f32 * 0.25))
                .find(|p| {
                    p.x >= 0.
                        && p.z >= 0.
                        && p.x <= 255.
                        && p.z <= 255.
                        && p.y <= t.height(p.x, p.z)
                })
                .unwrap();
            let actual = intersect(&t, ray).unwrap();
            assert!(
                actual.distance(expected) < 0.251,
                "{actual:?} != {expected:?}"
            );
            assert!((actual.y - t.height(actual.x, actual.z)).abs() < 0.001);
        }
    }
    #[test]
    fn full_map_panning_keeps_coarse_tiles_and_near_detail_is_bounded() {
        let t = Terrain::new(crate::battlefield::ID, 1024, 1024);
        let tiles: Vec<_> = tile_coords(&t).collect();
        assert_eq!(tiles.len(), 256);
        for x in [0., 150., 512., 850., 1024.] {
            assert!(tiles.iter().all(|at| !tile_wants_fine(
                &t,
                *at,
                Vec3::new(x, 0., 1024. - x),
                true,
                crate::graphics::Detail::High
            )));
            assert!(
                tiles
                    .iter()
                    .filter(|at| tile_wants_fine(
                        &t,
                        **at,
                        Vec3::new(x, 0., 1024. - x),
                        false,
                        crate::graphics::Detail::Low
                    ))
                    .count()
                    <= 16
            );
        }
        assert_eq!(tile_bounds(&t, IVec2::new(15, 15)), [960, 1023, 960, 1023]);
    }
    #[test]
    fn single_ground_buffer_preserves_all_triangles_normals_and_blend_weights() {
        let t = Terrain::new(crate::training::ID, 384, 384);
        let (land, _) = mesh(&t, [90, 120, 60, 90], 1., None, None);
        let layers = ground_layers(land.clone(), &t);
        let merged = prepare_ground(land.clone(), &t, &[]);
        assert_eq!(merged.count_vertices(), land.count_vertices());
        assert_eq!(
            layers.iter().map(Mesh::count_vertices).sum::<usize>(),
            merged.count_vertices() * 3
        );
        assert_eq!(
            merged.indices().unwrap().len(),
            layers
                .iter()
                .map(|m| m.indices().unwrap().len())
                .sum::<usize>()
        );
        for attribute in [
            Mesh::ATTRIBUTE_NORMAL,
            Mesh::ATTRIBUTE_POSITION,
            Mesh::ATTRIBUTE_UV_0,
            Mesh::ATTRIBUTE_UV_1,
        ] {
            assert_eq!(merged.attribute(attribute), layers[0].attribute(attribute));
        }
    }
    #[test]
    fn natural_plains_keep_relief_and_prepared_training_lanes_stay_flat() {
        let terrain = Terrain::new("river-crossing", 128, 128);
        assert_eq!(terrain.cell(16., 16.).kind, 4);
        assert_ne!(ground_material(&terrain, 16., 16.), 2);
        assert_eq!(surface_blend(&terrain, 16., 16., &[])[1], 0.);
        assert!((visual_height(&terrain, 16., 16.) - terrain.height(16., 16.)).abs() > 0.00001);
        let training = Terrain::new(crate::training::ID, 384, 384);
        assert_eq!(surface_blend(&training, 90., 100., &[])[1], 1.);
        assert_eq!(
            visual_height(&training, 90., 100.),
            training.height(90., 100.)
        );
    }
    #[test]
    fn training_material_layers_keep_every_triangle_and_the_same_terrain_surface() {
        let terrain = Terrain::new(crate::training::ID, 384, 384);
        assert_eq!(ground_material(&terrain, 90., 100.), 2);
        assert_eq!(ground_material(&terrain, 140., 120.), 2);
        assert_eq!(ground_material(&terrain, 140., 140.), 1);
        let (land, _) = mesh(&terrain, [90, 120, 60, 90], 1., None, None);
        let mut original: Vec<_> = land.indices().unwrap().iter().collect();
        let layers = ground_layers(land.clone(), &terrain);
        assert!(!layers[1].indices().unwrap().is_empty());
        assert!(!layers[2].indices().unwrap().is_empty());
        let mut split: Vec<_> = layers
            .iter()
            .flat_map(|l| l.indices().unwrap().iter())
            .collect();
        original.sort_unstable();
        split.sort_unstable();
        assert_eq!(original, split);
        for layer in layers {
            assert_eq!(
                layer.attribute(Mesh::ATTRIBUTE_POSITION),
                land.attribute(Mesh::ATTRIBUTE_POSITION)
            );
            assert_eq!(
                layer.attribute(Mesh::ATTRIBUTE_NORMAL),
                land.attribute(Mesh::ATTRIBUTE_NORMAL)
            );
        }
    }
    #[test]
    fn shoreline_clipping_keeps_winding_and_intersections() {
        let polygon = clip_water([(Vec3::ZERO, 1.), (Vec3::Z, 0.), (Vec3::X, 0.)]);
        assert_eq!(polygon.len(), 3);
        assert!(
            polygon
                .iter()
                .any(|p| (*p - Vec3::X * 0.5).length() < 0.00001)
        );
        assert!((polygon[1] - polygon[0]).cross(polygon[2] - polygon[0]).y > 0.);
    }
    #[test]
    fn terrain_uses_shared_vertices_and_unit_normals() {
        let terrain = Terrain::default();
        let (land, _) = mesh(&terrain, [0, 8, 0, 8], 0.5, None, None);
        assert_eq!(land.count_vertices(), 17 * 17);
        assert_eq!(land.indices().unwrap().len(), 16 * 16 * 6);
        let bevy::mesh::VertexAttributeValues::Float32x3(normals) =
            land.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
        else {
            panic!("normal attribute missing")
        };
        assert!(
            normals
                .iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.).abs() < 0.00001)
        );
    }
}
