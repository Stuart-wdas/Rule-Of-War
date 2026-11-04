//! Measured render relief and bounded, world-anchored vehicle wear.
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::collections::{HashMap, VecDeque};

const HEIGHT: &[u8] = include_bytes!("../../materials/pbr/meadow_relief_v1_height.bin");
pub fn scan_height(x: f32, z: f32) -> f32 {
    let u = x.rem_euclid(90.) / 90. * 512.;
    let v = z.rem_euclid(90.) / 90. * 512.;
    let at = |a: usize, b: usize| HEIGHT[(b % 512) * 512 + a % 512] as f32 / 255.;
    let (a, b) = (u.floor() as usize, v.floor() as usize);
    let row = |y| at(a, y) + (at(a + 1, y) - at(a, y)) * u.fract();
    row(b) + (row(b + 1) - row(b)) * v.fract()
}
#[derive(Clone, Copy)]
struct Stamp {
    a: Vec2,
    b: Vec2,
    width: f32,
    strength: f32,
}
#[derive(Resource)]
pub struct GroundWear {
    pub image: Handle<Image>,
    pub extent: Vec2,
    size: u32,
    map: String,
    tick: u64,
    elapsed: f32,
    previous: HashMap<String, (Vec2, f32)>,
    stamps: VecDeque<Stamp>,
}
impl GroundWear {
    pub fn stamp_count(&self) -> usize {
        self.stamps.len()
    }
    pub fn bounds(&self) -> Option<[f32; 4]> {
        self.stamps.front().map(|first| {
            let mut low = first.a;
            let mut high = first.a;
            for s in &self.stamps {
                low = low.min(s.a).min(s.b);
                high = high.max(s.a).max(s.b);
            }
            [low.x, low.y, high.x, high.y]
        })
    }
}
fn mask(size: u32) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; (size * size) as usize],
        TextureFormat::R8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    graphics: Res<crate::graphics::Graphics>,
    game: Res<crate::simulation::NativeGame>,
) {
    let size = graphics.settings.environment.texture_size().min(2048);
    commands.insert_resource(GroundWear {
        image: images.add(mask(size)),
        extent: Vec2::new(game.terrain.cols as f32, game.terrain.rows as f32),
        size,
        map: String::new(),
        tick: 0,
        elapsed: 0.,
        previous: HashMap::new(),
        stamps: VecDeque::new(),
    });
}
fn paint(data: &mut [u8], size: u32, extent: Vec2, s: Stamp) {
    let pixel = extent / size as f32;
    let radius = s.width * 0.5 + pixel.max_element() * 0.35;
    let low = ((s.a.min(s.b) - Vec2::splat(radius)) / pixel)
        .floor()
        .max(Vec2::ZERO);
    let high = ((s.a.max(s.b) + Vec2::splat(radius)) / pixel)
        .ceil()
        .min(Vec2::splat(size as f32 - 1.));
    let delta = s.b - s.a;
    let length = delta.length();
    if length < 0.001 {
        return;
    }
    for y in low.y as u32..=high.y as u32 {
        for x in low.x as u32..=high.x as u32 {
            let p = (Vec2::new(x as f32, y as f32) + Vec2::splat(0.5)) * pixel;
            let u = ((p - s.a).dot(delta) / delta.length_squared()).clamp(0., 1.);
            let distance = p.distance(s.a + delta * u);
            let edge = (1. - distance / radius).clamp(0., 1.);
            let tread = 0.82 + 0.18 * (u * length * 22.).cos();
            let value = (255. * s.strength * edge.sqrt() * tread) as u8;
            let index = (y * size + x) as usize;
            data[index] = data[index].max(value);
        }
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects presentation state and asset collections"
)]
pub fn update(
    mut wear: ResMut<GroundWear>,
    game: Res<crate::simulation::NativeGame>,
    battle: Res<crate::runtime::Battle>,
    graphics: Res<crate::graphics::Graphics>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<crate::grass_material::GrassMaterial>>,
    mut growth: ResMut<Assets<crate::grass_material::GrowthMaterial>>,
    nature: Res<crate::landscape::Nature>,
) {
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    let size = graphics.settings.environment.texture_size().min(2048);
    let reset = wear.map != map || battle.snapshot.tick < wear.tick;
    let resized = wear.size != size;
    if reset {
        wear.previous.clear();
        wear.stamps.clear();
        wear.map = map;
    }
    wear.tick = battle.snapshot.tick;
    wear.extent = Vec2::new(game.terrain.cols as f32, game.terrain.rows as f32);
    let mut rebuild = reset || resized;
    if rebuild {
        wear.size = size;
        *images.get_mut(&wear.image).unwrap() = mask(size);
    }
    if reset || resized {
        let extent = Vec4::new(1. / wear.extent.x, 1. / wear.extent.y, 0., 0.);
        if let Some(material) = materials.get_mut(&nature.grass) {
            material.extension.track_world = extent;
        }
        if let Some(material) = growth.get_mut(&nature.low_growth) {
            material.extension.track_world = extent;
        }
    }
    wear.elapsed += time.delta_secs();
    if !reset && !resized && (wear.elapsed < 0.12 || battle.snapshot.paused) {
        return;
    }
    wear.elapsed = 0.;
    let mut additions = vec![];
    let mut present = std::collections::HashSet::new();
    for p in battle
        .snapshot
        .entities
        .iter()
        .filter(|p| p.category == "vehicle" && !p.building && !p.airborne && p.hp > 0.)
    {
        present.insert(p.id.clone());
        let at = Vec2::new(p.x, p.z);
        if let Some(&(before, heading)) = wear.previous.get(&p.id) {
            let distance = at.distance(before);
            if distance < 0.45 {
                continue;
            }
            if distance < 12. && !game.terrain.cell(p.x, p.z).water() {
                let half = 0.82 * p.model_transform_scale().x;
                for side in [-1., 1.] {
                    let offset = |h: f32| Vec2::new(-h.sin(), h.cos()) * half * side;
                    additions.push(Stamp {
                        a: before + offset(heading),
                        b: at + offset(p.heading),
                        width: 0.34 * p.model_scale,
                        strength: 0.95,
                    });
                }
            }
        }
        wear.previous.insert(p.id.clone(), (at, p.heading));
    }
    wear.previous.retain(|id, _| present.contains(id));
    let limit = match graphics.settings.environment {
        crate::graphics::Detail::Low => 2048,
        crate::graphics::Detail::Balanced => 6144,
        crate::graphics::Detail::High => 12288,
    };
    wear.stamps.extend(additions.iter().copied());
    if wear.stamps.len() > limit {
        while wear.stamps.len() > limit.saturating_sub(128) {
            wear.stamps.pop_front();
        }
        rebuild = true;
    }
    if rebuild || !additions.is_empty() {
        let image = images.get_mut(&wear.image).unwrap();
        let data = image.data.as_mut().unwrap();
        if rebuild {
            data.fill(0);
            for s in &wear.stamps {
                paint(data, size, wear.extent, *s);
            }
        } else {
            for s in additions {
                paint(data, size, wear.extent, s);
            }
        }
        // Version the handle: bindless material texture slots are keyed by
        // image ID, so modifying pixels under the original ID can retain an
        // old GPU view. Only paint updates allocate a replacement R8 image.
        let replacement = image.clone();
        wear.image = images.add(replacement);
        if let Some(material) = materials.get_mut(&nature.grass) {
            material.extension.tracks = wear.image.clone();
            material.extension.track_world.w = wear.stamps.len() as f32;
        }
        if let Some(material) = growth.get_mut(&nature.low_growth) {
            material.extension.tracks = wear.image.clone();
            material.extension.track_world.w = wear.stamps.len() as f32;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_relief_wraps_without_a_height_jump() {
        assert_eq!(HEIGHT.len(), 512 * 512);
        for x in [-91., -0.01, 0., 17.4, 89.99, 180.] {
            assert!((scan_height(x, 12.) - scan_height(x + 90., 12.)).abs() < 0.0001);
        }
    }
    #[test]
    fn paired_tracks_keep_the_middle_clear_and_raster_stays_bounded() {
        let mut data = vec![0; 128 * 128];
        for z in [3., 5.] {
            paint(
                &mut data,
                128,
                Vec2::splat(16.),
                Stamp {
                    a: Vec2::new(2., z),
                    b: Vec2::new(14., z),
                    width: 0.4,
                    strength: 0.8,
                },
            );
        }
        assert!(data.iter().any(|v| *v > 100));
        assert_eq!(data[32 * 128 + 64], 0);
        paint(
            &mut data,
            128,
            Vec2::splat(16.),
            Stamp {
                a: Vec2::splat(-20.),
                b: Vec2::splat(40.),
                width: 0.5,
                strength: 1.,
            },
        );
    }
}
