//! Flat, weathered concrete ground surfaces with metre-scale construction detail.
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    mesh::{Indices, VertexAttributeValues},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::{Shader, ShaderRef},
};

const SHADER: Handle<Shader> = uuid_handle!("0d067b76-87da-40d5-ad82-827605599938");
pub type GroundMaterial = ExtendedMaterial<StandardMaterial, GroundFinish>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Concrete,
    Foundation,
}
pub fn surface(kind: &str, finish: crate::models::Finish) -> Option<Surface> {
    match finish {
        crate::models::Finish::Concrete => Some(Surface::Concrete),
        crate::models::Finish::Gravel if kind == "airfield" => Some(Surface::Foundation),
        _ => None,
    }
}
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GroundFinish {
    #[uniform(100)]
    pub tuning: Vec4,
}
impl MaterialExtension for GroundFinish {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    load_internal_asset!(app, SHADER, "ground_finish.wgsl", Shader::from_wgsl);
    app.add_systems(Update, quality);
}

/// Extract only broad, upward ground faces. Walls, roofs, kerbs and fittings
/// retain their original material and every triangle stays in exactly one batch.
pub fn take_floor(mesh: &mut Mesh) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return None;
    };
    let indices = mesh.indices()?;
    let (mut floor, mut rest) = (vec![], vec![]);
    for tri in indices.iter().collect::<Vec<_>>().chunks_exact(3) {
        let a = Vec3::from_array(p[tri[0]]);
        let b = Vec3::from_array(p[tri[1]]);
        let c = Vec3::from_array(p[tri[2]]);
        let ground = tri.iter().all(|&i| n[i][1] > 0.95 && p[i][1] < 0.32)
            && (b - a).cross(c - a).length() > 0.8;
        let group = if ground { &mut floor } else { &mut rest };
        group.extend(tri.iter().map(|i| *i as u32));
    }
    if floor.is_empty() {
        return None;
    }
    let mut result = mesh.clone();
    result.insert_indices(Indices::U32(floor));
    mesh.insert_indices(Indices::U32(rest));
    Some(result)
}

struct Source {
    live: Handle<Image>,
    full: Image,
}
/// Model-local tiling must account for building scale to retain metre repeats.
pub fn scale_uv(mesh: &mut Mesh, scale: f32) {
    if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for p in uv {
            p[0] *= scale;
            p[1] *= scale;
        }
    }
}
/// Perimeter coordinates are independent of the six-metre photographic repeat.
pub fn foundation_uv(mesh: &mut Mesh) {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return;
    };
    let centre = crate::airfield_layout::FOUNDATION_CENTER;
    let size = crate::airfield_layout::FOUNDATION_SIZE;
    let uv: Vec<[f32; 2]> = p
        .iter()
        .map(|p| {
            [
                (p[0] - centre[0]) / size[0] + 0.5,
                (p[2] - centre[1]) / size[1] + 0.5,
            ]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv);
}
#[derive(Resource)]
pub struct GroundTextures {
    sources: Vec<Source>,
    size: u32,
}
fn at_resolution(source: &Image, maximum: u32) -> Image {
    let mut result = source.clone();
    let (mut w, mut h, mut offset, mut skipped) = (source.width(), source.height(), 0, 0);
    while w.max(h) > maximum && skipped + 1 < source.texture_descriptor.mip_level_count {
        offset += (w * h * 4) as usize;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        skipped += 1;
    }
    result.texture_descriptor.size.width = w;
    result.texture_descriptor.size.height = h;
    result.texture_descriptor.mip_level_count -= skipped;
    result.data = source.data.as_ref().map(|data| data[offset..].to_vec());
    result
}
pub fn materials(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    ground: &mut Assets<GroundMaterial>,
    size: u32,
) -> [Handle<GroundMaterial>; 2] {
    let mut concrete = crate::surfaces::material("apron_concrete", Color::WHITE, images);
    concrete.reflectance = 0.22;
    let handles = [
        concrete.base_color_texture.clone(),
        concrete.normal_map_texture.clone(),
        concrete.metallic_roughness_texture.clone(),
    ];
    let mut sources = vec![];
    for handle in handles.into_iter().flatten() {
        let full = images.get(&handle).unwrap().clone();
        *images.get_mut(&handle).unwrap() = at_resolution(&full, size);
        sources.push(Source { live: handle, full });
    }
    commands.insert_resource(GroundTextures { sources, size });
    let foundation = ground.add(GroundMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.97, 0.95, 0.90),
            ..concrete.clone()
        },
        extension: GroundFinish {
            tuning: Vec4::new(
                5.5,
                1.,
                crate::airfield_layout::FOUNDATION_SIZE[0],
                crate::airfield_layout::FOUNDATION_SIZE[1],
            ),
        },
    });
    let concrete = ground.add(GroundMaterial {
        base: concrete,
        extension: GroundFinish {
            tuning: Vec4::new(5.5, 0., 0., 0.),
        },
    });
    [concrete, foundation]
}
fn quality(
    graphics: Res<crate::graphics::Graphics>,
    textures: Option<ResMut<GroundTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<GroundMaterial>>,
) {
    let Some(mut textures) = textures else {
        return;
    };
    let size = graphics.settings.textures.texture_size();
    if textures.size == size {
        return;
    }
    for source in &mut textures.sources {
        let next = images.add(at_resolution(&source.full, size));
        for (_, material) in materials.iter_mut() {
            for handle in [
                &mut material.base.base_color_texture,
                &mut material.base.normal_map_texture,
                &mut material.base.metallic_roughness_texture,
                &mut material.base.occlusion_texture,
            ] {
                if handle.as_ref() == Some(&source.live) {
                    *handle = Some(next.clone());
                }
            }
        }
        source.live = next;
    }
    textures.size = size;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn airbase_foundations_receive_weathering_without_reclassifying_natural_gravel() {
        use crate::models::Finish;
        assert_eq!(
            surface("airfield", Finish::Gravel),
            Some(Surface::Foundation)
        );
        assert_eq!(surface("airfield", Finish::Asphalt), None);
        assert_eq!(surface("town", Finish::Gravel), None);
        assert_eq!(
            surface("airfield", Finish::Concrete),
            Some(Surface::Concrete)
        );
        let part = crate::models::parts(&crate::runtime::Piece {
            kind: "airfield".into(),
            building: true,
            ..default()
        })
        .into_iter()
        .find(|p| p.finish == Finish::Gravel)
        .unwrap();
        let mut mesh = crate::models::bake(&[part], &crate::models::shape_meshes())
            .remove(0)
            .1;
        let mut floor = take_floor(&mut mesh).unwrap();
        foundation_uv(&mut floor);
        let Some(VertexAttributeValues::Float32x2(uv)) = floor.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("perimeter mask missing");
        };
        for i in floor.indices().unwrap().iter() {
            assert!((0. ..=1.).contains(&uv[i][0]) && (0. ..=1.).contains(&uv[i][1]));
        }
    }
    #[test]
    fn floor_extraction_preserves_walls_faces_and_height() {
        let mut mesh = Mesh::from(Cuboid::new(40., 0.04, 180.));
        let before = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().clone();
        let count = mesh.indices().unwrap().len();
        let floor = take_floor(&mut mesh).unwrap();
        assert_eq!(floor.indices().unwrap().len(), 6);
        assert_eq!(
            mesh.indices().unwrap().len() + floor.indices().unwrap().len(),
            count
        );
        assert_eq!(floor.attribute(Mesh::ATTRIBUTE_POSITION).unwrap(), &before);
        let mut roof = Mesh::from(Cuboid::new(4., 1., 4.));
        assert!(take_floor(&mut roof).is_none());
        let mut kerb = Mesh::from(Cuboid::new(0.3, 0.04, 0.3));
        assert!(take_floor(&mut kerb).is_none());
    }
    #[test]
    fn quality_selects_complete_filtered_mips_and_can_restore_full_detail() {
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let mut source = Image::new(
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![128; 8 * 8 * 4],
            TextureFormat::Rgba8Unorm,
            bevy::asset::RenderAssetUsages::default(),
        );
        crate::surfaces::filtered_mips(&mut source, true);
        let low = at_resolution(&source, 2);
        assert_eq!(low.width(), 2);
        assert_eq!(low.data.as_ref().unwrap().len(), (4 + 1) * 4);
        assert_eq!(low.texture_descriptor.mip_level_count, 2);
        assert_eq!(at_resolution(&source, 8).data, source.data);
    }
}
