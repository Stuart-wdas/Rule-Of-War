//! Distant scenery only: no navigation cells, orders, buildings or resources.
//! Reuses the live terrain material so the battlefield never ends in a void.
use crate::{landscape::Nature, simulation::NativeGame, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::sync::mpsc;
#[derive(Component)]
pub struct Border(Handle<Mesh>);
#[derive(Default)]
pub struct State {
    key: String,
    pending: Option<mpsc::Receiver<Vec<Mesh>>>,
}
fn strip(t: &Terrain, b: [i32; 4]) -> Mesh {
    let axis = |start: i32, end: i32, boundary: i32| {
        let mut points = vec![start, end];
        for value in [0, boundary] {
            if value > start && value < end {
                points.push(value);
            }
        }
        let first = start.div_euclid(8) * 8;
        for value in (first..=end).step_by(8) {
            if value > start {
                points.push(value);
            }
        }
        points.sort_unstable();
        points.dedup();
        points
    };
    let xs = axis(b[0], b[1], t.cols - 1);
    let zs = axis(b[2], b[3], t.rows - 1);
    let nx = (xs.len() - 1) as i32;
    let nz = (zs.len() - 1) as i32;
    let mut points = vec![];
    let mut indices = vec![];
    for z in 0..=nz {
        for x in 0..=nx {
            let (px, pz) = (xs[x as usize] as f32, zs[z as usize] as f32);
            points.push([px, crate::battlefield::scenic_height(t, px, pz), pz]);
        }
    }
    for z in 0..nz {
        for x in 0..nx {
            let a = (z * (nx + 1) + x) as u32;
            let w = (nx + 1) as u32;
            indices.extend([a, a + w, a + 1, a + 1, a + w, a + w + 1]);
        }
    }
    let colors: Vec<_> = points
        .iter()
        .map(|p| {
            if t.id == crate::gis_prototype::ID {
                let c = crate::gis_prototype::tint(p[0], p[2]) * 0.64;
                [c.x, c.y, c.z, 1.]
            } else {
                [0.554, 0.572, 0.541, 1.]
            }
        })
        .collect();
    let count = points.len();
    let uv: Vec<_> = points.iter().map(|p| [p[0] * 0.25, p[2] * 0.25]).collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.035, 0.]; count])
    .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}
pub fn update(
    mut commands: Commands,
    game: Res<NativeGame>,
    nature: Res<Nature>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<(Entity, &Border)>,
    mut state: Local<State>,
    mut landscape_ready: ResMut<crate::landscape::Ready>,
) {
    let key = crate::terrain::render_source_key(&game.terrain);
    if state.key != key {
        state.key = key;
        state.pending = None;
        for (entity, mesh) in &old {
            commands.entity(entity).despawn();
            meshes.remove(mesh.0.id());
        }
        if game.terrain.id == crate::battlefield::ID || game.terrain.id == crate::gis_prototype::ID
        {
            let terrain = game.terrain.clone();
            let (sender, receiver) = mpsc::channel();
            state.pending = Some(receiver);
            std::thread::spawn(move || {
                let m = crate::battlefield::SCENERY_MARGIN as i32;
                let (x, z) = (terrain.cols - 1, terrain.rows - 1);
                let result = [
                    [-m, x + m, -m, 0],
                    [-m, x + m, z, z + m],
                    [-m, 0, 0, z],
                    [x, x + m, 0, z],
                ]
                .map(|bounds| strip(&terrain, bounds))
                .into_iter()
                .collect::<Vec<_>>();
                let _ = sender.send(result);
            });
        }
    }
    let ready = state
        .pending
        .as_ref()
        .and_then(|receiver| receiver.try_recv().ok());
    if let Some(prepared) = ready {
        state.pending = None;
        for mesh in prepared {
            let handle = meshes.add(mesh);
            commands.spawn((
                Border(handle.clone()),
                Mesh3d(handle),
                MeshMaterial3d(nature.grass.clone()),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
        }
    }
    if game.terrain.id == crate::battlefield::ID || game.terrain.id == crate::gis_prototype::ID {
        landscape_ready.ready &= state.pending.is_none() && old.iter().count() == 4;
    }
}
