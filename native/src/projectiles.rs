//! Weapon-specific rounds, animated model sockets, muzzle flash and rocket exhaust.
#[cfg(test)]
use crate::runtime::Piece;
use crate::{menu::MainMenu, runtime::Battle, simulation::NativeGame};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Resource)]
pub struct Ammunition {
    round: Handle<Mesh>,
    sphere: Handle<Mesh>,
    cloud: Handle<Mesh>,
    metal: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    smoke: Vec<Handle<StandardMaterial>>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponEffect {
    Bullet,
    Cannon,
    Artillery,
    Rocket,
    AntiAir,
    Gau8,
}
fn effect(kind: &str) -> WeaponEffect {
    match kind {
        "medium" | "c130-30" | "c130-105" => WeaponEffect::Cannon,
        "long" => WeaponEffect::Artillery,
        "xtralong" | "manpads" | "heli-rocket" => WeaponEffect::Rocket,
        "antiAir" => WeaponEffect::AntiAir,
        "plane" => WeaponEffect::Gau8,
        _ => WeaponEffect::Bullet,
    }
}
/// Transform the socket tagged on the actual rendered barrel, including its joint pose.
pub fn socket_pose(
    socket: &crate::models::WeaponSocket,
    root: &Transform,
    pose: &crate::animation::Pose,
    phase: f32,
) -> (Vec3, Vec3) {
    let joint =
        crate::animation::joint(Transform::IDENTITY, socket.pivot, socket.role, pose, phase);
    let at = root.transform_point(joint.transform_point(socket.at));
    let forward =
        (root.rotation * (root.scale * (joint.rotation * socket.forward))).normalize_or(Vec3::Z);
    (at, forward)
}
#[cfg(test)]
pub fn muzzle(
    piece: &Piece,
    tick: u64,
    root: &Transform,
    pose: &crate::animation::Pose,
    phase: f32,
) -> Vec3 {
    let sockets = crate::models::WeaponSockets::from_parts(&crate::models::parts(piece));
    socket_pose(
        &sockets.0[tick as usize % sockets.0.len()],
        root,
        pose,
        phase,
    )
    .0
}
#[derive(Component)]
pub struct Flight {
    from: Vec3,
    to: Vec3,
    pub age: f32,
    duration: f32,
    arc: f32,
    weapon: WeaponEffect,
    last_smoke: f32,
    target: Option<String>,
    miss: bool,
}
#[derive(Component)]
pub struct Transient {
    age: f32,
    life: f32,
    scale: f32,
    pub smoke: bool,
    seed: f32,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut cloud = crate::flipbook::Flipbook::load(
        include_bytes!("../../assets/Realistic_Explosion_VFX/smoke/smoke_dense_8x8_2048.png"),
        include_str!("../../assets/Realistic_Explosion_VFX/smoke/smoke_dense.json"),
        Vec2::splat(0.5),
        &mut meshes,
        &mut images,
        &mut materials,
    );
    cloud.give_depth(&mut meshes, 0.28);
    for handle in &cloud.frames {
        let material = materials.get_mut(handle).unwrap();
        material.unlit = false;
        material.reflectance = 0.08;
        material.perceptual_roughness = 1.;
        material.base_color = Color::srgba(0.95, 0.94, 0.91, 0.68);
        let image = images
            .get_mut(material.base_color_texture.as_ref().unwrap())
            .unwrap();
        // Preserve the supplied turbulent alpha and volume shading, changing
        // soot into pale exhaust. Mips are rebuilt in linear light afterwards.
        let pixel_count = (image.width() * image.height() * 4) as usize;
        image.data.as_mut().unwrap().truncate(pixel_count);
        for pixel in image.data.as_mut().unwrap().chunks_exact_mut(4) {
            let shade = pixel[0] as f32 / 255.;
            pixel[0] = (175. + shade * 65.) as u8;
            pixel[1] = (173. + shade * 64.) as u8;
            pixel[2] = (166. + shade * 62.) as u8;
        }
        image.texture_descriptor.mip_level_count = 1;
        crate::surfaces::filtered_mips(image, false);
    }
    commands.insert_resource(Ammunition {
        round: meshes.add(Cylinder::new(0.5, 1.).mesh().resolution(8)),
        sphere: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        cloud: cloud.mesh,
        metal: materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.24, 0.21),
            metallic: 0.7,
            perceptual_roughness: 0.4,
            ..default()
        }),
        tracer: materials.add(StandardMaterial {
            base_color: Color::srgb(1., 0.88, 0.55),
            emissive: LinearRgba::new(5., 3., 0.8, 1.),
            unlit: true,
            ..default()
        }),
        flame: materials.add(StandardMaterial {
            base_color: Color::srgb(1., 0.58, 0.19),
            emissive: LinearRgba::new(10., 3., 0.35, 1.),
            unlit: true,
            ..default()
        }),
        smoke: cloud.frames,
    });
}
pub fn trajectory(from: Vec3, to: Vec3, t: f32, arc: f32) -> Vec3 {
    from.lerp(to, t) + Vec3::Y * (4. * arc * t * (1. - t))
}
fn puff(commands: &mut Commands, ammo: &Ammunition, at: Vec3, scale: f32, smoke: bool, life: f32) {
    let seed = (at.x * 17.13 + at.z * 13.79 + at.y * 7.31).sin() * 31.;
    commands.spawn((
        Mesh3d(if smoke {
            ammo.cloud.clone()
        } else {
            ammo.sphere.clone()
        }),
        MeshMaterial3d(if smoke {
            ammo.smoke[12].clone()
        } else {
            ammo.flame.clone()
        }),
        Transform::from_translation(at)
            .with_scale(Vec3::splat(scale))
            .with_rotation(if smoke {
                crate::fixed_view::rotation() * Quat::from_rotation_z(seed)
            } else {
                Quat::IDENTITY
            }),
        Transient {
            age: 0.,
            life,
            scale,
            smoke,
            seed,
        },
        bevy::light::NotShadowCaster,
    ));
}
type Flights<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Flight, &'static mut Transform),
    (Without<crate::scene::Actor>, Without<Transient>),
>;
type Models<'w, 's> = Query<
    'w,
    's,
    (
        &'static crate::scene::Actor,
        &'static Transform,
        &'static crate::scene::Walk,
        &'static crate::models::WeaponSockets,
        Option<&'static crate::infantry::InfantryMuzzle>,
        Option<&'static crate::infantry::LauncherMuzzle>,
    ),
    (Without<Flight>, Without<Transient>),
>;
type Puffs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transient,
        &'static mut Transform,
        &'static mut MeshMaterial3d<StandardMaterial>,
    ),
    (Without<Flight>, Without<crate::scene::Actor>),
>;
#[expect(clippy::too_many_arguments, reason = "Bevy injects system parameters")]
pub fn update(
    mut commands: Commands,
    battle: Res<Battle>,
    game: Res<NativeGame>,
    menu: Res<MainMenu>,
    front: Res<crate::frontend::Frontend>,
    time: Res<Time>,
    ammo: Res<Ammunition>,
    graphics: Res<crate::graphics::Graphics>,
    mut flights: Flights,
    actors: Models,
    mut puffs: Puffs,
    mut seen: Local<HashSet<String>>,
) {
    let _timing = crate::performance::span("projectiles.update");
    let active = !battle.snapshot.paused;
    let (limit, spacing) = match graphics.settings.environment {
        crate::graphics::Detail::Low => (256usize, 0.95),
        crate::graphics::Detail::Balanced => (512, 0.65),
        crate::graphics::Detail::High => (768, 0.45),
    };
    let mut budget = limit.saturating_sub(puffs.iter().count());
    for (entity, mut p, mut transform) in &mut flights {
        if active {
            p.age += time.delta_secs();
        }
        if p.age >= p.duration || menu.open || front.world {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(target) = p
            .target
            .as_ref()
            .and_then(|id| battle.snapshot.entities.iter().find(|e| &e.id == id))
        {
            p.to = crate::runtime::missile_destination(target, p.miss);
        }
        let t = (p.age / p.duration).clamp(0., 1.);
        let flight = |t: f32| trajectory(p.from, p.to, t, p.arc);
        let head = flight(t);
        let next = flight((t + 0.005).min(1.));
        transform.translation = head;
        transform.rotation =
            Quat::from_rotation_arc(Vec3::Y, (next - head).normalize_or(Vec3::NEG_Y));
        if p.weapon == WeaponEffect::Rocket && active && p.age - p.last_smoke >= 0.02 && budget > 0
        {
            // Distance-based samples fill gaps even at low frame rates. A broad pale
            // exhaust persists behind the motor instead of isolated dark dots.
            let previous = (p.last_smoke / p.duration).clamp(0., 1.);
            let length = flight(previous).distance(head);
            let samples = (length / spacing).ceil().clamp(1., 48.) as usize;
            for n in 1..=samples.min(budget) {
                let u = previous + (t - previous) * n as f32 / samples as f32;
                let mut at = flight(u);
                let variation = (at.x * 1.7 + at.z * 2.3 + at.y * 0.8).sin();
                at += Vec3::new(
                    variation * 0.10,
                    (variation * 1.7).cos() * 0.07,
                    -variation * 0.12,
                );
                puff(
                    &mut commands,
                    &ammo,
                    at,
                    3.2 + variation * 0.65,
                    true,
                    2.4 + variation * 0.25,
                );
                budget -= 1;
            }
            p.last_smoke = p.age;
        }
    }
    for (entity, mut p, mut transform, mut material) in &mut puffs {
        if active {
            p.age += time.delta_secs();
        }
        if p.age >= p.life || menu.open || front.world {
            commands.entity(entity).despawn();
            continue;
        }
        let t = p.age / p.life;
        transform.scale = Vec3::splat(
            p.scale
                * if p.smoke {
                    1. + t * 0.8
                } else {
                    (1. - t).max(0.05)
                },
        );
        if p.smoke {
            if active {
                transform.translation += Vec3::new(
                    0.12 + p.seed.sin() * 0.12,
                    0.20,
                    -0.08 + p.seed.cos() * 0.09,
                ) * time.delta_secs();
                transform.rotation =
                    crate::fixed_view::rotation() * Quat::from_rotation_z(p.seed + p.age * 0.08);
            }
            let frame = 12 + ((t * 51.) as usize);
            material.0 = ammo.smoke[frame.min(ammo.smoke.len() - 1)].clone();
        }
    }
    let current: HashSet<_> = battle
        .snapshot
        .events
        .iter()
        .filter(|e| e.kind == "shot")
        .map(shot_key)
        .collect();
    for e in &battle.snapshot.events {
        let key = shot_key(e);
        if e.kind != "shot" || seen.contains(&key) || menu.open || front.world {
            continue;
        }
        let age = battle.snapshot.tick.saturating_sub(e.tick) as f32 * 0.05;
        if age
            > if matches!(
                e.weapon.as_str(),
                "plane" | "manpads" | "heli-rocket" | "c130-30" | "c130-105"
            ) {
                e.duration
            } else {
                0.15
            }
        {
            continue;
        }
        let Some(end) = e.from else {
            continue;
        };
        let fallback = crate::runtime::Piece {
            x: e.at[0],
            z: e.at[1],
            elevation: game.terrain.height(e.at[0], e.at[1]),
            ..default()
        };
        let piece = battle
            .snapshot
            .entities
            .iter()
            .find(|p| p.id == e.source)
            .unwrap_or(&fallback);
        let (from, forward) = if let Some(height) = e.release_height {
            (Vec3::new(e.at[0], height, e.at[1]), Vec3::NEG_Y)
        } else if let Some((_, root, walk, sockets, infantry, launcher)) =
            actors.iter().find(|(a, _, _, _, _, _)| a.0 == e.source)
        {
            if let Some(muzzle) = launcher
                .filter(|_| e.weapon == "manpads")
                .map(|m| m.pose(root))
            {
                muzzle
            } else if let Some(muzzle) = infantry.and_then(|m| m.pose(root)) {
                muzzle
            } else if e.weapon == "heli-rocket" {
                (
                    root.transform_point(Vec3::new(
                        if e.tick.is_multiple_of(2) {
                            -0.76
                        } else {
                            0.76
                        },
                        0.56,
                        0.16,
                    )),
                    root.rotation * Vec3::Z,
                )
            } else if let Some(socket) = sockets.0.get(if e.weapon.starts_with("c130-") {
                usize::from(e.weapon == "c130-105")
            } else {
                e.tick as usize % sockets.0.len().max(1)
            }) {
                socket_pose(socket, root, &walk.pose, walk.phase)
            } else {
                (piece.position() + Vec3::Y * 0.1, Vec3::NEG_Y)
            }
        } else {
            let root = Transform::from_translation(piece.position())
                .with_rotation(Quat::from_rotation_y(
                    std::f32::consts::FRAC_PI_2 - piece.heading,
                ))
                .with_scale(piece.model_transform_scale());
            let sockets = crate::models::WeaponSockets::from_parts(&crate::models::parts(piece));
            if let Some(socket) = sockets.0.get(if e.weapon.starts_with("c130-") {
                usize::from(e.weapon == "c130-105")
            } else {
                e.tick as usize % sockets.0.len().max(1)
            }) {
                socket_pose(socket, &root, &crate::animation::Pose::target(piece), 0.)
            } else {
                (piece.position() + Vec3::Y * 0.1, Vec3::NEG_Y)
            }
        };
        let weapon = effect(&e.weapon);
        let target_height = if weapon == WeaponEffect::Gau8 || e.weapon.starts_with("c130-") {
            crate::runtime::cannon_contact_height(
                battle.snapshot.entities.iter(),
                end,
                e.owner,
                game.terrain.height(end[0], end[1]),
            ) - game.terrain.height(end[0], end[1])
        } else {
            battle
                .snapshot
                .entities
                .iter()
                .filter(|p| {
                    p.owner != e.owner && Vec2::new(p.x - end[0], p.z - end[1]).length() < 2.
                })
                .map(|p| {
                    p.elevation - game.terrain.height(p.x, p.z)
                        + p.flight_height()
                        + if p.category == "vehicle" {
                            1.3 * p.model_scale
                        } else {
                            0.8
                        }
                })
                .next()
                .unwrap_or(0.65)
        };
        let mut to = Vec3::new(
            end[0],
            game.terrain.height(end[0], end[1]) + target_height,
            end[1],
        );
        if let Some(target) = e
            .target
            .as_ref()
            .and_then(|id| battle.snapshot.entities.iter().find(|p| &p.id == id))
        {
            to = crate::runtime::missile_destination(target, e.hit == Some(false));
        }
        if matches!(
            weapon,
            WeaponEffect::Cannon
                | WeaponEffect::Artillery
                | WeaponEffect::Rocket
                | WeaponEffect::Gau8
        ) && e.weapon != "manpads"
        {
            to.y = to.y.max(crate::runtime::structure_contact_height(
                battle.snapshot.entities.iter(),
                end,
                game.terrain.height(end[0], end[1]),
            ));
        }
        let arc = if e.weapon == "manpads" {
            0.
        } else if weapon == WeaponEffect::Artillery {
            (from.distance(to) * 0.12).clamp(5., 20.)
        } else if weapon == WeaponEffect::Rocket {
            (from.distance(to) * 0.04).clamp(2., 8.)
        } else {
            0.
        };
        let (width, length, trail) = match weapon {
            WeaponEffect::Bullet => (0.065, 0.35, 2.2),
            WeaponEffect::AntiAir => (0.09, 0.4, 2.8),
            WeaponEffect::Cannon | WeaponEffect::Artillery => (0.16, 0.7, 1.3),
            WeaponEffect::Rocket => (0.28, 1.3, 4.5),
            WeaponEffect::Gau8 => (0.075, 1.5, 4.5),
        };
        if budget > 0 && !e.source.starts_with("incoming-") {
            let flash = match weapon {
                WeaponEffect::Bullet => 0.32,
                WeaponEffect::AntiAir => 0.5,
                WeaponEffect::Gau8 => 0.65,
                WeaponEffect::Rocket => 0.65,
                _ => 1.25,
            };
            puff(
                &mut commands,
                &ammo,
                from,
                flash,
                false,
                if weapon == WeaponEffect::Bullet {
                    0.07
                } else {
                    0.13
                },
            );
            budget -= 1;
            if !matches!(weapon, WeaponEffect::Bullet | WeaponEffect::Gau8) && budget > 0 {
                puff(&mut commands, &ammo, from, flash * 0.65, true, 0.55);
                budget -= 1;
            }
        }
        // Flight duration is tied to the simulation's impact tick; no slow fake tank tracer.
        let duration = e.duration.max(0.075);
        if age >= duration {
            continue;
        }
        commands
            .spawn((
                Transform::from_translation(from)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Y, forward)),
                Visibility::default(),
                Flight {
                    from,
                    to,
                    age,
                    duration,
                    arc,
                    weapon,
                    last_smoke: 0.,
                    target: e.target.clone(),
                    miss: e.hit == Some(false),
                },
                bevy::light::NotShadowCaster,
            ))
            .with_children(|root| {
                root.spawn((
                    Mesh3d(ammo.round.clone()),
                    MeshMaterial3d(
                        if matches!(
                            weapon,
                            WeaponEffect::Bullet | WeaponEffect::AntiAir | WeaponEffect::Gau8
                        ) {
                            ammo.tracer.clone()
                        } else {
                            ammo.metal.clone()
                        },
                    ),
                    Transform::from_scale(Vec3::new(width, length, width)),
                ));
                if trail > 0. {
                    root.spawn((
                        Mesh3d(ammo.round.clone()),
                        MeshMaterial3d(if weapon == WeaponEffect::Rocket {
                            ammo.flame.clone()
                        } else {
                            ammo.tracer.clone()
                        }),
                        Transform::from_xyz(0., -trail * 0.5, 0.).with_scale(Vec3::new(
                            width * 0.65,
                            trail,
                            width * 0.65,
                        )),
                    ));
                }
            });
    }
    *seen = current;
}
fn shot_key(e: &crate::simulation::BattleEvent) -> String {
    if e.source.starts_with("incoming-") {
        e.source.clone()
    } else {
        format!("{}:{}:{}", e.tick, e.source, e.weapon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gau8_rounds_follow_a_fast_straight_path_from_nose_to_ground() {
        let from = Vec3::new(10., 32., 20.);
        let to = Vec3::new(110., 0., 20.);
        assert_eq!(effect("plane"), WeaponEffect::Gau8);
        assert_eq!(trajectory(from, to, 0., 0.), from);
        assert_eq!(trajectory(from, to, 1., 0.), to);
        assert_eq!(trajectory(from, to, 0.5, 0.), Vec3::new(60., 16., 20.));
    }
    #[test]
    fn shells_have_a_readable_arc_and_finish_at_the_target() {
        let from = Vec3::new(0., 2., 0.);
        let to = Vec3::new(20., 2., 0.);
        assert_eq!(trajectory(from, to, 0., 7.), from);
        assert_eq!(trajectory(from, to, 1., 7.), to);
        assert_eq!(trajectory(from, to, 0.5, 7.).y, 9.);
        assert_eq!(trajectory(from, to, 0.5, 0.).y, 2.);
    }
    #[test]
    fn animated_barrel_surface_and_socket_have_identical_world_height() {
        for kind in [
            "medium",
            "long",
            "antiAir",
            "xtralong",
            "short",
            "plane",
            "helicopter",
        ] {
            let piece = Piece {
                kind: kind.into(),
                category: if kind == "short" { "troop" } else { "vehicle" }.into(),
                model_scale: 2.25,
                ..default()
            };
            let root = Transform::from_xyz(12., 17., 20.)
                .with_rotation(Quat::from_rotation_y(1.7))
                .with_scale(piece.model_transform_scale());
            let parts = crate::models::parts(&piece);
            let marked: Vec<_> = parts.iter().filter(|p| p.socket.is_some()).collect();
            assert!(!marked.is_empty());
            let pose = crate::animation::Pose {
                aim: 0.65,
                aiming: 1.,
                shooting: 0.8,
                crouch: 1.,
                ..default()
            };
            for (i, part) in marked.iter().enumerate() {
                let rendered =
                    crate::animation::joint(part.transform, part.pivot, part.joint, &pose, 0.3);
                let tip = root.transform_point(rendered.transform_point(part.socket.unwrap()));
                let shot = muzzle(&piece, i as u64, &root, &pose, 0.3);
                assert!(
                    tip.distance(shot) < 0.00001,
                    "{kind} barrel {i}: {tip:?} vs {shot:?}"
                );
            }
        }
    }
    #[test]
    fn vehicle_sockets_follow_scale_heading_and_joint_recoil() {
        let piece = Piece {
            kind: "medium".into(),
            category: "vehicle".into(),
            model_scale: 2.25,
            ..default()
        };
        let root = Transform::from_xyz(50., 10., 30.)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(piece.model_transform_scale());
        let a = muzzle(&piece, 0, &root, &crate::animation::Pose::default(), 0.);
        assert!((a.x - 55.990625).abs() < 0.001);
        assert!((a.y - 12.9475).abs() < 0.001);
        let b = muzzle(
            &piece,
            0,
            &root,
            &crate::animation::Pose {
                shooting: 1.,
                ..default()
            },
            0.,
        );
        assert!(b.x < a.x);
        assert_eq!(effect("xtralong"), WeaponEffect::Rocket);
        assert_eq!(effect("plane"), WeaponEffect::Gau8);
        assert_eq!(effect("antiAir"), WeaponEffect::AntiAir);
    }
}
