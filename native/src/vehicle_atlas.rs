//! Reviewed per-vehicle atlases. UV seams expand vertices, never gameplay geometry or joints.
use crate::{
    graphics::Detail,
    model_finish::{VehicleFinish, VehicleMaterial},
    models::Finish,
};
use bevy::{
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::Indices,
    prelude::*,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

const KINDS: [&str; 6] = [
    "long",
    "xtralong",
    "troopCarrier",
    "antiAir",
    "helicopter",
    "plane",
];
#[derive(serde::Deserialize)]
struct Mapping {
    models: BTreeMap<String, Model>,
}
#[derive(serde::Deserialize)]
struct Model {
    batches: Vec<Batch>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Batch {
    finish: String,
    joint: i8,
    geometry_hash: String,
    source_vertex_count: usize,
    corner_uv: Vec<[f32; 2]>,
}
fn read_mapping(path: &Path) -> BTreeMap<String, Model> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Mapping>(&b).ok())
        .map(|m| m.models)
        .unwrap_or_default()
}
fn mapping() -> &'static BTreeMap<String, Model> {
    static DATA: OnceLock<BTreeMap<String, Model>> = OnceLock::new();
    DATA.get_or_init(|| read_mapping(Path::new("materials/vehicle_atlases/uv_mapping.json")))
}
static APPLIED: AtomicUsize = AtomicUsize::new(0);
pub fn matches() -> usize {
    APPLIED.load(Ordering::Relaxed)
}
fn remap_from(
    mesh: &mut Mesh,
    kind: &str,
    joint: i8,
    finish: Finish,
    data: &BTreeMap<String, Model>,
) -> bool {
    let Some(hash) = crate::model_finish::signature(mesh) else {
        return false;
    };
    let name = format!("{finish:?}");
    let Some(batch) = data.get(kind).and_then(|m| {
        m.batches.iter().find(|b| {
            b.joint == joint
                && b.finish == name
                && b.geometry_hash == hash
                && b.source_vertex_count == mesh.count_vertices()
        })
    }) else {
        return false;
    };
    let corners = mesh.indices().map_or(mesh.count_vertices(), Indices::len);
    if corners != batch.corner_uv.len()
        || batch
            .corner_uv
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return false;
    }
    let mut expanded = mesh.clone();
    expanded.duplicate_vertices();
    expanded.insert_indices(Indices::U32((0..corners as u32).collect()));
    expanded.insert_attribute(Mesh::ATTRIBUTE_UV_0, batch.corner_uv.clone());
    expanded.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    if expanded.generate_tangents().is_err() {
        return false;
    }
    *mesh = expanded;
    true
}
#[derive(Clone)]
struct Atlas {
    materials: [Handle<VehicleMaterial>; 2],
    images: [Handle<Image>; 3],
}
#[derive(Default)]
pub struct Art {
    models: BTreeMap<String, Atlas>,
    detail: Option<Detail>,
}
fn root(detail: Detail) -> std::path::PathBuf {
    Path::new("materials/vehicle_atlases").join(detail.label().to_lowercase())
}
fn textures(root: &Path, kind: &str) -> Option<[Image; 3]> {
    let mut result = vec![];
    for suffix in ["albedo", "normal", "orm"] {
        let mut image = crate::surfaces::read_texture(root, kind, suffix, suffix == "albedo")?;
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            anisotropy_clamp: 16,
            ..ImageSamplerDescriptor::linear()
        });
        result.push(image);
    }
    result.try_into().ok()
}
impl Art {
    pub fn load(
        images: &mut Assets<Image>,
        materials: &mut Assets<VehicleMaterial>,
        detail: Detail,
    ) -> Self {
        if std::env::args()
            .any(|a| a == "--legacy-vehicle-atlases" || a == "--legacy-vehicle-materials")
        {
            return Self::default();
        }
        let mut result = Self {
            detail: Some(detail),
            ..default()
        };
        for kind in KINDS {
            if !mapping().contains_key(kind) {
                continue;
            }
            let Some(maps) = textures(&root(detail), kind) else {
                warn!("Vehicle atlas {kind} incomplete; keeping tiled materials");
                continue;
            };
            let handles = maps.map(|m| images.add(m));
            let aircraft = matches!(kind, "plane" | "helicopter");
            let material_handles = [0, 1].map(|owner| {
                let mut extension = VehicleFinish::new(false);
                extension.tuning = Vec4::new(
                    0.72,
                    if aircraft { 0. } else { 0.14 },
                    if aircraft { 0. } else { 0.4 },
                    0.,
                );
                extension.atlas_tint = if owner == 0 {
                    Vec4::new(1., 1., 1., 1.)
                } else if aircraft {
                    Vec4::new(1.38, 1.13, 0.84, 1.)
                } else {
                    Vec4::new(1.28, 1.10, 0.84, 1.)
                };
                materials.add(VehicleMaterial {
                    base: StandardMaterial {
                        base_color: Color::WHITE,
                        base_color_texture: Some(handles[0].clone()),
                        normal_map_texture: Some(handles[1].clone()),
                        flip_normal_map_y: true,
                        metallic_roughness_texture: Some(handles[2].clone()),
                        occlusion_texture: Some(handles[2].clone()),
                        perceptual_roughness: 1.,
                        metallic: 1.,
                        reflectance: 0.45,
                        cull_mode: None,
                        double_sided: true,
                        ..default()
                    },
                    extension,
                })
            });
            result.models.insert(
                kind.into(),
                Atlas {
                    materials: material_handles,
                    images: handles,
                },
            );
        }
        result
    }
    pub fn count(&self) -> usize {
        self.models.len()
    }
    pub fn material(
        &self,
        mesh: &mut Mesh,
        kind: &str,
        joint: i8,
        finish: Finish,
        owner: i32,
    ) -> Option<Handle<VehicleMaterial>> {
        let atlas = self.models.get(kind)?;
        if !remap_from(mesh, kind, joint, finish, mapping()) {
            return None;
        }
        APPLIED.fetch_add(1, Ordering::Relaxed);
        Some(atlas.materials[owner.clamp(0, 1) as usize].clone())
    }
    fn refresh(&mut self, images: &mut Assets<Image>, detail: Detail) {
        if self.detail == Some(detail) || self.models.is_empty() {
            return;
        }
        for (kind, atlas) in &self.models {
            let Some(maps) = textures(&root(detail), kind) else {
                warn!("Keeping previous {kind} atlas quality: maps missing");
                continue;
            };
            for (handle, image) in atlas.images.iter().zip(maps) {
                if let Some(existing) = images.get_mut(handle) {
                    *existing = image;
                }
            }
        }
        self.detail = Some(detail);
    }
}
pub fn refresh(
    graphics: Res<crate::graphics::Graphics>,
    mut art: ResMut<crate::scene::Art>,
    mut images: ResMut<Assets<Image>>,
) {
    art.vehicle_atlases
        .refresh(&mut images, graphics.settings.textures);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    #[test]
    fn all_six_atlases_preserve_triangles_normals_contacts_and_joint_batches() {
        let data = read_mapping(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../materials/vehicle_atlases/uv_mapping.json"),
        );
        let shapes = crate::models::shape_meshes();
        let mut triangles = 0;
        let mut batches = 0;
        for kind in KINDS {
            let piece = crate::runtime::Piece {
                kind: kind.into(),
                category: if matches!(kind, "plane" | "helicopter") {
                    "aircraft"
                } else {
                    "vehicle"
                }
                .into(),
                ..default()
            };
            for (joint, _, finish, mut mesh) in
                crate::models::bake_animated(crate::models::parts(&piece), &shapes)
            {
                if matches!(
                    finish,
                    Finish::Glass | Finish::AircraftGlass | Finish::Light
                ) || (kind == "helicopter" && finish == Finish::Red)
                {
                    continue;
                }
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                let original = mesh.clone();
                let vertices = mesh.count_vertices();
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.63, 0.27]; vertices]);
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.8, 0.7, 0.6, 1.]; vertices]);
                assert!(
                    remap_from(&mut mesh, kind, joint, finish, &data),
                    "{kind} {joint} {finish:?}"
                );
                assert_eq!(mesh.count_vertices(), indices.len());
                for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
                    let Some(VertexAttributeValues::Float32x3(before)) =
                        original.attribute(attribute)
                    else {
                        panic!()
                    };
                    let Some(VertexAttributeValues::Float32x3(after)) = mesh.attribute(attribute)
                    else {
                        panic!()
                    };
                    assert_eq!(
                        *after,
                        indices.iter().map(|i| before[*i]).collect::<Vec<_>>()
                    );
                }
                let Some(VertexAttributeValues::Float32x2(contact)) =
                    mesh.attribute(Mesh::ATTRIBUTE_UV_1)
                else {
                    panic!()
                };
                assert!(contact.iter().all(|v| *v == [0.63, 0.27]));
                let Some(VertexAttributeValues::Float32x4(colour)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!()
                };
                assert!(colour.iter().all(|v| *v == [0.8, 0.7, 0.6, 1.]));
                assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
                triangles += indices.len() / 3;
                batches += 1;
            }
        }
        assert_eq!(
            triangles,
            data.values()
                .flat_map(|m| &m.batches)
                .map(|b| b.corner_uv.len() / 3)
                .sum::<usize>()
        );
        assert_eq!(batches, 210);
    }
    #[test]
    fn geometry_mismatch_never_changes_a_mesh() {
        let mut mesh = Mesh::from(Cuboid::new(1., 1., 1.));
        let before = format!("{mesh:?}");
        assert!(!remap_from(
            &mut mesh,
            "long",
            0,
            Finish::Team,
            &BTreeMap::new()
        ));
        assert_eq!(format!("{mesh:?}"), before);
    }
    #[test]
    fn quality_maps_include_mask_alpha_and_correct_colour_spaces() {
        use bevy::render::render_resource::TextureFormat;
        for detail in [Detail::Low, Detail::Balanced, Detail::High] {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../materials/vehicle_atlases")
                .join(detail.label().to_lowercase());
            for kind in KINDS {
                let [albedo, normal, orm] = textures(&root, kind).unwrap();
                assert_eq!(
                    albedo.texture_descriptor.format,
                    TextureFormat::Rgba8UnormSrgb
                );
                assert_eq!(normal.texture_descriptor.format, TextureFormat::Rgba8Unorm);
                assert_eq!(orm.texture_descriptor.format, TextureFormat::Rgba8Unorm);
                assert!(normal.width() <= detail.texture_size());
                assert!(albedo.width() <= detail.texture_size());
                let bytes = orm.data.as_ref().unwrap();
                assert!(bytes.chunks_exact(4).any(|p| p[3] > 200));
                assert!(bytes.chunks_exact(4).any(|p| p[3] < 50));
            }
        }
    }
}
