//! Shared, prelit animation frames from the supplied explosion VFX collection.
use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageSampler, ImageSamplerDescriptor, ImageType,
    },
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
        TextureViewDimension,
    },
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Metadata {
    frame_count: usize,
    grid: [u32; 2],
    fps: f32,
    #[serde(default)]
    pivot: Option<[f32; 2]>,
    #[serde(default)]
    encoding: String,
    #[serde(default)]
    radiance_range: f32,
    #[serde(default = "unit_scale")]
    radiance_scale: f32,
}
fn unit_scale() -> f32 {
    1.
}

pub struct Flipbook {
    pub mesh: Handle<Mesh>,
    pub frames: Vec<Handle<StandardMaterial>>,
    pub duration: f32,
}

fn split_frames(bytes: &[u8], metadata: &Metadata) -> Result<Vec<Image>, String> {
    if metadata.frame_count == 0
        || metadata.frame_count > 256
        || metadata.grid.contains(&0)
        || !metadata.fps.is_finite()
        || metadata.fps <= 0.
        || metadata.grid[0] * metadata.grid[1] < metadata.frame_count as u32
    {
        return Err("Invalid flipbook timing or grid".into());
    }
    let sheet = Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        metadata.encoding != "log-radiance",
        ImageSampler::linear(),
        RenderAssetUsages::default(),
    )
    .map_err(|error| error.to_string())?;
    let format = if metadata.encoding == "log-radiance" {
        TextureFormat::Rgba8Unorm
    } else {
        TextureFormat::Rgba8UnormSrgb
    };
    if sheet.texture_descriptor.format != format
        || !sheet.width().is_multiple_of(metadata.grid[0])
        || !sheet.height().is_multiple_of(metadata.grid[1])
    {
        return Err("Flipbook must be RGBA8 with whole frame cells".into());
    }
    let data = sheet.data.as_ref().ok_or("Flipbook has no pixels")?;
    let (width, height) = (
        sheet.width() / metadata.grid[0],
        sheet.height() / metadata.grid[1],
    );
    let mut frames = Vec::with_capacity(metadata.frame_count);
    for frame in 0..metadata.frame_count {
        let x = frame as u32 % metadata.grid[0] * width;
        let y = frame as u32 / metadata.grid[0] * height;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in y..y + height {
            let start = ((row * sheet.width() + x) * 4) as usize;
            pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        let mut image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            format,
            RenderAssetUsages::default(),
        );
        // Filter each cell independently so distant effects cannot bleed neighbouring frames.
        if metadata.encoding == "log-radiance" {
            radiance_mips(&mut image, metadata.radiance_range, metadata.radiance_scale)?;
        } else {
            crate::surfaces::filtered_mips(&mut image, false);
        }
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            ..ImageSamplerDescriptor::linear()
        });
        frames.push(image);
    }
    Ok(frames)
}

fn radiance_mips(image: &mut Image, range: f32, scale: f32) -> Result<(), String> {
    if !range.is_finite()
        || !(1. ..=128.).contains(&range)
        || !scale.is_finite()
        || !(1. ..=64.).contains(&scale)
    {
        return Err("Invalid radiance range".into());
    }
    let decode: [f32; 256] =
        std::array::from_fn(|i| ((1. + range * scale).powf(i as f32 / 255.) - 1.) / scale);
    let encode: Vec<u8> = (0..=65536)
        .map(|i| {
            ((1. + i as f32 / 65536. * range * scale).ln() / (1. + range * scale).ln() * 255.)
                .round() as u8
        })
        .collect();
    let (mut w, mut h) = (image.width() as usize, image.height() as usize);
    let mut level = image.data.take().ok_or("No radiance pixels")?;
    let mut all = level.clone();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                let mut light = Vec3::ZERO;
                let mut alpha = 0.;
                let mut samples = 0.;
                for sy in y * h / nh..(y + 1) * h / nh {
                    for sx in x * w / nw..(x + 1) * w / nw {
                        let p = &level[(sy * w + sx) * 4..][..4];
                        let a = p[3] as f32 / 255.;
                        light += Vec3::new(
                            decode[p[0] as usize],
                            decode[p[1] as usize],
                            decode[p[2] as usize],
                        ) * a;
                        alpha += a;
                        samples += 1.;
                    }
                }
                light /= alpha.max(0.000001);
                next.extend(
                    light
                        .to_array()
                        .map(|v| encode[(v / range * 65536.).round().clamp(0., 65536.) as usize]),
                );
                next.push((alpha / samples * 255.).round() as u8);
            }
        }
        all.extend(&next);
        level = next;
        w = nw;
        h = nh;
        levels += 1;
    }
    image.data = Some(all);
    image.texture_descriptor.mip_level_count = levels;
    Ok(())
}

/// Array layers isolate every animation frame at every mip level. Layer-major
/// uploads match wgpu's image upload order and avoid atlas-neighbour bleeding.
pub fn texture_array(bytes: &[u8], json: &str) -> Result<Image, String> {
    texture_array_limit(bytes, json, u32::MAX)
}
pub fn texture_array_limit(bytes: &[u8], json: &str, max_size: u32) -> Result<Image, String> {
    let metadata: Metadata = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut frames = split_frames(bytes, &metadata)?;
    for frame in &mut frames {
        while frame.width().max(frame.height()) > max_size.max(1)
            && frame.texture_descriptor.mip_level_count > 1
        {
            let skip = (frame.width() * frame.height() * 4) as usize;
            let data = frame.data.as_mut().ok_or("Missing mip pixels")?;
            *data = data.split_off(skip);
            frame.texture_descriptor.size.width = (frame.width() / 2).max(1);
            frame.texture_descriptor.size.height = (frame.height() / 2).max(1);
            frame.texture_descriptor.mip_level_count -= 1;
        }
    }
    let mut array = frames[0].clone();
    array.texture_descriptor.size.depth_or_array_layers = frames.len() as u32;
    array.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    array.data_order = TextureDataOrder::LayerMajor;
    array.data = Some(frames.into_iter().flat_map(|f| f.data.unwrap()).collect());
    array.asset_usage = RenderAssetUsages::RENDER_WORLD;
    Ok(array)
}

impl Flipbook {
    /// A shallow, camera-facing volume surface supplies real depth testing.
    /// Its ground pivot stays fixed; only the body bulges toward the viewer.
    pub fn give_depth(&mut self, meshes: &mut Assets<Mesh>, depth: f32) {
        let source = meshes.get(&self.mesh).unwrap();
        let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
            source.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            return;
        };
        let pivot = Vec2::new(
            -vertices.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min),
            vertices
                .iter()
                .map(|v| v[1])
                .fold(f32::NEG_INFINITY, f32::max),
        );
        self.mesh = meshes.add(volume_mesh(pivot, depth));
    }
    pub fn load(
        bytes: &[u8],
        json: &str,
        pivot: Vec2,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let metadata: Metadata = serde_json::from_str(json).expect("Bundled VFX metadata");
        let pivot = metadata.pivot.map(Vec2::from_array).unwrap_or(pivot);
        let frames = split_frames(bytes, &metadata)
            .expect("Bundled VFX animation")
            .into_iter()
            .map(|image| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(images.add(image)),
                    // These straight-alpha frames already contain lighting and fire colour.
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                })
            })
            .collect();
        let mut mesh = Mesh::from(Rectangle::new(1., 1.));
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            for vertex in vertices {
                vertex[0] += 0.5 - pivot.x;
                vertex[1] += pivot.y - 0.5;
            }
        }
        Self {
            mesh: meshes.add(mesh),
            frames,
            duration: metadata.frame_count as f32 / metadata.fps,
        }
    }
    pub fn frame(&self, age: f32, life: f32) -> &Handle<StandardMaterial> {
        let progress = (age / life.max(0.001)).clamp(0., 1.);
        &self.frames[((progress * self.frames.len() as f32) as usize).min(self.frames.len() - 1)]
    }
}

fn volume_mesh(pivot: Vec2, depth: f32) -> Mesh {
    const COLS: u32 = 17;
    const ROWS: u32 = 25;
    let mut positions = Vec::new();
    let mut uv = Vec::new();
    let mut indices = Vec::new();
    for y in 0..ROWS {
        let v = y as f32 / (ROWS - 1) as f32;
        let height = ((pivot.y - v) / pivot.y.max(0.01)).clamp(0., 1.);
        for x in 0..COLS {
            let u = x as f32 / (COLS - 1) as f32;
            let across = (1. - ((u - pivot.x) / 0.5).powi(2)).max(0.).sqrt();
            let z = depth * across * (height * std::f32::consts::PI).sin().max(0.);
            positions.push([u - pivot.x, pivot.y - v, z]);
            uv.push([u, v]);
            if x < COLS - 1 && y < ROWS - 1 {
                let i = y * COLS + x;
                indices.extend([i, i + COLS, i + 1, i + 1, i + COLS, i + COLS + 1]);
            }
        }
    }
    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., 1.]; positions.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdr_mips_preserve_light_energy_and_ignore_transparent_hot_pixels() {
        let make = |alpha| {
            Image::new(
                Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                vec![
                    255, 255, 255, alpha, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                ],
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::default(),
            )
        };
        let mut image = make(255);
        radiance_mips(&mut image, 32., 1.).unwrap();
        let mip = image.data.as_ref().unwrap()[16] as f32 / 255.;
        assert!((33_f32.powf(mip) - 1. - 8.).abs() < 0.2);
        let mut transparent = make(0);
        radiance_mips(&mut transparent, 32., 1.).unwrap();
        assert_eq!(&transparent.data.as_ref().unwrap()[16..19], &[0, 0, 0]);
        assert!(radiance_mips(&mut make(255), f32::NAN, 1.).is_err());
        assert!(radiance_mips(&mut make(255), 32., 0.).is_err());
        let encoded = |v: f32| ((1. + v * 32.).ln() / 1025_f32.ln() * 255.).round() as u8;
        let mut shadows = make(255);
        shadows.data = Some(
            [0.01, 0.02, 0.01, 0.02]
                .into_iter()
                .flat_map(|v| [encoded(v), encoded(v), encoded(v), 255])
                .collect(),
        );
        radiance_mips(&mut shadows, 32., 32.).unwrap();
        let mip = shadows.data.as_ref().unwrap()[16] as f32 / 255.;
        let light = (1025_f32.powf(mip) - 1.) / 32.;
        assert!(
            (light - 0.015).abs() < 0.001,
            "dark smoke must retain sub-0.001 precision"
        );
    }
    #[test]
    fn blast_volume_has_front_depth_but_keeps_its_ground_anchor_and_bounds() {
        let mesh = volume_mesh(Vec2::new(0.5, 0.75), 0.24);
        let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("positions")
        };
        assert!(
            vertices
                .iter()
                .all(|v| v.iter().all(|n| n.is_finite()) && v[2] >= 0. && v[2] <= 0.24)
        );
        assert!(vertices.iter().any(|v| v[2] > 0.23));
        assert!(
            vertices
                .iter()
                .any(|v| v[0].abs() < 0.00001 && v[1].abs() < 0.00001 && v[2].abs() < 0.00001)
        );
        assert!(vertices.iter().filter(|v| v[1] < 0.).all(|v| v[2] == 0.));
    }
    #[test]
    fn reference_blasts_have_independent_frames_ground_pivots_and_empty_tails() {
        let variants: [(&[u8], &str); 2] = [
            (
                include_bytes!(
                    "../../assets/Realistic_Explosion_VFX/reference_blasts/mushroom/animation_8x8.png"
                ),
                include_str!(
                    "../../assets/Realistic_Explosion_VFX/reference_blasts/mushroom/animation.json"
                ),
            ),
            (
                include_bytes!(
                    "../../assets/Realistic_Explosion_VFX/reference_blasts/debris/animation_8x8.png"
                ),
                include_str!(
                    "../../assets/Realistic_Explosion_VFX/reference_blasts/debris/animation.json"
                ),
            ),
        ];
        assert_ne!(variants[0].0, variants[1].0);
        for (bytes, json) in variants {
            let metadata: Metadata = serde_json::from_str(json).unwrap();
            assert_eq!(metadata.frame_count, 64);
            assert_eq!(metadata.fps, 12.);
            let pivot = metadata.pivot.unwrap();
            assert!(
                pivot
                    .iter()
                    .all(|n| n.is_finite() && (0. ..=1.).contains(n))
            );
            let frames = split_frames(bytes, &metadata).unwrap();
            let tail = frames.last().unwrap();
            let tail_data = tail.data.as_ref().unwrap();
            assert!(tail_data.chunks_exact(4).all(|p| p[3] == 0));
            assert!(
                frames[12]
                    .data
                    .as_ref()
                    .unwrap()
                    .chunks_exact(4)
                    .any(|p| p[3] > 180)
            );
            assert_eq!([tail.width(), tail.height()], [384, 448]);
        }
    }
    #[test]
    fn supplied_frames_keep_top_left_order_transparency_and_isolated_mips() {
        let metadata: Metadata = serde_json::from_str(include_str!(
            "../../assets/Realistic_Explosion_VFX/explosions/explosion_fireball.json"
        ))
        .unwrap();
        let bytes = include_bytes!(
            "../../assets/Realistic_Explosion_VFX/explosions/explosion_fireball_8x8_2048.png"
        );
        let sheet = Image::from_buffer(
            bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::linear(),
            RenderAssetUsages::default(),
        )
        .unwrap();
        let frames = split_frames(bytes, &metadata).unwrap();
        assert_eq!(frames.len(), 64);
        let array = texture_array(
            bytes,
            &serde_json::json!({
                "frame_count":64,"grid":[8,8],"fps":24.
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(array.texture_descriptor.size.depth_or_array_layers, 64);
        let small = texture_array_limit(
            bytes,
            &serde_json::json!({
                "frame_count":64,"grid":[8,8],"fps":24.
            })
            .to_string(),
            128,
        )
        .unwrap();
        assert_eq!([small.width(), small.height()], [128, 128]);
        assert_eq!(
            small.texture_descriptor.mip_level_count,
            array.texture_descriptor.mip_level_count - 1
        );
        let expected: Vec<u8> = frames
            .iter()
            .flat_map(|f| f.data.as_ref().unwrap()[256 * 256 * 4..].iter().copied())
            .collect();
        assert_eq!(small.data.as_ref().unwrap(), &expected);
        assert_eq!(
            array.texture_descriptor.mip_level_count,
            frames[0].texture_descriptor.mip_level_count
        );
        let stride = frames[0].data.as_ref().unwrap().len();
        for i in [0, 7, 31, 63] {
            assert_eq!(
                &array.data.as_ref().unwrap()[i * stride..(i + 1) * stride],
                frames[i].data.as_ref().unwrap()
            );
        }
        for index in [0, 7, 8, 31, 63] {
            let frame = &frames[index];
            assert_eq!((frame.width(), frame.height()), (256, 256));
            assert_eq!(frame.texture_descriptor.mip_level_count, 9);
            let data = frame.data.as_ref().unwrap();
            let source = sheet.data.as_ref().unwrap();
            for row in 0..256 {
                let start = ((index / 8 * 256 + row) * 2048 + index % 8 * 256) * 4;
                assert_eq!(
                    &data[row * 256 * 4..(row + 1) * 256 * 4],
                    &source[start..start + 256 * 4]
                );
            }
            assert!(data[..256 * 256 * 4].chunks_exact(4).any(|p| p[3] == 0));
        }
        assert!(
            split_frames(
                bytes,
                &Metadata {
                    frame_count: 0,
                    grid: [8, 8],
                    fps: 24.,
                    pivot: None,
                    encoding: String::new(),
                    radiance_range: 0.,
                    radiance_scale: 1.,
                }
            )
            .is_err()
        );
    }
}
