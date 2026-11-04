//! The offline volume lighting direction, with a warm afternoon world treatment.
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    light::EnvironmentMapLight,
    render::render_resource::{
        Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
        TextureViewDimension,
    },
};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
pub struct Profile {
    pub sun_linear: [f32; 3],
    pub sun_ray_blender: [f32; 3],
    pub sun_energy: f32,
    pub sun_lux: f32,
    pub sky_linear: [f32; 3],
    pub ambient_brightness: f32,
    pub fill_linear: [f32; 3],
    pub fill_energy: f32,
}
#[derive(Component)]
pub struct Sun;
#[derive(Component)]
pub struct Fill;

pub fn profile() -> &'static Profile {
    static PROFILE: OnceLock<Profile> = OnceLock::new();
    PROFILE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../assets/Realistic_Explosion_VFX/lighting.json"
        ))
        .expect("bundled world/VFX lighting profile")
    })
}
fn linear(c: [f32; 3]) -> Color {
    Color::linear_rgb(c[0], c[1], c[2])
}
pub fn sun() -> DirectionalLight {
    let p = profile();
    // Keep the baked shadow direction. Warmer direct light and less camera-side
    // fill separate sunlit surfaces from cool shade without a yellow screen tint.
    let c = Vec3::from_array(p.sun_linear) * Vec3::new(1., 0.94, 0.85);
    DirectionalLight {
        color: linear(c.to_array()),
        illuminance: p.sun_lux * 1.16,
        shadow_normal_bias: 1.15,
        shadows_enabled: !std::env::args().any(|a| a == "--no-shadows"),
        ..default()
    }
}
pub fn sun_transform() -> Transform {
    let [x, y, z] = profile().sun_ray_blender;
    Transform::default().looking_to(Vec3::new(x, z, y).normalize(), Vec3::Y)
}
pub fn fill() -> DirectionalLight {
    let p = profile();
    DirectionalLight {
        color: linear(p.fill_linear),
        illuminance: p.sun_lux * p.fill_energy / p.sun_energy * 0.70,
        shadows_enabled: false,
        ..default()
    }
}
pub fn ambient() -> GlobalAmbientLight {
    GlobalAmbientLight {
        color: linear(profile().sky_linear),
        brightness: profile().ambient_brightness * 0.90,
        ..default()
    }
}

pub fn grading() -> ColorGrading {
    // This same mild filmic response covers world surfaces and HDR explosions.
    // The sun supplies the warmth; neutral white balance preserves smoke colour.
    ColorGrading {
        global: ColorGradingGlobal {
            exposure: -0.02,
            post_saturation: 1.0,
            ..default()
        },
        midtones: ColorGradingSection {
            contrast: 1.04,
            ..default()
        },
        ..default()
    }
}
/// A thin layer of clear-air haze. Its opacity is capped even on an ultrawide
/// display; a distant RTS eye must never make the whole battlefield opaque.
pub fn atmosphere(depths: [f32; 2]) -> DistanceFog {
    let distance = (depths[0] + depths[1]) * 0.5;
    let opacity = 0.025 + ((distance - 220.) / 3000.).clamp(0., 1.) * 0.055;
    DistanceFog {
        color: Color::srgba(0.62, 0.69, 0.72, opacity),
        directional_light_color: Color::NONE,
        falloff: FogFalloff::Linear {
            start: depths[0].max(0.),
            end: depths[1].max(depths[0] + 1.) * 1.12 + 16.,
        },
        ..default()
    }
}
fn probe_image(size: u32, mips: u32, data: &[u8]) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        vec![0; (size * size * 6 * 8) as usize],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = mips;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.data_order = TextureDataOrder::LayerMajor;
    image.sampler = ImageSampler::linear();
    image.data = Some(data.to_vec());
    image
}
pub fn environment(images: &mut Assets<Image>) -> EnvironmentMapLight {
    EnvironmentMapLight {
        diffuse_map: images.add(probe_image(
            8,
            1,
            include_bytes!("../../materials/lighting/afternoon_diffuse.rgba16f"),
        )),
        specular_map: images.add(probe_image(
            64,
            7,
            include_bytes!("../../materials/lighting/afternoon_specular.rgba16f"),
        )),
        // The existing sky probe retains cool open shade. Less uniform sky
        // energy lets the restored cast shadows describe roofs and vegetation.
        intensity: 220.,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tactical_atmosphere_remains_clear_at_all_playing_distances() {
        for depths in [[25., 48.], [220., 450.], [900., 1400.], [2600., 4000.]] {
            let fog = atmosphere(depths);
            assert!(fog.color.alpha() <= 0.08001);
            let FogFalloff::Linear { start, end } = fog.falloff else {
                panic!("bounded haze");
            };
            assert!(start >= depths[0] && end > depths[1]);
        }
    }
    #[test]
    fn new_daylight_bakes_match_effective_warm_map_lighting() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/Realistic_Explosion_VFX/daylight_impacts");
        for family in ["debris", "mushroom", "shell", "rocket"] {
            let data = std::fs::read_to_string(base.join(family).join("animation.json")).unwrap();
            let meta: serde_json::Value = serde_json::from_str(&data).unwrap();
            let light = &meta["lighting"];
            let color = sun().color.to_linear().to_f32_array();
            for (i, channel) in color.iter().take(3).enumerate() {
                assert!(
                    (light["sun_linear"][i].as_f64().unwrap() as f32 - channel).abs() < 0.00001
                );
            }
            assert!((light["sun_lux"].as_f64().unwrap() as f32 - sun().illuminance).abs() < 0.01);
            assert!(
                (light["ambient_brightness"].as_f64().unwrap() as f32 - ambient().brightness).abs()
                    < 0.01
            );
            assert!(
                (light["fill_energy"].as_f64().unwrap() as f32
                    / light["sun_energy"].as_f64().unwrap() as f32
                    - fill().illuminance / sun().illuminance)
                    .abs()
                    < 0.00001
            );
            assert_eq!(meta["simulated_heat"], true);
            assert_eq!(meta["mirror_u"], true);
        }
    }
    #[test]
    fn daylight_probe_has_complete_linear_hdr_cube_faces_and_mips() {
        let data = include_bytes!("../../materials/lighting/afternoon_specular.rgba16f");
        assert_eq!(
            data.len(),
            (0..7).map(|m| (64usize >> m).pow(2) * 6 * 8).sum::<usize>()
        );
        for pixel in data.chunks_exact(8) {
            for c in pixel.chunks_exact(2) {
                assert_ne!((u16::from_le_bytes([c[0], c[1]]) >> 10) & 31, 31);
            }
            assert_eq!(&pixel[6..8], &[0, 60]);
        }
        let image = probe_image(64, 7, data);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(image.texture_descriptor.mip_level_count, 7);
    }
    #[test]
    fn warm_world_preserves_the_shipped_volume_direction_and_cool_sky() {
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../../assets/Realistic_Explosion_VFX/lighting.json"
        ))
        .unwrap();
        for variant in ["mushroom", "debris"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../assets/Realistic_Explosion_VFX/locked_view_hd")
                .join(variant)
                .join("animation.json");
            let metadata = std::fs::read_to_string(path).unwrap();
            let baked: serde_json::Value = serde_json::from_str(&metadata).unwrap();
            assert_eq!(baked["lighting"], source);
        }
        let [x, y, z] = profile().sun_ray_blender;
        assert!(
            sun_transform()
                .forward()
                .as_vec3()
                .distance(Vec3::new(x, z, y).normalize())
                < 0.0001
        );
        assert_eq!(
            sun().color.to_linear().to_f32_array()[..3],
            (Vec3::from_array(profile().sun_linear) * Vec3::new(1., 0.94, 0.85)).to_array()
        );
        assert_eq!(
            ambient().color.to_linear().to_f32_array()[..3],
            profile().sky_linear
        );
        assert!(!fill().shadows_enabled);
        assert!(
            (fill().illuminance / sun().illuminance
                - profile().fill_energy / profile().sun_energy * 0.70 / 1.16)
                .abs()
                < 0.0001
        );
    }
}
