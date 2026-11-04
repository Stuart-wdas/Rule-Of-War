//! Slow, seamless cloud shadows projected by the existing sun onto PBR surfaces.
//! A single static texture moves in light space; terrain/roads need no overlays.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    light::DirectionalLightTexture,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    transform::TransformSystems,
};

const PERIOD: f32 = 768.;
const SIZE: u32 = 512;
const MIN_SUN_TRANSMISSION: f32 = 0.45;
const WIND: Vec3 = Vec3::new(1.65, 0., 0.58);

#[derive(Resource)]
pub struct CloudShadows {
    enabled: bool,
    seconds: f64,
    review_time: Option<f64>,
}
impl Default for CloudShadows {
    fn default() -> Self {
        let args: Vec<_> = std::env::args().collect();
        Self {
            enabled: !args
                .iter()
                .any(|a| a == "--no-cloud-shadows" || a == "--no-shadows"),
            seconds: 0.,
            review_time: args
                .iter()
                .position(|a| a == "--cloud-time")
                .and_then(|i| args.get(i + 1))
                .and_then(|a| a.parse::<f64>().ok())
                .filter(|s| s.is_finite() && *s >= 0.),
        }
    }
}
impl CloudShadows {
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"enabled":self.enabled,"seconds":self.seconds,
            "windMetersPerSecond":WIND.to_array(),"lightPlanePeriod":PERIOD,
            "texturePixels":SIZE,"minimumSunTransmission":MIN_SUN_TRANSMISSION})
    }
    fn advance(&mut self, delta: f64, paused: bool) {
        if let Some(seconds) = self.review_time {
            self.seconds = seconds;
        } else if self.enabled && !paused {
            self.seconds += delta.clamp(0., 0.1);
        }
    }
}
fn lattice(x: i32, y: i32, period: i32, seed: u32) -> f32 {
    let mut n = (x.rem_euclid(period) as u32).wrapping_mul(374761393)
        ^ (y.rem_euclid(period) as u32).wrapping_mul(668265263)
        ^ seed;
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) & 65535) as f32 / 65535.
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * t * (t * (t * 6. - 15.) + 10.)
}
fn noise(uv: Vec2, period: i32, seed: u32) -> f32 {
    let p = uv * period as f32;
    let cell = p.floor().as_ivec2();
    let f = p - p.floor();
    let x = smooth(f.x);
    let y = smooth(f.y);
    let a = lattice(cell.x, cell.y, period, seed);
    let b = lattice(cell.x + 1, cell.y, period, seed);
    let c = lattice(cell.x, cell.y + 1, period, seed);
    let d = lattice(cell.x + 1, cell.y + 1, period, seed);
    (a + (b - a) * x) * (1. - y) + (c + (d - c) * x) * y
}
fn transmission(uv: Vec2) -> f32 {
    let broad = noise(uv, 4, 8471) * 0.56
        + noise(uv, 8, 3917) * 0.28
        + noise(uv, 16, 1771) * 0.12
        + noise(uv, 32, 6337) * 0.04;
    1. - (1. - MIN_SUN_TRANSMISSION) * smooth((broad - 0.40) / 0.31)
}
fn texture() -> Image {
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let t =
                (transmission(Vec2::new(x as f32, y as f32) / SIZE as f32) * 255.).round() as u8;
            data.extend_from_slice(&[t, t, t, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
fn position(rotation: Quat, seconds: f64) -> Vec3 {
    let local = rotation.inverse() * WIND;
    let shift = Vec2::new(
        (121. + local.x as f64 * seconds).rem_euclid(PERIOD as f64) as f32,
        (317. + local.y as f64 * seconds).rem_euclid(PERIOD as f64) as f32,
    );
    rotation * shift.extend(0.)
}
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    cloud: Res<CloudShadows>,
    mut suns: Query<(Entity, &mut Transform), With<crate::lighting::Sun>>,
) {
    if !cloud.enabled {
        return;
    }
    let image = images.add(texture());
    for (entity, mut transform) in &mut suns {
        // Bevy maps local light coordinates -1..1 across the image. Its sun
        // cascade code deliberately uses rotation only, so object shadows keep
        // their existing directions, extents and stable cascade texels.
        transform.scale = Vec3::new(PERIOD * 0.5, PERIOD * 0.5, 1.);
        transform.translation = position(transform.rotation, cloud.review_time.unwrap_or(0.));
        commands.entity(entity).insert(DirectionalLightTexture {
            image: image.clone(),
            tiled: true,
        });
    }
}
fn update(
    time: Res<Time>,
    game: Res<crate::simulation::NativeGame>,
    menu: Res<crate::menu::MainMenu>,
    mut cloud: ResMut<CloudShadows>,
    mut suns: Query<&mut Transform, With<crate::lighting::Sun>>,
) {
    cloud.advance(time.delta_secs_f64(), game.paused || menu.open);
    if !cloud.enabled {
        return;
    }
    for mut transform in &mut suns {
        let next = position(transform.rotation, cloud.seconds);
        if transform.translation != next {
            transform.translation = next;
        }
    }
}
pub fn register(app: &mut App) {
    app.init_resource::<CloudShadows>()
        .add_systems(PostStartup, setup)
        .add_systems(PostUpdate, update.before(TransformSystems::Propagate));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_field_has_no_tile_seams_and_never_blacks_out_the_map() {
        let mut low = 1f32;
        let mut high = 0f32;
        for y in 0..64 {
            for x in 0..64 {
                let uv = Vec2::new(x as f32 / 64., y as f32 / 64.);
                let a = transmission(uv);
                assert!((MIN_SUN_TRANSMISSION - 0.0001..=1.).contains(&a));
                assert!((a - transmission(uv + Vec2::new(1., -1.))).abs() < 0.00001);
                low = low.min(a);
                high = high.max(a);
            }
        }
        assert!(low < 0.55 && high > 0.95);
    }
    #[test]
    fn wind_is_world_anchored_and_pauses_without_changing_sun_direction() {
        let mut cloud = CloudShadows {
            enabled: true,
            seconds: 0.,
            review_time: None,
        };
        cloud.advance(0.1, false);
        assert_eq!(cloud.seconds, 0.1);
        cloud.advance(0.1, true);
        assert_eq!(cloud.seconds, 0.1);
        let rotation = crate::lighting::sun_transform().rotation;
        assert!(position(rotation, 0.).distance(position(rotation, 30.)) > 30.);
        let a = rotation.inverse() * position(rotation, 0.);
        let b = rotation.inverse() * position(rotation, 30.);
        let expected = rotation.inverse() * WIND * 30.;
        assert!((Vec2::new(b.x - a.x, b.y - a.y) - expected.truncate()).length() < 0.001);
        let image = texture();
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    }
}
