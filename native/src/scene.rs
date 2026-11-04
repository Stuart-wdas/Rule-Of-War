use crate::{
    Controls, RtsCamera,
    models::{self, Finish},
    runtime::{Battle, Piece},
};
use bevy::{
    anti_alias::fxaa::Fxaa, core_pipeline::tonemapping::Tonemapping,
    pbr::ScreenSpaceAmbientOcclusion, prelude::*,
};
use std::collections::{HashMap, HashSet};
type BuildingBatchKey = (Option<usize>, Option<[u32; 3]>);
pub const BLUE: Color = Color::srgb(0.25, 0.56, 0.66);
pub const RED: Color = Color::srgb(0.73, 0.34, 0.27);
pub const GOLD: Color = Color::srgb(0.84, 0.68, 0.40);
#[derive(Component)]
pub struct Actor(pub String);
#[derive(Component)]
pub struct RoofLayer {
    pub building: String,
    pub lot: Option<usize>,
    wall_normal: Option<Vec3>,
    center: Vec3,
    opaque: Handle<StandardMaterial>,
    transparent: Handle<StandardMaterial>,
}
pub fn roof_visibility(
    mut commands: Commands,
    battle: Res<Battle>,
    camera: Query<&GlobalTransform, With<RtsCamera>>,
    mut roofs: Query<(
        Entity,
        &RoofLayer,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    let _timing = crate::performance::span("scene.roof_visibility");
    let occupied: HashSet<_> = battle
        .snapshot
        .entities
        .iter()
        .filter(|p| battle.snapshot.simulation || p.owner == battle.snapshot.viewer)
        .filter_map(|p| {
            p.garrison
                .as_ref()
                .filter(|g| g.entered && (!g.exiting || g.door_stage == 2))
                .map(|g| (g.building.as_str(), g.lot))
        })
        .collect();
    let eye = camera
        .single()
        .ok()
        .map(|t| t.translation())
        .unwrap_or(Vec3::new(0., 20., 30.));
    for (entity, roof, mut material, mut visibility) in &mut roofs {
        let reveal = occupied.contains(&(roof.building.as_str(), roof.lot));
        if let Some(normal) = roof.wall_normal {
            let target_visibility = if reveal && normal.dot(eye - roof.center) > 0.01 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            visibility.set_if_neq(target_visibility);
            if material.0 != roof.opaque {
                material.0 = roof.opaque.clone();
            }
            continue;
        }
        let target = if reveal {
            &roof.transparent
        } else {
            &roof.opaque
        };
        if material.0 == *target {
            continue;
        }
        material.0 = target.clone();
        if reveal {
            commands
                .entity(entity)
                .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
        } else {
            commands
                .entity(entity)
                .remove::<(bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver)>();
        }
    }
}
#[derive(Component)]
pub struct PropActor(String, bool, u64);
fn prop_signature(p: &crate::simulation::BattlefieldProp) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    p.kind.hash(&mut hash);
    if let Some(scar) = p.scar {
        scar.radius.to_bits().hash(&mut hash);
        scar.excavated.hash(&mut hash);
        scar.seed.hash(&mut hash);
    }
    if p.wreck.is_none() {
        p.at.map(f32::to_bits).hash(&mut hash);
        p.heading.to_bits().hash(&mut hash);
    }
    if let Some(w) = &p.wreck {
        w.piece.kind.hash(&mut hash);
        w.piece.model_scale.to_bits().hash(&mut hash);
        w.piece.heading.to_bits().hash(&mut hash);
        w.piece.aim.map(|at| at.map(f32::to_bits)).hash(&mut hash);
        w.born.hash(&mut hash);
    }
    hash.finish()
}
#[derive(Component)]
pub struct BakedGeometry(pub(crate) Vec<Handle<Mesh>>);
#[derive(Component)]
pub struct LotGeometry {
    building: String,
    index: usize,
    meshes: Vec<Handle<Mesh>>,
}
#[derive(Component, Default)]
pub struct Walk {
    pub phase: f32,
    pub rotor_phase: f32,
    pub speed: f32,
    pub pose: crate::animation::Pose,
}
#[derive(Component)]
pub struct Limb {
    actor: Entity,
    base: Transform,
    role: i8,
    pivot: Vec3,
}
#[derive(Clone, PartialEq)]
pub struct Appearance {
    garrisoned: bool,
    training_target: bool,
    owner: i32,
    kind: String,
    end: Option<[f32; 2]>,
    blocks: Vec<serde_json::Value>,
    town_plan: crate::settlement_layout::TownPlan,
    soldiers: usize,
    lots: Vec<crate::settlement_layout::Lot>,
    scale: f32,
    structural_damage: u8,
}
impl From<&Piece> for Appearance {
    fn from(p: &Piece) -> Self {
        Self {
            garrisoned: p
                .garrison
                .as_ref()
                .is_some_and(|g| g.entered && (!g.exiting || g.door_stage == 2)),
            training_target: p.training_target,
            owner: if p.lots.is_empty() { p.owner } else { -1 },
            kind: p.kind.clone(),
            end: p.bridge_end,
            blocks: p.town_blocks.clone(),
            town_plan: p.town_plan,
            lots: p
                .lots
                .iter()
                .cloned()
                .map(|mut lot| {
                    lot.integrity = if lot.integrity <= 0. {
                        0.
                    } else {
                        1. - crate::destruction::damage_stage(lot.integrity) as f32 / 9.
                    };
                    lot
                })
                .collect(),
            scale: p.model_scale,
            structural_damage: if p.building && p.lots.is_empty() && p.max_hp > 0. {
                crate::destruction::damage_stage(p.hp / p.max_hp)
            } else {
                0
            },
            soldiers: if p.category == "troop" {
                models::soldiers(p)
            } else {
                0
            },
        }
    }
}
impl Appearance {
    fn changed_lots(&self, next: &Self) -> Option<Vec<usize>> {
        if self.lots.is_empty() || self.lots.len() != next.lots.len() {
            return None;
        }
        for (old, new) in self.lots.iter().zip(&next.lots) {
            let mut layout = old.clone();
            layout.integrity = new.integrity;
            if layout != *new {
                return None;
            }
        }
        let mut structure = self.clone();
        structure.lots.clone_from(&next.lots);
        (structure == *next).then(|| {
            self.lots
                .iter()
                .zip(&next.lots)
                .enumerate()
                .filter_map(|(i, (a, b))| (a != b).then_some(i))
                .collect()
        })
    }
}
#[derive(Resource)]
pub struct Art {
    pub(crate) foundation_ground: Handle<crate::ground_finish::GroundMaterial>,
    pub(crate) concrete_ground: Handle<crate::ground_finish::GroundMaterial>,
    pub(crate) transparent_roofs: Vec<Handle<StandardMaterial>>,
    pub(crate) aircraft_paint: [Handle<StandardMaterial>; 4],
    pub(crate) aircraft_finishes: Vec<Handle<StandardMaterial>>,
    pub(crate) apache_glass: Handle<StandardMaterial>,
    pub(crate) huey_paint: [Handle<StandardMaterial>; 2],
    pub(crate) huey_panels: [Handle<StandardMaterial>; 2],
    pub(crate) huey_dark: Handle<StandardMaterial>,
    pub(crate) huey_metal: Handle<StandardMaterial>,
    pub(crate) vehicle_detail_maps: usize,
    pub(crate) vehicle_atlases: crate::vehicle_atlas::Art,
    pub(crate) vehicle_paint: [Handle<crate::model_finish::VehicleMaterial>; 2],
    pub(crate) vehicle_finishes: Vec<Handle<crate::model_finish::VehicleMaterial>>,
    pub(crate) impact_scar_material: Handle<StandardMaterial>,
    pub(crate) meshes: Vec<Handle<Mesh>>,
    pub(crate) materials: Vec<Handle<StandardMaterial>>,
}
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut vehicle_materials: ResMut<Assets<crate::model_finish::VehicleMaterial>>,
    mut ground_materials: ResMut<Assets<crate::ground_finish::GroundMaterial>>,
    mut images: ResMut<Assets<Image>>,
    graphics: Res<crate::graphics::Graphics>,
) {
    let detailed = |mut mesh: Mesh| {
        crate::surfaces::uv(&mut mesh);
        meshes.add(mesh)
    };
    let mut detailed = detailed;
    let shapes = models::shape_meshes()
        .into_iter()
        .map(&mut detailed)
        .collect();
    let palette = [
        Color::srgb(0.29, 0.33, 0.23),
        Color::srgb(0.44, 0.39, 0.28),
        Color::srgb(0.38, 0.36, 0.29),
        Color::srgb(0.09, 0.12, 0.13),
        Color::srgb(0.27, 0.32, 0.33),
        Color::srgb(0.69, 0.51, 0.36),
        Color::srgb(0.33, 0.40, 0.28),
        Color::srgb(0.33, 0.29, 0.25),
        Color::srgb(0.66, 0.63, 0.53),
        Color::srgb(0.11, 0.16, 0.17),
        Color::srgb(0.40, 0.29, 0.17),
        Color::srgb(0.91, 0.84, 0.59),
        GOLD,
        RED,
        Color::srgb(0.84, 0.85, 0.76),
        Color::srgb(0.37, 0.32, 0.22),
        Color::srgb(0.57, 0.34, 0.25),
        Color::srgb(0.73, 0.70, 0.59),
        Color::srgb(0.065, 0.060, 0.052),
        Color::srgb(0.18, 0.19, 0.13),
        Color::srgb(0.28, 0.15, 0.085),
        Color::srgb(0.15, 0.15, 0.15),
        Color::WHITE,
        Color::WHITE,
        Color::WHITE,
        Color::WHITE,
        Color::srgb(0.82, 0.61, 0.30),
        Color::srgb(0.48, 0.54, 0.41),
        Color::srgb(0.43, 0.51, 0.56),
        Color::srgb(0.64, 0.45, 0.39),
        Color::srgb(0.91, 0.86, 0.70),
        Color::srgb(0.67, 0.29, 0.15),
        Color::srgb(0.22, 0.30, 0.37),
        Color::srgb(0.31, 0.37, 0.29),
        Color::srgb(0.18, 0.235, 0.255),
        Color::WHITE,
    ];
    // Two faction colours precede the material slots indexed by Finish + 2.
    let names: [&str; 36] = [
        "painted_metal",
        "painted_metal",
        "painted_metal",
        "painted_metal",
        "steel",
        "skin",
        "cloth",
        "roof_slate",
        "concrete",
        "glass",
        "wood",
        "light",
        "steel",
        "red",
        "white",
        "soil",
        "brick",
        "plaster",
        "wood",
        "painted_metal",
        "steel",
        "rubber",
        "asphalt",
        "gravel",
        "aircraft_paint",
        "scorch",
        "plaster",
        "plaster",
        "plaster",
        "plaster",
        "plaster",
        "roof_terracotta",
        "roof_slate",
        "roof_slate",
        "glass",
        "a10_livery",
    ];
    let mut templates = std::collections::BTreeMap::<&str, StandardMaterial>::new();
    let finishes: Vec<Handle<StandardMaterial>> = palette
        .into_iter()
        .enumerate()
        .map(|(i, color)| {
            let mut material = if i == Finish::Scorch as usize + 2 {
                crate::ground_impacts::material(&mut images)
            } else if [5, 9, 11, 13, 14, 34].contains(&i) {
                StandardMaterial {
                    base_color: color,
                    perceptual_roughness: if i == 34 {
                        0.38
                    } else if i == 9 {
                        0.2
                    } else {
                        0.85
                    },
                    reflectance: if i == 34 { 0.16 } else { 0.5 },
                    ..default()
                }
            } else {
                let mut material = templates
                    .entry(names[i])
                    .or_insert_with(|| crate::surfaces::material(names[i], color, &mut images))
                    .clone();
                if material.base_color != Color::WHITE {
                    material.base_color = color;
                }
                material
            };
            if [12, 18, 19, 20].contains(&i) || i >= 26 {
                material.base_color = color;
            }
            if i == 22 {
                material.base_color = Color::srgb(0.58, 0.59, 0.56);
                material.reflectance = 0.18;
                material.perceptual_roughness = 0.97;
            }
            if i == 23 {
                material.base_color = Color::srgb(0.76, 0.74, 0.68);
                material.reflectance = 0.2;
                material.perceptual_roughness = 0.96;
            }
            if i == Finish::A10Livery as usize + 2 {
                material.metallic = 0.08;
                material.perceptual_roughness = 0.46;
                material.reflectance = 0.42;
            }
            if i == 8 {
                material.base_color = Color::srgb(0.79, 0.78, 0.72);
                material.reflectance = 0.22;
                material.perceptual_roughness = 0.94;
            }
            if i == Finish::Steel as usize + 2 {
                // Exposed steel catches the sun and sky; it retains its fine
                // roughness map rather than becoming a uniform chrome surface.
                material.base_color = Color::srgb(0.43, 0.46, 0.47);
                material.perceptual_roughness = 0.55;
                material.metallic = 1.;
                material.reflectance = 0.50;
            }
            if i == Finish::Brass as usize + 2 {
                material.perceptual_roughness = 0.60;
                material.metallic = 1.;
            }
            if i == Finish::Dark as usize + 2 {
                material.perceptual_roughness = 0.72;
            }
            if i == Finish::Rust as usize + 2 {
                material.metallic_roughness_texture = None;
                material.metallic = 0.;
                material.perceptual_roughness = 0.94;
                material.reflectance = 0.18;
            }
            if i == Finish::Rubber as usize + 2 {
                material.base_color = Color::srgb(0.16, 0.17, 0.16);
                material.reflectance = 0.16;
            }
            materials.add(material)
        })
        .collect();
    commands.spawn((
        crate::lighting::sun(),
        crate::lighting::Sun,
        crate::graphics::Detail::Balanced.shadow_cascades(),
        crate::lighting::sun_transform(),
    ));
    commands.spawn((
        crate::lighting::fill(),
        crate::lighting::Fill,
        Transform::default().looking_to(-crate::fixed_view::normal(), Vec3::Y),
    ));
    commands.insert_resource(crate::lighting::ambient());
    let camera = commands
        .spawn((
            Camera3d::default(),
            bevy::core_pipeline::prepass::DepthPrepass,
            crate::fixed_view::projection(),
            Msaa::Off,
            Fxaa::default(),
            bevy::render::view::Hdr,
            bevy::post_process::bloom::Bloom {
                intensity: 0.045,
                ..bevy::post_process::bloom::Bloom::NATURAL
            },
            Tonemapping::AgX,
            crate::lighting::grading(),
            crate::lighting::atmosphere([80., 220.]),
            crate::lighting::environment(&mut images),
            ScreenSpaceAmbientOcclusion::default(),
            crate::fixed_view::transform(Vec3::new(30., 0., 80.), 68.),
            RtsCamera,
        ))
        .id();
    if std::env::args().any(|a| a == "--no-ssao") {
        commands
            .entity(camera)
            .remove::<ScreenSpaceAmbientOcclusion>();
    }
    let aircraft_template = crate::surfaces::aircraft_template(&mut images);
    let vehicle_textures = crate::surfaces::VehicleTextures::load(&mut images);
    let aircraft_paint = [
        Color::srgb(0.67, 0.70, 0.71),
        Color::srgb(0.70, 0.63, 0.50),
        Color::srgb(0.29, 0.34, 0.255),
        Color::srgb(0.57, 0.48, 0.32),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, color)| {
        let mut paint = aircraft_template.clone();
        paint.base_color = color;
        vehicle_textures.apply(crate::surfaces::AIRCRAFT_DETAIL_NAMES[index], &mut paint);
        materials.add(paint)
    })
    .collect::<Vec<_>>()
    .try_into()
    .unwrap();
    let aircraft_finishes = finishes
        .iter()
        .enumerate()
        .map(|(index, handle)| {
            let mut material = materials.get(handle).unwrap().clone();
            if vehicle_textures.apply_finish(index, &mut material) {
                materials.add(material)
            } else {
                handle.clone()
            }
        })
        .collect();
    let transparent_roofs = finishes
        .iter()
        .map(|handle| {
            let mut material = materials.get(handle).unwrap().clone();
            material.base_color = material.base_color.with_alpha(0.12);
            material.alpha_mode = AlphaMode::Blend;
            material.cull_mode = None;
            material.double_sided = true;
            materials.add(material)
        })
        .collect();
    let [concrete_ground, foundation_ground] = crate::ground_finish::materials(
        &mut commands,
        &mut images,
        &mut ground_materials,
        graphics.settings.textures.texture_size(),
    );
    commands.insert_resource(Art {
        // The modern utility airframe has painted skins and composite blades;
        // the heavily chipped vehicle albedos are inappropriate for these surfaces.
        huey_paint: [
            Color::srgb(0.39, 0.435, 0.415),
            Color::srgb(0.50, 0.465, 0.395),
        ]
        .map(|base_color| {
            let mut paint = aircraft_template.clone();
            paint.base_color = base_color;
            paint.perceptual_roughness = 0.74;
            paint.metallic = 0.035;
            paint.reflectance = 0.30;
            materials.add(paint)
        }),
        huey_panels: [
            Color::srgb(0.355, 0.40, 0.38),
            Color::srgb(0.46, 0.425, 0.36),
        ]
        .map(|base_color| {
            let mut paint = aircraft_template.clone();
            paint.base_color = base_color;
            paint.perceptual_roughness = 0.77;
            paint.metallic = 0.03;
            paint.reflectance = 0.28;
            materials.add(paint)
        }),
        huey_dark: materials.add(StandardMaterial {
            base_color: Color::srgb(0.032, 0.040, 0.043),
            perceptual_roughness: 0.86,
            metallic: 0.03,
            reflectance: 0.22,
            ..default()
        }),
        huey_metal: materials.add(StandardMaterial {
            base_color: Color::srgb(0.19, 0.205, 0.21),
            perceptual_roughness: 0.55,
            metallic: 0.60,
            reflectance: 0.30,
            ..default()
        }),
        apache_glass: materials.add(StandardMaterial {
            base_color: Color::srgb(0.055, 0.105, 0.12),
            perceptual_roughness: 0.18,
            metallic: 0.04,
            reflectance: 0.60,
            clearcoat: 0.5,
            clearcoat_perceptual_roughness: 0.16,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        vehicle_atlases: crate::vehicle_atlas::Art::load(
            &mut images,
            &mut vehicle_materials,
            graphics.settings.textures,
        ),
        foundation_ground,
        concrete_ground,
        vehicle_paint: [0, 1].map(|owner| {
            let mut base = crate::surfaces::armour(owner, &mut images);
            vehicle_textures.apply(["armour_green", "armour_sand"][owner], &mut base);
            vehicle_materials.add(crate::model_finish::VehicleMaterial {
                base,
                extension: crate::model_finish::VehicleFinish::new(true),
            })
        }),
        vehicle_finishes: finishes
            .iter()
            .enumerate()
            .map(|(index, handle)| {
                let mut base = materials.get(handle).unwrap().clone();
                vehicle_textures.apply_finish(index, &mut base);
                vehicle_materials.add(crate::model_finish::VehicleMaterial {
                    base,
                    extension: crate::model_finish::VehicleFinish::surface(index),
                })
            })
            .collect(),
        transparent_roofs,
        aircraft_paint,
        aircraft_finishes,
        vehicle_detail_maps: vehicle_textures.count(),
        impact_scar_material: finishes[Finish::Scorch as usize + 2].clone(),
        meshes: shapes,
        materials: finishes,
    });
}
fn spawn_building_parts(
    commands: &mut Commands,
    actor: Entity,
    art: &Art,
    p: &Piece,
    meshes: &mut Assets<Mesh>,
    shapes: &[Mesh],
    parts: Vec<models::Part>,
) -> Vec<Handle<Mesh>> {
    let mut handles = vec![];
    let mut groups: std::collections::BTreeMap<BuildingBatchKey, Vec<models::Part>> =
        std::collections::BTreeMap::new();
    for mut part in parts {
        if p.lots.is_empty() && part.finish == Finish::Glass {
            part.roof_lot = Some(usize::MAX);
        }
        groups
            .entry((
                part.roof_lot,
                part.wall_normal.map(|n| n.to_array().map(f32::to_bits)),
            ))
            .or_default()
            .push(part);
    }
    commands.entity(actor).with_children(|parent| {
        for (roof, normal, finish, mut mesh) in
            groups.into_iter().flat_map(|((roof, normal), parts)| {
                models::bake(&parts, shapes)
                    .into_iter()
                    .map(move |(finish, mesh)| (roof, normal, finish, mesh))
            })
        {
            if roof.is_none()
                && let Some(surface) = crate::ground_finish::surface(&p.kind, finish)
                && let Some(mut floor) = crate::ground_finish::take_floor(&mut mesh)
            {
                crate::ground_finish::scale_uv(&mut floor, p.model_scale);
                let foundation = surface == crate::ground_finish::Surface::Foundation;
                if foundation {
                    crate::ground_finish::foundation_uv(&mut floor);
                }
                let handle = meshes.add(floor);
                handles.push(handle.clone());
                parent.spawn((
                    Mesh3d(handle),
                    MeshMaterial3d(if foundation {
                        art.foundation_ground.clone()
                    } else {
                        art.concrete_ground.clone()
                    }),
                    Transform::default(),
                ));
                if mesh.indices().is_some_and(|indices| indices.is_empty()) {
                    continue;
                }
            }
            crate::surfaces::weather_architecture(&mut mesh, finish);
            let material = if finish == Finish::Team {
                if p.owner == 0 {
                    0
                } else if p.owner == 1 {
                    1
                } else {
                    2
                }
            } else {
                finish as usize + 2
            };
            let handle = meshes.add(mesh);
            handles.push(handle.clone());
            let mut child = parent.spawn((
                Mesh3d(handle),
                MeshMaterial3d(if finish == Finish::AircraftPaint {
                    art.aircraft_paint[usize::from(!matches!(p.kind.as_str(), "plane" | "c130"))
                        * 2
                        + p.owner.clamp(0, 1) as usize]
                        .clone()
                } else if p.category == "aircraft" {
                    art.aircraft_finishes[material].clone()
                } else {
                    art.materials[material].clone()
                }),
                Transform::default(),
            ));
            if let Some(index) = roof {
                child.insert(RoofLayer {
                    building: p.id.clone(),
                    lot: (index != usize::MAX).then_some(index),
                    wall_normal: normal.map(|n| {
                        Quat::from_rotation_y(p.heading) * Vec3::from_array(n.map(f32::from_bits))
                    }),
                    center: p.position()
                        + Quat::from_rotation_y(p.heading)
                            * p.lots
                                .get(index)
                                .map(|l| Vec3::new(l.at[0], 0., l.at[1]) * p.model_scale)
                                .unwrap_or(Vec3::ZERO),
                    opaque: art.materials[material].clone(),
                    transparent: art.transparent_roofs[material].clone(),
                });
            }
        }
    });
    handles
}

fn spawn_lot(
    commands: &mut Commands,
    actor: Entity,
    art: &Art,
    p: &Piece,
    index: usize,
    meshes: &mut Assets<Mesh>,
    shapes: &[Mesh],
) -> Vec<Handle<Mesh>> {
    let _timing = crate::performance::span("scene.lot");
    let root = commands
        .spawn((Transform::default(), Visibility::default(), ChildOf(actor)))
        .id();
    let mut parts = models::house_parts(&p.lots[index]);
    for part in &mut parts {
        if part.roof_lot.is_some() {
            part.roof_lot = Some(index);
        }
    }
    let handles = spawn_building_parts(commands, root, art, p, meshes, shapes, parts);
    commands.entity(root).insert(LotGeometry {
        building: p.id.clone(),
        index,
        meshes: handles.clone(),
    });
    handles
}

fn spawn_piece(
    commands: &mut Commands,
    art: &Art,
    p: &Piece,
    meshes: &mut Assets<Mesh>,
    terrain: &crate::terrain::Terrain,
    infantry: &crate::infantry::InfantryArt,
) {
    if p.category == "troop" {
        crate::infantry::spawn(commands, infantry, p);
        return;
    }
    let parts = if p.building && !p.lots.is_empty() {
        vec![]
    } else {
        models::parts(p)
    };
    let sockets = models::WeaponSockets::from_parts(&parts);
    let actor = commands
        .spawn((
            Actor(p.id.clone()),
            Walk::default(),
            sockets,
            Transform::from_translation(p.position())
                .with_scale(p.model_transform_scale())
                .with_rotation(Quat::from_rotation_y(if p.building {
                    p.heading
                } else {
                    std::f32::consts::FRAC_PI_2 - p.heading
                })),
            Visibility::default(),
        ))
        .id();
    if p.building {
        let shapes: Vec<_> = art
            .meshes
            .iter()
            .map(|h| meshes.get(h).unwrap().clone())
            .collect();
        let mut handles = if p.lots.is_empty() {
            spawn_building_parts(commands, actor, art, p, meshes, &shapes, parts)
        } else {
            (0..p.lots.len())
                .flat_map(|index| spawn_lot(commands, actor, art, p, index, meshes, &shapes))
                .collect()
        };
        if ["town", "village"].contains(&p.kind.as_str())
            && terrain.id != crate::visual_demo::ID
            && terrain.id != crate::gis_prototype::ID
        {
            let dressing = models::yard_parts(p, terrain);
            handles.extend(spawn_building_parts(
                commands, actor, art, p, meshes, &shapes, dressing,
            ));
            for (finish, mut mesh) in models::settlement_ground(p, terrain) {
                if finish == Finish::Concrete {
                    crate::ground_finish::scale_uv(&mut mesh, p.model_scale);
                }
                let handle = meshes.add(mesh);
                handles.push(handle.clone());
                let mut child =
                    commands.spawn((Mesh3d(handle), Transform::default(), ChildOf(actor)));
                if finish == Finish::Concrete {
                    child.insert(MeshMaterial3d(art.concrete_ground.clone()));
                } else {
                    child.insert(MeshMaterial3d(art.materials[finish as usize + 2].clone()));
                }
            }
        }
        commands.entity(actor).insert(BakedGeometry(handles));
        return;
    }
    let shapes: Vec<_> = art
        .meshes
        .iter()
        .map(|h| meshes.get(h).unwrap().clone())
        .collect();
    let mut handles = vec![];
    commands.entity(actor).with_children(|parent| {
        for (role, pivot, finish, mut mesh) in models::bake_animated(parts, &shapes) {
            if p.category == "aircraft" {
                crate::surfaces::weather_aircraft(&mut mesh, finish);
                crate::model_finish::apply(&mut mesh, &p.kind);
            } else if p.category == "vehicle" {
                crate::model_finish::apply(&mut mesh, &p.kind);
                crate::surfaces::weather_vehicle(&mut mesh, finish);
            }
            let material = if finish == Finish::Team {
                if p.owner == 0 {
                    0
                } else if p.owner == 1 {
                    1
                } else {
                    2
                }
            } else {
                finish as usize + 2
            };
            let atlas = art
                .vehicle_atlases
                .material(&mut mesh, &p.kind, role, finish, p.owner);
            let handle = meshes.add(mesh);
            handles.push(handle.clone());
            let mut entity = parent.spawn((Mesh3d(handle), Transform::default()));
            if let Some(atlas) = atlas {
                entity.insert(MeshMaterial3d(atlas));
            } else if p.category == "vehicle" {
                entity.insert(MeshMaterial3d(if finish == Finish::Team {
                    art.vehicle_paint[p.owner.clamp(0, 1) as usize].clone()
                } else {
                    art.vehicle_finishes[material].clone()
                }));
            } else {
                entity.insert(MeshMaterial3d(
                    if p.kind == "huey" && finish == Finish::AircraftPaint {
                        art.huey_paint[p.owner.clamp(0, 1) as usize].clone()
                    } else if p.kind == "huey" && finish == Finish::Sage {
                        art.huey_panels[p.owner.clamp(0, 1) as usize].clone()
                    } else if p.kind == "huey" && matches!(finish, Finish::Dark | Finish::Rubber) {
                        art.huey_dark.clone()
                    } else if p.kind == "huey" && finish == Finish::Steel {
                        art.huey_metal.clone()
                    } else if matches!(p.kind.as_str(), "helicopter" | "huey")
                        && finish == Finish::AircraftGlass
                    {
                        art.apache_glass.clone()
                    } else if finish == Finish::AircraftPaint {
                        art.aircraft_paint[usize::from(!matches!(
                            p.kind.as_str(),
                            "plane" | "c130"
                        )) * 2
                            + p.owner.clamp(0, 1) as usize]
                            .clone()
                    } else if p.category == "aircraft" {
                        art.aircraft_finishes[material].clone()
                    } else {
                        art.materials[material].clone()
                    },
                ));
            }
            if role != 0 {
                entity.insert(Limb {
                    actor,
                    base: Transform::default(),
                    role,
                    pivot,
                });
            }
        }
    });
    commands.entity(actor).insert(BakedGeometry(handles));
}
pub fn animate(
    walks: Query<&Walk, With<Actor>>,
    mut limbs: Query<(&Limb, &mut Transform), Without<Actor>>,
) {
    for (limb, mut transform) in &mut limbs {
        if let Ok(walk) = walks.get(limb.actor) {
            transform.set_if_neq(crate::animation::joint(
                limb.base,
                limb.pivot,
                limb.role,
                &walk.pose,
                if matches!(limb.role, 30 | 32) {
                    walk.rotor_phase
                } else {
                    walk.phase
                },
            ));
        }
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn sync_scene(
    mut commands: Commands,
    battle: Res<Battle>,
    game: Res<crate::simulation::NativeGame>,
    art: Res<Art>,
    infantry: Res<crate::infantry::InfantryArt>,
    time: Res<Time>,
    mut actors: Query<(
        Entity,
        &Actor,
        &mut Transform,
        &mut Walk,
        Option<&mut BakedGeometry>,
    )>,
    lots: Query<(Entity, &LotGeometry)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut owners: Local<HashMap<String, Appearance>>,
    mut clock: Local<(u64, f32)>,
) {
    let _timing = crate::performance::span("scene.sync");
    if clock.0 != battle.snapshot.tick {
        *clock = (battle.snapshot.tick, 0.);
    } else {
        clock.1 = (clock.1 + time.delta_secs()).min(0.08);
    }
    let pieces: HashMap<_, _> = battle
        .snapshot
        .entities
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let mut present = HashSet::new();
    for (entity, actor, mut transform, mut walk, mut baked) in &mut actors {
        if let Some(p) = pieces.get(actor.0.as_str()) {
            let appearance = Appearance::from(*p);
            if let Some(changed) = owners
                .get(&p.id)
                .and_then(|old| old.changed_lots(&appearance))
            {
                if !changed.is_empty() {
                    let shapes: Vec<_> = art
                        .meshes
                        .iter()
                        .map(|h| meshes.get(h).unwrap().clone())
                        .collect();
                    for index in changed {
                        for (root, lot) in &lots {
                            if lot.building == p.id && lot.index == index {
                                commands.entity(root).despawn();
                                for handle in &lot.meshes {
                                    meshes.remove(handle.id());
                                }
                                if let Some(baked) = &mut baked {
                                    baked.0.retain(|h| !lot.meshes.contains(h));
                                }
                            }
                        }
                        let handles =
                            spawn_lot(&mut commands, entity, &art, p, index, &mut meshes, &shapes);
                        if let Some(baked) = &mut baked {
                            baked.0.extend(handles);
                        }
                    }
                    owners.insert(p.id.clone(), appearance);
                }
            } else if owners.get(&p.id) != Some(&appearance) {
                commands.entity(entity).despawn();
                if let Some(baked) = baked {
                    for mesh in &baked.0 {
                        meshes.remove(mesh.id());
                    }
                }
                continue;
            }
            present.insert(actor.0.clone());
            let mut target = p.position()
                + if p.downed && p.category == "troop" {
                    Vec3::Y * 0.20 * p.model_scale
                } else {
                    Vec3::ZERO
                }
                + if battle.snapshot.paused {
                    Vec3::ZERO
                } else {
                    Vec3::new(p.velocity[0], 0., p.velocity[1]) * clock.1
                };
            if p.category == "vehicle" && !game.safe_vehicle_prediction(p, [target.x, target.z]) {
                target = p.position();
            }
            walk.speed = if p.building {
                0.
            } else {
                transform.translation.distance(target) * 12.
            };
            walk.pose
                .blend(crate::animation::Pose::target(p), time.delta_secs());
            if !battle.snapshot.paused {
                walk.phase += time.delta_secs() * (2. + walk.pose.speed.min(5.) * 3.5);
                walk.rotor_phase += time.delta_secs() * 2. * walk.pose.rotor_power;
            }
            let mut next_transform = *transform;
            next_transform.translation = transform
                .translation
                .lerp(target, (time.delta_secs() * 18.).min(1.));
            if let Some(end) = p.bridge_end {
                next_transform.rotation = Quat::from_rotation_y(
                    std::f32::consts::FRAC_PI_2 - (end[1] - p.z).atan2(end[0] - p.x),
                );
            } else if p.building {
                next_transform.rotation = Quat::from_rotation_y(p.heading);
                next_transform.scale = p.model_transform_scale();
            } else {
                let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 - p.heading)
                    * if p.downed {
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
                    } else {
                        Quat::from_rotation_x(-p.flight_pitch)
                            * Quat::from_rotation_z(p.flight_bank)
                    };
                next_transform.rotation = transform
                    .rotation
                    .slerp(rotation, 1. - (-time.delta_secs() * 14.).exp());
                next_transform.scale = p.model_transform_scale();
            }
            transform.set_if_neq(next_transform);
        } else {
            commands.entity(entity).despawn();
            if let Some(baked) = baked {
                for mesh in &baked.0 {
                    meshes.remove(mesh.id());
                }
            }
            owners.remove(&actor.0);
        }
    }
    for p in &battle.snapshot.entities {
        if !present.contains(&p.id) {
            if p.category == "troop" && !infantry.ready() {
                continue;
            }
            spawn_piece(
                &mut commands,
                &art,
                p,
                &mut meshes,
                &game.terrain,
                &infantry,
            );
            owners.insert(p.id.clone(), Appearance::from(p));
        }
    }
}
fn prop_transform(
    p: &crate::simulation::BattlefieldProp,
    terrain: &crate::terrain::Terrain,
) -> Transform {
    let crash = p.wreck.as_ref().and_then(|w| w.crash.as_ref());
    Transform::from_xyz(
        p.at[0],
        crash
            .map(|c| c.world_y)
            .unwrap_or_else(|| terrain.height(p.at[0], p.at[1])),
        p.at[1],
    )
    .with_rotation(
        Quat::from_rotation_y(p.heading)
            * crash
                .map(|c| Quat::from_rotation_x(c.pitch) * Quat::from_rotation_z(c.roll))
                .unwrap_or(Quat::IDENTITY),
    )
    .with_scale(
        p.wreck
            .as_ref()
            .map(|w| w.piece.model_transform_scale())
            .unwrap_or(Vec3::ONE),
    )
}
pub fn sync_props(
    mut commands: Commands,
    battle: Res<Battle>,
    game: Res<crate::simulation::NativeGame>,
    art: Res<Art>,
    time: Res<Time>,
    ground: Query<&crate::landscape::Ground>,
    mut props: Query<(
        Entity,
        &PropActor,
        Option<&BakedGeometry>,
        &mut Transform,
        Option<&mut crate::ground_impacts::Scar>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let _timing = crate::performance::span("scene.props");
    let mut present = HashSet::new();
    let projection = crate::ground_impacts::Projection::new(ground.iter().map(|g| &g.0), &meshes);
    let source = crate::terrain::render_source_key(&game.terrain);
    let lookup: HashMap<_, _> = battle
        .snapshot
        .props
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    for (entity, p, geometry, mut transform, scar) in &mut props {
        if let Some(current) = lookup
            .get(p.0.as_str())
            .filter(|a| (a.hp <= 0.) == p.1 && prop_signature(a) == p.2)
        {
            present.insert(p.0.clone());
            let target = prop_transform(current, &game.terrain);
            if let Some(mut scar) = scar {
                scar.refresh(&game.terrain, &source, &projection, &mut meshes);
                transform.set_if_neq(target);
                continue;
            }
            let mut next_transform = *transform;
            next_transform.translation = transform
                .translation
                .lerp(target.translation, (time.delta_secs() * 24.).min(1.));
            next_transform.rotation = transform
                .rotation
                .slerp(target.rotation, (time.delta_secs() * 24.).min(1.));
            transform.set_if_neq(next_transform);
        } else {
            if let Some(geometry) = geometry {
                for mesh in &geometry.0 {
                    meshes.remove(mesh.id());
                }
            }
            commands.entity(entity).despawn();
        }
    }
    for p in &battle.snapshot.props {
        if present.contains(&p.id) {
            continue;
        }
        let root = commands
            .spawn((
                PropActor(p.id.clone(), p.hp <= 0., prop_signature(p)),
                crate::landscape::Environment,
                prop_transform(p, &game.terrain),
                Visibility::default(),
            ))
            .id();
        if p.kind == "crater" {
            let mark = crate::ground_impacts::Mark::from_prop(p);
            let mesh = crate::ground_impacts::projected(
                &game.terrain,
                p.at,
                p.heading,
                mark,
                &projection,
                &meshes,
            );
            let handle = meshes.add(mesh);
            commands.entity(root).with_children(|parent| {
                parent.spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(art.impact_scar_material.clone()),
                    Transform::default(),
                    bevy::light::NotShadowCaster,
                ));
            });
            commands.entity(root).insert((
                crate::ground_impacts::Scar::new(p, &game.terrain, handle.clone(), &projection),
                BakedGeometry(vec![handle]),
            ));
            continue;
        }
        if let Some(wreck) = &p.wreck {
            let shapes: Vec<_> = art
                .meshes
                .iter()
                .map(|h| meshes.get(h).unwrap().clone())
                .collect();
            let mut handles = vec![];
            commands.entity(root).with_children(|parent| {
                for (finish, mesh) in models::bake(&crate::destruction::wreck_parts(wreck), &shapes)
                {
                    let handle = meshes.add(mesh);
                    handles.push(handle.clone());
                    parent.spawn((
                        Mesh3d(handle),
                        MeshMaterial3d(art.materials[finish as usize + 2].clone()),
                        Transform::default(),
                    ));
                }
            });
            commands.entity(root).insert(BakedGeometry(handles));
        } else {
            commands.entity(root).with_children(|root| {
                for part in models::battlefield(&p.kind, p.hp <= 0.) {
                    root.spawn((
                        Mesh3d(art.meshes[part.shape as usize].clone()),
                        MeshMaterial3d(art.materials[part.finish as usize + 2].clone()),
                        part.transform,
                    ));
                }
            });
        }
    }
}

pub fn overlays(
    mut gizmos: Gizmos,
    battle: Res<Battle>,
    controls: Res<Controls>,
    front: Res<crate::frontend::Frontend>,
    menu: Res<crate::menu::MainMenu>,
    game: Res<crate::simulation::NativeGame>,
) {
    if controls.hud_hidden
        || front.world
        || menu.open
        || (crate::visual_demo::enabled() && !crate::visual_demo::command_mode())
    {
        return;
    }
    if battle.snapshot.training.is_some() && game.terrain.id != crate::visual_demo::ID {
        let at = |x: f32, z: f32| Vec3::new(x, game.terrain.height(x, z) + 0.12, z);
        gizmos.line(at(96., 48.), at(96., 260.), GOLD);
        for z in [72., 120., 168., 216.] {
            gizmos.line(
                at(96., z),
                at(312., z),
                Color::srgba(0.85, 0.78, 0.55, 0.35),
            );
            for metres in (25..=200).step_by(25) {
                let x = 96. + metres as f32;
                gizmos.line(at(x, z - 2.), at(x, z + 2.), GOLD);
            }
        }
    }
    for p in &battle.snapshot.entities {
        let pos = p.position();
        let selected = controls.selected.contains(&p.id);
        if p.kind == "objective" && (controls.capture_zone.as_ref() == Some(&p.id) || selected) {
            let color = if p.owner == 0 {
                BLUE
            } else if p.owner == 1 {
                RED
            } else {
                GOLD
            };
            for i in 0..96 {
                let a = i as f32 * std::f32::consts::TAU / 96.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 96.;
                let point = |angle: f32| {
                    let x = p.x + angle.cos() * crate::simulation::CAPTURE_RADIUS;
                    let z = p.z + angle.sin() * crate::simulation::CAPTURE_RADIUS;
                    Vec3::new(x, game.terrain.height(x, z) + 0.16, z)
                };
                gizmos.line(point(a), point(b), color);
            }
        }
        if selected || p.kind == "objective" {
            gizmos.circle(
                Isometry3d::new(
                    pos + Vec3::Y * 0.15,
                    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                ),
                if p.building {
                    2.7
                } else if p.category == "troop" {
                    0.75
                } else {
                    2.6
                },
                if selected { BLUE } else { GOLD },
            );
        }
        if selected && !p.building {
            gizmos.circle(
                Isometry3d::new(
                    pos + Vec3::Y * 0.16,
                    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                ),
                p.sight,
                Color::srgba(0.44, 0.69, 0.70, 0.30),
            );
        }
        if selected || p.hp < p.max_hp {
            let width = if p.category == "troop" { 0.9 } else { 2. };
            let a = pos
                + Vec3::new(
                    -width * 0.5,
                    if p.category == "troop" { 1.95 } else { 4. },
                    0.,
                );
            gizmos.line(a, a + Vec3::X * width, Color::srgb(0.3, 0.1, 0.1));
            gizmos.line(
                a + Vec3::Y * 0.02,
                a + Vec3::Y * 0.02 + Vec3::X * width * (p.hp / p.max_hp.max(1.)).clamp(0., 1.),
                BLUE,
            );
        }
        if p.category == "troop" && (selected || p.suppression > 0.45 || p.downed) {
            let color = if p.downed || p.suppression >= 0.8 {
                RED
            } else if p.suppression >= 0.45 {
                GOLD
            } else if p.cover == 2 {
                Color::srgb(0.38, 0.78, 0.48)
            } else if p.cover == 1 {
                GOLD
            } else {
                BLUE
            };
            gizmos.sphere(
                Isometry3d::from_translation(pos + Vec3::Y * 2.2),
                0.12,
                color,
            );
        }
    }
    for j in &battle.snapshot.jobs {
        if j["kind"] == "building" {
            let pos = Vec3::new(
                j["x"].as_f64().unwrap_or(0.) as f32,
                1.,
                j["y"].as_f64().unwrap_or(0.) as f32,
            );
            gizmos.cube(
                Transform::from_translation(pos).with_scale(Vec3::new(4., 2., 4.)),
                GOLD,
            );
        }
    }
    if let Some(p) = controls.ground {
        if let Some(start) = controls.bridge_start {
            gizmos.line(start + Vec3::Y, p + Vec3::Y, GOLD);
        }
        if controls.build.is_some() {
            gizmos.cube(
                Transform::from_translation(p + Vec3::Y).with_scale(Vec3::new(4., 2., 4.)),
                GOLD,
            );
        } else if controls.attack_move {
            gizmos.circle(
                Isometry3d::new(
                    p + Vec3::Y * 0.2,
                    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                ),
                1.,
                RED,
            );
        }
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn town_damage_invalidates_only_changed_lots_and_layout_changes_rebuild_the_town() {
        let mut p = Piece {
            kind: "town".into(),
            building: true,
            lots: crate::settlement_layout::lots("town", "incremental"),
            ..default()
        };
        let old = Appearance::from(&p);
        p.lots[3].integrity = 0.4;
        p.lots[7].integrity = 0.;
        let damaged = Appearance::from(&p);
        assert_eq!(old.changed_lots(&damaged), Some(vec![3, 7]));
        assert_eq!(damaged.changed_lots(&damaged), Some(vec![]));
        p.lots[0].at[0] += 1.;
        assert!(damaged.changed_lots(&Appearance::from(&p)).is_none());
        p.lots[0].at[0] -= 1.;
        p.model_scale = 2.;
        assert!(damaged.changed_lots(&Appearance::from(&p)).is_none());
        p.model_scale = 1.;
        p.lots.pop();
        assert!(damaged.changed_lots(&Appearance::from(&p)).is_none());
    }
    #[test]
    fn house_capture_keeps_geometry_but_nonfatal_damage_and_collapse_change_it() {
        let mut p = Piece {
            kind: "town".into(),
            building: true,
            lots: crate::settlement_layout::lots("town", "signature"),
            ..default()
        };
        let old = Appearance::from(&p);
        let geometry = |p: &Piece| {
            models::parts(p)
                .into_iter()
                .map(|part| (part.shape as usize, part.finish as usize, part.transform))
                .collect::<Vec<_>>()
        };
        let before = geometry(&p);
        p.owner = 1;
        assert!(old == Appearance::from(&p));
        assert_eq!(before, geometry(&p));
        p.lots[0].integrity = 0.4;
        assert!(old != Appearance::from(&p));
        assert_ne!(before, geometry(&p));
        let damaged = geometry(&p);
        let restored: Piece = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(damaged, geometry(&restored));
        p.lots[0].integrity = 0.;
        assert!(old != Appearance::from(&p));
        assert_ne!(damaged, geometry(&p));
    }
    #[test]
    fn occupied_walls_follow_the_camera_and_restore_on_exit() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Battle>()
            .add_systems(Update, roof_visibility);
        let mut materials = Assets::<StandardMaterial>::default();
        let handle = materials.add(StandardMaterial::default());
        let camera = app
            .world_mut()
            .spawn((
                RtsCamera,
                GlobalTransform::from_translation(Vec3::new(20., 20., 30.)),
            ))
            .id();
        let walls: Vec<_> = [Vec3::Z, -Vec3::Z, Vec3::X, -Vec3::X]
            .into_iter()
            .map(|normal| {
                app.world_mut()
                    .spawn((
                        RoofLayer {
                            building: "house".into(),
                            lot: Some(0),
                            wall_normal: Some(normal),
                            center: Vec3::ZERO,
                            opaque: handle.clone(),
                            transparent: handle.clone(),
                        },
                        MeshMaterial3d(handle.clone()),
                        Visibility::Inherited,
                    ))
                    .id()
            })
            .collect();
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .push(Piece {
                garrison: Some(crate::simulation::Garrison {
                    building: "house".into(),
                    lot: Some(0),
                    window: 0,
                    entered: true,
                    ready_at: 0,
                    door_stage: 2,
                    exiting: false,
                }),
                ..default()
            });
        app.update();
        for (i, id) in walls.iter().enumerate() {
            assert_eq!(
                *app.world().get::<Visibility>(*id).unwrap(),
                if i % 2 == 0 {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                }
            );
        }
        let wall_ticks: Vec<_> = walls
            .iter()
            .map(|&id| {
                (
                    app.world()
                        .entity(id)
                        .get_ref::<Visibility>()
                        .unwrap()
                        .last_changed(),
                    app.world()
                        .entity(id)
                        .get_ref::<MeshMaterial3d<StandardMaterial>>()
                        .unwrap()
                        .last_changed(),
                )
            })
            .collect();
        app.update();
        for (&id, ticks) in walls.iter().zip(wall_ticks) {
            assert_eq!(
                app.world()
                    .entity(id)
                    .get_ref::<Visibility>()
                    .unwrap()
                    .last_changed(),
                ticks.0
            );
            assert_eq!(
                app.world()
                    .entity(id)
                    .get_ref::<MeshMaterial3d<StandardMaterial>>()
                    .unwrap()
                    .last_changed(),
                ticks.1
            );
        }
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() =
            GlobalTransform::from_translation(Vec3::new(-20., 20., -30.));
        app.update();
        for (i, id) in walls.iter().enumerate() {
            assert_eq!(
                *app.world().get::<Visibility>(*id).unwrap(),
                if i % 2 == 1 {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                }
            );
        }
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .clear();
        app.update();
        assert!(
            walls
                .iter()
                .all(|id| *app.world().get::<Visibility>(*id).unwrap() == Visibility::Inherited)
        );
    }
    #[test]
    fn only_the_occupied_house_roof_is_transparent_and_it_recovers_on_exit() {
        let mut materials = Assets::<StandardMaterial>::default();
        let opaque = materials.add(StandardMaterial::default());
        let transparent = materials.add(StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Battle>()
            .add_systems(Update, roof_visibility);
        let roofs: Vec<_> = [Some(0), Some(1), None]
            .into_iter()
            .map(|lot| {
                app.world_mut()
                    .spawn((
                        RoofLayer {
                            building: "houses".into(),
                            lot,
                            wall_normal: None,
                            center: Vec3::ZERO,
                            opaque: opaque.clone(),
                            transparent: transparent.clone(),
                        },
                        MeshMaterial3d(opaque.clone()),
                        Visibility::Inherited,
                    ))
                    .id()
            })
            .collect();
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .push(Piece {
                garrison: Some(crate::simulation::Garrison {
                    building: "houses".into(),
                    lot: Some(0),
                    window: 0,
                    entered: true,
                    ready_at: 0,
                    door_stage: 2,
                    exiting: false,
                }),
                ..default()
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(roofs[0])
                .unwrap()
                .0,
            transparent
        );
        assert!(
            app.world()
                .get::<bevy::light::NotShadowCaster>(roofs[0])
                .is_some()
        );
        for id in &roofs[1..] {
            assert_eq!(
                app.world()
                    .get::<MeshMaterial3d<StandardMaterial>>(*id)
                    .unwrap()
                    .0,
                opaque
            );
        }
        {
            let mut battle = app.world_mut().resource_mut::<Battle>();
            let g = battle.snapshot.entities[0].garrison.as_mut().unwrap();
            g.exiting = true;
            g.door_stage = 1;
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(roofs[0])
                .unwrap()
                .0,
            opaque
        );
        assert!(
            app.world()
                .get::<bevy::light::NotShadowCaster>(roofs[0])
                .is_none()
        );
    }
    #[test]
    fn a_moving_crash_keeps_its_mesh_signature_and_render_height_above_the_ground() {
        let terrain = crate::terrain::Terrain::default();
        let piece = Piece {
            id: "heli".into(),
            kind: "helicopter".into(),
            category: "aircraft".into(),
            airborne: true,
            altitude: Some(20.),
            elevation: terrain.height(30., 30.),
            ..default()
        };
        let mut prop = crate::simulation::BattlefieldProp {
            scar: None,
            id: "wreck-heli".into(),
            kind: "wreck".into(),
            at: [30., 30.],
            heading: 0.,
            hp: 0.,
            wreck: Some(crate::destruction::Wreck::new(piece, 20)),
        };
        let signature = prop_signature(&prop);
        let transform = prop_transform(&prop, &terrain);
        assert!((transform.translation.y - terrain.height(30., 30.) - 20.).abs() < 0.001);
        prop.at = [31., 32.];
        prop.heading = 0.4;
        let crash = prop.wreck.as_mut().unwrap().crash.as_mut().unwrap();
        crash.world_y -= 5.;
        crash.roll = 1.;
        assert_eq!(signature, prop_signature(&prop));
        assert!(prop_transform(&prop, &terrain).rotation != Quat::IDENTITY);
        prop.wreck.as_mut().unwrap().crash = None;
        assert_eq!(
            prop_transform(&prop, &terrain).translation.y,
            terrain.height(31., 32.)
        );
    }
}
