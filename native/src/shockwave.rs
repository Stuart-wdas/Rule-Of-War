//! Short 3D pressure shells refract the opaque scene behind large explosions.
use crate::volume_effects::{Blast, Quality, Variant};
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{MeshTag, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::{batching::NoAutomaticBatching, render_resource::*},
    shader::{Shader, ShaderRef},
};

const SHADER: Handle<Shader> = uuid_handle!("3e99ff74-f9b2-4aa3-93d3-1ce90f47d25c");
const LIFE: f32 = 0.62;
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct PressureMaterial {
    #[uniform(0)]
    pixels: Vec4,
}
impl Material for PressureMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn reads_view_transmission_texture(&self) -> bool {
        true
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
        // Keep the transmissive phase so Bevy copies the opaque scene before
        // drawing the shell; blend without writing spherical depth over smoke.
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = false;
        }
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState::ALPHA_BLENDING);
            }
        }
        Ok(())
    }
}
#[derive(Resource)]
struct Art {
    sphere: Handle<Mesh>,
    material: Handle<PressureMaterial>,
}
#[derive(Component)]
pub struct Shell {
    age: f32,
    radius: f32,
}
#[derive(Resource, Default)]
pub struct Stats {
    pub spawned: u32,
    pub peak: usize,
}
#[derive(Default)]
struct Clock {
    map: String,
    tick: u64,
}

pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<PressureMaterial>::default())
        .init_resource::<Stats>()
        .add_systems(Startup, setup)
        .add_systems(Update, update);
    load_internal_asset!(app, SHADER, "shockwave.wgsl", Shader::from_wgsl);
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PressureMaterial>>,
) {
    commands.insert_resource(Art {
        sphere: meshes.add(Sphere::new(1.).mesh().ico(4).unwrap()),
        material: materials.add(PressureMaterial {
            pixels: Vec4::new(
                1.8,
                if std::env::args().any(|a| a == "--smoke-test")
                    && std::env::args().any(|a| a == "--shockwave-mask")
                {
                    1.
                } else {
                    0.
                },
                0.,
                0.,
            ),
        }),
    });
}
fn scale(age: f32, radius: f32) -> f32 {
    let t = (age / LIFE).clamp(0., 1.);
    // Fast launch, then a settling front. The shader supplies its own soft
    // fade, so the shared shell never needs an individual material upload.
    radius * (0.3 + 3.4 * (1. - (1. - t).powi(2)))
}
fn limit(quality: Quality) -> usize {
    match quality {
        Quality::Low => 6,
        Quality::Balanced => 10,
        Quality::High => 14,
    }
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| !std::env::args().any(|a| a == "--no-shockwaves"))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Shared blast events and scene lifecycle resources"
)]
fn update(
    mut commands: Commands,
    mut events: MessageReader<Blast>,
    art: Res<Art>,
    quality: Res<Quality>,
    time: Res<Time>,
    game: Res<crate::simulation::NativeGame>,
    battle: Res<crate::runtime::Battle>,
    menu: Res<crate::menu::MainMenu>,
    front: Res<crate::frontend::Frontend>,
    mut clock: Local<Clock>,
    mut stats: ResMut<Stats>,
    mut shells: Query<(Entity, &mut Shell, &mut Transform, &mut MeshTag)>,
) {
    let _timing = crate::performance::span("effects.pressure_waves");
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.seed, game.terrain.cols, game.terrain.rows
    );
    let reset = clock.map != map || clock.tick > battle.snapshot.tick;
    clock.map = map;
    clock.tick = battle.snapshot.tick;
    if reset {
        *stats = Stats::default();
    }
    let clear = reset || menu.open || front.world;
    let dt = if battle.snapshot.paused {
        0.
    } else {
        time.delta_secs().min(0.1)
    };
    let mut count = 0;
    for (entity, mut shell, mut transform, mut tag) in &mut shells {
        shell.age += dt;
        if clear || shell.age >= LIFE || count >= limit(*quality) {
            commands.entity(entity).despawn();
        } else {
            let next_scale = Vec3::splat(scale(shell.age, shell.radius));
            if transform.scale != next_scale {
                transform.scale = next_scale;
            }
            let next_tag = ((shell.age / LIFE).clamp(0., 1.) * 65535.).round() as u32;
            if tag.0 != next_tag {
                tag.0 = next_tag;
            }
            count += 1;
        }
    }
    if menu.open || front.world || !enabled() {
        events.clear();
        return;
    }
    for blast in events.read() {
        if blast.variant == Variant::SpikyRemake || count >= limit(*quality) {
            continue;
        }
        let radius = blast.radius.clamp(1.0, 5.0) * blast.multiplier();
        commands.spawn((
            Shell { age: 0., radius },
            Mesh3d(art.sphere.clone()),
            MeshMaterial3d(art.material.clone()),
            MeshTag(0),
            Transform::from_translation(blast.at + Vec3::Y * 0.45 * blast.multiplier())
                .with_scale(Vec3::splat(scale(0., radius))),
            NotShadowCaster,
            NotShadowReceiver,
            NoAutomaticBatching,
        ));
        stats.spawned += 1;
        count += 1;
    }
    stats.peak = stats.peak.max(count);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pressure_front_decelerates_without_changing_its_maximum_extent() {
        assert!((scale(0., 3.) - 0.9).abs() < 0.0001);
        assert!((scale(LIFE, 3.) - 11.1).abs() < 0.0001);
        let early = scale(0.10, 3.) - scale(0.05, 3.);
        let late = scale(0.55, 3.) - scale(0.50, 3.);
        assert!(early > late && late > 0.);
    }
    #[test]
    fn pressure_shells_are_bounded_pause_and_clear_without_affecting_small_impacts() {
        let mut app = App::new();
        let mut menu = crate::menu::MainMenu::default();
        menu.open = false;
        app.insert_resource(menu)
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::runtime::Battle>()
            .init_resource::<crate::simulation::NativeGame>()
            .init_resource::<Time>()
            .init_resource::<Stats>()
            .insert_resource(Quality::Low)
            .insert_resource(Art {
                sphere: Handle::default(),
                material: Handle::default(),
            })
            .add_message::<Blast>()
            .add_systems(Update, update);
        let blast = Blast {
            at: Vec3::ZERO,
            radius: 3.,
            variant: Variant::Mushroom,
            seed: 1,
            visual_scale: 1.,
        };
        for _ in 0..30 {
            app.world_mut().write_message(blast);
        }
        app.update();
        assert_eq!(app.world().resource::<Stats>().spawned, 6);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(80));
        app.update();
        let before = app
            .world_mut()
            .query::<&Shell>()
            .iter(app.world())
            .next()
            .unwrap()
            .age;
        app.world_mut()
            .resource_mut::<crate::runtime::Battle>()
            .snapshot
            .paused = true;
        app.update();
        assert!(
            app.world_mut()
                .query::<&Shell>()
                .iter(app.world())
                .all(|s| s.age == before)
        );
        app.world_mut().resource_mut::<crate::menu::MainMenu>().open = true;
        app.update();
        assert_eq!(
            app.world_mut().query::<&Shell>().iter(app.world()).count(),
            0
        );
        app.world_mut().resource_mut::<crate::menu::MainMenu>().open = false;
        app.world_mut().write_message(Blast {
            variant: Variant::SpikyRemake,
            ..blast
        });
        app.update();
        assert_eq!(
            app.world_mut().query::<&Shell>().iter(app.world()).count(),
            0
        );
        assert!(scale(0.5, 3.) > scale(0.1, 3.));
        for visual_scale in [1., 2., 3.] {
            app.world_mut().write_message(Blast {
                radius: 6.,
                visual_scale,
                ..blast
            });
        }
        app.update();
        let mut radii: Vec<_> = app
            .world_mut()
            .query::<&Shell>()
            .iter(app.world())
            .map(|s| s.radius)
            .collect();
        radii.sort_by(f32::total_cmp);
        assert_eq!(
            radii,
            vec![5., 10., 15.],
            "pressure waves scale after their radius cap"
        );
        *app.world_mut().resource_mut::<Quality>() = Quality::High;
        for _ in 0..30 {
            app.world_mut().write_message(blast);
        }
        app.update();
        assert_eq!(
            app.world_mut().query::<&Shell>().iter(app.world()).count(),
            14
        );
        *app.world_mut().resource_mut::<Quality>() = Quality::Low;
        app.update();
        assert_eq!(
            app.world_mut().query::<&Shell>().iter(app.world()).count(),
            6,
            "lowering quality trims existing pressure shells immediately"
        );
    }
}
