//! A brief, shallow volume of soil spreads horizontally beneath mushroom blasts.
use crate::volume_effects::{Blast, Quality, Variant, VolumeMaterial};
use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::MeshTag,
    prelude::*,
    render::batching::NoAutomaticBatching,
};
const LIFE: f32 = 0.95;
#[derive(Component)]
pub struct Disc {
    age: f32,
    radius: f32,
    multiplier: f32,
    seed: u32,
    origin: Vec3,
    rotation: Quat,
}
#[derive(Resource)]
struct Art {
    mesh: Handle<Mesh>,
    materials: [Handle<VolumeMaterial>; 3],
}
#[derive(Default, Resource)]
pub struct Stats {
    pub spawned: u32,
    pub peak: usize,
    pub maximum_radius: f32,
}
#[derive(Default)]
struct Clock {
    map: String,
    tick: u64,
}
fn index(q: Quality) -> usize {
    match q {
        Quality::Low => 0,
        Quality::Balanced => 1,
        Quality::High => 2,
    }
}
fn limit(q: Quality) -> usize {
    [6, 10, 14][index(q)]
}
fn expansion(age: f32) -> f32 {
    // An initial ground impulse loses speed continuously; a hard expansion
    // cutoff made the latter half of the dust read as a stationary flat card.
    let t = (age / LIFE).clamp(0., 1.);
    1. - (1. - t).powi(5)
}
fn opacity(age: f32) -> f32 {
    let t = ((age - 0.12) / (LIFE - 0.12)).clamp(0., 1.);
    (age / 0.018).clamp(0., 1.) * (1. - t * t * (3. - 2. * t)) * 0.70
}
fn pose(d: &Disc) -> (Transform, u32) {
    let grow = expansion(d.age);
    let radius = d.radius * (0.40 + 1.80 * grow);
    let height = (0.18 + 0.08 * grow) * d.multiplier;
    let transform = Transform::from_translation(d.origin + Vec3::Y * height * 0.55)
        .with_rotation(d.rotation)
        .with_scale(Vec3::new(radius, height, radius));
    let tag = ((opacity(d.age) * 255.).round() as u32) | ((d.seed & 16383) << 16) | (2u32 << 30);
    (transform, tag)
}
pub fn register(app: &mut App) {
    app.init_resource::<Stats>()
        .add_systems(Startup, setup)
        .add_systems(Update, update.after(crate::effects::ground_rings));
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VolumeMaterial>>,
) {
    commands.insert_resource(Art {
        mesh: meshes.add(Cuboid::new(2., 2., 2.)),
        materials: [Quality::Low, Quality::Balanced, Quality::High]
            .map(|q| materials.add(VolumeMaterial::ground_dust(q))),
    });
}
#[expect(
    clippy::too_many_arguments,
    reason = "Shared blast events and render lifecycle resources"
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
    mut discs: Query<(
        Entity,
        &mut Disc,
        &mut Transform,
        &mut MeshTag,
        &mut MeshMaterial3d<VolumeMaterial>,
    )>,
) {
    let _timing = crate::performance::span("effects.dirt_disc");
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
    for (entity, mut disc, mut transform, mut tag, mut material) in &mut discs {
        disc.age += dt;
        if clear || disc.age >= LIFE || count >= limit(*quality) {
            commands.entity(entity).despawn();
            continue;
        }
        let (next, bits) = pose(&disc);
        transform.set_if_neq(next);
        if tag.0 != bits {
            tag.0 = bits;
        }
        if material.0 != art.materials[index(*quality)] {
            material.0 = art.materials[index(*quality)].clone();
        }
        stats.maximum_radius = stats.maximum_radius.max(transform.scale.x);
        count += 1;
    }
    if menu.open || front.world {
        events.clear();
        return;
    }
    for blast in events.read() {
        let ground = game.terrain.height(blast.at.x, blast.at.z);
        if blast.variant != Variant::Mushroom
            || count >= limit(*quality)
            || game.terrain.cell(blast.at.x, blast.at.z).water()
            || blast.at.y - ground > blast.radius.clamp(1., 6.) * blast.multiplier() * 2. + 2.
        {
            continue;
        }
        let normal = Vec3::new(
            game.terrain.height(blast.at.x - 0.5, blast.at.z)
                - game.terrain.height(blast.at.x + 0.5, blast.at.z),
            1.,
            game.terrain.height(blast.at.x, blast.at.z - 0.5)
                - game.terrain.height(blast.at.x, blast.at.z + 0.5),
        )
        .normalize();
        let disc = Disc {
            age: 0.,
            radius: (blast.radius * 1.6).clamp(3.2, 8.5) * blast.multiplier(),
            multiplier: blast.multiplier(),
            seed: blast.seed,
            origin: Vec3::new(blast.at.x, ground + 0.06, blast.at.z),
            rotation: Quat::from_rotation_arc(Vec3::Y, normal),
        };
        let (transform, bits) = pose(&disc);
        commands.spawn((
            disc,
            Mesh3d(art.mesh.clone()),
            MeshMaterial3d(art.materials[index(*quality)].clone()),
            MeshTag(bits),
            transform,
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
    fn dirt_impulse_slows_continuously_and_fades_before_lifting() {
        assert_eq!(expansion(0.), 0.);
        assert_eq!(expansion(LIFE), 1.);
        assert!(expansion(0.55) > expansion(0.50));
        let early = expansion(0.10) - expansion(0.05);
        let late = expansion(0.60) - expansion(0.55);
        assert!(early > late && late > 0.);
        assert!(opacity(0.05) > opacity(0.5));
        assert!(opacity(0.5) > opacity(0.85));
        assert_eq!(opacity(0.), 0.);
        assert_eq!(opacity(LIFE), 0.);
        let disc = Disc {
            age: 0.8,
            radius: 5.,
            multiplier: 1.,
            seed: 7,
            origin: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        let (transform, _) = pose(&disc);
        assert!(transform.scale.y < 0.3);
        assert!(transform.translation.y + transform.scale.y < 0.45);
    }
    #[test]
    fn dirt_disc_spreads_as_a_filled_low_volume_then_fades_and_cleans_up() {
        let mut app = App::new();
        let mut menu = crate::menu::MainMenu::default();
        menu.open = false;
        let mut game = crate::simulation::NativeGame::default();
        game.terrain = crate::terrain::Terrain::new("river-crossing", 128, 128);
        app.insert_resource(menu)
            .insert_resource(game)
            .init_resource::<crate::runtime::Battle>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time>()
            .init_resource::<Stats>()
            .insert_resource(Quality::Low)
            .insert_resource(Art {
                mesh: Handle::default(),
                materials: std::array::from_fn(|_| Handle::default()),
            })
            .add_message::<Blast>()
            .add_systems(Update, update);
        let blast = Blast {
            at: Vec3::new(16., 0., 16.),
            radius: 1.5,
            variant: Variant::Mushroom,
            seed: 7,
            visual_scale: 1.,
        };
        for _ in 0..30 {
            app.world_mut().write_message(blast);
        }
        app.update();
        assert_eq!(app.world().resource::<Stats>().spawned, 6);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app.update();
        let (radius, alpha) = {
            let mut q = app.world_mut().query::<(&Transform, &MeshTag)>();
            let (t, tag) = q.iter(app.world()).next().unwrap();
            assert!(t.scale.x > 3. && t.scale.y < 0.4 && t.translation.y + t.scale.y < 0.6);
            (t.scale.x, tag.0 & 255)
        };
        app.world_mut()
            .resource_mut::<crate::runtime::Battle>()
            .snapshot
            .paused = true;
        app.update();
        assert!(
            app.world_mut()
                .query::<&Transform>()
                .iter(app.world())
                .all(|t| t.scale.x == radius)
        );
        app.world_mut()
            .resource_mut::<crate::runtime::Battle>()
            .snapshot
            .paused = false;
        for _ in 0..7 {
            app.update();
        }
        assert!(
            app.world_mut()
                .query::<(&Transform, &MeshTag)>()
                .iter(app.world())
                .all(|(t, tag)| t.scale.x > radius && (tag.0 & 255) < alpha)
        );
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world_mut().query::<&Disc>().iter(app.world()).count(),
            0
        );
        for (variant, height) in [(Variant::Debris, 0.), (Variant::Mushroom, 20.)] {
            app.world_mut().write_message(Blast {
                variant,
                at: blast.at.with_y(height),
                ..blast
            });
            app.update();
            assert_eq!(
                app.world_mut().query::<&Disc>().iter(app.world()).count(),
                0
            );
        }
        app.world_mut().write_message(blast);
        app.update();
        app.world_mut().resource_mut::<crate::menu::MainMenu>().open = true;
        app.update();
        assert_eq!(
            app.world_mut().query::<&Disc>().iter(app.world()).count(),
            0
        );
        assert!(opacity(0.05) > opacity(0.9) && opacity(LIFE) == 0.);
    }
}
