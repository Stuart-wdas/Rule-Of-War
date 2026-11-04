//! Blender-baked 3D particle playback. No sprite sheets or runtime fluid solver.
use crate::{menu::MainMenu, runtime::Battle, simulation::NativeGame};
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{MeshTag, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::batching::NoAutomaticBatching,
    render::render_resource::{
        AsBindGroup, CompareFunction, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::{Shader, ShaderRef},
};
use serde::Deserialize;

const SHADER: Handle<Shader> = uuid_handle!("88831958-60ee-4a96-bbf5-79ef57bc309f");
const CACHE: &str = include_str!("../../assets/Realistic_Explosion_VFX/baked_3d/particles.json");
const COMBINED_CACHE: &str =
    include_str!("../../assets/Realistic_Explosion_VFX/baked_3d/combined.json");
const SPIKY_CACHE: &str =
    include_str!("../../assets/Realistic_Explosion_VFX/3d/spiky_explosion/particle_tracks.json");

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct VolumeMaterial {
    #[uniform(0)]
    settings: Vec4,
    #[uniform(1)]
    profile: Vec4,
}
impl VolumeMaterial {
    /// A shallow, filled soil volume uses the existing depth-clipped shader.
    pub fn ground_dust(quality: Quality) -> Self {
        Self {
            settings: Vec4::new(
                0.,
                match quality {
                    Quality::Low => 6.,
                    Quality::Balanced => 8.,
                    Quality::High => 12.,
                },
                if quality == Quality::High { 1. } else { 0. },
                0.,
            ),
            profile: Vec4::new(1., 0., 0., 0.),
        }
    }
}
impl Material for VolumeMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Draw the far proxy faces, also when the eye is inside a volume.
        // The shader clips the *integrated volume* to opaque prepass depth.
        descriptor.primitive.cull_mode = Some(Face::Front);
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_compare = CompareFunction::Always;
            depth.depth_write_enabled = false;
        }
        Ok(())
    }
}

#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub enum Quality {
    Low,
    #[default]
    Balanced,
    High,
}
impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Balanced => "BALANCED",
            Self::High => "HIGH",
        }
    }
    fn parameters(self) -> (usize, usize, usize, f32) {
        match self {
            Self::Low => (24, 8, 208, 8.),
            Self::Balanced => (48, 12, 320, 12.),
            Self::High => (72, 24, 480, 20.),
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "balanced" => Some(Self::Balanced),
            "high" => Some(Self::High),
            _ => None,
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Low => Self::Balanced,
            Self::Balanced => Self::High,
            Self::High => Self::Low,
        }
    }
}
#[derive(Message, Clone, Copy)]
pub struct Blast {
    pub at: Vec3,
    pub radius: f32,
    pub variant: Variant,
    pub seed: u32,
    /// Presentation multiplier, applied after gameplay-radius safety clamps.
    pub visual_scale: f32,
}
impl Blast {
    pub fn multiplier(self) -> f32 {
        if self.visual_scale.is_finite() {
            self.visual_scale.clamp(1., 3.)
        } else {
            1.
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Mushroom,
    Debris,
    Shell,
    Rocket,
    SpikyRemake,
    Combined,
}
impl Variant {
    /// Stable per-event randomness keeps clients and replays on the same effect.
    pub fn random(mut seed: u32) -> Self {
        seed ^= seed >> 16;
        seed = seed.wrapping_mul(0x7feb352d);
        seed ^= seed >> 15;
        seed = seed.wrapping_mul(0x846ca68b);
        seed ^= seed >> 16;
        match seed % 4 {
            0 => Self::Mushroom,
            1 => Self::Combined,
            2 => Self::Debris,
            _ => Self::SpikyRemake,
        }
    }
    fn indices(self, quality: Quality) -> Vec<usize> {
        let (lobes, debris, _, _) = quality.parameters();
        if self == Self::Combined {
            // Skipping trail samples breaks each smoke arm into floating puffs.
            // Keep their connected geometry; reduce the core and ray steps for
            // lower presets. Leave room for the mushroom in the low preview.
            let core = if quality == Quality::Low { 20 } else { lobes };
            return (0..core).chain(72..72 + debris).chain(96..239).collect();
        }
        if self != Self::SpikyRemake {
            return (0..lobes).chain(72..72 + debris).collect();
        }
        // Preserve every fire tip and distribute trail samples along all eleven
        // trajectories, including on low quality. Original bakes are unchanged.
        let mut indices = Vec::new();
        for (start, len) in [(0, 9), (152, 17)] {
            let count = (len * lobes).div_ceil(72);
            indices.extend((0..count).map(|i| start + i * len / count));
        }
        indices.extend(9..20);
        let per_trail = 12 * lobes / 72;
        for spike in 0..11 {
            indices.extend((0..per_trail).map(|i| 20 + spike * 12 + i * 12 / per_trail));
        }
        indices
    }
}
#[derive(Deserialize)]
struct Track {
    kind: u32,
    seed: u32,
    axis: [f32; 3],
    frames: Vec<[f32; 8]>,
}
#[derive(Default, Deserialize)]
struct Clip {
    fps: f32,
    duration: f32,
    tracks: Vec<Track>,
}
/// A bounded art study: three motion profiles, all sampled in true world-space 3D.
/// The live game's original Blender caches remain untouched.
fn demonstration_clip(style: usize) -> Clip {
    let fps = 24.;
    let duration = 7.;
    let tracks = (0..96)
        .map(|i| {
            let n = crate::destruction::variation(74129 + style as u32, i);
            let angle = i as f32 * 2.399963;
            let kind = if i >= 72 {
                3
            } else if i >= 48 || style == 0 && i < 12 {
                2
            } else {
                0
            };
            let frames = (0..=168)
                .map(|frame| {
                    let t = frame as f32 / fps;
                    let u = (t - (i % 6) as f32 * 0.018).max(0.);
                    let expansion = 1. - (-u * 5.).exp();
                    let lift = 1. - (-u * 1.8).exp();
                    let gain = if style == 0 { 0.42 } else { 1. };
                    let fade = if style == 0 {
                        ((1.65 - t) / 0.8).clamp(0., 1.)
                    } else {
                        ((duration - t) / 3.).clamp(0., 1.)
                    };
                    let on = (u * 24.).min(1.);
                    let (p, size, heat, opacity) = if i >= 72 {
                        let v = 4. + n * 7.;
                        let flight = u.min(2.4);
                        let y = (0.3 + (5. + n * 7.) * flight - 4.9 * flight * flight).max(0.12);
                        (
                            [angle.cos() * v * flight, angle.sin() * v * flight, y],
                            [0.07 + n * 0.12; 3],
                            0.,
                            on * ((3.2 - u) / 0.8).clamp(0., 1.),
                        )
                    } else if i >= 48 {
                        let r = (0.5 + (2.5 + n * 3.5) * expansion + u * 0.25) * gain;
                        let s = (0.45 + expansion * (0.8 + n * 0.5) + u * 0.2) * gain;
                        (
                            [angle.cos() * r, angle.sin() * r, 0.12 + s * 0.20],
                            [s, s, s * 0.32],
                            0.,
                            on * fade * if style == 0 { 0.38 } else { 0.50 },
                        )
                    } else if i < 12 {
                        let r = expansion * (0.7 + n * 1.4) * gain;
                        let s = (0.24 + expansion * (0.9 + n * 0.8) + lift * 0.2) * gain;
                        (
                            [
                                angle.cos() * r,
                                angle.sin() * r,
                                (0.4 + expansion * 0.7 + lift * 0.6) * gain,
                            ],
                            [s, s, s * 0.95],
                            if style == 0 {
                                0.
                            } else {
                                (1. - u / if style == 2 { 2.4 } else { 1.2 }).clamp(0., 1.)
                            },
                            on * fade * 0.95,
                        )
                    } else {
                        let branch = (i - 12) / 6;
                        let segment = (i - 12) % 6;
                        let a = branch as f32 * std::f32::consts::FRAC_PI_3 + 0.4;
                        let head = segment as f32 / 5.;
                        let spread = if style == 2 {
                            1.5 + head * 2.0
                        } else {
                            0.6 + head * 1.9
                        };
                        let r = spread * expansion * gain;
                        let rise = if style == 0 {
                            0.4
                        } else if style == 2 {
                            8.5
                        } else {
                            7.5
                        };
                        let y = (0.35 + head * rise * lift + expansion * head * 1.1) * gain;
                        let s =
                            (0.3 + expansion * (0.5 + n * 0.4) + lift * (0.4 + head * 0.65)) * gain;
                        (
                            [
                                a.cos() * r + angle.cos() * 0.28,
                                a.sin() * r + angle.sin() * 0.28,
                                y,
                            ],
                            [s, s, s * (1.1 + head * 0.18)],
                            if style == 0 {
                                0.
                            } else {
                                (-u * 2.6).exp() * 0.5
                            },
                            on * fade
                                * 0.92
                                * if style == 0 {
                                    1.
                                } else {
                                    ((u - 0.10) / 0.50).clamp(0., 1.)
                                },
                        )
                    };
                    [p[0], p[1], p[2], size[0], size[1], size[2], heat, opacity]
                })
                .collect();
            Track {
                kind,
                seed: 74129 + i as u32 * 197,
                axis: [0., 0., 1.],
                frames,
            }
        })
        .collect();
    Clip {
        fps,
        duration,
        tracks,
    }
}
#[derive(Resource, Deserialize)]
pub struct Bakes {
    mushroom: Clip,
    debris: Clip,
    #[serde(skip)]
    spiky_remake: Clip,
    #[serde(skip)]
    combined: Clip,
}
impl Bakes {
    fn load() -> Self {
        #[derive(Deserialize)]
        struct RawParticle {
            kind: u32,
            #[serde(default)]
            axis: Option<[f32; 3]>,
            samples: Vec<f32>,
        }
        #[derive(Deserialize)]
        struct RawClip {
            fps: f32,
            frames: usize,
            stride: usize,
            particles: Vec<RawParticle>,
        }
        let mut bakes: Self = serde_json::from_str(CACHE).expect("embedded Blender particle cache");
        let raw: RawClip = serde_json::from_str(SPIKY_CACHE).expect("embedded spiky remake cache");
        assert_eq!((raw.frames, raw.stride, raw.particles.len()), (64, 7, 169));
        bakes.spiky_remake = Clip {
            fps: raw.fps,
            duration: raw.frames as f32 / raw.fps,
            tracks: raw
                .particles
                .into_iter()
                .enumerate()
                .map(|(index, p)| {
                    assert_eq!(p.samples.len(), raw.frames * raw.stride);
                    let fire = p.kind != 0;
                    Track {
                        // The exported tip kind is 2; native kind 2 means ground dust.
                        kind: u32::from(fire),
                        seed: 701 + index as u32 * 37,
                        axis: p.axis.unwrap_or([0., 0., 1.]),
                        frames: p
                            .samples
                            .chunks_exact(7)
                            .map(|f| {
                                [
                                    f[0],
                                    f[1],
                                    f[2],
                                    f[3],
                                    f[4],
                                    f[5],
                                    if fire { 1. } else { 0. },
                                    f[6],
                                ]
                            })
                            .collect(),
                    }
                })
                .collect(),
        };
        bakes.combined = serde_json::from_str(COMBINED_CACHE)
            .expect("embedded combined fireburst and smoke tracks");
        bakes
    }
    fn clip(&self, variant: Variant) -> &Clip {
        match variant {
            Variant::Mushroom => &self.mushroom,
            Variant::Debris | Variant::Shell | Variant::Rocket => &self.debris,
            Variant::SpikyRemake => &self.spiky_remake,
            Variant::Combined => &self.combined,
        }
    }
}
#[derive(Resource)]
pub struct Art {
    cube: Handle<Mesh>,
    fragment: Handle<Mesh>,
    volume: Handle<VolumeMaterial>,
    soil: Handle<StandardMaterial>,
}
#[derive(Component)]
pub struct BakedParticle {
    pub age: f32,
    life: f32,
    track: usize,
    blast: Blast,
    scale: f32,
    yaw: Quat,
}
#[derive(Default, Resource)]
pub struct Clock {
    time: f32,
    tick: u64,
    map: String,
}
/// Frozen poses and the previous sampler are opt-in render-regression controls.
#[derive(Resource, Default)]
pub struct RenderCheck {
    frozen_age: Option<f32>,
    legacy_jitter: bool,
    legacy_batching: bool,
}
#[derive(Component)]
struct QualityButton;
#[derive(Component)]
struct QualityLabel;

pub fn register(app: &mut App) {
    let args: Vec<_> = std::env::args().collect();
    let path = crate::simulation::save_path().with_file_name("explosion-quality.txt");
    let quality = args
        .iter()
        .position(|a| a == "--vfx-quality")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| Quality::parse(s))
        .or_else(|| crate::visual_demo::enabled().then_some(Quality::Balanced))
        .or_else(|| {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| Quality::parse(&s))
        })
        .unwrap_or_default();
    let mut bakes = Bakes::load();
    if crate::visual_demo::enabled() || crate::baked_blast::requested() {
        bakes.mushroom = demonstration_clip(2);
        bakes.debris = demonstration_clip(1);
        bakes.spiky_remake = demonstration_clip(0);
        bakes.combined = demonstration_clip(1);
    }
    app.add_plugins(MaterialPlugin::<VolumeMaterial>::default())
        .add_message::<Blast>()
        .insert_resource(quality)
        .insert_resource(bakes)
        .init_resource::<Clock>()
        .insert_resource(RenderCheck {
            frozen_age: args
                .iter()
                .any(|a| a == "--smoke-test")
                .then(|| {
                    args.iter()
                        .position(|a| a == "--vfx-frozen-age")
                        .and_then(|i| args.get(i + 1))
                        .and_then(|s| s.parse::<f32>().ok())
                        .filter(|age| age.is_finite() && *age >= 0.)
                })
                .flatten(),
            legacy_jitter: args.iter().any(|a| a == "--smoke-test")
                && args.iter().any(|a| a == "--vfx-legacy-jitter"),
            legacy_batching: args.iter().any(|a| a == "--smoke-test")
                && args.iter().any(|a| a == "--vfx-legacy-batching"),
        })
        .add_systems(Update, quality_input.after(crate::ui::buttons));
    load_internal_asset!(app, SHADER, "volume_particle.wgsl", Shader::from_wgsl);
    if args.iter().any(|a| a == "--vfx-showcase") {
        app.init_resource::<Showcase>()
            .add_systems(PostStartup, setup_showcase)
            .add_systems(
                Update,
                showcase.before(update).after(crate::effects::update),
            );
    }
}
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut volumes: ResMut<Assets<VolumeMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(Art {
        cube: meshes.add(Cuboid::new(2., 2., 2.)),
        fragment: meshes.add(Sphere::new(1.).mesh().ico(0).unwrap()),
        volume: volumes.add(VolumeMaterial {
            settings: Vec4::new(0., 12., 1., 0.),
            profile: Vec4::ZERO,
        }),
        soil: materials.add(StandardMaterial {
            base_color: Color::srgb(0.19, 0.16, 0.12),
            perceptual_roughness: 1.,
            ..default()
        }),
    });
    commands
        .spawn((
            Button,
            QualityButton,
            crate::ui::Tooltip(
                "Explosion quality [F6]\nClick to cycle Low, Balanced and High.".into(),
            ),
            crate::menu::TacticalUi,
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                bottom: px(12),
                width: px(56),
                height: px(48),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(crate::theme::INK),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("FX · BAL"),
                QualityLabel,
                TextFont {
                    font_size: 12.,
                    ..default()
                },
                TextColor(crate::theme::PAPER),
            ));
        });
}
fn quality_input(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<MainMenu>,
    front: Res<crate::frontend::Frontend>,
    mut quality: ResMut<Quality>,
    buttons: Query<&Interaction, (With<QualityButton>, Changed<Interaction>)>,
    mut labels: Query<&mut Text, With<QualityLabel>>,
) {
    if !menu.open
        && !front.world
        && !front.lobby
        && !front.settings
        && (keys.just_pressed(KeyCode::F6) || buttons.iter().any(|i| *i == Interaction::Pressed))
    {
        *quality = quality.next();
        let path = crate::simulation::save_path().with_file_name("explosion-quality.txt");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, quality.label());
    }
    for mut text in &mut labels {
        let label = format!(
            "FX · {}",
            match *quality {
                Quality::Low => "LOW",
                Quality::Balanced => "BAL",
                Quality::High => "HI",
            }
        );
        if text.0 != label {
            text.0 = label;
        }
    }
}
fn sample(track: &Track, fps: f32, age: f32) -> [f32; 8] {
    let frame = (age.max(0.) * fps).min((track.frames.len() - 1) as f32);
    let a = frame.floor() as usize;
    let b = (a + 1).min(track.frames.len() - 1);
    std::array::from_fn(|i| {
        track.frames[a][i] + (track.frames[b][i] - track.frames[a][i]) * frame.fract()
    })
}
fn pose(
    p: &BakedParticle,
    track: &Track,
    fps: f32,
    terrain: &crate::terrain::Terrain,
) -> (Transform, u32) {
    let f = sample(track, fps, p.age);
    // Blender is Z-up; the game is Y-up. Preserve the handedness.
    let offset = p.yaw * Vec3::new(f[0], f[2], -f[1]) * p.scale;
    let size = Vec3::new(f[3], f[5], f[4]) * p.scale;
    let mut at = p.blast.at + offset;
    if track.kind == 3 || track.kind == 2 {
        at.y = at.y.max(terrain.height(at.x, at.z) + size.y);
    }
    let rotation = if track.kind == 3 {
        Quat::from_euler(EulerRot::XYZ, p.age * 2.1, track.seed as f32, p.age * 3.2)
    } else {
        p.yaw
            * Quat::from_rotation_arc(
                Vec3::Y,
                Vec3::new(track.axis[0], track.axis[2], -track.axis[1]).normalize_or(Vec3::Y),
            )
            * Quat::from_rotation_y(track.seed as f32 + p.age * 0.07)
    };
    let tag = (f[7].clamp(0., 1.) * 255.) as u32
        | (((f[6].clamp(0., 1.) * 255.) as u32) << 8)
        | ((track.seed & 16383) << 16)
        | (track.kind << 30);
    (
        Transform::from_translation(at)
            .with_scale(size.max(Vec3::splat(0.001)))
            .with_rotation(rotation),
        tag,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects independent effect resources"
)]
pub fn update(
    mut commands: Commands,
    mut events: MessageReader<Blast>,
    bakes: Res<Bakes>,
    art: Res<Art>,
    quality: Res<Quality>,
    mut materials: ResMut<Assets<VolumeMaterial>>,
    time: Res<Time>,
    battle: Res<Battle>,
    game: Res<NativeGame>,
    menu: Res<MainMenu>,
    front: Res<crate::frontend::Frontend>,
    mut clock: ResMut<Clock>,
    check: Res<RenderCheck>,
    mut particles: Query<(Entity, &mut BakedParticle, &mut Transform, &mut MeshTag)>,
) {
    let _timing = crate::performance::span("effects.baked_3d");
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    let clear = menu.open || front.world || clock.map != map || clock.tick > battle.snapshot.tick;
    clock.map = map;
    clock.tick = battle.snapshot.tick;
    let dt = if battle.snapshot.paused || menu.open {
        0.
    } else {
        time.delta_secs().min(0.1)
    };
    clock.time += dt;
    let (_, _, budget, steps) = quality.parameters();
    if let Some(mat) = materials.get_mut(&art.volume) {
        mat.profile.x = if crate::visual_demo::styled() { 1. } else { 0. };
        mat.settings = Vec4::new(
            clock.time,
            steps,
            if *quality == Quality::High { 1. } else { 0. },
            if check.legacy_jitter { 1. } else { 0. },
        );
    }
    let mut count = 0;
    for (entity, mut p, mut transform, mut tag) in &mut particles {
        let clip = bakes.clip(p.blast.variant);
        p.age = check.frozen_age.unwrap_or(p.age + dt);
        if clear || p.age >= p.life || count >= budget {
            commands.entity(entity).despawn();
            continue;
        }
        count += 1;
        let (t, bits) = pose(&p, &clip.tracks[p.track], clip.fps, &game.terrain);
        *transform = t;
        tag.0 = bits;
    }
    if menu.open || front.world {
        events.clear();
        return;
    }
    for blast in events.read() {
        let clip = bakes.clip(blast.variant);
        let mut indices: Vec<usize> =
            if crate::visual_demo::enabled() || crate::baked_blast::requested() {
                let (lobes, debris, _, _) = quality.parameters();
                let count = lobes.min(48);
                (0..count)
                    .map(|i| i * 72 / count)
                    .chain(72..72 + debris)
                    .collect()
            } else {
                blast.variant.indices(*quality)
            };
        if crate::baked_blast::enabled() && blast.variant != Variant::SpikyRemake {
            indices.retain(|i| clip.tracks[*i].kind == 3);
        }
        // Reserve a whole representation or drop it; a saturated battle must not
        // leave half a mushroom or let one barrage allocate unlimited entities.
        if count + indices.len() > budget {
            continue;
        }
        for index in indices {
            let track = &clip.tracks[index];
            let p = BakedParticle {
                age: check.frozen_age.unwrap_or(0.),
                life: track
                    .frames
                    .iter()
                    .rposition(|f| f[7] > 0.001)
                    .map(|i| (i + 1) as f32 / clip.fps)
                    .unwrap_or(0.)
                    .min(clip.duration),
                track: index,
                blast: *blast,
                scale: (blast.radius / 2.8).clamp(0.35, 1.65) * blast.multiplier(),
                yaw: Quat::from_rotation_y(blast.seed as f32 * 0.019),
            };
            let (transform, tag) = pose(&p, track, clip.fps, &game.terrain);
            let mut entity = commands.spawn((
                p,
                transform,
                MeshTag(tag),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            if track.kind == 3 {
                entity.insert((
                    Mesh3d(art.fragment.clone()),
                    MeshMaterial3d(art.soil.clone()),
                ));
            } else {
                entity.insert((Mesh3d(art.cube.clone()), MeshMaterial3d(art.volume.clone())));
                // GPU instance compaction can reorder instances within one
                // batch. Alpha-blended volumes must retain the transparent
                // phase's back-to-front draw order to avoid jumping layers.
                if !check.legacy_batching {
                    entity.insert(NoAutomaticBatching);
                }
            }
            count += 1;
        }
    }
}

#[derive(Resource, Default)]
pub struct Showcase {
    pub elapsed: f32,
    selection: Option<Variant>,
    pub initialized: bool,
}
impl Showcase {
    pub fn label(&self) -> &'static str {
        match self.selection {
            Some(Variant::Mushroom) => "mushroom",
            Some(Variant::Combined) => "combined",
            Some(Variant::Debris) => "original-fireburst",
            Some(Variant::Shell) => "shell-soil",
            Some(Variant::Rocket) => "rocket-impact",
            Some(Variant::SpikyRemake) => "original-spikes",
            None => "comparison",
        }
    }
}
fn setup_showcase(
    mut game: ResMut<NativeGame>,
    mut menu: ResMut<MainMenu>,
    mut controls: ResMut<crate::Controls>,
    mut commands: Commands,
) {
    *game = NativeGame::operation(
        crate::training::ID,
        crate::training::SIZE,
        crate::training::SIZE,
        crate::simulation::core_config(&game.config),
    );
    game.entities.clear();
    game.bases.clear();
    game.props.clear();
    game.human_opponent = true;
    game.config["fogEnabled"] = serde_json::json!(false);
    game.paused = false;
    // Headquarters keep the normal battle win condition dormant.
    game.spawn("barracks", 0, [84., 200.]);
    game.spawn("barracks", 1, [280., 240.]);
    menu.open = false;
    menu.started = true;
    controls.center = Vec3::new(150., game.terrain.height(150., 160.) + 6., 160.);
    controls.zoom = 85.;
    controls.yaw = if std::env::args().any(|a| a == "--blast-side") {
        2.2
    } else {
        0.3
    };
    if std::env::args().any(|a| a == "--smoke-test") {
        let args: Vec<_> = std::env::args().collect();
        if let Some(yaw) = args
            .iter()
            .position(|a| a == "--vfx-view-yaw")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|yaw| yaw.is_finite())
        {
            controls.yaw = yaw;
        }
    }
    controls.last_map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    commands.spawn((Text::new("COMBINED FIREBALL + SMOKE OFFSHOOTS\n1: MUSHROOM   2: COMBINED   5: COMPARE\n3: ORIGINAL FIREBURST   4: ORIGINAL SPIKES\nENTER: REPLAY   F6: QUALITY   SPACE: PAUSE\nFIXED CAMERA ANGLE   WHEEL: ZOOM\nRepeats every 8 seconds"), TextFont { font_size: 15., ..default() }, TextColor(crate::theme::PAPER), crate::menu::TacticalUi, Node { position_type: PositionType::Absolute, left: px(18), top: px(130), padding: UiRect::all(px(10)), ..default() }, BackgroundColor(crate::theme::INK)));
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects showcase input and effect resources"
)]
fn showcase(
    mut state: ResMut<Showcase>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    battle: Res<Battle>,
    ready: Res<crate::landscape::Ready>,
    menu: Res<MainMenu>,
    game: Res<NativeGame>,
    mut blasts: MessageWriter<Blast>,
    quality: Res<Quality>,
    particles: Query<Entity, With<BakedParticle>>,
    mut commands: Commands,
    mut controls: ResMut<crate::Controls>,
) {
    if !ready.ready || menu.open || battle.snapshot.paused {
        return;
    }
    state.elapsed += time.delta_secs().min(0.1);
    if !state.initialized
        || keys.just_pressed(KeyCode::Digit1)
        || keys.just_pressed(KeyCode::Digit2)
        || keys.just_pressed(KeyCode::Digit3)
        || keys.just_pressed(KeyCode::Digit4)
        || keys.just_pressed(KeyCode::Digit5)
        || keys.just_pressed(KeyCode::Enter)
        || state.elapsed > 8.
        || quality.is_changed()
    {
        for entity in &particles {
            commands.entity(entity).despawn();
        }
        if !state.initialized {
            state.selection = if std::env::args().any(|a| a == "--spiky-remake") {
                Some(Variant::SpikyRemake)
            } else if std::env::args().any(|a| a == "--original-debris") {
                Some(Variant::Debris)
            } else if std::env::args().any(|a| a == "--mushroom") {
                Some(Variant::Mushroom)
            } else if std::env::args().any(|a| a == "--compare-vfx") {
                None
            } else {
                Some(Variant::Combined)
            };
        }
        let previous = state.selection;
        if keys.just_pressed(KeyCode::Digit1) {
            state.selection = Some(Variant::Mushroom);
        }
        if keys.just_pressed(KeyCode::Digit2) {
            state.selection = Some(Variant::Combined);
        }
        if keys.just_pressed(KeyCode::Digit3) {
            state.selection = Some(Variant::Debris);
        }
        if keys.just_pressed(KeyCode::Digit4) {
            state.selection = Some(Variant::SpikyRemake);
        }
        if keys.just_pressed(KeyCode::Digit5) {
            state.selection = None;
        }
        if !state.initialized || previous != state.selection {
            controls.zoom = if state.selection.is_none() { 85. } else { 55. };
        }
        state.initialized = true;
        state.elapsed = 0.;
        // Main comparison shows the two gameplay effects; the sources remain inspectable.
        let right = Vec3::new(controls.yaw.cos(), 0., -controls.yaw.sin());
        for (index, variant) in [
            Variant::Mushroom,
            Variant::Combined,
            Variant::Debris,
            Variant::SpikyRemake,
        ]
        .into_iter()
        .enumerate()
        {
            if state.selection.is_none() && index >= 2
                || state.selection.is_some_and(|selected| selected != variant)
            {
                continue;
            }
            let offset = if state.selection.is_none() {
                (index as f32 - 0.5) * 34.
            } else {
                0.
            };
            let at = Vec3::new(150., 0., 160.) + right * offset;
            blasts.write(Blast {
                at: Vec3::new(at.x, game.terrain.height(at.x, at.z) + 0.2, at.z),
                radius: 3.8,
                variant,
                seed: 814,
                visual_scale: 1.,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demonstration_clips_have_grounded_dust_finite_motion_and_a_quiet_tail() {
        for style in 0..3 {
            let clip = demonstration_clip(style);
            assert_eq!(clip.tracks.len(), 96);
            for track in &clip.tracks {
                assert!(track.frames.iter().flatten().all(|v| v.is_finite()));
                assert_eq!(track.frames.last().unwrap()[7], 0.);
                if track.kind == 2 {
                    assert!(track.frames.iter().all(|f| f[2] < 1.2));
                }
            }
        }
        let impact = demonstration_clip(0);
        assert!(
            impact.tracks[..72]
                .iter()
                .flat_map(|t| &t.frames)
                .all(|f| f[6] == 0.)
        );
        assert!(impact.tracks[..72].iter().all(|t| t.frames[48][7] == 0.));
        let vehicle = demonstration_clip(2);
        assert!(vehicle.tracks[47].frames[48][2] > impact.tracks[47].frames[48][2] * 3.);
    }
    #[test]
    fn combined_bake_retains_the_fireburst_and_all_eleven_smoke_trajectories() {
        let bakes = Bakes::load();
        let combined = &bakes.combined;
        assert_eq!(combined.fps, 24.);
        assert_eq!(combined.tracks.len(), 239);
        for (merged, original) in combined.tracks[..96].iter().zip(&bakes.debris.tracks) {
            assert_eq!(merged.kind, original.kind);
            assert_eq!(merged.frames, original.frames);
        }
        for (merged, original) in combined.tracks[96..228]
            .iter()
            .zip(&bakes.spiky_remake.tracks[20..152])
        {
            assert_eq!(merged.kind, 0);
            assert_eq!(merged.axis, original.axis);
            assert_eq!(merged.frames.len(), original.frames.len());
            for (a, b) in merged.frames.iter().zip(&original.frames) {
                assert_eq!(
                    a[..7],
                    b[..7],
                    "merge must preserve the baked 3D path and timing"
                );
                assert!(a[7] <= b[7] + 0.00001);
            }
            assert_eq!(merged.frames.last().unwrap()[7], 0.);
        }
    }
    #[test]
    fn every_quality_shows_fire_and_smoke_together_within_the_existing_budget() {
        let bakes = Bakes::load();
        for (quality, count) in [
            (Quality::Low, 171),
            (Quality::Balanced, 203),
            (Quality::High, 239),
        ] {
            let indices = Variant::Combined.indices(quality);
            assert_eq!(indices.len(), count);
            assert_eq!(
                indices
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                count
            );
            assert!(indices.iter().all(|&i| i < bakes.combined.tracks.len()));
            assert!(count + Variant::Mushroom.indices(quality).len() <= quality.parameters().2);
            assert!(indices.iter().filter(|&&i| i < 72).any(|&i| {
                let f = sample(&bakes.combined.tracks[i], 24., 1.);
                f[6] > 0.25 && f[7] > 0.25
            }));
            for spike in 0..11 {
                assert!(
                    (96 + spike * 12..108 + spike * 12).all(|i| indices.contains(&i)),
                    "quality {quality:?} broke the continuity of smoke offshoot {spike}"
                );
                assert!(
                    indices.iter().any(|&i| i >= 96 + spike * 12
                        && i < 108 + spike * 12
                        && sample(&bakes.combined.tracks[i], 24., 1.)[7] > 0.05),
                    "quality {quality:?} lost smoke offshoot {spike}"
                );
            }
        }
    }
    #[test]
    fn every_smoke_arm_has_a_brief_burning_head_at_every_quality() {
        let bakes = Bakes::load();
        for (tip, source) in bakes.combined.tracks[228..239]
            .iter()
            .zip(&bakes.spiky_remake.tracks[9..20])
        {
            assert_eq!(
                tip.kind, 1,
                "burning tips must not be rendered as ground dust"
            );
            for (a, b) in tip.frames.iter().zip(&source.frames) {
                assert_eq!(
                    a[..3],
                    b[..3],
                    "tip must follow the trail's original ballistic head"
                );
                assert_eq!(a[7], b[7]);
            }
            let head = sample(tip, 24., 0.7);
            assert!(head[6] > 0.9 && head[7] > 0.8);
            assert!(
                Vec3::from_slice(&head[..3]).length() > 4.,
                "fire stays at the moving head"
            );
            assert_eq!(
                sample(tip, 24., 1.7)[7],
                0.,
                "fire must fade before the smoke"
            );
        }
        for quality in [Quality::Low, Quality::Balanced, Quality::High] {
            let indices = Variant::Combined.indices(quality);
            assert!(
                (228..239).all(|i| indices.contains(&i)),
                "every arm needs a burning tip"
            );
        }
    }
    #[test]
    fn playback_honors_pause_menu_reset_and_particle_budget() {
        let mut app = App::new();
        let mut battle = Battle::default();
        battle.snapshot.tick = 10;
        let mut menu = MainMenu::default();
        menu.open = false;
        app.insert_resource(battle)
            .insert_resource(menu)
            .insert_resource(NativeGame::default())
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<VolumeMaterial>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Quality>()
            .init_resource::<Clock>()
            .init_resource::<RenderCheck>()
            .insert_resource(Bakes::load())
            .add_message::<Blast>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.update();
        let blast = Blast {
            at: Vec3::new(30., 1., 30.),
            radius: 3.,
            variant: Variant::Mushroom,
            seed: 814,
            visual_scale: 1.,
        };
        for _ in 0..10 {
            app.world_mut().write_message(blast);
        }
        app.update();
        let entities: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<BakedParticle>>()
            .iter(app.world())
            .collect();
        assert_eq!(
            entities.len(),
            300,
            "five complete balanced effects fit the budget"
        );
        for &e in &entities {
            let particle = app.world().get::<BakedParticle>(e).unwrap();
            assert_eq!(
                app.world().get::<NoAutomaticBatching>(e).is_some(),
                app.world()
                    .resource::<Bakes>()
                    .clip(particle.blast.variant)
                    .tracks[particle.track]
                    .kind
                    != 3,
                "transparent smoke must preserve depth order; opaque debris can batch"
            );
        }
        let entity = entities[0];
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        let age = app.world().get::<BakedParticle>(entity).unwrap().age;
        let position = app.world().get::<Transform>(entity).unwrap().translation;
        assert!(age > 0.);
        app.world_mut().resource_mut::<Battle>().snapshot.paused = true;
        app.update();
        assert_eq!(app.world().get::<BakedParticle>(entity).unwrap().age, age);
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation,
            position
        );
        app.world_mut().resource_mut::<MainMenu>().open = true;
        app.update();
        assert!(
            app.world_mut()
                .query::<&BakedParticle>()
                .iter(app.world())
                .next()
                .is_none()
        );
        app.world_mut().resource_mut::<MainMenu>().open = false;
        app.world_mut().resource_mut::<Battle>().snapshot.paused = false;
        app.world_mut().write_message(blast);
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&BakedParticle>()
                .iter(app.world())
                .count(),
            60
        );
        app.world_mut().resource_mut::<Battle>().snapshot.tick = 0;
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&BakedParticle>()
                .iter(app.world())
                .count(),
            0
        );
    }
    #[test]
    fn blender_cache_is_complete_finite_and_has_different_3d_shapes() {
        let bakes: Bakes = serde_json::from_str(CACHE).unwrap();
        for clip in [&bakes.mushroom, &bakes.debris] {
            assert_eq!(clip.tracks.len(), 96);
            for track in &clip.tracks {
                assert_eq!(track.frames.len(), (clip.fps * clip.duration) as usize + 1);
                for sample in &track.frames {
                    assert!(sample.iter().all(|v| v.is_finite()));
                    assert!(sample[3..6].iter().all(|v| *v > 0.));
                    assert!((0. ..=1.).contains(&sample[7]));
                }
                assert!(sample(track, clip.fps, clip.duration)[7] < 0.001);
            }
            let positions: Vec<_> = clip.tracks[..48]
                .iter()
                .map(|t| sample(t, clip.fps, 1.))
                .collect();
            assert!(positions.iter().any(|p| p[0] > 1.) && positions.iter().any(|p| p[0] < -1.));
            assert!(positions.iter().any(|p| p[1] > 1.) && positions.iter().any(|p| p[1] < -1.));
        }
        assert_ne!(
            bakes.mushroom.tracks[0].frames,
            bakes.debris.tracks[0].frames
        );
    }
    #[test]
    fn quality_preserves_shape_and_stays_bounded() {
        for quality in [Quality::Low, Quality::Balanced, Quality::High] {
            let (lobes, debris, budget, steps) = quality.parameters();
            assert!(lobes <= 72 && lobes % 6 == 0 && debris <= 24);
            assert!(budget <= 480 && lobes + debris <= budget && steps <= 20.);
            assert_eq!(Quality::parse(quality.label()), Some(quality));
        }
        assert_eq!(Quality::Low.next().next().next(), Quality::Low);
    }
    #[test]
    fn detonation_randomness_uses_all_four_preview_effects_evenly() {
        let mut counts = [0usize; 4];
        for event in 0..6000 {
            let variant = Variant::random(event);
            assert_eq!(variant, Variant::random(event));
            counts[match variant {
                Variant::Mushroom => 0,
                Variant::Combined => 1,
                Variant::Debris => 2,
                Variant::SpikyRemake => 3,
                Variant::Shell | Variant::Rocket => panic!("legacy random selection"),
            }] += 1;
        }
        assert!(
            counts.iter().all(|count| (1300..1700).contains(count)),
            "{counts:?}"
        );
    }
    #[test]
    fn all_three_variants_fit_the_preview_budget_and_remake_keeps_every_tip() {
        let bakes = Bakes::load();
        for quality in [Quality::Low, Quality::Balanced, Quality::High] {
            let mut count = 0;
            for variant in [Variant::Mushroom, Variant::Debris, Variant::SpikyRemake] {
                let indices = variant.indices(quality);
                let clip = bakes.clip(variant);
                assert!(indices.iter().all(|&i| i < clip.tracks.len()));
                let unique: std::collections::HashSet<_> = indices.iter().collect();
                assert_eq!(unique.len(), indices.len());
                count += indices.len();
                if variant == Variant::SpikyRemake {
                    assert!(
                        (9..20).all(|i| indices.contains(&i)),
                        "every fire tip survives LOD"
                    );
                    for spike in 0..11 {
                        assert!(
                            indices
                                .iter()
                                .any(|&i| (20 + spike * 12..32 + spike * 12).contains(&i))
                        );
                    }
                }
            }
            assert!(count <= quality.parameters().2);
        }
        let clip = bakes.clip(Variant::SpikyRemake);
        assert_eq!(Variant::SpikyRemake.indices(Quality::High).len(), 169);
        assert_eq!(clip.fps, 24.);
        for (index, track) in clip.tracks.iter().enumerate() {
            assert_eq!(track.frames.len(), 64);
            assert!(track.axis.iter().all(|v| v.is_finite()));
            assert!((Vec3::from_array(track.axis).length() - 1.).abs() < 0.001);
            assert_eq!(track.kind, u32::from(index < 20));
            for frame in &track.frames {
                assert!(frame.iter().all(|v| v.is_finite()));
                assert!(frame[3..6].iter().all(|&v| v > 0.));
                assert!((0. ..=1.).contains(&frame[7]));
                assert_eq!(frame[6], if index < 20 { 1. } else { 0. });
            }
            assert_eq!(track.frames.first().unwrap()[7], 0.);
            assert_eq!(track.frames.last().unwrap()[7], 0.);
        }
        assert!(
            clip.tracks[20..152].iter().all(|t| t.axis[2] < 0.99),
            "smoke segments follow the curved path"
        );
    }
    #[test]
    fn ejecta_smoke_fades_near_the_ground_instead_of_forming_a_rising_column() {
        let bakes: Bakes = serde_json::from_str(CACHE).unwrap();
        for track in &bakes.debris.tracks[..72] {
            assert_eq!(sample(track, bakes.debris.fps, 2.6)[7], 0.);
            for frame in &track.frames {
                if frame[7] > 0.05 {
                    assert!(frame[2] < 5., "spiky smoke rose to {}", frame[2]);
                }
            }
        }
        assert!(bakes.mushroom.tracks[..72].iter().any(|track| sample(
            track,
            bakes.mushroom.fps,
            3.
        )[7] > 0.5));
    }
    #[test]
    fn compact_volume_fallback_handles_every_variant_without_the_old_cache_indices() {
        let mut app = App::new();
        let mut menu = MainMenu::default();
        menu.open = false;
        app.insert_resource(Bakes {
            mushroom: demonstration_clip(2),
            debris: demonstration_clip(1),
            spiky_remake: demonstration_clip(0),
            combined: demonstration_clip(1),
        })
        .insert_resource(Art {
            cube: Handle::default(),
            fragment: Handle::default(),
            volume: Handle::default(),
            soil: Handle::default(),
        })
        .insert_resource(Quality::Balanced)
        .insert_resource(menu)
        .init_resource::<crate::frontend::Frontend>()
        .init_resource::<Assets<VolumeMaterial>>()
        .init_resource::<NativeGame>()
        .init_resource::<Battle>()
        .init_resource::<Time>()
        .init_resource::<Clock>()
        .init_resource::<RenderCheck>()
        .add_message::<Blast>()
        .add_systems(Update, update);
        for variant in [
            Variant::Mushroom,
            Variant::Debris,
            Variant::SpikyRemake,
            Variant::Combined,
        ] {
            app.world_mut().write_message(Blast {
                at: Vec3::new(30., 1., 30.),
                radius: 3.5,
                variant,
                seed: 7,
                visual_scale: 1.,
            });
        }
        app.update();
        let particles: Vec<_> = app
            .world_mut()
            .query::<&BakedParticle>()
            .iter(app.world())
            .collect();
        assert!(!particles.is_empty());
        assert!(particles.iter().all(|p| p.track < 96));
        assert!(
            particles
                .iter()
                .any(|p| p.blast.variant == Variant::Combined)
        );
    }
}
