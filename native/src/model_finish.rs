//! Offline contact visibility, keyed to the exact bind-pose mesh geometry.
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    mesh::VertexAttributeValues,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::{Shader, ShaderRef},
};
use std::{
    collections::BTreeMap,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};
const SHADER: Handle<Shader> = uuid_handle!("935ba753-238e-4d06-b452-9fe4c0948230");
pub type VehicleMaterial = ExtendedMaterial<StandardMaterial, VehicleFinish>;
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct VehicleFinish {
    #[uniform(100)]
    pub tuning: Vec4,
    #[uniform(101)]
    pub atlas_tint: Vec4,
}
impl VehicleFinish {
    pub fn new(painted: bool) -> Self {
        Self {
            atlas_tint: Vec4::ZERO,
            tuning: Vec4::new(
                0.72,
                if painted { 0.60 } else { 0. },
                if painted { 1. } else { 0. },
                if painted { 1. } else { 0. },
            ),
        }
    }
    pub fn surface(index: usize) -> Self {
        use crate::models::Finish;
        let mut finish = Self::new(false);
        if [Finish::Steel, Finish::Brass, Finish::Dark]
            .iter()
            .any(|f| *f as usize + 2 == index)
        {
            finish.tuning.z = 1.;
        }
        if [
            Finish::Steel,
            Finish::Brass,
            Finish::Dark,
            Finish::Rubber,
            Finish::Cloth,
        ]
        .iter()
        .any(|f| *f as usize + 2 == index)
        {
            finish.tuning.w = 1.;
        }
        finish
    }
}
impl MaterialExtension for VehicleFinish {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<VehicleMaterial>::default());
    app.add_systems(Update, crate::vehicle_atlas::refresh);
    load_internal_asset!(app, SHADER, "model_finish.wgsl", Shader::from_wgsl);
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contact {
    geometry_hash: String,
    channels: Vec<[f32; 2]>,
}
static MATCHES: AtomicUsize = AtomicUsize::new(0);
pub fn matches() -> usize {
    MATCHES.load(Ordering::Relaxed)
}
fn contacts() -> &'static BTreeMap<String, Vec<Contact>> {
    static CACHE: OnceLock<BTreeMap<String, Vec<Contact>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        std::fs::read("assets/art_refresh/model_contacts.json")
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Vec<Contact>>>(&bytes).ok())
            .unwrap_or_default()
    })
}
pub(crate) fn signature(mesh: &Mesh) -> Option<String> {
    let mut hash = 14695981039346656037u64;
    for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
        let Some(VertexAttributeValues::Float32x3(data)) = mesh.attribute(attribute) else {
            return None;
        };
        for value in data.iter().flatten() {
            for byte in value.to_le_bytes() {
                hash = (hash ^ byte as u64).wrapping_mul(1099511628211);
            }
        }
    }
    Some(format!("{hash:016x}"))
}
pub fn apply(mesh: &mut Mesh, kind: &str) {
    let count = mesh.count_vertices();
    let channels = signature(mesh).and_then(|hash| {
        contacts()
            .get(kind)
            .and_then(|groups| {
                groups
                    .iter()
                    .find(|g| g.geometry_hash == hash && g.channels.len() == count)
            })
            .map(|g| g.channels.clone())
    });
    if channels.is_some() {
        MATCHES.fetch_add(1, Ordering::Relaxed);
    }
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_1,
        channels.unwrap_or_else(|| vec![[1., 0.]; count]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contact_signature_tracks_geometry_and_fallback_is_unoccluded() {
        let mut mesh = Mesh::from(Cuboid::new(1., 1., 1.));
        let first = signature(&mesh).unwrap();
        if let Some(VertexAttributeValues::Float32x3(p)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            p[0][0] += 0.01;
        }
        assert_ne!(first, signature(&mesh).unwrap());
        apply(&mut mesh, "not-a-model");
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("missing contact data")
        };
        assert!(uv.iter().all(|v| *v == [1., 0.]));
    }
}
