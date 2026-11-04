//! Deterministic material grain and foliage. No external asset pipeline is required.
use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageSampler, ImageSamplerDescriptor, ImageType,
    },
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
fn noise(x: u32, y: u32) -> u32 {
    let n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ 971;
    let n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^ (n >> 16)
}
fn repeat_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 16,
        ..ImageSamplerDescriptor::linear()
    })
}
pub(crate) fn read_texture(
    root: &std::path::Path,
    name: &str,
    suffix: &str,
    srgb: bool,
) -> Option<Image> {
    let mut labels = vec![name];
    if name == "painted_metal" {
        labels.push("painted_steel");
    }
    if name == "roof_slate" || name == "roof_terracotta" {
        labels.push("roof");
    }
    for label in labels {
        let mut files = vec![format!("{label}_{suffix}.png")];
        if suffix == "albedo" {
            files.push(format!("{label}.png"));
        }
        for path in files
            .into_iter()
            .flat_map(|file| [root.join("pbr").join(&file), root.join(file)])
        {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            match Image::from_buffer(
                &bytes,
                ImageType::Extension("png"),
                CompressedImageFormats::NONE,
                srgb,
                repeat_sampler(),
                RenderAssetUsages::default(),
            ) {
                Ok(mut image) => {
                    filtered_mips(&mut image, suffix == "normal");
                    return Some(image);
                }
                Err(error) => {
                    warn!("Texture {} could not be loaded: {error}", path.display());
                }
            }
        }
    }
    None
}
fn repeat_from(root: &std::path::Path, name: &str) -> Option<Vec2> {
    if !root
        .join("pbr")
        .join(format!("{name}_albedo.png"))
        .is_file()
    {
        return None;
    }
    let catalog: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("pbr/catalog.json")).ok()?).ok()?;
    let repeat = &catalog["materials"][name]["repeat_m"];
    let value = Vec2::new(repeat[0].as_f64()? as f32, repeat[1].as_f64()? as f32);
    (value.is_finite() && value.min_element() > 0.).then_some(value)
}
pub fn pbr_repeat(name: &str) -> Option<Vec2> {
    repeat_from(std::path::Path::new("materials"), name)
}
/// Read labelled assets at startup; missing maps retain the procedural material.
pub fn material(name: &str, tint: Color, images: &mut Assets<Image>) -> StandardMaterial {
    material_from(std::path::Path::new("materials"), name, tint, images)
}
/// Neutral matte paint shared by the renderer and the editable review export.
pub fn aircraft_template(images: &mut Assets<Image>) -> StandardMaterial {
    let mut aircraft_template = material("aircraft_paint", Color::WHITE, images);
    // Reuse the supplied paint wear as neutral grain; its green cast was making
    // the grey A-10 look olive. Source files remain unchanged.
    if let Some(handle) = &aircraft_template.base_color_texture {
        let image = images.get_mut(handle).unwrap();
        let length = (image.width() * image.height() * 4) as usize;
        if let Some(pixels) = &mut image.data {
            pixels.truncate(length);
            for pixel in pixels.chunks_exact_mut(4) {
                let grey = (132.
                    + pixel[0] as f32 * 0.25
                    + pixel[1] as f32 * 0.5
                    + pixel[2] as f32 * 0.25)
                    * 0.8;
                pixel[..3].fill(grey.clamp(0., 255.) as u8);
            }
        }
        image.texture_descriptor.mip_level_count = 1;
        crate::surfaces::filtered_mips(image, false);
    }
    aircraft_template.metallic = 0.05;
    aircraft_template.perceptual_roughness = 0.68;
    aircraft_template.reflectance = 0.42;
    aircraft_template
}
/// Satin painted armour: broad camouflage patches and fine wear, with no fabric weave.
pub fn armour(owner: usize, images: &mut Assets<Image>) -> StandardMaterial {
    let palette = if owner == 0 {
        [
            [89f32, 96., 72.],
            [66., 77., 58.],
            [48., 55., 46.],
            [109., 105., 80.],
        ]
    } else {
        [
            [128f32, 116., 87.],
            [106., 99., 75.],
            [79., 80., 63.],
            [149., 137., 107.],
        ]
    };
    // The supplied aircraft paint provides photographic scratches and mottling;
    // neutralise its colour before applying each faction's camouflage palette.
    let wear = read_texture(
        std::path::Path::new("materials"),
        "aircraft_paint",
        "albedo",
        true,
    );
    let size = 1024;
    let sample = |x: u32, y: u32| {
        wear.as_ref()
            .and_then(|image| {
                image.data.as_ref().map(|data| {
                    let at = ((((y % size) * image.height() / size) * image.width()
                        + (x % size) * image.width() / size)
                        * 4) as usize;
                    (data[at] as f32 * 0.25
                        + data[at + 1] as f32 * 0.5
                        + data[at + 2] as f32 * 0.25)
                        / 255.
                })
            })
            .unwrap_or(0.39)
    };
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    let mut normals = Vec::with_capacity(pixels.capacity());
    let mut surface = Vec::with_capacity(pixels.capacity());
    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / 140.;
            let v = y as f32 / 140.;
            let warp = crate::natural_noise::value(u * 0.6, v * 0.6, 413) * 1.7;
            let field = crate::natural_noise::value(u + warp, v - warp, 191) * 0.78
                + crate::natural_noise::value(u * 2.7, v * 2.7, 92) * 0.22;
            let index = if field < 0.33 {
                2
            } else if field < 0.48 {
                1
            } else if field < 0.68 {
                0
            } else {
                3
            };
            let photo = sample(x, y);
            let grain = (0.91 + photo * 0.23) * (0.985 + (noise(x, y) % 11) as f32 * 0.003);
            let chip = photo > 0.64 && noise(x / 2, y / 3).is_multiple_of(5);
            pixels.extend(
                palette[index].map(|c| (if chip { c * 0.70 } else { c * grain }).min(255.) as u8),
            );
            pixels.push(255);
            // Surface relief follows the paint scratches, never camouflage colour.
            let n = Vec3::new(
                (sample((x + size - 1) % size, y) - sample(x + 1, y)) * 0.18,
                (sample(x, (y + size - 1) % size) - sample(x, y + 1)) * 0.18,
                1.,
            )
            .normalize();
            normals.extend(
                (n * 0.5 + Vec3::splat(0.5))
                    .to_array()
                    .map(|v| (v * 255.) as u8),
            );
            normals.push(255);
            surface.extend([
                250,
                ((0.40 + photo * 0.20).clamp(0., 1.) * 255.) as u8,
                if chip { 75 } else { 0 },
                255,
            ]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = repeat_sampler();
    filtered_mips(&mut image, false);
    let make_map = |data| {
        let mut image = Image::new(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        image.sampler = repeat_sampler();
        image
    };
    let mut normal = make_map(normals);
    filtered_mips(&mut normal, true);
    let mut orm = make_map(surface);
    filtered_mips(&mut orm, false);
    let orm = images.add(orm);
    StandardMaterial {
        base_color_texture: Some(images.add(image)),
        normal_map_texture: Some(images.add(normal)),
        metallic_roughness_texture: Some(orm.clone()),
        occlusion_texture: Some(orm),
        uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(0.68)),
        metallic: 1.,
        perceptual_roughness: 1.,
        reflectance: 0.45,
        ..default()
    }
}
fn material_from(
    root: &std::path::Path,
    name: &str,
    tint: Color,
    images: &mut Assets<Image>,
) -> StandardMaterial {
    let albedo = read_texture(root, name, "albedo", true);
    let external = albedo.is_some();
    let relief = relief_maps(name);
    let normal = read_texture(root, name, "normal", false)
        .or_else(|| relief.as_ref().map(|maps| maps.0.clone()));
    let orm = read_texture(root, name, "orm", false)
        .or_else(|| relief.map(|maps| maps.1))
        .map(|image| images.add(image));
    let base = albedo.unwrap_or_else(|| {
        let mut image = detail(name);
        filtered_mips(&mut image, false);
        image
    });
    StandardMaterial {
        uv_transform: repeat_from(root, name).map_or(bevy::math::Affine2::IDENTITY, |repeat| {
            bevy::math::Affine2::from_scale(Vec2::ONE / (repeat * 0.65))
        }),
        base_color: if external && !["painted_metal", "cloth"].contains(&name) {
            Color::WHITE
        } else {
            tint
        },
        base_color_texture: Some(images.add(base)),
        normal_map_texture: normal.map(|image| images.add(image)),
        occlusion_texture: orm.clone(),
        metallic_roughness_texture: orm.clone(),
        metallic: if orm.is_some() {
            1.
        } else if name == "steel" {
            0.72
        } else {
            0.
        },
        perceptual_roughness: if orm.is_some() {
            1.
        } else if name == "steel" {
            0.65
        } else {
            0.9
        },
        cull_mode: if name == "cloth" {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        double_sided: name == "cloth",
        ..default()
    }
}
pub fn detail(name: &str) -> Image {
    if ![
        "brick",
        "plaster",
        "concrete",
        "roof",
        "wood",
        "steel",
        "painted_metal",
        "cloth",
        "soil",
        "bark",
        "stone",
    ]
    .contains(&name)
    {
        return grain(false);
    }
    let mut pixels = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let fine = (noise(x, y) % 13) as f32;
            let broad = (noise(x / 16, y / 16) % 18) as f32;
            let value = match name {
                "brick" => {
                    let row = y / 12;
                    let u = (x + if row % 2 == 0 { 0 } else { 16 }) % 32;
                    if y % 12 < 2 || u < 2 {
                        112. + fine
                    } else {
                        198. + (noise((x + row % 2 * 16) / 32, row) % 35) as f32 + fine
                    }
                }
                "roof" => {
                    let row = y / 32;
                    let u = (x + row % 2 * 32) % 64;
                    if y % 32 < 2 || u < 2 {
                        127. + fine
                    } else {
                        197. + (noise((x + row % 2 * 32) / 64, row) % 24) as f32 + fine
                    }
                }
                "wood" | "bark" => 180. + (noise(x / 3, y / 48) % 48) as f32 + fine,
                "cloth" => 216. + fine - if (x + y) % 3 == 0 { 22. } else { 0. },
                "concrete" => {
                    223. + fine - broad - if x % 128 < 2 || y % 128 < 2 { 19. } else { 0. }
                }
                "steel" | "painted_metal" => {
                    230. + fine
                        - broad
                        - if noise(x, y / 40).is_multiple_of(83) {
                            32.
                        } else {
                            0.
                        }
                }
                "soil" | "stone" => 216. + fine - broad,
                _ => 225. + fine - broad - (noise(x / 64, y / 64) % 10) as f32,
            }
            .clamp(0., 255.) as u8;
            pixels.extend([value, value, value, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = repeat_sampler();
    image
}
pub fn grain(industrial: bool) -> Image {
    let mut pixels = Vec::with_capacity(128 * 128 * 4);
    for y in 0..128u32 {
        for x in 0..128u32 {
            let fine = (noise(x, y) % 29) as f32;
            let stains = (noise(x / 12, y / 12) % 26) as f32;
            let scratch = if industrial && noise(x, y / 18).is_multiple_of(71) {
                35.
            } else {
                0.
            };
            let value = (215. + fine - stains - scratch).clamp(0., 255.) as u8;
            pixels.extend([value, value, value, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..default()
    });
    image
}
pub fn uv(mesh: &mut Mesh) {
    // Blender-authored aircraft use UV0 for their livery atlas; UV1 marks that
    // authored mapping so shared shape preparation cannot replace it with tiling.
    if mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some() {
        return;
    }
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return;
    };
    let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return;
    };
    let uv: Vec<[f32; 2]> = p.iter().zip(n).map(|(p, n)| coords(*p, *n)).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    let _ = mesh.generate_tangents();
}
pub fn coords(p: [f32; 3], n: [f32; 3]) -> [f32; 2] {
    if n[1].abs() > n[0].abs().max(n[2].abs()) {
        [p[0] * 0.65, p[2] * 0.65]
    } else if n[0].abs() > n[2].abs() {
        [p[2] * 0.65, p[1] * 0.65]
    } else {
        [p[0] * 0.65, p[1] * 0.65]
    }
}
/// Interwoven dry stems, bent grass and broad low leaves. The first six blades
/// retain complete topology for the Medium tier's existing compact subset.
pub fn groundcover() -> Mesh {
    let mut positions = vec![];
    let mut normals = vec![];
    let mut colors = vec![];
    let mut indices = vec![];
    for i in 0..20u32 {
        let seed = noise(i, 381);
        let straw = i % 5 == 0 || i % 7 == 3;
        let broad_leaf = i % 6 == 2;
        let flattened = i % 5 == 4;
        let angle = i as f32 * 2.399963 + ((seed >> 9) % 100) as f32 * 0.005;
        let direction = Vec3::new(angle.cos(), 0., angle.sin());
        let side = Vec3::new(-direction.z, 0., direction.x);
        let root = match i % 3 {
            0 => Vec3::new(-0.18, 0., 0.075),
            1 => Vec3::new(0.14, 0., -0.13),
            _ => Vec3::new(0.13, 0., 0.20),
        };
        let at = root + direction * ((seed % 100) as f32 * 0.0016);
        let height = if flattened {
            0.055 + (seed % 50) as f32 * 0.0008
        } else if broad_leaf {
            0.08 + (seed % 70) as f32 * 0.0011
        } else {
            0.14 + (seed % 90) as f32 * 0.0020
        };
        let width = if broad_leaf {
            0.038
        } else if straw {
            0.006
        } else {
            0.012
        };
        let lean = if flattened {
            0.28
        } else if broad_leaf {
            0.16
        } else {
            0.10
        };
        let tint = if straw || flattened {
            Vec3::new(1.74, 1.23, 1.65)
        } else if broad_leaf {
            Vec3::new(0.83, 0.96, 0.78)
        } else {
            Vec3::new(1.02, 1.02, 0.84)
        };
        let start = positions.len() as u32;
        for (level, spread, bend) in [
            (0., width * 0.35, 0.),
            (0.56, width, lean * 0.30),
            (1., 0., lean),
        ] {
            for sign in [-1., 1.] {
                positions.push(
                    (at + Vec3::Y * height * level + direction * bend + side * spread * sign)
                        .to_array(),
                );
                normals.push(
                    (Vec3::Y * 0.80 + direction * (0.25 + level * 0.30))
                        .normalize()
                        .to_array(),
                );
                // Dark roots connect the tuft to soil; dry stems keep a warm
                // straw hue instead of every blade sharing a bright green tip.
                let light = 0.43 + level * 0.47 + (seed % 9) as f32 * 0.010;
                colors.push((tint * light).extend(1.).to_array());
            }
        }
        indices.extend([
            start,
            start + 1,
            start + 2,
            start + 1,
            start + 3,
            start + 2,
            start + 2,
            start + 3,
            start + 4,
            start + 3,
            start + 5,
            start + 4,
        ]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Broad weathering is baked once into architecture, using its own local height.
pub fn weather_architecture(mesh: &mut Mesh, finish: crate::models::Finish) {
    use crate::models::Finish;
    if !matches!(
        finish,
        Finish::Plaster
            | Finish::Brick
            | Finish::Concrete
            | Finish::Wood
            | Finish::Ochre
            | Finish::Sage
            | Finish::BluePlaster
            | Finish::Rose
            | Finish::Ivory
            | Finish::Roof
            | Finish::Terracotta
            | Finish::SlateRoof
            | Finish::GreenRoof
    ) {
        return;
    }
    weather_vertices(mesh);
    let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let roof = matches!(
        finish,
        Finish::Roof | Finish::Terracotta | Finish::SlateRoof | Finish::GreenRoof
    );
    let foot = vertices.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let colors: Vec<[f32; 4]> = vertices
        .iter()
        .map(|p| {
            let patch = crate::natural_noise::value(p[0] / 2.7, p[2] / 2.7 + p[1] / 5., 167);
            let stain = crate::natural_noise::value(p[0] / 0.9, p[2] / 0.9, 613);
            let damp = (-(p[1] - foot).max(0.) * 1.8).exp() * if roof { 0.06 } else { 0.20 };
            let streak = crate::natural_noise::value(p[0] * 2.8 + p[2] * 3.1, p[1] * 0.17, 781);
            let damp = damp
                + if roof {
                    stain * 0.10
                } else {
                    (1. - streak) * 0.12
                };
            let light = (0.83 + patch * 0.17 - stain * 0.06 - damp).clamp(0.52, 1.);
            [
                light,
                light * (1. - damp * 0.035),
                light * (1. - damp * 0.12),
                1.,
            ]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

/// Curved leaf sprays follow branch whorls and offset crown lobes. The centre
/// vertex gives each spray actual depth instead of a pair of flat diamonds.
pub fn foliage(conifer: bool, low: bool) -> Mesh {
    foliage_clusters(conifer, low, false)
}
pub fn shrub() -> Mesh {
    foliage_clusters(false, true, true)
}
fn foliage_clusters(conifer: bool, low: bool, bushy: bool) -> Mesh {
    let count = if low {
        if bushy { 24 } else { 18 }
    } else if conifer {
        68
    } else {
        88
    };
    let mut positions = vec![];
    let mut normals = vec![];
    let mut colors = vec![];
    let mut uv = vec![];
    let mut indices = vec![];
    for i in 0..count {
        let n = noise(i, 17);
        let branch = i % 7;
        let angle = branch as f32 * 2.39996 + ((n >> 5) % 100) as f32 * 0.013;
        let h = if conifer {
            (branch as f32 + (n % 100) as f32 * 0.01) / 7.
        } else {
            (n % 1000) as f32 / 1000.
        };
        let radial = 0.25 + (((n >> 12) % 1000) as f32 / 1000.).sqrt() * 0.75;
        let at = if conifer {
            let width = (1. - h).powf(0.70) * (0.83 + (branch % 3) as f32 * 0.09);
            Vec3::new(
                angle.cos() * radial * width,
                (h - 0.5) * 2.35 - radial * width * 0.10,
                angle.sin() * radial * width,
            )
        } else {
            let lobe = branch as f32 * 2.39996;
            let center = Vec3::new(
                lobe.cos() * 0.37,
                (branch % 3) as f32 * 0.23 - 0.19,
                lobe.sin() * 0.34,
            );
            let y = h * 1.35 - 0.675;
            let r = (1. - y * y).sqrt() * radial * 0.53;
            center + Vec3::new(angle.cos() * r, y * 0.69, angle.sin() * r)
        };
        let at = if bushy {
            at * Vec3::new(0.98, 0.64, 0.98)
                + Vec3::new(
                    (branch as f32 * 1.7).sin() * 0.07,
                    0.,
                    (branch as f32 * 2.3).cos() * 0.06,
                )
        } else {
            at
        };
        let size = if low {
            0.34 + ((n >> 20) % 15) as f32 * 0.01
        } else {
            0.27 + ((n >> 20) % 12) as f32 * 0.010
        };
        for plane in 0..2 {
            let rotation = Quat::from_euler(
                EulerRot::YXZ,
                angle + plane as f32 * 1.57,
                0.18 + (n % 90) as f32 * 0.009,
                ((n >> 8) % 100) as f32 * 0.008 - 0.4,
            );
            let start = positions.len() as u32;
            let plane_normal = rotation * Vec3::Z;
            let radial_normal = Vec3::new(at.x, 0.50 + at.y * 0.20, at.z).normalize_or(Vec3::Y);
            let facing = if plane_normal.dot(radial_normal) < 0. {
                -plane_normal
            } else {
                plane_normal
            };
            for (corner, p) in [
                Vec3::new(-size, -size * 0.58, 0.),
                Vec3::new(size, -size * 0.45, 0.),
                Vec3::new(size * 0.87, size * 0.62, 0.),
                Vec3::new(-size * 0.77, size * 0.69, 0.),
                Vec3::new(0., size * 0.04, size * 0.20),
            ]
            .into_iter()
            .enumerate()
            {
                let tile = n % 12;
                let v = [[0., 1.], [1., 1.], [1., 0.], [0., 0.], [0.5, 0.5]][corner];
                // Stay inside the photographic atlas cell while mip filtering.
                let v = [0.016 + v[0] * 0.968, 0.016 + v[1] * 0.968];
                uv.push([
                    (tile % 3) as f32 / 3. + v[0] / 3.,
                    (tile / 3) as f32 / 4. + v[1] / 4.,
                ]);
                let p = at + rotation * p;
                // Respect the spacing bound used by the world placement grid.
                let limit = if conifer { 1.15 } else { 1. };
                let horizontal = Vec2::new(p.x, p.z).length();
                let p = Vec3::new(
                    p.x * (limit / horizontal).min(1.),
                    p.y,
                    p.z * (limit / horizontal).min(1.),
                );
                positions.push(p.to_array());
                normals.push(
                    (radial_normal * 0.82 + facing * 0.18)
                        .normalize()
                        .to_array(),
                );
                let exposed = (radial * 0.68 + h * 0.32).clamp(0., 1.);
                let tint = (0.62 + exposed * 0.30) * (0.94 + (n % 13) as f32 * 0.008);
                colors.push([tint, tint * (0.98 + h * 0.02), tint * 0.91, 1.]);
            }
            indices.extend([
                start,
                start + 1,
                start + 4,
                start + 1,
                start + 2,
                start + 4,
                start + 2,
                start + 3,
                start + 4,
                start + 3,
                start,
                start + 4,
            ]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod texture_tests {
    use super::*;
    #[test]
    fn curved_foliage_keeps_atlas_cells_valid_and_lod_vertices_bounded() {
        for (mesh, limit) in [
            (foliage(false, false), 1.),
            (foliage(true, false), 1.15),
            (foliage(false, true), 1.),
            (shrub(), 1.),
        ] {
            assert!(mesh.count_vertices() <= 880);
            let VertexAttributeValues::Float32x3(points) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!()
            };
            let VertexAttributeValues::Float32x3(normals) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
            else {
                panic!()
            };
            let VertexAttributeValues::Float32x2(uv) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
            else {
                panic!()
            };
            assert_eq!(points.len(), normals.len());
            assert_eq!(points.len(), uv.len());
            assert!(points.iter().all(|p| Vec3::from_array(*p).is_finite() && p[0].hypot(p[2])<=limit+0.0001));
            assert!(
                normals
                    .iter()
                    .all(|n| (Vec3::from_array(*n).length() - 1.).abs() < 0.0001)
            );
            for spray in uv.chunks_exact(5) {
                let cell = Vec2::from_array(spray[0]) * Vec2::new(3., 4.);
                assert!(spray.iter().all(|p| (Vec2::from_array(*p)*Vec2::new(3.,4.)).floor()==cell.floor()));
            }
            assert!(mesh.indices().unwrap().iter().all(|i| i < points.len()));
        }
        assert!(foliage(false, true).count_vertices() < foliage(false, false).count_vertices() / 3);
    }
    #[test]
    fn imported_pbr_sets_keep_physical_aspect_and_take_precedence_over_loose_colour() {
        let root = std::env::temp_dir().join(format!("row-pbr-{}", std::process::id()));
        std::fs::create_dir_all(root.join("pbr")).unwrap();
        std::fs::write(
            root.join("pbr/catalog.json"),
            r#"{"materials":{"brick":{"repeat_m":[2.4,1.2]}}}"#,
        )
        .unwrap();
        for suffix in ["albedo", "normal", "orm"] {
            detail("brick")
                .try_into_dynamic()
                .unwrap()
                .save(root.join(format!("pbr/brick_{suffix}.png")))
                .unwrap();
        }
        detail("wood")
            .try_into_dynamic()
            .unwrap()
            .save(root.join("brick.png"))
            .unwrap();
        let mut images = Assets::<Image>::default();
        let m = material_from(&root, "brick", Color::WHITE, &mut images);
        assert!(m.normal_map_texture.is_some() && m.metallic_roughness_texture.is_some());
        let actual = m.uv_transform.transform_vector2(Vec2::splat(0.65));
        assert!((actual - Vec2::new(1. / 2.4, 1. / 1.2)).length() < 0.00001);
        assert_eq!(
            images
                .get(m.base_color_texture.as_ref().unwrap())
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        for file in ["albedo", "normal", "orm"] {
            std::fs::remove_file(root.join(format!("pbr/brick_{file}.png"))).unwrap();
        }
        std::fs::remove_file(root.join("pbr/catalog.json")).unwrap();
        std::fs::remove_dir(root.join("pbr")).unwrap();
        std::fs::remove_file(root.join("brick.png")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn simple_filenames_and_painted_steel_alias_load_without_renaming_user_files() {
        let root = std::env::temp_dir().join(format!(
            "rule-of-war-loose-materials-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let png = Image::new(
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![200; 4 * 4 * 4],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        for name in ["wood", "painted_steel"] {
            png.clone()
                .try_into_dynamic()
                .unwrap()
                .save(root.join(format!("{name}.png")))
                .unwrap();
        }
        let mut images = Assets::<Image>::default();
        for name in ["wood", "painted_metal"] {
            let tint = Color::srgb(0.3, 0.4, 0.2);
            let material = material_from(&root, name, tint, &mut images);
            assert_eq!(
                images
                    .get(material.base_color_texture.as_ref().unwrap())
                    .unwrap()
                    .texture_descriptor
                    .size
                    .width,
                4
            );
            assert_eq!(
                material.base_color,
                if name == "wood" { Color::WHITE } else { tint }
            );
        }
        // A canonical PBR albedo takes precedence when both filename styles exist.
        detail("wood")
            .try_into_dynamic()
            .unwrap()
            .save(root.join("wood_albedo.png"))
            .unwrap();
        let material = material_from(&root, "wood", Color::WHITE, &mut images);
        assert_eq!(
            images
                .get(material.base_color_texture.as_ref().unwrap())
                .unwrap()
                .texture_descriptor
                .size
                .width,
            256
        );
        for file in ["wood.png", "wood_albedo.png", "painted_steel.png"] {
            std::fs::remove_file(root.join(file)).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn labelled_png_sets_use_srgb_colour_and_linear_normal_and_orm_maps() {
        let root =
            std::env::temp_dir().join(format!("rule-of-war-materials-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for suffix in ["albedo", "normal", "orm"] {
            detail("brick")
                .try_into_dynamic()
                .unwrap()
                .save(root.join(format!("brick_{suffix}.png")))
                .unwrap();
        }
        let mut images = Assets::<Image>::default();
        let material = material_from(&root, "brick", Color::srgb(0.5, 0.3, 0.2), &mut images);
        assert_eq!(material.base_color, Color::WHITE);
        assert_eq!(
            images
                .get(material.base_color_texture.as_ref().unwrap())
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        for image in [
            material.normal_map_texture.as_ref().unwrap(),
            material.metallic_roughness_texture.as_ref().unwrap(),
        ] {
            assert_eq!(
                images.get(image).unwrap().texture_descriptor.format,
                TextureFormat::Rgba8Unorm
            );
        }
        for suffix in ["albedo", "normal", "orm"] {
            std::fs::remove_file(root.join(format!("brick_{suffix}.png"))).unwrap();
        }
        std::fs::remove_dir(&root).unwrap();
        let fallback = material_from(&root, "brick", Color::srgb(0.5, 0.3, 0.2), &mut images);
        assert!(fallback.base_color_texture.is_some() && fallback.normal_map_texture.is_none());
    }
}

include!("texture_filter.rs");
include!("terrain_textures.rs");
include!("surface_wear.rs");
include!("vehicle_textures.rs");
