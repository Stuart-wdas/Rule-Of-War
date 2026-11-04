//! Original, weapon-specific PCM layers and a bounded moving-vehicle mix.
use crate::{Controls, feedback::Feedback, menu::MainMenu, runtime::Battle};
use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum Cue {
    Order,
    Rifle,
    MachineGun,
    Cannon,
    Artillery,
    Rocket,
    Autocannon,
    Impact,
    Blast,
    Medical,
    Diesel,
    Rotor,
    Naval,
    Jet,
    Gau8,
    Propeller,
}
const CUES: [Cue; 16] = [
    Cue::Order,
    Cue::Rifle,
    Cue::MachineGun,
    Cue::Cannon,
    Cue::Artillery,
    Cue::Rocket,
    Cue::Autocannon,
    Cue::Impact,
    Cue::Blast,
    Cue::Medical,
    Cue::Diesel,
    Cue::Rotor,
    Cue::Naval,
    Cue::Jet,
    Cue::Gau8,
    Cue::Propeller,
];
#[derive(Resource)]
pub struct Sounds {
    clips: Vec<Vec<Handle<AudioSource>>>,
}
pub fn weapon(kind: &str) -> Cue {
    match kind {
        "machineGunner" => Cue::MachineGun,
        "medium" | "c130-30" => Cue::Cannon,
        "c130-105" => Cue::Artillery,
        "long" => Cue::Artillery,
        "xtralong" | "manpads" | "heli-rocket" => Cue::Rocket,
        "helicopter" | "heli-gun" | "antiAir" => Cue::Autocannon,
        "plane" => Cue::Gau8,
        "aircraftCarrier" => Cue::Artillery,
        _ => Cue::Rifle,
    }
}
fn duration(cue: Cue) -> f32 {
    match cue {
        Cue::Order => 0.12,
        Cue::Rifle => 0.30,
        Cue::MachineGun => 0.30,
        Cue::Cannon => 0.95,
        Cue::Artillery => 1.3,
        Cue::Rocket => 0.8,
        Cue::Autocannon => 0.4,
        Cue::Gau8 => 1.25,
        Cue::Impact => 0.14,
        Cue::Blast => {
            if crate::visual_demo::enabled() {
                2.1
            } else {
                1.4
            }
        }
        Cue::Medical => 0.35,
        _ => 1.,
    }
}
fn wave(cue: Cue, variant: u32) -> Vec<u8> {
    let rate = 22050u32;
    let seconds = duration(cue);
    let samples = (seconds * rate as f32) as usize;
    let mut bytes = Vec::with_capacity(samples * 2 + 44);
    bytes.extend(b"RIFF");
    bytes.extend((36 + samples as u32 * 2).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * 2).to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples as u32 * 2).to_le_bytes());
    let mut seed = 74129u32.wrapping_add(variant * 8791);
    let mut low = 0.;
    let mut mid = 0.;
    let pitch = 0.94 + variant as f32 * 0.045;
    for i in 0..samples {
        let t = i as f32 / rate as f32;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = seed as f32 / u32::MAX as f32 * 2. - 1.;
        low = low * 0.97 + noise * 0.03;
        mid = mid * 0.65 + noise * 0.35;
        let tone = |f: f32| (t * std::f32::consts::TAU * f * pitch).sin();
        let env = |decay: f32| (-t * decay).exp() * (t / 0.001).min(1.);
        let burst = |period: f32, decay: f32| {
            let local = t % period;
            (-local * decay).exp() * (local / 0.001).min(1.)
        };
        let signal = match cue {
            Cue::Order => (tone(920.) * 0.14 + tone(1360.) * 0.06) * env(30.),
            Cue::Rifle => {
                noise * 0.48 * env(85.) + mid * 0.22 * env(24.) + tone(125.) * 0.22 * env(30.)
            }
            Cue::MachineGun => {
                (noise * 0.37 + tone(190.) * 0.20) * burst(0.07, 75.) * (1. - t / seconds).max(0.)
            }
            Cue::Cannon => {
                noise * 0.27 * env(50.) + tone(64. - t * 22.) * 0.49 * env(9.) + low * 1.1 * env(4.)
            }
            Cue::Artillery => {
                noise * 0.24 * env(40.) + tone(43. - t * 12.) * 0.48 * env(5.) + low * 1.2 * env(3.)
            }
            Cue::Rocket => {
                (mid * 0.60 + low * 0.9 + tone(120.) * 0.15)
                    * (t / 0.08).min(1.)
                    * (1. - t / seconds).max(0.).powi(2)
            }
            Cue::Autocannon => {
                (noise * 0.30 + tone(270.) * 0.23) * burst(0.045, 100.) * (1. - t / seconds).max(0.)
            }
            Cue::Gau8 => {
                let gate = (t / 0.025).min(1.) * ((seconds - t) / 0.04).min(1.);
                ((noise * 0.32 + mid * 0.24) * burst(1. / 65., 160.)
                    + tone(65.) * 0.24
                    + tone(130.) * 0.10
                    + low * 1.1)
                    * gate
            }
            Cue::Impact => (noise * 0.20 + tone(650.) * 0.10) * env(55.),
            Cue::Blast => {
                if crate::visual_demo::enabled() {
                    let debris = if t > 0.12 {
                        mid * (t * 77.).sin().abs().powi(12) * 0.18 * (-t * 2.8).exp()
                    } else {
                        0.
                    };
                    noise * 0.32 * env(90.)
                        + tone(44. - t * 9.) * 0.44 * env(4.5)
                        + low * 2.1 * env(2.4)
                        + mid * 0.18 * env(8.)
                        + debris
                } else {
                    noise * 0.28 * env(55.)
                        + low * 1.8 * env(3.)
                        + tone(51. - t * 16.) * 0.45 * env(4.)
                }
            }
            Cue::Medical => (mid * 0.07 + tone(520.) * 0.08 + tone(780.) * 0.04) * env(15.),
            Cue::Diesel => {
                (tone(72.) * 0.13 + tone(144.) * 0.045 + low * 0.13) * (0.8 + tone(12.) * 0.2)
            }
            Cue::Rotor => {
                (low * 0.22 + tone(48.) * 0.13 + mid * 0.04) * (0.45 + tone(16.).abs() * 0.55)
            }
            Cue::Naval => tone(48.) * 0.12 + low * 0.20 + mid * 0.02,
            Cue::Jet => mid * 0.18 + low * 0.25 + tone(390.) * 0.025 + tone(780.) * 0.015,
            Cue::Propeller => {
                (low * 0.2 + mid * 0.08 + tone(90.) * 0.1 + tone(180.) * 0.045)
                    * (0.82 + tone(24.) * 0.18)
            }
        };
        // Short fades at loop boundaries; no hard discontinuity or clipped peaks.
        let edge = if matches!(
            cue,
            Cue::Diesel | Cue::Rotor | Cue::Naval | Cue::Jet | Cue::Propeller
        ) {
            (t / 0.006).min(1.) * ((seconds - t) / 0.006).min(1.)
        } else {
            ((seconds - t) / 0.01).min(1.)
        };
        bytes.extend((((signal * edge).clamp(-0.9, 0.9) * i16::MAX as f32) as i16).to_le_bytes());
    }
    bytes
}
const MAX_VOICES: usize = 24;
#[derive(Component)]
pub struct OneShot {
    remaining: f32,
}
#[derive(Default)]
pub struct History {
    seen: HashMap<String, u64>,
    map: String,
    tick: u64,
}
impl History {
    fn fresh(&mut self, e: &crate::simulation::BattleEvent, tick: u64) -> bool {
        if e.tick > tick || tick - e.tick > 8 {
            return false;
        }
        // Keep identity through temporary visibility gaps. Old snapshot events
        // must never restart the gun just because they disappeared for a frame.
        let key = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            e.tick,
            e.kind,
            e.source,
            e.weapon,
            e.owner,
            e.at[0].to_bits(),
            e.at[1].to_bits()
        );
        self.seen.insert(key, e.tick).is_none()
    }
}
pub fn setup(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    commands.insert_resource(Sounds {
        clips: CUES
            .into_iter()
            .map(|cue| {
                (0..3)
                    .map(|v| {
                        sources.add(AudioSource {
                            bytes: wave(cue, v).into(),
                        })
                    })
                    .collect()
            })
            .collect(),
    });
}
#[expect(clippy::too_many_arguments, reason = "Bevy injects system parameters")]
pub fn play(
    mut commands: Commands,
    sounds: Res<Sounds>,
    menu: Res<MainMenu>,
    battle: Res<Battle>,
    controls: Res<Controls>,
    feedback: Res<Feedback>,
    game: Res<crate::simulation::NativeGame>,
    front: Res<crate::frontend::Frontend>,
    time: Res<Time<Real>>,
    mut history: Local<History>,
    mut playing: Query<(Entity, &mut OneShot, Option<&AudioSink>)>,
    mut sequence: Local<u64>,
) {
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.seed, game.terrain.cols, game.terrain.rows
    );
    let reset = history.map != map || battle.snapshot.tick < history.tick;
    if reset {
        history.seen.clear();
    }
    history.map = map;
    history.tick = battle.snapshot.tick;
    history
        .seen
        .retain(|_, tick| battle.snapshot.tick.saturating_sub(*tick) <= 32);
    let enabled = menu.sound && !menu.open && !battle.snapshot.paused && !front.world && !reset;
    let mut active = 0;
    for (entity, mut voice, sink) in &mut playing {
        voice.remaining -= time.delta_secs();
        if !enabled || voice.remaining <= 0. {
            if let Some(sink) = sink {
                sink.stop();
            }
            commands.entity(entity).despawn();
        } else {
            active += 1;
        }
    }
    if *sequence != feedback.sequence {
        *sequence = feedback.sequence;
        if enabled && active < MAX_VOICES {
            let cue = if controls
                .selected
                .first()
                .and_then(|id| battle.snapshot.entities.iter().find(|p| &p.id == id))
                .is_some_and(|p| p.kind == "medic")
            {
                Cue::Medical
            } else {
                Cue::Order
            };
            commands.spawn((
                OneShot {
                    remaining: duration(cue) + 0.15,
                },
                AudioPlayer::new(sounds.clips[cue as usize][0].clone()),
                PlaybackSettings {
                    volume: Volume::Linear(0.35),
                    ..PlaybackSettings::DESPAWN
                },
            ));
            active += 1;
        }
    }
    let mut events: Vec<_> = battle
        .snapshot
        .events
        .iter()
        .filter(|e| history.fresh(e, battle.snapshot.tick))
        .collect();
    events.sort_by(|a, b| {
        let dist = |e: &crate::simulation::BattleEvent| {
            Vec2::new(e.at[0] - controls.center.x, e.at[1] - controls.center.z).length_squared()
        };
        dist(a).total_cmp(&dist(b))
    });
    let mut voices = 0;
    for e in events {
        if !enabled || voices >= 12 || active >= MAX_VOICES {
            continue;
        }
        let distance = Vec2::new(e.at[0] - controls.center.x, e.at[1] - controls.center.z).length();
        if distance > 110. {
            continue;
        }
        let cue = match e.kind.as_str() {
            "shot" if e.weapon == "plane" || e.source.starts_with("incoming-") => continue,
            "shot" => weapon(&e.weapon),
            "cannon-burst" => Cue::Gau8,
            "blast" | "destruction" | "crash-impact" | "aircraft-hit" => Cue::Blast,
            "impact" => Cue::Impact,
            "rescue" => Cue::Medical,
            _ => continue,
        };
        let base = match cue {
            Cue::Cannon | Cue::Artillery | Cue::Blast | Cue::Gau8 => 0.65,
            Cue::Rocket => 0.5,
            Cue::Impact => 0.13,
            _ => 0.30,
        };
        let variant = (e
            .tick
            .wrapping_add(e.source.bytes().map(u64::from).sum::<u64>())
            % 3) as usize;
        commands.spawn((
            OneShot {
                remaining: duration(cue) + 0.15,
            },
            AudioPlayer::new(sounds.clips[cue as usize][variant].clone()),
            PlaybackSettings {
                volume: Volume::Linear(base / (1. + distance / 35.)),
                ..PlaybackSettings::DESPAWN
            },
        ));
        voices += 1;
        active += 1;
    }
}
#[derive(Component)]
pub struct Engine(String);
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects audio lifecycle and scene resources"
)]
pub fn engines(
    mut commands: Commands,
    battle: Res<Battle>,
    controls: Res<Controls>,
    menu: Res<MainMenu>,
    sounds: Res<Sounds>,
    front: Res<crate::frontend::Frontend>,
    game: Res<crate::simulation::NativeGame>,
    mut clock: Local<(String, u64)>,
    mut engines: Query<(Entity, &Engine, Option<&mut AudioSink>)>,
) {
    let map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.seed, game.terrain.cols, game.terrain.rows
    );
    let reset = clock.0 != map || battle.snapshot.tick < clock.1;
    *clock = (map, battle.snapshot.tick);
    let mut audible: Vec<_> = battle
        .snapshot
        .entities
        .iter()
        .filter(|p| {
            p.hp > 0.
                && !reset
                && !front.world
                && !menu.open
                && menu.sound
                && !battle.snapshot.paused
                && (p.category == "vehicle"
                    || p.category == "naval"
                    || matches!(p.kind.as_str(), "helicopter" | "huey")
                    || matches!(p.kind.as_str(), "plane" | "c130"))
                && (Vec2::from(p.velocity).length() > 0.2 || p.airborne)
        })
        .map(|p| {
            (
                p,
                Vec2::new(p.x - controls.center.x, p.z - controls.center.z).length(),
            )
        })
        .filter(|(_, d)| *d < 85.)
        .collect();
    audible.sort_by(|a, b| a.1.total_cmp(&b.1));
    audible.truncate(4);
    let mut present = HashSet::new();
    for (entity, engine, sink) in &mut engines {
        if let Some((p, d)) = audible.iter().find(|(p, _)| p.id == engine.0) {
            present.insert(engine.0.clone());
            if let Some(mut sink) = sink {
                sink.set_volume(Volume::Linear(0.22 / (1. + d / 25.)));
                sink.set_speed(1. + Vec2::from(p.velocity).length().min(5.) * 0.035);
            }
        } else {
            if let Some(sink) = sink {
                sink.stop();
            }
            commands.entity(entity).despawn();
        }
    }
    for (p, d) in audible {
        if !present.contains(&p.id) {
            let cue = if matches!(p.kind.as_str(), "helicopter" | "huey") {
                Cue::Rotor
            } else if p.kind == "c130" {
                Cue::Propeller
            } else if p.kind == "plane" {
                Cue::Jet
            } else if p.category == "naval" {
                Cue::Naval
            } else {
                Cue::Diesel
            };
            commands.spawn((
                Engine(p.id.clone()),
                AudioPlayer::new(sounds.clips[cue as usize][0].clone()),
                PlaybackSettings {
                    volume: Volume::Linear(0.22 / (1. + d / 25.)),
                    ..PlaybackSettings::LOOP
                },
            ));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_infantry_stays_silent_but_weapons_still_play() {
        let mut app = App::new();
        let mut menu = MainMenu::default();
        menu.open = false;
        menu.sound = true;
        app.insert_resource(menu)
            .init_resource::<Battle>()
            .init_resource::<Controls>()
            .init_resource::<Feedback>()
            .init_resource::<crate::simulation::NativeGame>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time<Real>>()
            .init_resource::<Assets<AudioSource>>()
            .add_systems(Startup, setup)
            .add_systems(Update, (play, engines).chain());
        app.update();
        for tick in 10..50 {
            let mut b = app.world_mut().resource_mut::<Battle>();
            b.snapshot.tick = tick;
            b.snapshot.entities = (0..12)
                .map(|i| crate::runtime::Piece {
                    id: format!("walker-{i}"),
                    category: "troop".into(),
                    kind: "short".into(),
                    velocity: [2., 1.],
                    hp: 1.,
                    ..default()
                })
                .collect();
            app.update();
            assert_eq!(
                app.world_mut()
                    .query::<&AudioPlayer>()
                    .iter(app.world())
                    .count(),
                0
            );
        }
        app.world_mut().resource_mut::<Battle>().snapshot.events = vec![event(49)];
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&OneShot>()
                .iter(app.world())
                .count(),
            1
        );
    }
    fn event(tick: u64) -> crate::simulation::BattleEvent {
        serde_json::from_value(
            serde_json::json!({"tick":tick,"kind":"shot","at":[0.,0.],"from":null,
            "owner":0,"radius":1.,"weapon":"machineGunner","source":"test-gunner"}),
        )
        .unwrap()
    }
    #[test]
    fn retained_events_do_not_replay_after_visibility_gaps_or_late_snapshots() {
        let mut h = History::default();
        assert!(h.fresh(&event(10), 10));
        assert!(!h.fresh(&event(10), 10));
        assert!(!h.fresh(&event(10), 14));
        assert!(!h.fresh(&event(1), 14));
        assert!(!h.fresh(&event(15), 14));
        assert!(h.fresh(&event(15), 15));
        assert!(h.fresh(&event(16), 16));
        let remembered = h.seen.len();
        for tick in 0..1000 {
            assert!(!h.fresh(&event(tick), 2000));
        }
        assert_eq!(
            h.seen.len(),
            remembered,
            "retained stale projectiles must not grow audio history"
        );
    }
    #[test]
    fn burst_spam_is_bounded_and_muted_events_are_not_queued_for_replay() {
        let mut app = App::new();
        let mut menu = MainMenu::default();
        menu.open = false;
        menu.sound = true;
        app.insert_resource(menu)
            .init_resource::<Battle>()
            .init_resource::<Controls>()
            .init_resource::<Feedback>()
            .init_resource::<crate::simulation::NativeGame>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time<Real>>()
            .init_resource::<Assets<AudioSource>>()
            .add_systems(Startup, setup)
            .add_systems(Update, (play, engines).chain());
        app.update();
        for tick in 10..15 {
            {
                let mut b = app.world_mut().resource_mut::<Battle>();
                b.snapshot.tick = tick;
                b.snapshot.events = (0..100)
                    .map(|i| {
                        let mut e = event(tick);
                        e.source = format!("gun-{i}");
                        e
                    })
                    .collect();
            }
            app.update();
            assert!(
                app.world_mut()
                    .query::<&OneShot>()
                    .iter(app.world())
                    .count()
                    <= MAX_VOICES
            );
        }
        app.world_mut().resource_mut::<MainMenu>().sound = false;
        app.update();
        app.world_mut().resource_mut::<MainMenu>().sound = true;
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&OneShot>()
                .iter(app.world())
                .count(),
            0
        );
        app.world_mut().spawn(Engine("obsolete-unit".into()));
        app.update();
        assert_eq!(
            app.world_mut().query::<&Engine>().iter(app.world()).count(),
            0
        );
    }
    #[test]
    fn one_shots_expire_without_an_audio_device_and_silence_on_pause_menu_mute_and_map_reset() {
        let mut app = App::new();
        let mut menu = MainMenu::default();
        menu.open = false;
        menu.sound = true;
        app.insert_resource(menu)
            .init_resource::<Battle>()
            .init_resource::<Controls>()
            .init_resource::<Feedback>()
            .init_resource::<crate::simulation::NativeGame>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<Time<Real>>()
            .init_resource::<Assets<AudioSource>>()
            .add_systems(Startup, setup)
            .add_systems(Update, play);
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query::<&OneShot>()
                .iter(app.world())
                .count()
        };
        for mode in 0..5 {
            {
                let mut b = app.world_mut().resource_mut::<Battle>();
                b.snapshot.paused = false;
                b.snapshot.tick = 20 + mode * 20;
                b.snapshot.events = vec![event(b.snapshot.tick)];
            }
            app.world_mut().resource_mut::<MainMenu>().open = false;
            app.world_mut().resource_mut::<MainMenu>().sound = true;
            app.update();
            assert_eq!(count(&mut app), 1);
            // Temporarily losing the visible event must not restart its sound.
            let events = app
                .world_mut()
                .resource_mut::<Battle>()
                .snapshot
                .events
                .clone();
            app.world_mut()
                .resource_mut::<Battle>()
                .snapshot
                .events
                .clear();
            app.update();
            app.world_mut().resource_mut::<Battle>().snapshot.events = events;
            app.update();
            assert_eq!(count(&mut app), 1);
            match mode {
                0 => app
                    .world_mut()
                    .resource_mut::<Time<Real>>()
                    .advance_by(std::time::Duration::from_secs(2)),
                1 => app.world_mut().resource_mut::<Battle>().snapshot.paused = true,
                2 => app.world_mut().resource_mut::<MainMenu>().open = true,
                3 => app.world_mut().resource_mut::<MainMenu>().sound = false,
                _ => app.world_mut().resource_mut::<Battle>().snapshot.tick = 0,
            }
            app.update();
            assert_eq!(count(&mut app), 0);
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::ZERO);
        }
    }
    #[test]
    fn generated_cues_are_decodable_finite_pcm() {
        use bevy::audio::Decodable;
        let mut distinct = HashSet::new();
        for cue in CUES {
            for v in 0..3 {
                let bytes = wave(cue, v);
                assert_eq!(&bytes[..4], b"RIFF");
                assert_eq!(&bytes[8..12], b"WAVE");
                assert_eq!(
                    u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
                    bytes.len() - 44
                );
                assert!(
                    bytes[44..]
                        .chunks_exact(2)
                        .any(|p| i16::from_le_bytes(p.try_into().unwrap()) != 0)
                );
                if v == 0 {
                    assert!(distinct.insert(bytes.clone()));
                }
                let source = AudioSource {
                    bytes: bytes.into(),
                };
                assert_eq!(source.decoder().take(128).count(), 128);
            }
        }
        assert_eq!(weapon("machineGunner"), Cue::MachineGun);
        assert_ne!(weapon("short"), weapon("medium"));
        assert_ne!(weapon("long"), weapon("xtralong"));
    }
}
