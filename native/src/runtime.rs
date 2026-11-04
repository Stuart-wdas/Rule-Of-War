//! In-process command queue between Bevy input systems and the fixed-step game.
//! Local play stays in process; network sessions use the native TCP transport.
use crate::simulation::NativeGame;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Piece {
    pub id: String,
    pub kind: String,
    pub owner: i32,
    pub x: f32,
    pub z: f32,
    pub elevation: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub heading: f32,
    #[serde(default = "unit_scale")]
    pub model_scale: f32,
    pub building: bool,
    pub category: String,
    pub airborne: bool,
    pub altitude: Option<f32>,
    pub flight_pitch: f32,
    pub flight_bank: f32,
    pub fuel_fraction: f32,
    #[serde(alias = "bombPayload")]
    pub cannon_ammo: u8,
    pub flare_salvos: u8,
    pub flare_cooldown: f32,
    pub aa_reload: f32,
    pub aa_elevated: bool,
    pub tactical_role: String,
    pub label: String,
    pub strength: u32,
    pub bridge_end: Option<[f32; 2]>,
    pub town_blocks: Vec<Value>,
    pub town_plan: crate::settlement_layout::TownPlan,
    pub cover: u8,
    pub suppression: f32,
    pub downed: bool,
    pub retreating: bool,
    pub velocity: [f32; 2],
    pub orders: Vec<[f32; 2]>,
    pub waypoints: Vec<[f32; 2]>,
    pub lots: Vec<crate::settlement_layout::Lot>,
    pub activity: String,
    pub aim: Option<[f32; 2]>,
    pub auto_attack: bool,
    pub strafe_target: Option<[f32; 2]>,
    pub strafe_heading: Option<f32>,
    pub strafe_locked: bool,
    pub gunship_target: Option<[f32; 2]>,
    pub gunship_laps: u8,
    pub rotor_power: f32,
    pub home_airfield: Option<String>,
    pub under_fire: bool,
    pub training_target: bool,
    pub sight: f32,
    pub squad: Option<u8>,
    pub garrison: Option<crate::simulation::Garrison>,
    pub capacity: usize,
    pub occupants: usize,
}
impl Default for Piece {
    fn default() -> Self {
        Self {
            id: Default::default(),
            kind: Default::default(),
            owner: Default::default(),
            x: Default::default(),
            z: Default::default(),
            elevation: Default::default(),
            hp: Default::default(),
            max_hp: Default::default(),
            heading: Default::default(),
            model_scale: 1.,
            building: Default::default(),
            category: Default::default(),
            airborne: Default::default(),
            altitude: None,
            flight_pitch: 0.,
            flight_bank: 0.,
            fuel_fraction: 1.,
            cannon_ammo: 0,
            flare_salvos: 0,
            flare_cooldown: 0.,
            aa_reload: 0.,
            aa_elevated: false,
            tactical_role: String::new(),
            label: Default::default(),
            strength: Default::default(),
            bridge_end: Default::default(),
            town_blocks: Default::default(),
            town_plan: Default::default(),
            cover: Default::default(),
            suppression: Default::default(),
            downed: Default::default(),
            retreating: Default::default(),
            velocity: Default::default(),
            orders: Default::default(),
            waypoints: Vec::new(),
            lots: Default::default(),
            activity: Default::default(),
            aim: Default::default(),
            auto_attack: true,
            strafe_target: None,
            strafe_heading: None,
            strafe_locked: false,
            gunship_target: None,
            gunship_laps: 0,
            rotor_power: 0.,
            home_airfield: None,
            under_fire: Default::default(),
            training_target: false,
            sight: 0.,
            squad: None,
            garrison: None,
            capacity: 0,
            occupants: 0,
        }
    }
}

fn unit_scale() -> f32 {
    1.
}

/// Misses pass clear of even the A-10's wings, matching the saved interception roll.
/// The renderer and authoritative airburst use exactly the same destination.
pub fn missile_destination(target: &Piece, miss: bool) -> Vec3 {
    target.position()
        + if miss {
            Vec3::new(-target.heading.sin() * 12., 6., target.heading.cos() * 12.)
        } else {
            Vec3::ZERO
        }
}

/// Structure contact is evaluated only on new shots/impacts, not per particle.
fn structure_piece_height(p: &Piece, at: [f32; 2]) -> Option<f32> {
    if !p.building || p.hp <= 0. || p.bridge_end.is_some() || p.kind == "objective" {
        return None;
    }
    let scale = p.model_scale.max(0.1);
    let local = Quat::from_rotation_y(-p.heading)
        * (Vec3::new(at[0], p.elevation, at[1]) - p.position())
        / scale;
    let height = if !p.lots.is_empty() {
        p.lots
            .iter()
            .filter_map(|l| l.roof_height([local.x, local.z]))
            .max_by(f32::total_cmp)
    } else {
        let bound = if p.kind == "airfield" { 160. } else { 16. };
        if ["town", "village"].contains(&p.kind.as_str())
            || local.x.abs() > bound
            || local.z.abs() > bound
        {
            return None;
        }
        // Facility roofs use their actual component transforms, so hangars,
        // towers and empty runway/apron areas do not share one guessed height.
        crate::models::parts(p)
            .iter()
            .filter_map(|part| {
                let inverse = part.transform.to_matrix().inverse();
                let origin = inverse.transform_point3(Vec3::new(local.x, 64., local.z));
                let direction = inverse.transform_vector3(Vec3::NEG_Y);
                let (mut enter, mut exit) = (0_f32, 128_f32);
                for axis in 0..3 {
                    if direction[axis].abs() < 0.000001 {
                        if origin[axis].abs() > 0.5 {
                            return None;
                        }
                    } else {
                        let a = (-0.5 - origin[axis]) / direction[axis];
                        let b = (0.5 - origin[axis]) / direction[axis];
                        enter = enter.max(a.min(b));
                        exit = exit.min(a.max(b));
                        if enter > exit {
                            return None;
                        }
                    }
                }
                Some(64. - enter + 0.08)
            })
            .max_by(f32::total_cmp)
    };
    height.map(|h| p.position().y + h * scale)
}

/// Visual projectile/impact contact with a structure, retaining open courtyards.
pub fn structure_contact_height<'a>(
    pieces: impl Iterator<Item = &'a Piece>,
    at: [f32; 2],
    ground: f32,
) -> f32 {
    pieces
        .filter_map(|p| structure_piece_height(p, at))
        .fold(ground, f32::max)
}

/// Shared cannon contact surface for tracers and their impact particles.
pub fn cannon_contact_height<'a>(
    pieces: impl Iterator<Item = &'a Piece>,
    at: [f32; 2],
    owner: i32,
    ground: f32,
) -> f32 {
    pieces
        .filter(|p| p.hp > 0. && p.owner != owner && !p.airborne && p.garrison.is_none())
        .filter_map(|p| {
            let delta = Vec2::new(at[0] - p.x, at[1] - p.z);
            let (sin, cos) = p.heading.sin_cos();
            let along = delta.x * cos + delta.y * sin;
            let across = -delta.x * sin + delta.y * cos;
            let scale = p.model_scale.max(0.1);
            if p.building {
                structure_piece_height(p, at)
            } else if p.category == "vehicle" {
                if delta.length_squared() > (4. * scale).powi(2) {
                    return None;
                }
                // Test the posed component bounds from above. A single deck height
                // places hits inside a turret and loses the flash behind its armor.
                let ray = Vec3::new(-across / (scale * 0.82), 8., along / scale);
                let pose = crate::animation::Pose::target(p);
                crate::models::parts(p)
                    .into_iter()
                    .filter_map(|part| {
                        let matrix = crate::animation::joint(
                            part.transform,
                            part.pivot,
                            part.joint,
                            &pose,
                            0.,
                        )
                        .to_matrix()
                        .inverse();
                        let origin = matrix.transform_point3(ray);
                        let direction = matrix.transform_vector3(Vec3::NEG_Y);
                        let mut enter: f32 = 0.;
                        let mut exit: f32 = 16.;
                        for axis in 0..3 {
                            if direction[axis].abs() < 0.000001 {
                                if origin[axis].abs() > 0.5 {
                                    return None;
                                }
                            } else {
                                let a = (-0.5 - origin[axis]) / direction[axis];
                                let b = (0.5 - origin[axis]) / direction[axis];
                                enter = enter.max(a.min(b));
                                exit = exit.min(a.max(b));
                                if enter > exit {
                                    return None;
                                }
                            }
                        }
                        Some(p.elevation + (8. - enter) * scale + 0.035)
                    })
                    .max_by(f32::total_cmp)
            } else if p.category == "troop" && delta.length_squared() <= (0.6 * scale).powi(2) {
                Some(p.elevation + 0.85 * scale)
            } else {
                None
            }
        })
        .fold(ground + 0.08, f32::max)
}
impl Piece {
    pub fn flight_height(&self) -> f32 {
        if !self.airborne {
            0.
        } else if let Some(height) = self.altitude {
            height.max(0.)
        } else if self.kind == "plane" {
            72.
        } else {
            8.
        }
    }
    pub fn model_transform_scale(&self) -> Vec3 {
        Vec3::new(if self.category == "vehicle" { 0.82 } else { 1. }, 1., 1.) * self.model_scale
    }
    pub fn position(&self) -> Vec3 {
        let (x, z) = self
            .bridge_end
            .map(|end| ((self.x + end[0]) * 0.5, (self.z + end[1]) * 0.5))
            .unwrap_or((self.x, self.z));
        let inside = self
            .garrison
            .as_ref()
            .is_some_and(|g| g.entered && (!g.exiting || g.door_stage == 2));
        let inset = if inside {
            Vec3::new(self.heading.cos(), 0., self.heading.sin()) * 0.65
        } else {
            Vec3::ZERO
        };
        Vec3::new(x, self.elevation + self.flight_height(), z) - inset
    }
    pub fn pick_positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.lots
            .is_empty()
            .then(|| {
                self.position()
                    + Vec3::Y
                        * if self.building {
                            3.0 * self.model_scale
                        } else {
                            self.model_scale
                                * match self.category.as_str() {
                                    "troop" => 0.95,
                                    "vehicle" => 1.15,
                                    "aircraft" => 1.0,
                                    _ => 1.0,
                                }
                        }
            })
            .into_iter()
            .chain(
                (!self.building && self.lots.is_empty())
                    .then_some([0.55, 1.35])
                    .into_iter()
                    .flatten()
                    .map(|height| self.position() + Vec3::Y * height * self.model_scale),
            )
            .chain(
                (self.kind == "airfield")
                    .then_some([
                        Vec3::new(-14., 5.4, 42.),
                        Vec3::new(-14., 8., -36.6),
                        Vec3::new(30., 0.2, 0.),
                    ])
                    .into_iter()
                    .flatten()
                    .map(|at| {
                        self.position()
                            + Quat::from_rotation_y(self.heading) * at * self.model_scale
                    }),
            )
            .chain(self.lots.iter().flat_map(move |lot| {
                lot.wings().into_iter().map(move |wing| {
                    let at = lot.to_world(wing.at);
                    self.position()
                        + Quat::from_rotation_y(self.heading)
                            * Vec3::new(
                                at[0],
                                lot.ground + crate::settlement_layout::HOUSE_HEIGHT * 0.5,
                                at[1],
                            )
                            * self.model_scale
                })
            }))
            .chain(
                self.lots
                    .iter()
                    .filter(|lot| lot.integrity > 0.)
                    .map(|lot| {
                        let (door, _) = lot.door();
                        self.position()
                            + Quat::from_rotation_y(self.heading)
                                * Vec3::new(door[0], lot.ground + 1., door[1])
                                * self.model_scale
                    }),
            )
            .chain(
                crate::simulation::facility_door(&self.kind)
                    .into_iter()
                    .map(|door| {
                        self.position()
                            + Quat::from_rotation_y(self.heading)
                                * Vec3::new(door[0], 1., door[1])
                                * self.model_scale
                    }),
            )
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Sight {
    pub at: [f32; 2],
    pub radius: f32,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub simulation: bool,
    #[serde(default)]
    pub team_supply: [f32; 2],
    #[serde(default)]
    pub viewer: i32,
    #[serde(default)]
    pub training: Option<crate::simulation::TrainingSummary>,
    #[serde(default)]
    pub sights: Vec<Sight>,
    #[serde(default)]
    pub fog_enabled: bool,
    #[serde(default)]
    pub bases: Vec<crate::simulation::BasePlan>,
    pub tick: u64,
    pub paused: bool,
    pub winner: Value,
    pub supply: f32,
    pub notice: String,
    pub victory_progress: f32,
    #[serde(default)]
    pub objective_hold: [f32; 2],
    #[serde(default)]
    pub outcome_reason: String,
    pub entities: Vec<Piece>,
    pub jobs: Vec<Value>,
    pub objectives: Vec<Value>,
    pub projectiles: Vec<Value>,
    #[serde(default)]
    pub props: Vec<crate::simulation::BattlefieldProp>,
    #[serde(default)]
    pub events: Vec<crate::simulation::BattleEvent>,
}
#[derive(Resource, Default)]
pub struct Battle {
    pub snapshot: Snapshot,
    pub catalog: Value,
    pub error: Option<String>,
}
#[derive(Resource, Default)]
pub struct CommandQueue(Mutex<Vec<Value>>);
impl CommandQueue {
    pub fn send(&self, value: Value) {
        self.0.lock().unwrap().push(value);
    }
    pub fn order(
        &self,
        ids: &[String],
        kind: &str,
        destination: Option<Vec3>,
        target: Option<&str>,
        queued: bool,
    ) {
        self.send(json!({"type":"command","unitIds":ids,"commandType":kind,"destination":destination.map(|p|json!({"x":p.x,"y":p.z})),"targetId":target,"queued":queued}));
    }
}
pub fn receive(
    queue: Res<CommandQueue>,
    mut game: ResMut<NativeGame>,
    mut battle: ResMut<Battle>,
    mut session: ResMut<crate::network::Session>,
    mut loading: ResMut<crate::loading::Loading>,
    mut controls: ResMut<crate::Controls>,
) {
    let _timing = crate::performance::span("runtime.receive");
    let commands: Vec<Value> = queue.0.lock().unwrap().drain(..).collect();
    let mut immediate = vec![];
    for command in commands {
        if loading.busy() {
            continue;
        }
        match command["type"].as_str() {
            Some("map" | "new") if session.owner == 0 => {
                if command["type"] == "new" && game.simulation && !session.multiplayer() {
                    loading.begin(
                        crate::loading::Request::Simulation(crate::simulation::core_config(
                            &game.config,
                        )),
                        "Preparing AI vs AI simulation".into(),
                    );
                    continue;
                }
                if crate::visual_demo::enabled() && command["type"] == "new" {
                    game.setup_visual_demo();
                    continue;
                }
                let id = if command["type"] == "new" && game.terrain.id == crate::gis_prototype::ID
                {
                    crate::gis_prototype::ID
                } else if command["type"] == "new" && game.training.is_some() {
                    crate::training::ID
                } else {
                    crate::battlefield::ID
                }
                .to_owned();
                let cols = if id == crate::training::ID {
                    crate::training::SIZE
                } else {
                    crate::battlefield::COLS
                };
                let rows = if id == crate::training::ID {
                    crate::training::SIZE
                } else {
                    crate::battlefield::ROWS
                };
                loading.begin(
                    crate::loading::Request::Map {
                        id: id.clone(),
                        cols,
                        rows,
                        config: crate::simulation::core_config(&game.config),
                    },
                    format!("Preparing {id} · {cols} × {rows}"),
                );
            }
            Some("load") if !session.multiplayer() => loading.begin(
                crate::loading::Request::Restore(game.operation_save_path()),
                "Restoring your operation".into(),
            ),
            _ => immediate.push(command),
        }
    }
    battle.snapshot = session.receive(&mut game, immediate);
    if let Some(id) = battle
        .snapshot
        .training
        .as_ref()
        .and_then(|t| t.last_purchase.as_ref())
        && controls.training_purchase.as_ref() != Some(id)
        && let Some(unit) = battle.snapshot.entities.iter().find(|p| &p.id == id)
    {
        controls.training_purchase = Some(id.clone());
        controls.selected = vec![id.clone()];
        controls.focus_at(Vec3::new(unit.x, 0., unit.z));
    }
    if battle.catalog.is_null() {
        battle.catalog = crate::simulation::active_catalog();
    }
}
pub fn tick(
    mut game: ResMut<NativeGame>,
    session: Res<crate::network::Session>,
    menu: Res<crate::menu::MainMenu>,
    loading: Res<crate::loading::Loading>,
    ready: Res<crate::landscape::Ready>,
) {
    let _timing = crate::performance::span("simulation.tick");
    if session.can_step() && !menu.open && !loading.busy() && ready.ready {
        game.step();
    }
}

#[cfg(test)]
mod picking_tests {
    use super::*;
    #[test]
    fn building_contacts_follow_rotated_scaled_roofs_and_leave_courtyards_open() {
        let mut lot = crate::settlement_layout::Lot {
            at: [2., 3.],
            width: 10.,
            depth: 9.,
            height: 3.2,
            yaw: 0.4,
            style: 1,
            ground: 1.5,
            integrity: 1.,
        };
        let mut town = Piece {
            kind: "town".into(),
            building: true,
            hp: 100.,
            x: 50.,
            z: 60.,
            elevation: 8.,
            heading: 0.7,
            model_scale: 1.3,
            ..default()
        };
        let world = |q: [f32; 2]| {
            let local = lot.to_world(q);
            let v = town.position()
                + Quat::from_rotation_y(town.heading)
                    * Vec3::new(local[0], 0., local[1])
                    * town.model_scale;
            [v.x, v.z]
        };
        let roof = world(lot.wings()[0].at);
        let courtyard = world([2., -2.]);
        town.lots = vec![lot.clone()];
        let height = structure_contact_height(std::iter::once(&town), roof, 8.);
        assert!(height > 8. + (lot.ground + 3.2) * 1.3);
        assert_eq!(
            structure_contact_height(std::iter::once(&town), courtyard, 8.),
            8.
        );
        assert_eq!(
            cannon_contact_height(std::iter::once(&town), roof, 1, 8.),
            height
        );
        lot.integrity = 0.;
        town.lots[0] = lot;
        assert_eq!(
            structure_contact_height(std::iter::once(&town), roof, 8.),
            8.
        );
        let facility = Piece {
            kind: "barracks".into(),
            building: true,
            hp: 10.,
            x: 30.,
            z: 40.,
            elevation: 5.,
            ..default()
        };
        assert!(structure_contact_height(std::iter::once(&facility), [30., 40.], 5.) > 8.);
        assert_eq!(
            structure_contact_height(std::iter::once(&facility), [45., 55.], 5.),
            5.
        );
    }
    #[test]
    fn garrison_models_stand_behind_their_window_instead_of_outside_the_wall() {
        let mut p = Piece {
            x: 20.,
            z: 30.,
            heading: 0.7,
            garrison: Some(crate::simulation::Garrison {
                building: "house".into(),
                lot: Some(0),
                window: 0,
                entered: true,
                ready_at: 0,
                door_stage: 2,
                exiting: false,
            }),
            ..Default::default()
        };
        let facing = Vec3::new(p.heading.cos(), 0., p.heading.sin());
        let window = Vec3::new(p.x, p.elevation, p.z);
        assert!((p.position() - (window - facing * 0.65)).length() < 0.00001);
        p.garrison.as_mut().unwrap().entered = false;
        assert_eq!(p.position(), window);
    }

    #[test]
    fn pick_anchors_follow_model_scale_and_cover_hull_and_turret_height() {
        let tank = Piece {
            category: "vehicle".into(),
            model_scale: 2.25,
            elevation: 7.,
            ..default()
        };
        let anchors: Vec<_> = tank
            .pick_positions()
            .map(|p| p.y - tank.position().y)
            .collect();
        assert!(anchors.iter().any(|h| (*h - 1.15 * 2.25).abs() < 0.00001));
        assert!(anchors.iter().any(|h| (*h - 0.55 * 2.25).abs() < 0.00001));
        assert!(anchors.iter().any(|h| (*h - 1.35 * 2.25).abs() < 0.00001));
        let airborne = Piece {
            category: "aircraft".into(),
            kind: "helicopter".into(),
            airborne: true,
            elevation: 7.,
            model_scale: 1.7,
            ..default()
        };
        assert!((airborne.pick_positions().next().unwrap().y - 16.7).abs() < 0.00001);
    }

    #[test]
    fn cannon_contacts_armor_but_near_misses_and_friendly_units_hit_ground() {
        let tank = Piece {
            category: "vehicle".into(),
            hp: 100.,
            owner: 1,
            x: 10.,
            z: 20.,
            elevation: 7.,
            heading: std::f32::consts::FRAC_PI_2,
            ..default()
        };
        assert!(cannon_contact_height([&tank].into_iter(), [10., 22.], 0, 7.) > 8.);
        assert!(
            cannon_contact_height([&tank].into_iter(), [10., 20.], 0, 7.) > 8.5,
            "contact must clear the turret roof, not sit inside the hull"
        );
        assert!(cannon_contact_height([&tank].into_iter(), [12., 20.], 0, 7.) < 7.1);
        assert!(cannon_contact_height([&tank].into_iter(), [10., 20.], 1, 7.) < 7.1);
    }
}
