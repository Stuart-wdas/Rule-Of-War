//! Opt-in, offline real-world map. Source data and export tools live together in
//! prototypes/real-world-hottot; normal Forward Base never selects this map.
use bevy::prelude::*;
use serde::Deserialize;
use std::sync::{Arc, OnceLock};

pub const ID: &str = "gis-hottot";
pub const COLS: i32 = 2048;
pub const ROWS: i32 = 1024;
pub const SAVE: &str = "native-gis-hottot.json";

#[derive(Deserialize)]
pub struct Road {
    pub points: Vec<[f32; 2]>,
    pub width: f32,
    pub paved: bool,
}
#[derive(Deserialize)]
pub struct Building {
    pub osm_id: u64,
    pub at: [f32; 2],
    pub yaw: f32,
}
#[derive(Deserialize)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub step: f32,
}
#[derive(Deserialize)]
pub struct Hedge {
    pub points: Vec<[f32; 2]>,
}
#[derive(Deserialize)]
pub struct Map {
    pub bounds: [f32; 4],
    pub mask: Mask,
    pub roads: Vec<Road>,
    pub buildings: Vec<Building>,
    pub hedges: Vec<Hedge>,
}
pub fn map() -> &'static Map {
    static MAP: OnceLock<Map> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../../prototypes/real-world-hottot/map.json"))
            .expect("validated Hottot GIS metadata")
    })
}
pub fn data() -> Arc<crate::terrain::Data> {
    static DATA: OnceLock<Arc<crate::terrain::Data>> = OnceLock::new();
    DATA.get_or_init(|| {
        Arc::new(
            crate::terrain::decode(include_bytes!(
                "../../prototypes/real-world-hottot/hottot.terrain"
            ))
            .expect("validated Hottot terrain"),
        )
    })
    .clone()
}
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::args().any(|a| a == "--gis-prototype"))
}
/// Land cover, road exclusion and building exclusion, sampled in world metres.
pub fn mask(x: f32, z: f32) -> [u8; 3] {
    let map = map();
    let px = ((x - map.bounds[0]) / map.mask.step).round();
    let pz = ((z - map.bounds[2]) / map.mask.step).round();
    if px < 0. || pz < 0. || px >= map.mask.width as f32 || pz >= map.mask.height as f32 {
        return [0; 3];
    }
    let bytes = include_bytes!("../../prototypes/real-world-hottot/cover.rgb");
    let i = (pz as usize * map.mask.width + px as usize) * 3;
    [bytes[i], bytes[i + 1], bytes[i + 2]]
}
pub fn tint(x: f32, z: f32) -> Vec3 {
    match mask(x, z)[0] {
        1 => Vec3::new(0.90, 0.83, 0.60),
        2 => Vec3::new(0.87, 0.82, 0.72),
        3 => Vec3::new(0.83, 0.94, 0.73),
        4 => Vec3::new(0.68, 0.82, 0.64),
        5 => Vec3::new(0.89, 0.91, 0.83),
        _ => Vec3::ONE,
    }
}

#[derive(Component)]
pub struct RoadMesh(Handle<Mesh>);
#[derive(Component)]
pub struct Banner;
#[derive(Default)]
pub struct Roads {
    key: String,
}
pub fn roads(
    mut commands: Commands,
    game: Res<crate::simulation::NativeGame>,
    art: Res<crate::scene::Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<(Entity, &RoadMesh)>,
    mut state: Local<Roads>,
    mut banner: Query<&mut Visibility, With<Banner>>,
) {
    let t = &game.terrain;
    for mut visibility in &mut banner {
        *visibility = if t.id == ID {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    let key = format!("{}:{}", crate::terrain::render_source_key(t), t.revision);
    if state.key == key {
        return;
    }
    state.key = key;
    for (entity, mesh) in &old {
        commands.entity(entity).despawn();
        meshes.remove(mesh.0.id());
    }
    if t.id != ID {
        return;
    }
    for (finish, mesh) in crate::rural_surfaces::geographic_roads(t) {
        let handle = meshes.add(mesh);
        commands.spawn((
            RoadMesh(handle.clone()),
            Mesh3d(handle),
            MeshMaterial3d(art.materials[finish as usize + 2].clone()),
            Transform::default(),
        ));
    }
}
pub fn setup(
    mut commands: Commands,
    mut loading: ResMut<crate::loading::Loading>,
    game: Res<crate::simulation::NativeGame>,
    mut menu: ResMut<crate::menu::MainMenu>,
) {
    commands.spawn((
        Banner,
        Node { position_type: PositionType::Absolute, left: px(16), top: px(76),
            padding: UiRect::axes(px(10), px(6)), ..default() },
        BackgroundColor(Color::srgba(0.025, 0.045, 0.04, 0.9)),
    )).with_children(|p| { p.spawn((
        Text::new("HOTTOT-LES-BAGUES / REAL-WORLD PROTOTYPE\nMap © OpenStreetMap contributors · ODbL | Terrain: Mapzen / Copernicus EU-DEM / USGS"),
        TextFont { font_size: 13., ..default() },
        TextColor(Color::srgb(0.79, 0.84, 0.75)),
    )); });
    menu.open = false;
    menu.started = true;
    loading.begin(
        crate::loading::Request::Map {
            id: ID.into(),
            cols: COLS,
            rows: ROWS,
            config: crate::simulation::core_config(&game.config),
        },
        "Importing Hottot-les-Bagues / real-world prototype".into(),
    );
}

pub fn hedge_plants() -> &'static Vec<crate::rural_layout::Plant> {
    static PLANTS: OnceLock<Vec<crate::rural_layout::Plant>> = OnceLock::new();
    PLANTS.get_or_init(|| {
        let mut plants = vec![];
        for hedge in &map().hedges {
            for pair in hedge.points.windows(2) {
                let a = Vec2::from_array(pair[0]);
                let b = Vec2::from_array(pair[1]);
                let count = (a.distance(b) / 2.2).ceil().max(1.) as usize;
                for i in 0..count {
                    let at = a.lerp(b, i as f32 / count as f32);
                    if at.x > 0.
                        && at.x < COLS as f32
                        && at.y > 0.
                        && at.y < ROWS as f32
                        && mask(at.x, at.y)[1] == 0
                        && mask(at.x, at.y)[2] == 0
                    {
                        plants.push(crate::rural_layout::Plant {
                            at: at.to_array(),
                            tree: false,
                            scale: 0.85,
                        });
                    }
                }
            }
        }
        plants
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geographic_house_weathering_keeps_all_vertex_attributes_complete() {
        use bevy::mesh::VertexAttributeValues;
        let shapes = crate::models::shape_meshes();
        let lot = crate::settlement_layout::Lot {
            at: [12., 8.],
            width: 10.,
            depth: 10.,
            height: 3.2,
            yaw: 0.73,
            style: 0,
            ground: 0.,
            integrity: 1.,
        };
        for (finish, mut mesh) in crate::models::bake(&crate::models::house_parts(&lot), &shapes) {
            crate::surfaces::weather_architecture(&mut mesh, finish);
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("house vertices");
            };
            let count = points.len();
            for (attribute, values) in mesh.attributes() {
                assert_eq!(
                    values.len(),
                    count,
                    "house attribute {} must cover every weathered vertex",
                    attribute.name
                );
            }
            if let Some(indices) = mesh.indices() {
                assert!(indices.iter().all(|i| i < count));
            } else {
                assert_eq!(count % 3, 0);
            }
        }
    }
    #[test]
    fn geographic_sources_are_finite_complete_and_keep_their_relief() {
        let map = map();
        assert!(map.buildings.len() > 100 && map.roads.len() > 50);
        assert_eq!(
            include_bytes!("../../prototypes/real-world-hottot/cover.rgb").len(),
            map.mask.width * map.mask.height * 3
        );
        let terrain = crate::terrain::Terrain::new(ID, COLS, ROWS);
        assert!(terrain.data.is_some());
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for z in (0..ROWS).step_by(32) {
            for x in (0..COLS).step_by(32) {
                let h = terrain.height(x as f32, z as f32);
                assert!(h.is_finite());
                low = low.min(h);
                high = high.max(h);
            }
        }
        assert!(high - low > 10., "must retain the measured relief");
        for b in &map.buildings {
            assert!(
                b.at[0] > 0.
                    && b.at[0] < COLS as f32
                    && b.at[1] > 0.
                    && b.at[1] < ROWS as f32
                    && b.yaw.is_finite()
            );
        }
        for road in &map.roads {
            assert!(road.width > 0. && road.points.len() >= 2);
            assert!(road.points.iter().flatten().all(|p| p.is_finite()));
        }
    }
}
