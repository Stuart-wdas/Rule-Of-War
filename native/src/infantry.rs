//! Shared authored infantry mesh, independent skeletons and action-driven poses.
use crate::{animation::Pose, models::WeaponSockets, runtime::Piece, scene::Walk};
use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    gltf::{Gltf, GltfLoaderSettings, GltfMesh, GltfNode, GltfSkin},
    math::Affine3A,
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
    scene::SceneInstanceReady,
};
use std::{collections::HashMap, path::Path};

const PATH: &str = "embedded://infantry/rifleman.glb";
// Keep the established world scale: the authored 1.85 m model replaces a 1.52 m kit.
const SCALE: f32 = 0.82;
const CLIPS: [&str; 4] = ["Idle_Rifle", "Walk_Rifle", "Fire_Rifle", "Crouch_Rifle"];

#[derive(Resource)]
pub struct InfantryArt {
    source: Handle<Gltf>,
    shared: Option<Shared>,
}
struct Shared {
    scene: Handle<Scene>,
    graph: Handle<AnimationGraph>,
    nodes: [AnimationNodeIndex; 4],
    armed: Handle<Mesh>,
    unarmed: Handle<Mesh>,
    uniforms: [Handle<StandardMaterial>; 2],
    badge: Handle<Mesh>,
    mantle: Handle<Mesh>,
    team: [Handle<StandardMaterial>; 2],
    white: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    tube: Handle<Mesh>,
    dark: Handle<StandardMaterial>,
    olive: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
}
impl InfantryArt {
    pub fn ready(&self) -> bool {
        self.shared.is_some()
    }
}

pub fn register(app: &mut App) {
    app.world()
        .resource::<EmbeddedAssetRegistry>()
        .insert_asset(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/infantry/infantry_rifleman.glb"),
            Path::new("infantry/rifleman.glb"),
            include_bytes!("../../assets/infantry/infantry_rifleman.glb").as_slice(),
        );
    app.add_systems(Startup, load);
}
fn load(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(InfantryArt {
        source: server.load_with_settings(PATH, |s: &mut GltfLoaderSettings| {
            s.load_cameras = false;
            s.load_lights = false;
            // The rest of the game's models face +Z, as does this GLB.
            s.convert_coordinates = Some(default());
        }),
        shared: None,
    });
}

/// Remove weapon-only triangles while retaining precisely the same skin and UVs.
fn without_weapon(mesh: &Mesh, weapon: usize) -> Mesh {
    let Some(VertexAttributeValues::Uint16x4(joints)) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
    else {
        panic!("Infantry GLB requires four joint indices per vertex");
    };
    let Some(VertexAttributeValues::Float32x4(weights)) =
        mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
    else {
        panic!("Infantry GLB requires four skin weights per vertex");
    };
    let is_weapon = |i: usize| {
        joints[i]
            .iter()
            .zip(weights[i])
            .any(|(&joint, weight)| joint as usize == weapon && weight > 0.99)
    };
    let indices: Vec<_> = mesh
        .indices()
        .expect("Indexed infantry mesh")
        .iter()
        .collect();
    let kept = indices
        .chunks_exact(3)
        .filter(|tri| !tri.iter().all(|&i| is_weapon(i)))
        .flat_map(|tri| tri.iter().map(|&i| i as u32))
        .collect();
    mesh.clone().with_inserted_indices(Indices::U32(kept))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy asset stores are separate system resources"
)]
pub fn prepare(
    mut art: ResMut<InfantryArt>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    skins: Res<Assets<GltfSkin>>,
    nodes: Res<Assets<GltfNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if art.ready() || !server.is_loaded_with_dependencies(art.source.id()) {
        return;
    }
    let Some(gltf) = gltfs.get(&art.source) else {
        return;
    };
    let clips = CLIPS.map(|name| {
        gltf.named_animations
            .get(name)
            .unwrap_or_else(|| panic!("Missing infantry clip {name}"))
            .clone()
    });
    let (graph, indices) = AnimationGraph::from_clips(clips);
    let primitive = &gltf_meshes.get(&gltf.meshes[0]).unwrap().primitives[0];
    let skin = skins.get(&gltf.skins[0]).unwrap();
    let weapon = skin
        .joints
        .iter()
        .position(|h| nodes.get(h).is_some_and(|n| n.name == "weapon"))
        .expect("Infantry weapon bone");
    let _ = meshes.get_mut(&primitive.mesh).unwrap().generate_tangents();
    let unarmed = without_weapon(meshes.get(&primitive.mesh).unwrap(), weapon);
    let original = materials
        .get(primitive.material.as_ref().unwrap())
        .unwrap()
        .clone();
    for handle in [
        &original.base_color_texture,
        &original.metallic_roughness_texture,
        &original.occlusion_texture,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(image) = images.get_mut(handle) {
            crate::surfaces::filtered_mips(image, false);
        }
    }
    if let Some(handle) = &original.normal_map_texture
        && let Some(image) = images.get_mut(handle)
    {
        crate::surfaces::filtered_mips(image, true);
    }
    let weave = crate::surfaces::relief_maps("cloth").map(|maps| images.add(maps.0));
    let uniforms = [Color::srgb(0.89, 0.92, 0.86), Color::srgb(0.94, 0.88, 0.78)].map(|tint| {
        let mut uniform = original.clone();
        uniform.base_color = tint;
        if uniform.normal_map_texture.is_none() {
            uniform.normal_map_texture = weave.clone();
        }
        uniform.reflectance = 0.20;
        uniform.perceptual_roughness = 0.94;
        materials.add(uniform)
    });
    art.shared = Some(Shared {
        scene: gltf.scenes[0].clone(),
        graph: graphs.add(graph),
        nodes: indices.try_into().unwrap(),
        armed: primitive.mesh.clone(),
        unarmed: meshes.add(unarmed),
        uniforms,
        badge: meshes.add(Cuboid::new(1., 1., 1.)),
        mantle: meshes.add(crate::surfaces::mantle()),
        team: [crate::scene::BLUE, crate::scene::RED].map(|base_color| {
            materials.add(StandardMaterial {
                base_color,
                perceptual_roughness: 0.9,
                ..default()
            })
        }),
        white: materials.add(Color::srgb(0.80, 0.79, 0.70)),
        red: materials.add(Color::srgb(0.64, 0.09, 0.06)),
        gold: materials.add(crate::scene::GOLD),
        tube: meshes.add(Cylinder::new(0.5, 1.).mesh().resolution(8).build()),
        dark: materials.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.08, 0.065),
            metallic: 0.65,
            perceptual_roughness: 0.6,
            ..default()
        }),
        olive: materials.add(crate::surfaces::material(
            "cloth",
            Color::srgb(0.29, 0.33, 0.19),
            &mut images,
        )),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.18, 0.19),
            metallic: 0.6,
            perceptual_roughness: 0.15,
            ..default()
        }),
    });
    info!(
        "Infantry GLB ready: shared mesh, four clips, faction materials and unarmed support variant"
    );
}

#[derive(Component)]
struct Instance {
    actor: Entity,
    owner: usize,
    kind: String,
    training_target: bool,
}
#[derive(Component)]
pub struct Driver {
    actor: Entity,
    shot: Option<u64>,
    recoil: f32,
    gait: f32,
}
#[derive(Component)]
pub struct InfantrySkin {
    actor: Entity,
    medic: bool,
}
#[derive(Component)]
pub struct InfantryBone {
    actor: Entity,
    role: BoneRole,
    medic: bool,
}
#[derive(Clone, Copy)]
enum BoneRole {
    Chest,
    Arm {
        side: f32,
        relaxed: Quat,
        working: Quat,
        swing_axis: Vec3,
        launcher_ready: Quat,
    },
    Hand {
        rest: Quat,
        finger: bool,
    },
}

#[derive(Component)]
pub struct InfantryMuzzle {
    joint: Option<Entity>,
    local: Affine3A,
}
#[derive(Component)]
pub struct LauncherMuzzle {
    joint: Entity,
    local: Affine3A,
}
impl LauncherMuzzle {
    pub fn pose(&self, root: &Transform) -> (Vec3, Vec3) {
        let matrix = root.compute_affine() * self.local;
        (
            matrix.transform_point3(Vec3::ZERO),
            matrix.transform_vector3(Vec3::Y).normalize_or(Vec3::Z),
        )
    }
}
#[derive(Component)]
pub struct Launcher {
    actor: Entity,
}
#[derive(Component)]
pub struct SniperAttachment {
    actor: Entity,
}
impl InfantryMuzzle {
    pub fn pose(&self, root: &Transform) -> Option<(Vec3, Vec3)> {
        self.joint?;
        let matrix = root.compute_affine() * self.local;
        Some((
            matrix.transform_point3(Vec3::ZERO),
            matrix.transform_vector3(Vec3::Y).normalize_or(Vec3::Z),
        ))
    }
}

pub fn spawn(commands: &mut Commands, art: &InfantryArt, p: &Piece) {
    let Some(shared) = &art.shared else { return };
    let phase =
        p.id.bytes()
            .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
            % 1000;
    let actor = commands
        .spawn((
            crate::scene::Actor(p.id.clone()),
            Walk {
                phase: phase as f32 * 0.01,
                pose: Pose::target(p),
                ..default()
            },
            WeaponSockets::default(),
            InfantryMuzzle {
                joint: None,
                local: Affine3A::IDENTITY,
            },
            Transform::from_translation(p.position())
                .with_scale(p.model_transform_scale())
                .with_rotation(Quat::from_rotation_y(
                    std::f32::consts::FRAC_PI_2 - p.heading,
                )),
            Visibility::default(),
        ))
        .id();
    commands.entity(actor).with_children(|parent| {
        parent
            .spawn((
                SceneRoot(shared.scene.clone()),
                Transform::from_scale(Vec3::splat(SCALE)),
                Instance {
                    actor,
                    owner: p.owner.clamp(0, 1) as usize,
                    kind: p.kind.clone(),
                    training_target: p.training_target,
                },
            ))
            .observe(bind);
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "Scene binding inspects the imported hierarchy"
)]
fn bind(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    art: Res<InfantryArt>,
    instances: Query<&Instance>,
    children: Query<&Children>,
    names: Query<&Name>,
    meshes: Query<(), With<Mesh3d>>,
    mut players: Query<&mut AnimationPlayer>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
) {
    let Ok(instance) = instances.get(ready.entity) else {
        return;
    };
    let Some(shared) = &art.shared else { return };
    for child in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            for (i, &node) in shared.nodes.iter().enumerate() {
                player
                    .play(node)
                    .repeat()
                    .pause()
                    .set_seek_time(if i == 3 { 1. } else { 0.04 })
                    .set_weight(if i == 0 { 1. } else { 0. });
            }
            commands.entity(child).insert((
                AnimationGraphHandle(shared.graph.clone()),
                Driver {
                    actor: instance.actor,
                    shot: None,
                    recoil: 1.,
                    gait: 0.,
                },
            ));
        }
        if meshes.contains(child) {
            let medic = instance.kind == "medic";
            commands.entity(child).insert((
                Mesh3d(if medic {
                    shared.unarmed.clone()
                } else {
                    shared.armed.clone()
                }),
                MeshMaterial3d(shared.uniforms[instance.owner].clone()),
                InfantrySkin {
                    actor: instance.actor,
                    medic,
                },
            ));
        }
        let Ok(name) = names.get(child) else { continue };
        let role = match name.as_str() {
            "chest" => Some(BoneRole::Chest),
            "upper_arm.L" | "upper_arm.R" => {
                let side = if name.as_str().ends_with(".L") {
                    1.
                } else {
                    -1.
                };
                let parent = parents.get(child).unwrap().parent();
                let matrix =
                    local_from_actor(parent, instance.actor, &transforms, &parents).unwrap();
                let rotation = matrix.to_scale_rotation_translation().1;
                let rest = transforms.get(child).unwrap().rotation;
                let direction = rotation * rest * Vec3::Y;
                let orient = |target: Vec3| {
                    rotation.inverse()
                        * Quat::from_rotation_arc(direction.normalize(), target.normalize())
                        * rotation
                        * rest
                };
                Some(BoneRole::Arm {
                    side,
                    relaxed: orient(Vec3::new(side * 0.08, -0.99, 0.)),
                    working: orient(Vec3::new(side * 0.12, -0.72, 0.65)),
                    swing_axis: rotation.inverse() * Vec3::X,
                    launcher_ready: orient(Vec3::new(side * 0.10, -0.18, 0.95)),
                })
            }
            "muzzle" => {
                if instance.kind == "scout" {
                    let mut tip = *transforms.get(child).unwrap();
                    tip.translation += tip.rotation * Vec3::Y * 0.17;
                    commands.entity(child).insert(tip);
                }
                commands.entity(instance.actor).insert(InfantryMuzzle {
                    joint: Some(child),
                    local: Affine3A::IDENTITY,
                });
                None
            }
            n if ["forearm.", "hand.", "fingers.", "index.", "thumb."]
                .iter()
                .any(|prefix| n.starts_with(prefix)) =>
            {
                Some(BoneRole::Hand {
                    rest: transforms.get(child).unwrap().rotation,
                    finger: ["fingers.", "index.", "thumb."]
                        .iter()
                        .any(|prefix| n.starts_with(prefix)),
                })
            }
            _ => None,
        };
        if let Some(role) = role {
            commands.entity(child).insert(InfantryBone {
                actor: instance.actor,
                role,
                medic: instance.kind == "medic",
            });
        }
        if name.as_str() == "chest" {
            if instance.kind == "engineer" {
                let launcher = commands
                    .spawn((
                        Launcher {
                            actor: instance.actor,
                        },
                        Transform::from_xyz(0.28, 0.03, -0.16),
                        Visibility::default(),
                        ChildOf(child),
                    ))
                    .id();
                commands.entity(launcher).with_children(|p| {
                    // An olive launch tube, dark end caps, sight unit and pistol grip.
                    for (at, size, mat, tube) in [
                        ([0., 0., 0.], [0.115, 1.05, 0.115], &shared.olive, true),
                        ([0., -0.49, 0.], [0.15, 0.09, 0.15], &shared.dark, true),
                        ([0., 0.49, 0.], [0.13, 0.06, 0.13], &shared.dark, true),
                        (
                            [-0.06, 0.08, 0.03],
                            [0.09, 0.18, 0.085],
                            &shared.dark,
                            false,
                        ),
                        (
                            [-0.06, 0.08, 0.08],
                            [0.065, 0.13, 0.015],
                            &shared.olive,
                            false,
                        ),
                        ([0., 0.04, -0.10], [0.045, 0.07, 0.13], &shared.dark, false),
                        ([0., -0.18, -0.08], [0.07, 0.14, 0.08], &shared.olive, false),
                    ] {
                        p.spawn((
                            Mesh3d(if tube {
                                shared.tube.clone()
                            } else {
                                shared.badge.clone()
                            }),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_translation(Vec3::from_array(at))
                                .with_scale(Vec3::from_array(size)),
                        ));
                    }
                    let tip = p.spawn(Transform::from_xyz(0., 0.54, 0.)).id();
                    p.commands().entity(instance.actor).insert(LauncherMuzzle {
                        joint: tip,
                        local: Affine3A::IDENTITY,
                    });
                });
            }
            commands.entity(child).with_children(|parent| {
                let mut badge = |at: [f32; 3], size: [f32; 3], mat: &Handle<StandardMaterial>| {
                    parent.spawn((
                        Mesh3d(shared.badge.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(Vec3::from_array(at))
                            .with_scale(Vec3::from_array(size)),
                    ));
                };
                badge(
                    [0.065, 0.195, 0.185],
                    [0.09, 0.028, 0.012],
                    &shared.team[instance.owner],
                );
                badge(
                    [0., 0.12, -0.33],
                    [0.075, 0.035, 0.008],
                    &shared.team[instance.owner],
                );
                if instance.kind == "medic" {
                    for z in [0.24, -0.34] {
                        badge([0., 0.07, z], [0.12, 0.10, 0.008], &shared.white);
                        let z = z + z.signum() * 0.006;
                        badge([0., 0.07, z], [0.075, 0.024, 0.008], &shared.red);
                        badge([0., 0.07, z], [0.024, 0.075, 0.009], &shared.red);
                    }
                } else if instance.kind == "commander" {
                    for x in [-0.06, -0.04, -0.02] {
                        badge([x, 0.195, 0.185], [0.01, 0.027, 0.013], &shared.gold);
                    }
                } else if instance.kind == "engineer" {
                    badge([0.19, -0.11, 0.05], [0.08, 0.12, 0.12], &shared.gold);
                    badge([0.19, -0.035, 0.05], [0.055, 0.025, 0.025], &shared.red);
                }
                if instance.training_target {
                    badge([0., 0.13, 0.245], [0.16, 0.16, 0.008], &shared.white);
                    badge([0., 0.13, 0.252], [0.085, 0.085, 0.009], &shared.red);
                }
                if instance.kind == "scout" {
                    parent.spawn((
                        Mesh3d(shared.mantle.clone()),
                        MeshMaterial3d(shared.olive.clone()),
                        Transform::default(),
                    ));
                }
            });
        }
        if name.as_str() == "weapon" && instance.kind == "scout" {
            commands.entity(child).with_children(|p| {
                // Weapon-bone Y is the bore axis. The optic and suppressor follow rifle recoil.
                for (at, size, mat, tube) in [
                    ([0., 0.04, -0.09], [0.045, 0.26, 0.045], &shared.dark, true),
                    ([0., 0.14, -0.09], [0.065, 0.07, 0.065], &shared.dark, true),
                    (
                        [0., -0.07, -0.09],
                        [0.055, 0.05, 0.055],
                        &shared.glass,
                        true,
                    ),
                    ([0., 0.12, -0.057], [0.025, 0.03, 0.04], &shared.dark, false),
                    (
                        [0., -0.04, -0.057],
                        [0.025, 0.03, 0.04],
                        &shared.dark,
                        false,
                    ),
                    ([0., 0.62, -0.022], [0.037, 0.18, 0.037], &shared.dark, true),
                ] {
                    p.spawn((
                        SniperAttachment {
                            actor: instance.actor,
                        },
                        Mesh3d(if tube {
                            shared.tube.clone()
                        } else {
                            shared.badge.clone()
                        }),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(Vec3::from_array(at))
                            .with_scale(Vec3::from_array(size)),
                    ));
                }
            });
        }
    }
}

pub fn class_gear(
    battle: Res<crate::runtime::Battle>,
    walks: Query<(&crate::scene::Actor, &Walk)>,
    mut launchers: Query<(&Launcher, &mut Transform)>,
    mut sniper: Query<(&SniperAttachment, &mut Visibility)>,
) {
    for (launcher, mut t) in &mut launchers {
        let Ok((actor, _)) = walks.get(launcher.actor) else {
            continue;
        };
        let Some(p) = battle.snapshot.entities.iter().find(|p| p.id == actor.0) else {
            continue;
        };
        if p.aa_elevated {
            let pitch = p
                .aim
                .and_then(|at| {
                    battle
                        .snapshot
                        .entities
                        .iter()
                        .filter(|t| t.airborne)
                        .min_by(|a, b| {
                            ((a.x - at[0]).hypot(a.z - at[1]))
                                .total_cmp(&((b.x - at[0]).hypot(b.z - at[1])))
                        })
                })
                .map(|target| {
                    ((target.position().y - p.position().y - 1.35)
                        / Vec2::new(target.x - p.x, target.z - p.z).length().max(1.))
                    .atan()
                })
                .unwrap_or(0.35);
            t.translation = Vec3::new(0.28, 0.15, 0.20);
            t.rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 - pitch);
        } else {
            t.translation = Vec3::new(0.28, 0.03, -0.16);
            t.rotation = Quat::from_rotation_z(-0.12);
        }
    }
    for (attachment, mut visibility) in &mut sniper {
        *visibility = if walks
            .get(attachment.actor)
            .is_ok_and(|(_, w)| w.pose.treating > 0.5)
        {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// Same normalized blend weights for every instance; authored clips supply skin poses.
fn weights(pose: &Pose, recoil: f32) -> [f32; 4] {
    let moving = (pose.speed / 0.7).clamp(0., 1.);
    // Moving under fire uses a lowered walk, rather than sliding a frozen squat.
    let crouch = (pose.crouch * (1. - moving * 0.75))
        .max(pose.treating)
        .clamp(0., 1.);
    let fire = (1. - recoil / 0.32).clamp(0., 1.) * 0.65 * (1. - crouch);
    [
        (1. - moving) * (1. - crouch) * (1. - fire),
        moving * (1. - crouch) * (1. - fire),
        fire * (1. - crouch),
        crouch,
    ]
}
pub fn drive(
    art: Res<InfantryArt>,
    battle: Res<crate::runtime::Battle>,
    time: Res<Time>,
    walks: Query<(&crate::scene::Actor, &Walk, &Transform)>,
    mut players: Query<(&mut Driver, &mut AnimationPlayer)>,
    mut skins: Query<(&InfantrySkin, &mut Mesh3d)>,
) {
    let Some(shared) = &art.shared else { return };
    let shots: HashMap<_, _> = battle
        .snapshot
        .events
        .iter()
        .filter(|e| e.kind == "shot")
        .fold(HashMap::new(), |mut map, e| {
            map.entry(e.source.as_str())
                .and_modify(|tick: &mut u64| *tick = (*tick).max(e.tick))
                .or_insert(e.tick);
            map
        });
    for (mut driver, mut player) in &mut players {
        let Ok((actor, walk, root)) = walks.get(driver.actor) else {
            continue;
        };
        if !battle.snapshot.paused {
            driver.recoil += time.delta_secs();
            driver.gait = (driver.gait
                + time.delta_secs() * walk.pose.speed
                    / (0.7741935 * SCALE * root.scale.y.max(0.1)))
            .fract();
        }
        if let Some(&shot) = shots.get(actor.0.as_str())
            && driver.shot != Some(shot)
        {
            driver.shot = Some(shot);
            driver.recoil = battle.snapshot.tick.saturating_sub(shot) as f32 * 0.05;
        }
        let weights = weights(&walk.pose, driver.recoil);
        let phase = walk.phase / std::f32::consts::TAU;
        let samples = [
            0.034 + phase.fract() * 2.99,
            0.034 + driver.gait * 0.999,
            (0.034 + driver.recoil).min(0.42),
            1.0,
        ];
        for ((&node, &weight), &sample) in shared.nodes.iter().zip(&weights).zip(&samples) {
            player
                .play(node)
                .pause()
                .set_seek_time(sample)
                .set_weight(weight);
        }
    }
    for (skin, mut mesh) in &mut skins {
        let working = walks.get(skin.actor).is_ok_and(|(actor, w, _)| {
            w.pose.treating > 0.5
                || battle
                    .snapshot
                    .entities
                    .iter()
                    .any(|p| p.id == actor.0 && p.aa_elevated)
        });
        let target = if skin.medic || working {
            &shared.unarmed
        } else {
            &shared.armed
        };
        if mesh.0 != *target {
            mesh.0 = target.clone();
        }
    }
}

/// Applied after clip evaluation, so torso aim never gets overwritten by playback.
pub fn aim(walks: Query<&Walk>, mut bones: Query<(&InfantryBone, &mut Transform)>) {
    for (bone, mut transform) in &mut bones {
        let Ok(walk) = walks.get(bone.actor) else {
            continue;
        };
        match bone.role {
            BoneRole::Chest => {
                transform.rotation *= Quat::from_rotation_y(walk.pose.aim.clamp(-1.2, 1.2))
            }
            BoneRole::Arm {
                side,
                relaxed,
                working,
                swing_axis,
                launcher_ready,
            } => {
                if walk.pose.anti_air > 0.01 {
                    transform.rotation =
                        transform.rotation.slerp(launcher_ready, walk.pose.anti_air);
                }
                if bone.medic || walk.pose.treating > 0.5 {
                    let work = walk.pose.treating;
                    let swing = (walk.phase + side * std::f32::consts::FRAC_PI_2).sin()
                        * ((walk.pose.speed / 2.8).min(1.) * 0.3 * (1. - work) + work * 0.10);
                    transform.rotation =
                        Quat::from_axis_angle(swing_axis, swing) * relaxed.slerp(working, work);
                }
            }
            BoneRole::Hand { rest, finger } => {
                if bone.medic || walk.pose.treating > 0.5 {
                    transform.rotation = rest
                        * Quat::from_rotation_x(if finger {
                            0.25
                        } else {
                            walk.pose.treating * 0.18
                        });
                }
            }
        }
    }
}

/// Compose current local transforms before propagation; shots and rendered barrels
/// use the same frame's animation, aim, actor interpolation and model scale.
pub fn cache_muzzles(
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    mut actors: Query<(Entity, &mut InfantryMuzzle)>,
    mut launchers: Query<(Entity, &mut LauncherMuzzle)>,
) {
    for (actor, mut muzzle) in &mut actors {
        let Some(joint) = muzzle.joint else {
            continue;
        };
        if let Some(matrix) = local_from_actor(joint, actor, &transforms, &parents) {
            muzzle.local = matrix;
        }
    }
    for (actor, mut muzzle) in &mut launchers {
        if let Some(matrix) = local_from_actor(muzzle.joint, actor, &transforms, &parents) {
            muzzle.local = matrix;
        }
    }
}

fn local_from_actor(
    mut joint: Entity,
    actor: Entity,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
) -> Option<Affine3A> {
    let mut matrix = Affine3A::IDENTITY;
    for _ in 0..64 {
        if joint == actor {
            return Some(matrix);
        }
        matrix = transforms.get(joint).ok()?.compute_affine() * matrix;
        joint = parents.get(joint).ok()?.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unarmed_variant_removes_only_weapon_triangles_and_preserves_skin_data() {
        let mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0f32, 0., 0.]; 6])
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_JOINT_INDEX,
                VertexAttributeValues::Uint16x4(vec![
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [7, 0, 0, 0],
                    [7, 0, 0, 0],
                    [7, 0, 0, 0],
                ]),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1f32, 0., 0., 0.]; 6])
            .with_inserted_indices(Indices::U32(vec![0, 1, 2, 3, 4, 5]));
        let unarmed = without_weapon(&mesh, 7);
        assert_eq!(
            unarmed.indices().unwrap().iter().collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(mesh.indices().unwrap().len(), 6);
        assert_eq!(
            unarmed.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
        );
        assert_eq!(
            unarmed.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
        );
    }
    #[test]
    fn pose_weights_are_normalized_and_working_uses_the_crouched_pose() {
        for speed in [0., 0.2, 1., 3.] {
            for crouch in [0., 0.25, 0.7, 1.] {
                for recoil in [0., 0.1, 0.5] {
                    let w = weights(
                        &Pose {
                            speed,
                            crouch,
                            ..default()
                        },
                        recoil,
                    );
                    assert!((w.iter().sum::<f32>() - 1.).abs() < 0.0001, "{w:?}");
                }
            }
        }
        assert_eq!(
            weights(
                &Pose {
                    treating: 1.,
                    ..default()
                },
                1.
            ),
            [0., 0., 0., 1.]
        );
    }
    #[test]
    fn muzzle_follows_scaled_parent_chain_and_barrel_axis() {
        let mut app = App::new();
        let root = Transform::from_xyz(10., 2., 20.)
            .with_scale(Vec3::splat(1.15))
            .with_rotation(Quat::from_rotation_y(0.7));
        let actor = app
            .world_mut()
            .spawn((
                root,
                InfantryMuzzle {
                    joint: None,
                    local: Affine3A::IDENTITY,
                },
            ))
            .id();
        let model = app
            .world_mut()
            .spawn((Transform::from_scale(Vec3::splat(SCALE)), ChildOf(actor)))
            .id();
        let chest = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0., 1.23, 0.).with_rotation(Quat::from_rotation_y(-0.6)),
                ChildOf(model),
            ))
            .id();
        let tip = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-0.09, 0.20, 0.78)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ChildOf(chest),
            ))
            .id();
        app.world_mut()
            .get_mut::<InfantryMuzzle>(actor)
            .unwrap()
            .joint = Some(tip);
        app.add_systems(Update, cache_muzzles).update();
        let (at, forward) = app
            .world()
            .get::<InfantryMuzzle>(actor)
            .unwrap()
            .pose(&root)
            .unwrap();
        let expected = root.compute_affine()
            * app
                .world()
                .get::<Transform>(model)
                .unwrap()
                .compute_affine()
            * app
                .world()
                .get::<Transform>(chest)
                .unwrap()
                .compute_affine()
            * app.world().get::<Transform>(tip).unwrap().compute_affine();
        assert!(at.distance(expected.transform_point3(Vec3::ZERO)) < 0.0001);
        assert!(forward.distance(expected.transform_vector3(Vec3::Y).normalize()) < 0.0001);
    }
}
