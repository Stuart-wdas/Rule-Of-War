// Included by surfaces.rs. Albedo-only overrides are scoped to vehicle materials.
pub const AIRCRAFT_DETAIL_NAMES: [&str; 4] = [
    "aircraft_grey", "aircraft_sand", "helicopter_green", "helicopter_sand",
];
pub const VEHICLE_DETAIL_NAMES: [&str; 16] = [
    "armour_green", "armour_sand", "steel", "dark_fittings", "rubber", "canvas",
    "brass", "rust", "aircraft_grey", "aircraft_sand", "helicopter_green",
    "helicopter_sand", "a10_livery", "vehicle_glass", "cockpit_glass", "lights",
];
#[derive(Default)]
pub struct VehicleTextures {
    albedos: std::collections::BTreeMap<&'static str, Handle<Image>>,
}
impl VehicleTextures {
    pub fn load(images: &mut Assets<Image>) -> Self {
        if std::env::args().any(|a| a == "--legacy-vehicle-materials") {
            return Self::default();
        }
        Self::load_from(std::path::Path::new("materials/vehicle_detail"), images)
    }
    fn load_from(root: &std::path::Path, images: &mut Assets<Image>) -> Self {
        let albedos = VEHICLE_DETAIL_NAMES.into_iter().filter_map(|name| {
            read_texture(root, name, "albedo", true).map(|image| (name, images.add(image)))
        }).collect();
        Self { albedos }
    }
    pub fn count(&self) -> usize { self.albedos.len() }
    pub fn apply(&self, name: &str, material: &mut StandardMaterial) -> bool {
        let Some(handle) = self.albedos.get(name) else { return false; };
        // These exported/edited maps already contain the final faction colour.
        // Preserve UVs, normals and all lighting parameters; avoid a second tint.
        material.base_color = Color::WHITE;
        material.base_color_texture = Some(handle.clone());
        true
    }
    pub fn apply_finish(&self, index: usize, material: &mut StandardMaterial) -> bool {
        use crate::models::Finish as F;
        for (finish, name) in [
            (F::Dark, "dark_fittings"), (F::Steel, "steel"), (F::Cloth, "canvas"),
            (F::Glass, "vehicle_glass"), (F::Light, "lights"), (F::Brass, "brass"),
            (F::Rust, "rust"), (F::Rubber, "rubber"), (F::AircraftGlass, "cockpit_glass"),
            (F::A10Livery, "a10_livery"),
        ] {
            if finish as usize + 2 == index { return self.apply(name, material); }
        }
        false
    }
}

#[cfg(test)]
mod vehicle_texture_tests {
    use super::*;
    #[test]
    fn approved_colour_maps_keep_uvs_normal_maps_and_metal_response_without_double_tinting() {
        let mut images = Assets::<Image>::default();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../materials/vehicle_detail");
        let detail = VehicleTextures::load_from(&root, &mut images);
        assert_eq!(detail.count(), 16);
        let mut material = material("steel", Color::srgb(0.43, 0.46, 0.47), &mut images);
        material.perceptual_roughness = 0.55;
        material.metallic = 1.;
        let before = material.clone();
        assert!(detail.apply_finish(crate::models::Finish::Steel as usize + 2, &mut material));
        assert_eq!(material.base_color, Color::WHITE);
        assert_ne!(material.base_color_texture, before.base_color_texture);
        assert_eq!(material.uv_transform, before.uv_transform);
        assert_eq!(material.normal_map_texture, before.normal_map_texture);
        assert_eq!(material.metallic_roughness_texture, before.metallic_roughness_texture);
        assert_eq!(material.perceptual_roughness, before.perceptual_roughness);
        assert_eq!(material.metallic, before.metallic);
        let albedo = images.get(material.base_color_texture.as_ref().unwrap()).unwrap();
        assert_eq!([albedo.width(), albedo.height()], [1254, 1254]);
        assert_eq!(albedo.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert!(albedo.texture_descriptor.mip_level_count > 1);
        assert!(!detail.apply_finish(crate::models::Finish::Concrete as usize + 2, &mut material));
        assert!(!VehicleTextures::default().apply("steel", &mut material));
        assert_eq!(material.base_color, Color::WHITE);
    }
}
