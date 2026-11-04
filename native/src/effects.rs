//! Animated fire-to-smoke blasts, source-model fragments and persistent wreck fires.
use crate::{RtsCamera, destruction, menu::MainMenu, runtime::Battle, simulation::NativeGame};
use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use std::collections::{HashMap, HashSet};

const PARTICLE_LIMIT: usize = 768;
#[derive(Resource)]
pub struct BlastArt {
    quad: Handle<Mesh>,
    clod: Handle<Mesh>,
    explosion: crate::flipbook::Flipbook,
    dense_smoke: crate::flipbook::Flipbook,
    dust: crate::flipbook::Flipbook,
    base_dust: crate::flipbook::Flipbook,
    soil: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Fire,
    Smoke,
    Dust,
    GroundDust,
    BaseDust,
    Fragment,
    Spark,
    Explosion,
    HeavyExplosion,
    DebrisExplosion,
}

// Secondary dust/chip treatment; the main 3D blast is selected independently.
fn blast_kind(fuel: bool, weapon: &str) -> Kind {
    if fuel {
        Kind::HeavyExplosion
    } else if ["long", "xtralong", "heli-rocket", "c130-105"].contains(&weapon) {
        Kind::DebrisExplosion
    } else {
        Kind::Explosion
    }
}
fn detonates(kind: &str, weapon: &str) -> bool {
    matches!(
        kind,
        "blast" | "destruction" | "crash-impact" | "missile-miss"
    ) && !(kind == "blast" && weapon == "medium")
}
fn blast_variant(fuel: bool, weapon: &str) -> crate::volume_effects::Variant {
    use crate::volume_effects::Variant;
    if fuel {
        Variant::Mushroom
    } else if ["xtralong", "heli-rocket", "manpads", "antiAir"].contains(&weapon) {
        Variant::Rocket
    } else {
        Variant::Debris
    }
}
#[derive(Component)]
pub struct Particle {
    age: f32,
    life: f32,
    velocity: Vec3,
    size: Vec3,
    growth: f32,
    kind: Kind,
    spin: Vec3,
    settled: bool,
}
type ParticleQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Particle,
        &'static mut Transform,
        &'static mut MeshMaterial3d<StandardMaterial>,
        &'static mut Visibility,
    ),
>;
#[derive(Component)]
pub struct BlastLight {
    age: f32,
    life: f32,
    intensity: f32,
}
#[derive(Default)]
pub struct EffectState {
    seen: HashSet<String>,
    burns: HashMap<String, u64>,
    tick: u64,
    map: String,
    motion_elapsed: f32,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut explosion = crate::flipbook::Flipbook::load(
        include_bytes!(
            "../../assets/Realistic_Explosion_VFX/explosions/explosion_fireball_8x8_4096.png"
        ),
        include_str!("../../assets/Realistic_Explosion_VFX/explosions/explosion_fireball.json"),
        Vec2::new(0.5, 0.82),
        &mut meshes,
        &mut images,
        &mut materials,
    );
    let dense_smoke = crate::flipbook::Flipbook::load(
        include_bytes!("../../assets/Realistic_Explosion_VFX/smoke/smoke_dense_8x8_2048.png"),
        include_str!("../../assets/Realistic_Explosion_VFX/smoke/smoke_dense.json"),
        Vec2::splat(0.5),
        &mut meshes,
        &mut images,
        &mut materials,
    );
    explosion.give_depth(&mut meshes, 0.18);
    // Dust keeps the supplied turbulent alpha, but has its own earth-coloured
    // images. Multiplying dark soot by brown produced black streaks around hits.
    let mut dust_mesh = meshes.get(&dense_smoke.mesh).unwrap().clone();
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
        dust_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for vertex in vertices {
            vertex[1] += 0.34;
        }
    }
    let dust = crate::flipbook::Flipbook {
        mesh: meshes.add(dust_mesh),
        duration: dense_smoke.duration,
        frames: dense_smoke
            .frames
            .iter()
            .enumerate()
            .map(|(frame, handle)| {
                let mut material = materials.get(handle).unwrap().clone();
                let mut image = images
                    .get(material.base_color_texture.as_ref().unwrap())
                    .unwrap()
                    .clone();
                let pixels = image.data.as_mut().unwrap();
                pixels.truncate(
                    (image.texture_descriptor.size.width * image.texture_descriptor.size.height * 4)
                        as usize,
                );
                for pixel in pixels.chunks_exact_mut(4) {
                    let shade = pixel[0] as f32 / 255.;
                    pixel[0] = (156. + shade * 38.) as u8;
                    pixel[1] = (136. + shade * 34.) as u8;
                    pixel[2] = (106. + shade * 28.) as u8;
                }
                image.texture_descriptor.mip_level_count = 1;
                crate::surfaces::filtered_mips(&mut image, false);
                material.base_color_texture = Some(images.add(image));
                let progress = frame as f32 / (dense_smoke.frames.len() - 1) as f32;
                let fade = ((1. - progress) / 0.65).clamp(0., 1.).powi(2);
                material.base_color = Color::srgba(1., 1., 1., 0.38 * fade);
                materials.add(material)
            })
            .collect(),
    };
    let base_dust = crate::flipbook::Flipbook {
        mesh: dust.mesh.clone(),
        duration: dust.duration,
        frames: dust
            .frames
            .iter()
            .map(|handle| {
                let mut material = materials.get(handle).unwrap().clone();
                material.base_color = material
                    .base_color
                    .with_alpha((material.base_color.to_srgba().alpha * 1.6).min(0.8));
                materials.add(material)
            })
            .collect(),
    };
    commands.insert_resource(BlastArt {
        quad: meshes.add(Rectangle::new(1., 1.)),
        clod: meshes.add(Sphere::new(0.5).mesh().ico(0).unwrap()),
        explosion,
        dense_smoke,
        dust,
        base_dust,
        soil: materials.add(StandardMaterial {
            base_color: Color::srgb(0.24, 0.20, 0.14),
            perceptual_roughness: 1.,
            ..default()
        }),
        spark: materials.add(StandardMaterial {
            base_color: Color::srgb(1., 0.72, 0.26),
            emissive: LinearRgba::new(7., 2., 0.2, 1.),
            unlit: true,
            ..default()
        }),
    });
}

impl Particle {
    fn floor(&self, t: &Transform, terrain: &crate::terrain::Terrain) -> f32 {
        let projected = Vec3::new(
            (t.rotation * Vec3::X).y.abs(),
            (t.rotation * Vec3::Y).y.abs(),
            (t.rotation * Vec3::Z).y.abs(),
        );
        terrain.height(t.translation.x, t.translation.z) + projected.dot(self.size) * 0.5
    }
    fn billboard(&self) -> bool {
        matches!(
            self.kind,
            Kind::Fire
                | Kind::Smoke
                | Kind::Dust
                | Kind::GroundDust
                | Kind::BaseDust
                | Kind::Explosion
                | Kind::HeavyExplosion
                | Kind::DebrisExplosion
        )
    }
    fn integrate(&mut self, t: &mut Transform, terrain: &crate::terrain::Terrain, dt: f32) {
        if self.billboard() {
            let drag = if matches!(self.kind, Kind::Dust | Kind::GroundDust | Kind::BaseDust) {
                1.5
            } else {
                0.35
            };
            self.velocity.x *= (-dt * drag).exp();
            self.velocity.z *= (-dt * drag).exp();
            if matches!(self.kind, Kind::Smoke) {
                self.velocity.x += (0.45 - self.velocity.x) * dt * 0.3;
                self.velocity.z += (0.20 - self.velocity.z) * dt * 0.3;
            }
            t.translation += self.velocity * dt;
            if matches!(self.kind, Kind::Dust | Kind::GroundDust | Kind::BaseDust) {
                self.velocity.y = (self.velocity.y - dt * 0.22).max(0.02);
                t.translation.y = t
                    .translation
                    .y
                    .max(terrain.height(t.translation.x, t.translation.z) + 0.08);
                if self.kind == Kind::BaseDust {
                    t.translation.y = t
                        .translation
                        .y
                        .min(terrain.height(t.translation.x, t.translation.z) + 0.53);
                }
            }
            return;
        }
        if self.settled {
            t.translation.y = self.floor(t, terrain);
            return;
        }
        let steps = (dt / (1. / 120.)).ceil().max(1.) as usize;
        let step = dt / steps as f32;
        for _ in 0..steps {
            self.velocity.y -= 9.81 * step;
            t.translation += self.velocity * step;
            t.rotate(Quat::from_euler(
                EulerRot::XYZ,
                self.spin.x * step,
                self.spin.y * step,
                self.spin.z * step,
            ));
            let floor = self.floor(t, terrain);
            if t.translation.y <= floor {
                t.translation.y = floor;
                self.velocity.y = self.velocity.y.abs() * 0.23;
                self.velocity.x *= 0.55;
                self.velocity.z *= 0.55;
                self.spin *= 0.4;
                if self.velocity.length_squared() < 0.45 {
                    self.velocity = Vec3::ZERO;
                    self.settled = true;
                }
            }
        }
    }
}

struct Emitter<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    art: &'a BlastArt,
    budget: usize,
    rotation: Quat,
}
impl Emitter<'_, '_, '_> {
    #[expect(
        clippy::too_many_arguments,
        reason = "An effect specifies its motion, size and timing"
    )]
    fn puff(
        &mut self,
        kind: Kind,
        at: Vec3,
        velocity: Vec3,
        size: f32,
        life: f32,
        growth: f32,
        delay: f32,
        spin: f32,
    ) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        let billboard = matches!(
            kind,
            Kind::Fire
                | Kind::Smoke
                | Kind::Dust
                | Kind::GroundDust
                | Kind::BaseDust
                | Kind::Explosion
                | Kind::HeavyExplosion
                | Kind::DebrisExplosion
        );
        let material = match kind {
            Kind::Fire => self.art.explosion.frames[0].clone(),
            Kind::Smoke => self.art.dense_smoke.frames[0].clone(),
            Kind::Dust | Kind::GroundDust => self.art.dust.frames[0].clone(),
            Kind::BaseDust => self.art.base_dust.frames[0].clone(),
            Kind::Spark => self.art.spark.clone(),
            Kind::Fragment => self.art.soil.clone(),
            Kind::Explosion => self.art.explosion.frames[0].clone(),
            Kind::HeavyExplosion => self.art.explosion.frames[0].clone(),
            Kind::DebrisExplosion => self.art.explosion.frames[0].clone(),
        };
        let scale = if kind == Kind::Spark {
            Vec3::new(size * 0.25, size * 2., size * 0.25)
        } else if kind == Kind::GroundDust {
            Vec3::new(size * 1.35, size * 0.26, size)
        } else if matches!(kind, Kind::Dust | Kind::BaseDust) {
            Vec3::new(size * 1.3, size * 0.58, size)
        } else if kind == Kind::Fire {
            Vec3::new(size * 0.8, size * 1.35, size)
        } else if matches!(kind, Kind::HeavyExplosion | Kind::DebrisExplosion) {
            Vec3::new(size, size * (448. / 384.), size)
        } else {
            Vec3::splat(size)
        };
        self.commands.spawn((
            Mesh3d(match kind {
                Kind::Explosion | Kind::Fire => self.art.explosion.mesh.clone(),
                Kind::Dust | Kind::GroundDust | Kind::BaseDust => self.art.dust.mesh.clone(),
                Kind::Smoke => self.art.dense_smoke.mesh.clone(),
                Kind::HeavyExplosion => self.art.explosion.mesh.clone(),
                Kind::DebrisExplosion => self.art.explosion.mesh.clone(),
                _ if billboard => self.art.quad.clone(),
                _ => self.art.clod.clone(),
            }),
            MeshMaterial3d(material),
            Transform::from_translation(at)
                .with_rotation(if billboard {
                    self.rotation
                        * Quat::from_rotation_z(
                            if matches!(kind, Kind::Dust | Kind::GroundDust | Kind::BaseDust) {
                                spin * 0.04
                            } else {
                                spin
                            },
                        )
                } else if kind == Kind::Spark {
                    Quat::from_rotation_arc(Vec3::Y, velocity.normalize_or(Vec3::Y))
                } else {
                    Quat::IDENTITY
                })
                .with_scale(scale),
            NotShadowCaster,
            NotShadowReceiver,
            if delay > 0. {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
            Particle {
                age: -delay,
                life,
                velocity,
                size: scale,
                growth,
                kind,
                spin: Vec3::new(
                    if matches!(kind, Kind::Dust | Kind::GroundDust | Kind::BaseDust) {
                        spin * 0.04
                    } else {
                        spin
                    },
                    1.3,
                    0.7,
                ),
                settled: false,
            },
        ));
    }
    fn fragment(
        &mut self,
        at: Vec3,
        rotation: Quat,
        size: Vec3,
        mesh: Handle<Mesh>,
        material: Handle<StandardMaterial>,
        velocity: Vec3,
    ) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        self.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(at)
                .with_rotation(rotation)
                .with_scale(size),
            Visibility::Visible,
            Particle {
                age: 0.,
                life: 12.,
                velocity,
                size,
                growth: 0.,
                kind: Kind::Fragment,
                spin: Vec3::new(2.3, 3.7, 1.5),
                settled: false,
            },
        ));
    }
}

#[expect(clippy::too_many_arguments, reason = "Bevy injects system parameters")]
pub fn update(
    mut commands: Commands,
    battle: Res<Battle>,
    game: Res<NativeGame>,
    art: Res<BlastArt>,
    models: Res<crate::scene::Art>,
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<RtsCamera>>,
    mut particles: ParticleQuery,
    mut lights: Query<(Entity, &mut BlastLight, &mut PointLight)>,
    mut state: Local<EffectState>,
    mut blasts: MessageWriter<crate::volume_effects::Blast>,
    menu: Res<MainMenu>,
    front: Res<crate::frontend::Frontend>,
) {
    let _timing = crate::performance::span("effects.update");
    let dt = if battle.snapshot.paused {
        0.
    } else {
        time.delta_secs().min(0.1)
    };
    let camera_rotation = camera
        .single()
        .map(|t| t.rotation())
        .unwrap_or(Quat::IDENTITY);
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    let reset = state.tick > battle.snapshot.tick || state.map != map;
    let clear = menu.open || front.world || reset;
    if reset {
        state.seen.clear();
        state.burns.clear();
    }
    state.tick = battle.snapshot.tick;
    state.map = map;
    let mut count = 0;
    for (entity, mut p, mut t, mut material, mut visible) in &mut particles {
        p.age += dt;
        if p.age >= p.life || clear {
            commands.entity(entity).despawn();
            continue;
        }
        count += 1;
        if p.age < 0. {
            continue;
        }
        *visible = Visibility::Visible;
        p.integrate(&mut t, &game.terrain, dt);
        let progress = (p.age / p.life).clamp(0., 1.);
        t.scale = p.size * (1. + p.age * p.growth);
        if p.billboard() {
            t.rotation = camera_rotation * Quat::from_rotation_z(p.spin.x);
        }
        match p.kind {
            Kind::Fire => material.0 = art.explosion.frame(p.age * 0.50, p.life).clone(),
            Kind::Smoke => material.0 = art.dense_smoke.frame(p.age, p.life).clone(),
            Kind::Dust | Kind::GroundDust => material.0 = art.dust.frame(p.age, p.life).clone(),
            Kind::BaseDust => material.0 = art.base_dust.frame(p.age, p.life).clone(),
            Kind::Explosion => material.0 = art.explosion.frame(p.age, p.life).clone(),
            Kind::HeavyExplosion => material.0 = art.explosion.frame(p.age, p.life).clone(),
            Kind::DebrisExplosion => material.0 = art.explosion.frame(p.age, p.life).clone(),
            Kind::Spark => {
                t.scale *= (1. - progress).max(0.02);
                t.rotation = Quat::from_rotation_arc(Vec3::Y, p.velocity.normalize_or(Vec3::Y));
            }
            Kind::Fragment => {
                if progress > 0.85 {
                    t.scale *= ((1. - progress) / 0.15).max(0.01);
                }
            }
        }
    }
    let mut light_count = 0;
    for (entity, mut flash, mut light) in &mut lights {
        flash.age += dt;
        if clear || flash.age >= flash.life {
            commands.entity(entity).despawn();
            continue;
        }
        light_count += 1;
        light.intensity = flash.intensity * (1. - flash.age / flash.life).powi(2);
    }
    if menu.open || front.world {
        return;
    }
    let mut emitter = Emitter {
        commands: &mut commands,
        art: &art,
        budget: PARTICLE_LIMIT.saturating_sub(count),
        rotation: camera_rotation,
    };
    let mut current = HashSet::new();
    for e in &battle.snapshot.events {
        if ![
            "blast",
            "destruction",
            "collapse",
            "impact",
            "aircraft-hit",
            "crash-impact",
            "flares",
            "missile-miss",
        ]
        .contains(&e.kind.as_str())
        {
            continue;
        }
        let key = format!("{}:{}:{}:{}:{}", e.tick, e.kind, e.source, e.at[0], e.at[1]);
        current.insert(key.clone());
        if state.seen.contains(&key)
            || e.tick > battle.snapshot.tick
            || battle.snapshot.tick.saturating_sub(e.tick) > 5
        {
            continue;
        }
        let wreck_prop = if e.kind == "destruction" || e.kind == "crash-impact" {
            battle
                .snapshot
                .props
                .iter()
                .find(|p| p.id == format!("wreck-{}", e.source))
        } else {
            None
        };
        let wreck = wreck_prop.and_then(|p| p.wreck.as_ref());
        let fuel = wreck.is_some_and(|w| !w.piece.building) || e.kind == "crash-impact";
        let water = game.terrain.cell(e.at[0], e.at[1]).water();
        let at = Vec3::new(
            e.at[0],
            game.terrain.height(e.at[0], e.at[1]) + 0.15,
            e.at[1],
        );
        let hash = destruction::seed(&key);
        // Both teams use the same weapon-specific daylight bake. Rifle and
        // GAU-8 contacts keep their small dust/spark particles.
        let mut mushroom = false;
        if !water && detonates(&e.kind, &e.weapon) {
            let contact_y = e
                .release_height
                .unwrap_or_else(|| {
                    crate::runtime::structure_contact_height(
                        battle.snapshot.entities.iter(),
                        e.at,
                        at.y,
                    )
                })
                .max(at.y);
            let deck = wreck
                .filter(|w| !w.piece.building)
                .map(|w| w.piece.model_scale.max(0.5))
                .unwrap_or(0.);
            let variant = if crate::visual_demo::enabled()
                || crate::baked_blast::requested()
                || e.weapon.starts_with("c130-")
            {
                blast_variant(fuel, &e.weapon)
            } else {
                crate::volume_effects::Variant::random(hash)
            };
            mushroom = variant == crate::volume_effects::Variant::Mushroom;
            blasts.write(crate::volume_effects::Blast {
                at: at.with_y(contact_y + deck),
                radius: e.radius.clamp(0.8, 6.),
                variant,
                seed: hash,
                visual_scale: 1.,
            });
        }
        if e.kind == "flares" {
            let origin = at.with_y(e.release_height.unwrap_or(at.y));
            let forward = e.from.map(|p| Vec3::new(p[0], 0., p[1])).unwrap_or(Vec3::Z);
            let right = Vec3::new(-forward.z, 0., forward.x);
            let at = origin - forward * if e.weapon == "plane" { 6. } else { 3. } - Vec3::Y;
            for i in 0..6 {
                let side = if i % 2 == 0 { -1. } else { 1. };
                let velocity = right * side * (8. + i as f32 * 0.9)
                    - forward * (3. + i as f32 * 0.5)
                    - Vec3::Y;
                emitter.puff(
                    Kind::Spark,
                    at,
                    velocity,
                    0.5,
                    2.8,
                    0.,
                    i as f32 * 0.035,
                    0.,
                );
                emitter.puff(
                    Kind::Fire,
                    at + velocity * 0.08,
                    velocity,
                    1.0,
                    1.5,
                    0.15,
                    i as f32 * 0.035,
                    0.,
                );
                for j in 1..7 {
                    let t = j as f32 * 0.16;
                    emitter.puff(
                        Kind::Smoke,
                        at + velocity * t - Vec3::Y * 4.9 * t * t,
                        velocity * 0.12,
                        0.4,
                        1.8,
                        0.7,
                        t,
                        0.,
                    );
                }
            }
            continue;
        }
        if e.kind == "missile-miss" {
            let at = at.with_y(e.release_height.unwrap_or(at.y));
            emitter.puff(Kind::Smoke, at, Vec3::Y * 0.5, 1., 1.4, 0.5, 0.1, 0.);
            continue;
        }
        if e.kind == "aircraft-hit" {
            let at = Vec3::new(e.at[0], e.release_height.unwrap_or(at.y), e.at[1]);
            for i in 0..6 {
                let n = destruction::variation(hash, i);
                let angle = i as f32 * 2.399;
                let radial = Vec3::new(angle.cos(), 0.4, angle.sin());
                emitter.puff(
                    Kind::Smoke,
                    at,
                    radial * 1.8,
                    1.6 + n,
                    3. + n,
                    0.55,
                    0.,
                    angle,
                );
                emitter.puff(
                    Kind::Spark,
                    at,
                    radial * (4. + n * 4.),
                    0.12,
                    0.7 + n,
                    0.,
                    0.,
                    angle,
                );
            }
            emitter.puff(Kind::Fire, at, Vec3::Y, 2.3, 0.35, 1.8, 0., 0.);
            continue;
        }
        if e.kind == "impact" || e.kind == "blast" && e.weapon == "medium" {
            let cannon = e.weapon == "plane";
            let tank = e.weapon == "medium";
            // Destruction has its own fireball and ground ring. Do not add a
            // second dirt puff for the same shell after its target disappears.
            if battle.snapshot.events.iter().any(|other| {
                other.tick == e.tick
                    && ["destruction", "crash-impact"].contains(&other.kind.as_str())
                    && Vec2::from(other.at).distance_squared(Vec2::from(e.at)) < 2. * 2.
            }) {
                continue;
            }
            // Cannon hits sit on the armor deck rather than underneath the tank.
            let contact = if cannon || tank {
                at.with_y(e.release_height.unwrap_or_else(|| {
                    crate::runtime::cannon_contact_height(
                        battle.snapshot.entities.iter(),
                        e.at,
                        e.owner,
                        at.y - 0.15,
                    )
                }))
            } else {
                at
            };
            let armor = contact.y > at.y + 0.35
                && battle.snapshot.entities.iter().any(|p| {
                    p.category == "vehicle"
                        && p.hp > 0.
                        && p.owner != e.owner
                        && !p.airborne
                        && Vec2::new(p.x - e.at[0], p.z - e.at[1]).length_squared()
                            < (4. * p.model_scale).powi(2)
                });
            if tank && !armor && !water && crate::baked_blast::daylight_enabled() {
                blasts.write(crate::volume_effects::Blast {
                    at: contact,
                    radius: e.radius.clamp(0.8, 2.4),
                    variant: crate::volume_effects::Variant::Shell,
                    seed: hash,
                    visual_scale: 1.,
                });
            }
            if !(armor || tank && !water && crate::baked_blast::daylight_enabled()) {
                emitter.puff(
                    Kind::Dust,
                    contact,
                    Vec3::Y * 0.6,
                    if cannon {
                        if armor { 1.65 } else { 2.2 }
                    } else if tank {
                        if armor { 1.2 } else { 1.8 }
                    } else {
                        0.55
                    },
                    if tank {
                        0.85
                    } else if cannon {
                        0.95
                    } else {
                        0.55
                    },
                    1.1,
                    0.,
                    0.,
                );
            }
            if cannon || tank {
                emitter.puff(
                    if armor { Kind::Spark } else { Kind::Fire },
                    contact + Vec3::Y * 0.15,
                    Vec3::Y * 0.3,
                    if armor { 0.35 } else { 0.8 },
                    if armor { 0.18 } else { 0.14 },
                    0.,
                    0.,
                    0.,
                );
                for i in 0..if cannon { 4 } else { 7 } {
                    let n = destruction::variation(hash, i);
                    let angle = i as f32 * 2.399 + n * 0.6;
                    let radial = Vec3::new(angle.cos(), 0., angle.sin());
                    emitter.puff(
                        Kind::Spark,
                        contact + Vec3::Y * 0.07,
                        radial * (3. + n * 6.) + Vec3::Y * (2. + n * 4.),
                        if cannon {
                            if armor { 0.19 } else { 0.14 }
                        } else if armor {
                            0.23
                        } else {
                            0.08
                        },
                        if cannon {
                            0.22 + n * 0.18
                        } else {
                            0.22 + n * 0.22
                        },
                        0.,
                        0.,
                        angle,
                    );
                    if !armor && i < if cannon { 1 } else { 3 } {
                        emitter.puff(
                            Kind::Fragment,
                            contact,
                            radial * (1.5 + n * 3.) + Vec3::Y * (2. + n * 2.),
                            0.07 + n * 0.05,
                            0.5 + n * 0.25,
                            0.,
                            0.,
                            angle,
                        );
                    }
                }
            }
            continue;
        }
        let radius = e.radius.clamp(0.8, 6.);
        let animated = !water && e.kind != "collapse" && !wreck.is_some_and(|w| w.piece.building);
        let blast_at = if animated {
            at.with_y(
                e.release_height
                    .unwrap_or_else(|| {
                        crate::runtime::structure_contact_height(
                            battle.snapshot.entities.iter(),
                            e.at,
                            at.y,
                        )
                    })
                    .max(at.y),
            )
        } else {
            at
        };
        if animated && light_count < 16 {
            let intensity = if fuel { 160000. } else { 65000. } * radius;
            emitter.commands.spawn((
                PointLight {
                    color: Color::srgb(1., 0.61, 0.25),
                    intensity,
                    range: radius * 9.,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(blast_at + Vec3::Y * 1.5),
                BlastLight {
                    age: 0.,
                    life: if fuel { 0.35 } else { 0.12 },
                    intensity,
                },
            ));
            light_count += 1;
        }
        for i in 0..12 {
            let n = destruction::variation(hash, i);
            let angle = i as f32 * 2.399 + n * 0.5;
            let radial = Vec3::new(angle.cos(), 0., angle.sin());
            let velocity = radial * radius * (2.5 + n * 3.);
            if !mushroom && (!animated || i < 8) {
                emitter.puff(
                    Kind::Dust,
                    at + radial * radius * 0.15,
                    velocity * 0.65 + Vec3::Y * (0.10 + n * 0.20),
                    radius * (0.7 + n * 0.5),
                    1.25 + n * 0.65,
                    0.85,
                    if i == 0 { 0. } else { n * 0.08 },
                    angle,
                );
            }
            if !animated && i < 6 {
                emitter.puff(
                    Kind::Dust,
                    at + radial * radius * 0.3 + Vec3::Y * radius * 0.45,
                    radial * (0.6 + n) + Vec3::Y * (1.3 + n),
                    radius * (0.8 + n * 0.5),
                    3.8 + n,
                    0.25,
                    0.07,
                    angle,
                );
            }
            if (fuel || blast_kind(false, &e.weapon) == Kind::DebrisExplosion) && i < 8 && !water {
                emitter.puff(
                    Kind::Spark,
                    blast_at + Vec3::Y,
                    velocity + Vec3::Y * (3. + n * 6.),
                    0.12,
                    0.45 + n * 0.6,
                    0.,
                    0.,
                    angle,
                );
            }
            if wreck.is_none() && e.kind != "collapse" && !water && i < 8 {
                emitter.puff(
                    Kind::Fragment,
                    blast_at + Vec3::Y * 0.2,
                    velocity * 0.7 + Vec3::Y * (2. + n * 4.),
                    0.10 + n * 0.15,
                    3. + n,
                    0.,
                    0.,
                    angle,
                );
            }
        }
        if let (Some(wreck), Some(prop)) = (wreck, wreck_prop) {
            let rotation = Quat::from_rotation_y(prop.heading);
            let scale = wreck.piece.model_transform_scale();
            for (i, part) in destruction::fragments(wreck).into_iter().enumerate() {
                let n = destruction::variation(hash, i + 20);
                let local = part.transform.translation * scale;
                let radial = (rotation * Vec3::new(local.x, 0., local.z)).normalize_or(Vec3::X);
                emitter.fragment(
                    at + rotation * local,
                    rotation * part.transform.rotation,
                    part.transform.scale * scale,
                    models.meshes[part.shape as usize].clone(),
                    models.materials[part.finish as usize + 2].clone(),
                    radial * (2. + n * 4.) + Vec3::Y * (2.5 + n * 4.),
                );
            }
        }
        if e.kind == "collapse" {
            let (parts, rotation, masonry) = if let Some(index) = e
                .weapon
                .strip_prefix("house:")
                .and_then(|s| s.parse::<usize>().ok())
            {
                let Some(piece) = battle.snapshot.entities.iter().find(|p| p.id == e.source) else {
                    continue;
                };
                let Some(lot) = piece.lots.get(index) else {
                    continue;
                };
                let mut source = lot.clone();
                source.at = [0., 0.];
                source.ground = 0.;
                source.integrity = 1.;
                (crate::models::house_parts(&source), Quat::IDENTITY, true)
            } else {
                let Some(prop) = battle.snapshot.props.iter().find(|p| p.id == e.source) else {
                    continue;
                };
                (
                    crate::models::battlefield(&prop.kind, false),
                    Quat::from_rotation_y(prop.heading),
                    prop.kind == "wall" || prop.kind == "crates",
                )
            };
            if masonry {
                for (i, part) in parts
                    .iter()
                    .filter(|p| {
                        p.wall_normal.is_some()
                            && p.transform.translation.y > 0.8
                            && p.transform.scale.max_element() > 0.8
                    })
                    .step_by(3)
                    .take(12)
                    .enumerate()
                {
                    let n = destruction::variation(hash, i + 481);
                    let local = part.transform.translation;
                    let mut foot = at + rotation * Vec3::new(local.x, 0., local.z);
                    foot.y = game.terrain.height(foot.x, foot.z) + 0.12;
                    let radial = (foot - at).with_y(0.).normalize_or(Vec3::X);
                    emitter.puff(
                        Kind::Dust,
                        foot,
                        radial * (0.8 + n),
                        1.8 + n,
                        3.5 + n,
                        0.30,
                        0.12 + n * 0.5,
                        n * 3.,
                    );
                }
            }
            for (i, part) in destruction::fracture_parts(parts, hash, masonry)
                .into_iter()
                .enumerate()
            {
                let n = destruction::variation(hash, i + 35);
                let radial = rotation
                    * Vec3::new(
                        part.transform.translation.x,
                        0.,
                        part.transform.translation.z,
                    )
                    .normalize_or(Vec3::X);
                emitter.fragment(
                    at + rotation * part.transform.translation,
                    rotation * part.transform.rotation,
                    part.transform.scale,
                    models.meshes[part.shape as usize].clone(),
                    models.materials[part.finish as usize + 2].clone(),
                    radial * (0.5 + n * 1.4) - Vec3::Y * (0.3 + n * 0.5),
                );
            }
        }
    }
    state.seen = current;
    let mut burning = HashSet::new();
    for prop in &battle.snapshot.props {
        let Some(wreck) = &prop.wreck else {
            continue;
        };
        if !wreck.burning(battle.snapshot.tick)
            || (wreck.crash.is_none() && game.terrain.cell(prop.at[0], prop.at[1]).water())
        {
            continue;
        }
        burning.insert(prop.id.clone());
        if (dt == 0. && state.burns.contains_key(&prop.id))
            || state
                .burns
                .get(&prop.id)
                .is_some_and(|last| battle.snapshot.tick.saturating_sub(*last) < 14)
        {
            continue;
        }
        state.burns.insert(prop.id.clone(), battle.snapshot.tick);
        let elapsed = battle.snapshot.tick.saturating_sub(wreck.born) as f32 * 0.05;
        let strength = (1. - elapsed / 18.).clamp(0.1, 1.);
        let hash = destruction::seed(&prop.id);
        let n = destruction::variation(hash, battle.snapshot.tick as usize);
        let scale = wreck.piece.model_scale;
        let deck = match wreck.piece.kind.as_str() {
            "troopCarrier" => 1.5,
            "xtralong" => 1.15,
            "helicopter" | "plane" => 0.8,
            _ => 1.04,
        };
        let engine =
            Quat::from_rotation_y(prop.heading) * Vec3::new(0., deck * scale, -0.9 * scale);
        let at = Vec3::new(
            prop.at[0],
            wreck
                .crash
                .as_ref()
                .map(|c| c.world_y)
                .unwrap_or_else(|| game.terrain.height(prop.at[0], prop.at[1])),
            prop.at[1],
        ) + engine;
        if elapsed < 11. {
            emitter.puff(
                Kind::Fire,
                at + Vec3::new((n - 0.5) * scale * 0.4, 0., 0.),
                Vec3::Y * 1.2,
                scale * (0.45 + n * 0.25) * strength,
                0.5 + n * 0.2,
                0.8,
                0.,
                n * 6.,
            );
        }
        emitter.puff(
            Kind::Smoke,
            at + Vec3::Y * scale * 0.4,
            Vec3::new(0.6, 1.6 + n * 0.7, 0.25),
            scale * (0.8 + n * 0.3) * strength.max(0.4),
            4.5 + n * 1.5,
            0.45,
            0.,
            (n - 0.5) * 0.25,
        );
    }
    state.burns.retain(|id, _| burning.contains(id));
    // Ambient motion gets a separate, small allowance after combat effects.
    // Sample real time rather than snapshot ticks: no frame-rate-dependent
    // exhaust spam, no particles while paused, and no off-screen columns.
    state.motion_elapsed += dt;
    if dt > 0. && state.motion_elapsed >= 0.16 {
        state.motion_elapsed %= 0.16;
        if let Ok(eye) = camera.single() {
            let forward = eye.forward().as_vec3();
            let focus = eye.translation() + forward * (eye.translation().y / -forward.y).max(0.);
            let view_radius = (eye.translation().y * 0.65).clamp(30., 180.);
            let mut allowance = 12usize.min(emitter.budget / 3);
            for p in &battle.snapshot.entities {
                if allowance == 0 {
                    break;
                }
                if Vec2::new(p.x - focus.x, p.z - focus.z).length_squared()
                    > view_radius * view_radius
                {
                    continue;
                }
                let Some((speed, rotor)) = motion_dust(p, &game.terrain) else {
                    continue;
                };
                let hash = destruction::seed(&p.id);
                let n = destruction::variation(hash, battle.snapshot.tick as usize / 3);
                let ground = game.terrain.height(p.x, p.z) + 0.12;
                let direction = Vec3::new(p.velocity[0], 0., p.velocity[1]).normalize_or(Vec3::Z);
                let side = Vec3::new(-direction.z, 0., direction.x);
                if rotor {
                    let strength = rotor_dust_strength(p);
                    for sector in 0..4 {
                        let angle =
                            n * std::f32::consts::TAU + sector as f32 * std::f32::consts::FRAC_PI_2;
                        let radial = Vec3::new(angle.cos(), 0., angle.sin());
                        let radius = (2.2 + n * 1.1) * p.model_scale.max(0.5);
                        emitter.puff(
                            Kind::BaseDust,
                            Vec3::new(p.x, ground, p.z) + radial * radius,
                            radial * (3. + strength * 6.)
                                + direction * speed * 0.12
                                + Vec3::Y * 0.12,
                            2.0 + strength * 2.0,
                            1.1 + strength * 0.65,
                            0.7 + strength * 0.7,
                            0.,
                            (n - 0.5) * 0.3,
                        );
                    }
                    allowance -= 1;
                    continue;
                }
                for sign in [-1., 1.] {
                    let offset = if rotor {
                        side * sign * (1.5 + n * 2.)
                    } else {
                        -direction * p.model_scale + side * sign * p.model_scale * 0.55
                    };
                    emitter.puff(
                        Kind::Dust,
                        Vec3::new(p.x, ground, p.z) + offset,
                        if rotor {
                            offset.normalize_or(Vec3::X) * 3.
                        } else {
                            -direction * speed * 0.12 + side * sign * 0.3
                        } + Vec3::Y * 0.10,
                        if rotor {
                            1.7
                        } else {
                            0.65 + speed.min(5.) * 0.15
                        },
                        if rotor { 0.7 } else { 0.9 + n * 0.35 },
                        0.8,
                        0.,
                        sign * n,
                    );
                }
                allowance -= 1;
            }
        }
    }
}

/// A dense grounded core and an outward dust ring follow every mushroom blast.
#[expect(clippy::too_many_arguments, reason = "Bevy injects effect resources")]
pub fn ground_rings(
    mut commands: Commands,
    mut blasts: MessageReader<crate::volume_effects::Blast>,
    art: Res<BlastArt>,
    quality: Res<crate::volume_effects::Quality>,
    game: Res<NativeGame>,
    menu: Res<MainMenu>,
    front: Res<crate::frontend::Frontend>,
    camera: Query<&GlobalTransform, With<RtsCamera>>,
    particles: Query<(), With<Particle>>,
) {
    if menu.open || front.world {
        blasts.clear();
        return;
    }
    let mut emitter = Emitter {
        commands: &mut commands,
        art: &art,
        budget: PARTICLE_LIMIT.saturating_sub(particles.iter().count()),
        rotation: camera
            .single()
            .map(|t| t.rotation())
            .unwrap_or(crate::fixed_view::rotation()),
    };
    for blast in blasts.read() {
        if blast.variant != crate::volume_effects::Variant::Mushroom
            || game.terrain.cell(blast.at.x, blast.at.z).water()
        {
            continue;
        }
        let ground = game.terrain.height(blast.at.x, blast.at.z);
        let ground_radius = blast.radius.clamp(1., 6.);
        let radius = (ground_radius * 1.55).max(2.8) * blast.multiplier();
        // Airbursts cannot kick dirt from a ground plane far beneath them.
        if blast.at.y - ground > ground_radius * blast.multiplier() * 2. + 2. {
            continue;
        }
        let (core_samples, samples) = match *quality {
            crate::volume_effects::Quality::Low => (8, 20),
            crate::volume_effects::Quality::Balanced => (16, 32),
            crate::volume_effects::Quality::High => (24, 40),
        };
        // Keep a fast low burst beneath the stem. Redistribute existing slots
        // into a few smaller following puffs instead of stacking uniformly
        // large billows. All use the preloaded dust frames and particle budget.
        for i in 0..core_samples {
            let n = destruction::variation(blast.seed, i + 1637);
            let secondary = i % 4 == 3;
            let angle = i as f32 * 2.399963 + n * 0.45;
            let radial = Vec3::new(angle.cos(), 0., angle.sin());
            let mut at = blast.at + radial * radius * (0.08 + n.sqrt() * 0.48);
            at.y = game.terrain.height(at.x, at.z) + 0.18 + n * 0.18;
            emitter.puff(
                Kind::BaseDust,
                at,
                radial
                    * radius
                    * if secondary {
                        0.52 + n * 0.18
                    } else {
                        0.24 + n * 0.20
                    }
                    + Vec3::Y * (0.04 + n * 0.07),
                radius
                    * if secondary {
                        0.26 + n * 0.16
                    } else {
                        0.60 + n * 0.25
                    },
                if secondary {
                    0.65 + n * 0.20
                } else {
                    1.0 + n * 0.25
                },
                if secondary { 0.65 } else { 0.40 },
                if secondary {
                    0.07 + n * 0.10
                } else {
                    n * 0.045
                },
                (n - 0.5) * 0.45,
            );
        }
        let phase = destruction::variation(blast.seed, 937) * std::f32::consts::TAU;
        for i in 0..samples {
            let n = destruction::variation(blast.seed, i + 971);
            let angle = phase + std::f32::consts::TAU * (i as f32 + n * 0.6) / samples as f32;
            let radial = Vec3::new(angle.cos(), 0., angle.sin());
            let mut at = blast.at + radial * radius * (0.52 + n * 0.20);
            at.y = game.terrain.height(at.x, at.z) + 0.12;
            emitter.puff(
                Kind::GroundDust,
                at,
                radial * radius * (1.7 + n * 0.3) + Vec3::Y * 0.04,
                radius * (0.45 + n * 0.10),
                0.75 + n * 0.15,
                0.65,
                0.,
                (n - 0.5) * 0.35,
            );
        }
    }
}

fn motion_dust(
    p: &crate::runtime::Piece,
    terrain: &crate::terrain::Terrain,
) -> Option<(f32, bool)> {
    if p.hp <= 0. || terrain.cell(p.x, p.z).water() {
        return None;
    }
    let speed = Vec2::from(p.velocity).length();
    let rotor = matches!(p.kind.as_str(), "helicopter" | "huey") && rotor_dust_strength(p) > 0.025;
    (rotor || p.category == "vehicle" && !p.airborne && speed > 1.2).then_some((speed, rotor))
}
fn rotor_dust_strength(p: &crate::runtime::Piece) -> f32 {
    let power = p.rotor_power.max(if p.airborne { 1. } else { 0. });
    power * power * (1. - p.flight_height() / 18.).clamp(0., 1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn effect_app() -> App {
        let mut app = App::new();
        let mut menu = MainMenu::default();
        menu.open = false;
        app.insert_resource(menu)
            .init_resource::<Battle>()
            .init_resource::<NativeGame>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::volume_effects::Quality>()
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(crate::scene::Art {
                transparent_roofs: vec![],
                aircraft_paint: std::array::from_fn(|_| Handle::default()),
                aircraft_finishes: vec![],
                apache_glass: Handle::default(),
                huey_paint: std::array::from_fn(|_| Handle::default()),
                huey_panels: std::array::from_fn(|_| Handle::default()),
                huey_dark: Handle::default(),
                huey_metal: Handle::default(),
                vehicle_detail_maps: 0,
                vehicle_atlases: crate::vehicle_atlas::Art::default(),
                vehicle_paint: std::array::from_fn(|_| Handle::default()),
                vehicle_finishes: vec![],
                concrete_ground: Handle::default(),
                foundation_ground: Handle::default(),
                impact_scar_material: default(),
                meshes: vec![],
                materials: vec![],
            })
            .add_message::<crate::volume_effects::Blast>()
            .add_systems(Startup, setup)
            .add_systems(Update, (update, ground_rings).chain());
        app.update();
        app
    }
    #[test]
    fn surviving_armour_hit_is_sparks_without_soil_or_an_extra_plume() {
        let mut app = effect_app();
        let ground = app
            .world()
            .resource::<NativeGame>()
            .terrain
            .height(30., 30.);
        let mut battle = app.world_mut().resource_mut::<Battle>();
        battle.snapshot.tick = 10;
        battle.snapshot.entities = vec![crate::runtime::Piece {
            id: "target-tank".into(),
            kind: "medium".into(),
            category: "vehicle".into(),
            owner: 1,
            hp: 100.,
            max_hp: 100.,
            x: 30.,
            z: 30.,
            elevation: ground,
            model_scale: 2.,
            ..default()
        }];
        battle.snapshot.events = vec![
            serde_json::from_value(serde_json::json!({
                "tick":10,"kind":"impact","at":[30.,30.],"from":[18.,30.],"owner":0,
                "radius":2.4,"weapon":"medium","source":"test-shell","releaseHeight":ground+3.
            }))
            .unwrap(),
        ];
        // Set the contact directly as older event field names are serde aliases.
        battle.snapshot.events[0].release_height = Some(ground + 3.);
        app.update();
        let particles: Vec<_> = app
            .world_mut()
            .query::<(&Particle, &Transform)>()
            .iter(app.world())
            .collect();
        assert!(particles.len() >= 7);
        assert!(
            particles
                .iter()
                .all(|(p, t)| p.kind == Kind::Spark && t.translation.y > ground + 2.)
        );
        assert!(
            app.world()
                .resource::<Messages<crate::volume_effects::Blast>>()
                .is_empty()
        );
        let mut battle = app.world_mut().resource_mut::<Battle>();
        battle.snapshot.tick = 11;
        battle.snapshot.entities.clear();
        battle.snapshot.events[0].tick = 11;
        battle.snapshot.events[0].release_height = Some(ground + 0.08);
        app.update();
        assert!(
            app.world_mut()
                .query::<&Particle>()
                .iter(app.world())
                .any(|p| p.kind == Kind::Dust)
        );
    }
    #[test]
    fn mushroom_ring_expands_stays_low_fades_and_respects_pause_and_budget() {
        use crate::volume_effects::{Blast, Variant};
        let mut app = effect_app();
        app.world_mut().resource_mut::<NativeGame>().terrain =
            crate::terrain::Terrain::new("river-crossing", 128, 128);
        let ground = app
            .world()
            .resource::<NativeGame>()
            .terrain
            .height(30., 30.);
        let blast = Blast {
            at: Vec3::new(30., ground, 30.),
            radius: 3.5,
            variant: Variant::Mushroom,
            seed: 7,
            visual_scale: 1.,
        };
        // Read the edited height so the test's plume is grounded on its terrain.
        let blast = Blast {
            at: blast.at.with_y(
                app.world()
                    .resource::<NativeGame>()
                    .terrain
                    .height(30., 30.),
            ),
            ..blast
        };
        app.world_mut().write_message(blast);
        app.update();
        let initial: Vec<_> = app
            .world_mut()
            .query::<(&Particle, &Transform)>()
            .iter(app.world())
            .map(|(p, t)| (p.kind, p.age, t.translation))
            .collect();
        assert_eq!(initial.len(), 48);
        assert_eq!(
            initial
                .iter()
                .filter(|(kind, _, _)| *kind == Kind::BaseDust)
                .count(),
            16
        );
        let base_sizes: Vec<_> = app
            .world_mut()
            .query::<&Particle>()
            .iter(app.world())
            .filter(|p| p.kind == Kind::BaseDust)
            .map(|p| (p.size.x, p.age))
            .collect();
        let ground_radius = blast.radius * 1.55;
        assert_eq!(
            base_sizes
                .iter()
                .filter(|(width, _)| *width < ground_radius * 0.60)
                .count(),
            4,
            "secondary puffs reuse a quarter of the core slots"
        );
        assert!(base_sizes.iter().any(|(_, age)| *age >= -0.045));
        assert!(base_sizes.iter().any(|(_, age)| *age < -0.07));
        assert!(initial.iter().all(|(kind, _, at)| {
            let radius = Vec2::new(at.x - 30., at.z - 30.).length();
            match kind {
                Kind::GroundDust => radius > 2.5 && at.y <= ground + 0.13,
                Kind::BaseDust => radius < 3.1 && at.y <= ground + 0.37,
                _ => false,
            }
        }));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        assert!(
            app.world_mut()
                .query::<(&Particle, &Transform)>()
                .iter(app.world())
                .all(|(p, t)| {
                    let radius = Vec2::new(t.translation.x - 30., t.translation.z - 30.).length();
                    if p.kind == Kind::GroundDust {
                        radius > 3.
                    } else {
                        radius < 3.6 && t.translation.y < ground + 0.53
                    }
                })
        );
        let before_pause: Vec<_> = app
            .world_mut()
            .query::<&Particle>()
            .iter(app.world())
            .map(|p| p.age)
            .collect();
        app.world_mut().resource_mut::<Battle>().snapshot.paused = true;
        app.update();
        assert_eq!(
            before_pause,
            app.world_mut()
                .query::<&Particle>()
                .iter(app.world())
                .map(|p| p.age)
                .collect::<Vec<_>>()
        );
        app.world_mut().resource_mut::<Battle>().snapshot.paused = false;
        for _ in 0..22 {
            app.update();
        }
        assert_eq!(
            app.world_mut()
                .query::<&Particle>()
                .iter(app.world())
                .count(),
            0
        );
        for _ in 0..100 {
            app.world_mut().write_message(blast);
        }
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&Particle>()
                .iter(app.world())
                .count(),
            PARTICLE_LIMIT
        );
        app.world_mut().resource_mut::<MainMenu>().open = true;
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&Particle>()
                .iter(app.world())
                .count(),
            0
        );
    }
    #[test]
    fn mushroom_base_has_an_inner_layer_on_all_qualities_but_not_for_airbursts() {
        use crate::volume_effects::{Blast, Quality, Variant};
        let mut app = effect_app();
        app.world_mut().resource_mut::<NativeGame>().terrain =
            crate::terrain::Terrain::new("river-crossing", 128, 128);
        let blast = Blast {
            at: Vec3::new(30., 0., 30.),
            radius: 3.5,
            variant: Variant::Mushroom,
            seed: 17,
            visual_scale: 1.,
        };
        for (quality, core, total) in [
            (Quality::Low, 8, 28),
            (Quality::Balanced, 16, 48),
            (Quality::High, 24, 64),
        ] {
            *app.world_mut().resource_mut::<Quality>() = quality;
            app.world_mut().write_message(blast);
            app.update();
            let mut query = app.world_mut().query::<&Particle>();
            assert_eq!(query.iter(app.world()).count(), total);
            assert_eq!(
                query
                    .iter(app.world())
                    .filter(|p| p.kind == Kind::BaseDust)
                    .count(),
                core
            );
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            for _ in 0..22 {
                app.update();
            }
        }
        for (variant, height) in [(Variant::Debris, 0.), (Variant::Mushroom, 20.)] {
            app.world_mut().write_message(Blast {
                variant,
                at: blast.at.with_y(height),
                ..blast
            });
            app.update();
            assert_eq!(
                app.world_mut()
                    .query::<&Particle>()
                    .iter(app.world())
                    .count(),
                0
            );
        }
    }
    #[test]
    fn dust_comes_from_moving_ground_vehicles_and_low_rotors_only() {
        let mut terrain = crate::terrain::Terrain::default();
        terrain.edits.insert(
            "30,30".into(),
            crate::terrain::Cell {
                elevation: 0.5,
                surface: 0.5,
                moisture: 0.4,
                kind: 3,
            },
        );
        let mut p = crate::runtime::Piece {
            x: 30.,
            z: 30.,
            hp: 1.,
            category: "vehicle".into(),
            kind: "medium".into(),
            velocity: [3., 0.],
            ..default()
        };
        assert_eq!(motion_dust(&p, &terrain), Some((3., false)));
        p.velocity = [0., 0.];
        assert!(motion_dust(&p, &terrain).is_none());
        p.kind = "helicopter".into();
        p.category = "aircraft".into();
        p.rotor_power = 0.5;
        p.altitude = Some(0.);
        assert_eq!(
            motion_dust(&p, &terrain),
            Some((0., true)),
            "spooling rotors blow dust before lift-off"
        );
        p.rotor_power = 0.;
        assert!(motion_dust(&p, &terrain).is_none());
        p.airborne = true;
        p.kind = "helicopter".into();
        p.altitude = Some(8.);
        assert_eq!(motion_dust(&p, &terrain), Some((0., true)));
        p.altitude = Some(25.);
        assert!(motion_dust(&p, &terrain).is_none());
        p.kind = "plane".into();
        p.altitude = Some(8.);
        assert!(motion_dust(&p, &terrain).is_none());
        p.airborne = false;
        p.velocity = [3., 0.];
        p.category = "troop".into();
        assert!(motion_dust(&p, &terrain).is_none());
        p.category = "vehicle".into();
        terrain.edits.get_mut("30,30").unwrap().kind = 1;
        assert!(motion_dust(&p, &terrain).is_none());
    }
    #[test]
    fn destructive_blasts_use_plumes_and_cannon_contacts_use_their_separate_path() {
        for kind in ["blast", "destruction", "crash-impact", "missile-miss"] {
            for weapon in [
                "short",
                "medium",
                "long",
                "xtralong",
                "heli-rocket",
                "plane",
                "",
            ] {
                assert_eq!(
                    detonates(kind, weapon),
                    kind != "blast" || weapon != "medium"
                );
            }
        }
        assert!(!detonates("impact", "plane"));
        assert!(!detonates("impact", "medium"));
        use crate::volume_effects::Variant;
        assert_eq!(blast_variant(false, "long"), Variant::Debris);
        for rocket in ["xtralong", "heli-rocket", "manpads", "antiAir"] {
            assert_eq!(blast_variant(false, rocket), Variant::Rocket);
            assert_eq!(blast_variant(true, rocket), Variant::Mushroom);
        }
        for kind in ["flares", "aircraft-hit", "collapse"] {
            assert!(!detonates(kind, "plane"));
        }
        assert!(!detonates("impact", "short"));
    }
    #[test]
    fn secondary_particle_treatment_tracks_fuel_and_weapon() {
        for weapon in ["long", "xtralong", "heli-rocket"] {
            assert_eq!(blast_kind(false, weapon), Kind::DebrisExplosion);
            assert_eq!(blast_kind(true, weapon), Kind::HeavyExplosion);
        }
        assert_eq!(blast_kind(true, ""), Kind::HeavyExplosion);
        assert_eq!(blast_kind(false, "short"), Kind::Explosion);
        assert_eq!(blast_kind(false, "medium"), Kind::Explosion);
    }
    #[test]
    fn returning_from_the_menu_does_not_replay_an_explosion_but_a_new_operation_can() {
        let mut app = App::new();
        let mut battle = Battle::default();
        battle.snapshot.tick = 10;
        battle.snapshot.events.push(crate::simulation::BattleEvent {
            hit: None,
            target: None,
            tick: 10,
            kind: "blast".into(),
            at: [30., 30.],
            from: None,
            owner: 0,
            radius: 2.,
            weapon: "xtralong".into(),
            source: String::new(),
            duration: 0.,
            release_height: None,
        });
        let mut menu = MainMenu::default();
        menu.open = false;
        app.insert_resource(battle)
            .insert_resource(NativeGame::default())
            .insert_resource(menu)
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(crate::scene::Art {
                transparent_roofs: vec![],
                aircraft_paint: std::array::from_fn(|_| Handle::default()),
                aircraft_finishes: vec![],
                apache_glass: Handle::default(),
                huey_paint: std::array::from_fn(|_| Handle::default()),
                huey_panels: std::array::from_fn(|_| Handle::default()),
                huey_dark: Handle::default(),
                huey_metal: Handle::default(),
                vehicle_detail_maps: 0,
                vehicle_atlases: crate::vehicle_atlas::Art::default(),
                vehicle_paint: std::array::from_fn(|_| Handle::default()),
                vehicle_finishes: vec![],
                concrete_ground: Handle::default(),
                foundation_ground: Handle::default(),
                impact_scar_material: default(),
                meshes: vec![],
                materials: vec![],
            })
            .add_message::<crate::volume_effects::Blast>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        let count = |app: &mut App| {
            app.world_mut()
                .query_filtered::<Entity, With<Particle>>()
                .iter(app.world())
                .count()
        };
        app.update();
        assert!(count(&mut app) > 0);
        let explosion = app
            .world_mut()
            .query::<(Entity, &Particle)>()
            .iter(app.world())
            .find(|(_, p)| p.kind == Kind::Dust)
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(explosion)
            .unwrap()
            .0
            .clone();
        let age = app.world().get::<Particle>(explosion).unwrap().age;
        assert!(age > 0.);
        let art = app.world().resource::<BlastArt>();
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let images = app.world().resource::<Assets<Image>>();
        let last_dust = materials.get(art.dust.frames.last().unwrap()).unwrap();
        assert_eq!(
            last_dust.base_color.alpha(),
            0.,
            "dust must fade out rather than leave dark wisps"
        );
        let dust = materials.get(&art.dust.frames[20]).unwrap();
        let image = images
            .get(dust.base_color_texture.as_ref().unwrap())
            .unwrap();
        assert!(
            image
                .data
                .as_ref()
                .unwrap()
                .chunks_exact(4)
                .filter(|p| p[3] > 0)
                .all(|p| p[0] > 140 && p[1] > 120 && p[2] > 90)
        );
        let life = app.world().get::<Particle>(explosion).unwrap().life;
        assert_eq!(material, *art.dust.frame(age, life));
        assert_ne!(material, art.dust.frames[0]);
        app.world_mut().resource_mut::<Battle>().snapshot.paused = true;
        app.update();
        assert_eq!(app.world().get::<Particle>(explosion).unwrap().age, age);
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(explosion)
                .unwrap()
                .0,
            material
        );
        app.world_mut().resource_mut::<MainMenu>().open = true;
        app.update();
        assert_eq!(count(&mut app), 0);
        app.world_mut().resource_mut::<MainMenu>().open = false;
        app.update();
        assert_eq!(count(&mut app), 0);
        let mut battle = app.world_mut().resource_mut::<Battle>();
        battle.snapshot.tick = 0;
        battle.snapshot.events[0].tick = 0;
        app.update();
        assert!(count(&mut app) > 0);
    }
    #[test]
    fn ballistic_fragment_bounces_and_settles_on_terrain_without_losing_its_shape() {
        let terrain = crate::terrain::Terrain::default();
        let floor = terrain.height(30., 30.);
        let mut p = Particle {
            age: 0.,
            life: 12.,
            velocity: Vec3::new(2., -8., 1.),
            size: Vec3::new(0.4, 0.2, 0.8),
            growth: 0.,
            kind: Kind::Fragment,
            spin: Vec3::ONE,
            settled: false,
        };
        let mut t = Transform::from_xyz(30., floor + 0.5, 30.).with_scale(p.size);
        for _ in 0..600 {
            p.integrate(&mut t, &terrain, 1. / 60.);
            assert!(t.translation.y >= terrain.height(t.translation.x, t.translation.z));
        }
        assert!(p.settled);
        assert_eq!(t.scale, p.size);
        assert_eq!(p.velocity, Vec3::ZERO);
    }
}
