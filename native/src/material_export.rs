//! Export runtime texture pixels and material factors without starting the renderer.
use bevy::prelude::*;
use serde_json::json;
use std::path::Path;

pub fn export(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut images = Assets::<Image>::default();
    let mut sets = vec![];
    for (owner, name) in [(0, "armour_green"), (1, "armour_sand")] {
        sets.push((name, crate::surfaces::armour(owner, &mut images)));
    }
    // Match the current scene's finish overrides. Export the actual generated
    // relief maps too, rather than substituting photographic lookalikes.
    for (name, source, tint, roughness, metallic, reflectance) in [
        ("steel", "steel", [0.43, 0.46, 0.47], 0.55, 1., 0.50),
        (
            "dark_fittings",
            "painted_metal",
            [0.09, 0.12, 0.13],
            0.72,
            1.,
            0.50,
        ),
        ("rubber", "rubber", [0.16, 0.17, 0.16], 1., 1., 0.16),
        ("canvas", "cloth", [0.33, 0.40, 0.28], 1., 1., 0.50),
        ("brass", "steel", [0.84, 0.68, 0.40], 0.60, 1., 0.50),
        ("rust", "steel", [0.28, 0.15, 0.085], 0.94, 0., 0.18),
    ] {
        let color = Color::srgb(tint[0], tint[1], tint[2]);
        let mut material = crate::surfaces::material(source, color, &mut images);
        material.base_color = color;
        material.perceptual_roughness = roughness;
        material.metallic = metallic;
        material.reflectance = reflectance;
        if name == "rust" {
            material.metallic_roughness_texture = None;
        }
        sets.push((name, material));
    }
    let aircraft = crate::surfaces::aircraft_template(&mut images);
    for (name, tint) in [
        ("aircraft_grey", [0.67, 0.70, 0.71]),
        ("aircraft_sand", [0.70, 0.63, 0.50]),
        ("helicopter_green", [0.29, 0.34, 0.255]),
        ("helicopter_sand", [0.57, 0.48, 0.32]),
    ] {
        let mut material = aircraft.clone();
        material.base_color = Color::srgb(tint[0], tint[1], tint[2]);
        sets.push((name, material));
    }
    let mut a10 = crate::surfaces::material("a10_livery", Color::WHITE, &mut images);
    a10.metallic = 0.08;
    a10.perceptual_roughness = 0.46;
    a10.reflectance = 0.42;
    sets.push(("a10_livery", a10));
    for (name, color, roughness, reflectance) in [
        ("vehicle_glass", Color::srgb(0.11, 0.16, 0.17), 0.20, 0.50),
        ("cockpit_glass", Color::srgb(0.18, 0.235, 0.255), 0.38, 0.16),
        ("lights", Color::srgb(0.91, 0.84, 0.59), 0.85, 0.50),
    ] {
        sets.push((
            name,
            StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                reflectance,
                ..default()
            },
        ));
    }
    let mut manifest = vec![];
    let vehicle_textures = crate::surfaces::VehicleTextures::load(&mut images);
    for (name, mut material) in sets {
        vehicle_textures.apply(name, &mut material);
        let folder = directory.join(name);
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let mut maps = serde_json::Map::new();
        for (map, handle) in [
            ("albedo", &material.base_color_texture),
            ("normal", &material.normal_map_texture),
            ("orm", &material.metallic_roughness_texture),
            ("occlusion", &material.occlusion_texture),
        ] {
            if let Some(handle) = handle {
                let mut image = images.get(handle).ok_or("Missing exported image")?.clone();
                let size = [image.width(), image.height()];
                image
                    .data
                    .as_mut()
                    .ok_or("Missing image pixels")?
                    .truncate((size[0] * size[1] * 4) as usize);
                image.texture_descriptor.mip_level_count = 1;
                // The PNG adapter accepts the sRGB-tagged RGBA8 variant only.
                // Retag for encoding; do not transform normal/ORM pixel bytes.
                if image.texture_descriptor.format
                    == bevy::render::render_resource::TextureFormat::Rgba8Unorm
                {
                    image.texture_descriptor.format =
                        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb;
                }
                let file = format!("{map}.png");
                image
                    .try_into_dynamic()
                    .map_err(|e| e.to_string())?
                    .save(folder.join(&file))
                    .map_err(|e| e.to_string())?;
                maps.insert(map.into(), json!({"file":file,"size":size}));
            }
        }
        manifest.push(json!({
            "name":name,"maps":maps,
            "baseColorLinear":material.base_color.to_linear().to_f32_array(),
            "roughnessFactor":material.perceptual_roughness,
            "metallicFactor":material.metallic,"reflectance":material.reflectance,
            "uvMatrix":material.uv_transform.matrix2.to_cols_array_2d(),
            "uvTranslation":material.uv_transform.translation.to_array()
        }));
    }
    let output = json!({"format":1,"sets":manifest,
        "notes":"Raw runtime texture pixels and scalar factors. Model-dependent vertex dirt, shader weathering, contact AO and edge wear are not part of these reusable tiles."});
    std::fs::write(
        directory.join("runtime_materials.json"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Exported {} runtime material sets to {}",
        manifest.len(),
        directory.display()
    );
    Ok(())
}
