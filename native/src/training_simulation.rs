// Included in simulation.rs so practice commands reuse the actual combat and unit rules.
#[derive(Clone, Serialize, Deserialize)]
pub struct TrainingTarget {
    id: String,
    kind: String,
    at: Point,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TrainingGround {
    pub live_fire: bool,
    pub shots: u64,
    pub hits: u64,
    pub damage: f32,
    pub destroyed: u64,
    pub last_purchase: Option<String>,
    targets: Vec<TrainingTarget>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TrainingSummary {
    pub live_fire: bool,
    pub shots: u64,
    pub hits: u64,
    pub damage: f32,
    pub destroyed: u64,
    pub targets_alive: usize,
    pub last_purchase: Option<String>,
}
pub fn training_unit_price(kind: &str) -> Option<f32> {
    if !active_kind(kind) || !catalog()["units"][kind].is_object() || flag(spec(kind), "callIn") {
        return None;
    }
    if kind == "commander" {
        // Commanders are starting units rather than ordinary recruits.
        return Some(0.);
    }
    let price = number(spec(kind), "price", 0.);
    (price > 0.).then_some(price)
}
impl NativeGame {
    fn training_ground(config: Value) -> Self {
        let mut s = Self::new();
        s.terrain = crate::terrain::Terrain::new(
            crate::training::ID,
            crate::training::SIZE,
            crate::training::SIZE,
        );
        s.config = config;
        s.config["cols"] = json!(crate::training::SIZE);
        s.config["rows"] = json!(crate::training::SIZE);
        s.entities.clear();
        s.bases.clear();
        s.props.clear();
        s.counter = 0;
        s.training = Some(TrainingGround::default());
        s.human_opponent = true;
        s.supply = [5000., 0.];
        s.deploy_base(0);
        for (i, kind) in ["short", "medic", "engineer", "commander", "scout"]
            .into_iter()
            .enumerate()
        {
            s.spawn(kind, 0, [84., 144. + i as f32 * 4.]);
            s.entities.last_mut().unwrap().stance = if flag(spec(kind), "medical") {
                "attack"
            } else {
                "hold-fire"
            }
            .into();
        }
        s.spawn("objective", -1, [174., 280.]);
        s.spawn("village", -1, crate::training::BUILDINGS);
        let houses = &mut s.entities.last_mut().unwrap().view;
        houses.label = "PRACTICE HOUSES".into();
        houses.lots = crate::training::practice_houses();
        s.refresh_towns();
        for (i, (kind, at)) in [
            ("sandbags", [116., 96.]),
            ("wall", [146., 146.]),
            ("crates", [200., 200.]),
            ("sandbags", [124., 244.]),
            ("wall", [162., 244.]),
        ]
        .into_iter()
        .enumerate()
        {
            s.props.push(BattlefieldProp {
                scar: None,
                id: format!("practice-cover-{i}"),
                kind: kind.into(),
                at,
                heading: std::f32::consts::FRAC_PI_2,
                wreck: None,
                hp: 5.,
            });
        }
        for (kind, at) in [
            ("short", [128., 72.]),
            ("medium", [176., 120.]),
            ("medium", [272., 168.]),
            ("helicopter", [236., 216.]),
        ] {
            s.add_training_target(kind, at);
        }
        s.notice="TRAINING GROUND | T: shop / tools. Practice houses between lanes 3 and 4: right-click to enter, U to leave. F2 resets the range.".into();
        s
    }
    pub fn operation_save_path(&self) -> std::path::PathBuf {
        if self.terrain.id == crate::gis_prototype::ID {
            save_path().with_file_name(crate::gis_prototype::SAVE)
        } else if self.terrain.id == crate::visual_demo::ID {
            save_path().with_file_name("native-visual-study.json")
        } else if self.simulation {
            save_path().with_file_name("native-simulation.json")
        } else if self.training.is_some() {
            save_path().with_file_name("native-training.json")
        } else {
            save_path()
        }
    }
    fn training_summary(&self) -> Option<TrainingSummary> {
        self.training.as_ref().map(|t| TrainingSummary {
            live_fire: t.live_fire,
            shots: t.shots,
            hits: t.hits,
            damage: t.damage,
            destroyed: t.destroyed,
            last_purchase: t.last_purchase.clone(),
            targets_alive: self
                .entities
                .iter()
                .filter(|e| e.view.training_target && e.view.hp > 0.)
                .count(),
        })
    }
    fn add_training_target(&mut self, kind: &str, at: Point) {
        let id = self.spawn(kind, 1, at);
        let live = self.training.as_ref().is_some_and(|t| t.live_fire);
        let e = self.entities.last_mut().unwrap();
        e.view.training_target = true;
        e.view.heading = std::f32::consts::PI;
        e.view.label = format!("{} TARGET", spec(kind)["label"].as_str().unwrap_or(kind));
        e.view.airborne = flag(spec(kind), "airUnit");
        e.stance = if live { "attack" } else { "hold-fire" }.into();
        self.training
            .as_mut()
            .unwrap()
            .targets
            .push(TrainingTarget {
                id,
                kind: kind.into(),
                at,
            });
    }
    fn training_command(&mut self, owner: i32, msg: &Value) -> String {
        if owner != 0 || self.training.is_none() {
            return "Training tools are only available in your training ground.".into();
        }
        let action = msg["action"].as_str().unwrap_or("");
        match action {
            "buy" => {
                let kind = msg["kind"].as_str().unwrap_or("");
                let Some(price) = training_unit_price(kind) else {
                    return "Choose an active unit from the shop.".into();
                };
                if self.supply[0] < price {
                    return "Insufficient supply; use REFILL SUPPLY.".into();
                }
                let aircraft_stand = if flag(spec(kind), "airUnit") {
                    let available = self
                        .entities
                        .iter()
                        .filter(|e| e.view.owner == 0 && e.view.kind == "airfield")
                        .find_map(|e| {
                            self.free_aircraft_stand(&e.view.id, kind, None)
                                .map(|slot| (e.view.id.clone(), slot))
                        });
                    if available.is_none() {
                        return "No free stand for this aircraft at an owned airfield.".into();
                    }
                    available
                } else {
                    None
                };
                let Some(at) = self.open_land(crate::training::STAGING, kind) else {
                    return "Staging area blocked; move existing units first.".into();
                };
                let id = self.spawn(kind, 0, at);
                self.entities.last_mut().unwrap().stance = if flag(spec(kind), "medical") {
                    "attack"
                } else {
                    "hold-fire"
                }
                .into();
                if let Some((base, slot)) = aircraft_stand {
                    let i = self.entities.iter().position(|e| e.view.id == id).unwrap();
                    self.park_aircraft(i, &base, slot);
                }
                self.supply[0] -= price;
                self.training.as_mut().unwrap().last_purchase = Some(id);
                if kind == "plane" {
                    "Bought A-10 WARTHOG. Right-click a ground target or use Q for a cannon strafing sortie."
                        .into()
                } else if kind == "huey" {
                    "Bought UH-1Y VENOM. L loads up to 12 nearby troops; right-click to fly; U lands and unloads on clear ground.".into()
                } else if kind == "helicopter" {
                    "Bought HELICOPTER. Right-click a target or Q to attack. Gun vs infantry, rockets vs armour; automatic breakaway and fuel return.".into()
                } else {
                    format!(
                        "Bought {}. Right-click a target or press E to fire.",
                        spec(kind)["label"].as_str().unwrap_or(kind)
                    )
                }
            }
            "target" | "cover" => {
                let Some(at) = read_point(&msg["destination"]) else {
                    return "Click a position on the range.".into();
                };
                let kind = msg["kind"].as_str().unwrap_or("");
                if !self.passable(at, action == "target" && flag(spec(kind), "airUnit"))
                    || self.entities.iter().any(|e| distance(position(e), at) < 3.)
                    || (action == "cover" && self.props.iter().any(|p| distance(p.at, at) < 3.))
                {
                    return "Choose clear ground away from units and facilities.".into();
                }
                if action == "target" {
                    if !["short", "medium", "helicopter"].contains(&kind) {
                        return "Choose an infantry, armour or air target.".into();
                    }
                    if self.training.as_ref().unwrap().targets.len() >= 64 {
                        return "Target limit reached; reset the range with F2.".into();
                    }
                    self.add_training_target(kind, at);
                    "Target placed. Right-click it with a selected unit to fire.".into()
                } else {
                    if !["sandbags", "wall", "crates"].contains(&kind) {
                        return "Choose sandbags, a wall or crates.".into();
                    }
                    if self.props.len() >= 256 {
                        return "Cover limit reached; reset the range with F2.".into();
                    }
                    let id = self.id();
                    self.props.push(BattlefieldProp {
                        scar: None,
                        id,
                        kind: kind.into(),
                        at,
                        heading: std::f32::consts::FRAC_PI_2,
                        wreck: None,
                        hp: 5.,
                    });
                    "Practice cover placed.".into()
                }
            }
            "reset-targets" => {
                let plans = self.training.as_ref().unwrap().targets.clone();
                self.entities.retain(|e| !e.view.training_target);
                self.props
                    .retain(|p| !plans.iter().any(|t| p.id == format!("wreck-{}", t.id)));
                self.projectiles.clear();
                self.events.clear();
                let t = self.training.as_mut().unwrap();
                t.targets.clear();
                t.shots = 0;
                t.hits = 0;
                t.damage = 0.;
                t.destroyed = 0;
                for target in plans {
                    self.add_training_target(&target.kind, target.at);
                }
                "Targets restored at their placed positions; shot statistics cleared.".into()
            }
            "clear-targets" => {
                self.entities.retain(|e| !e.view.training_target);
                self.projectiles.clear();
                "Targets cleared. RESET TARGETS restores their positions.".into()
            }
            "live-fire" => {
                let t = self.training.as_mut().unwrap();
                t.live_fire = !t.live_fire;
                let live = t.live_fire;
                for e in self.entities.iter_mut().filter(|e| e.view.training_target) {
                    e.stance = if live { "attack" } else { "hold-fire" }.into();
                    e.target = None;
                    e.windup = 0;
                }
                if live {
                    "Targets now return fire. Test cover, suppression, retreat and medics."
                } else {
                    "Targets hold fire."
                }
                .into()
            }
            "refill" => {
                self.supply[0] = 5000.;
                "Supply refilled to 5000.".into()
            }
            "fog" => {
                let enabled = !self.config["fogEnabled"].as_bool().unwrap_or(true);
                self.config["fogEnabled"] = json!(enabled);
                format!(
                    "Training fog of war {}.",
                    if enabled { "enabled" } else { "disabled" }
                )
            }
            "heal" | "wound" => {
                let ids = msg["unitIds"].as_array().cloned().unwrap_or_default();
                let mut affected = 0;
                for e in self.entities.iter_mut().filter(|e| {
                    e.view.owner == 0
                        && !e.view.building
                        && ids.iter().any(|id| id.as_str() == Some(&e.view.id))
                }) {
                    if action == "wound" {
                        if !["short", "scout", "engineer"].contains(&e.view.kind.as_str()) {
                            continue;
                        }
                        e.view.hp = 0.01;
                        e.view.downed = true;
                        e.view.strength = 0;
                        e.downed_until = self.tick + 400;
                    } else {
                        e.view.hp = e.view.max_hp;
                        e.view.downed = false;
                        e.view.strength =
                            number(spec(&e.view.kind), "formationStrength", 1.) as u32;
                        e.fuel = number(spec(&e.view.kind), "fuelCapacity", 0.);
                    }
                    e.view.suppression = 0.;
                    e.view.retreating = false;
                    e.view.velocity = [0., 0.];
                    e.path.clear();
                    e.orders.clear();
                    e.cover_goal = None;
                    e.cover_ordered = false;
                    e.speed = 0.;
                    e.medical_until = 0;
                    e.treatment_target = None;
                    e.stance = if flag(spec(&e.view.kind), "medical") {
                        "attack"
                    } else {
                        "hold-fire"
                    }
                    .into();
                    affected += 1;
                }
                format!(
                    "{affected} selected unit(s) {}.",
                    if action == "wound" {
                        "wounded; a medic has 20 seconds to rescue them"
                    } else {
                        "restored"
                    }
                )
            }
            _ => "Unknown training action.".into(),
        }
    }
}

#[cfg(test)]
mod training_tests {
    use super::*;
    fn range() -> NativeGame {
        let mut c = default_config();
        c["fogEnabled"] = json!(false);
        NativeGame::operation(crate::training::ID, 384, 384, c)
    }
    fn command(g: &mut NativeGame, action: &str) {
        g.command(json!({"type":"training","action":action}));
    }
    #[test]
    fn practice_houses_keep_lanes_open_allow_entry_and_reset_after_destruction() {
        let mut g = range();
        let building = g
            .entities
            .iter()
            .find(|e| e.view.label == "PRACTICE HOUSES")
            .unwrap();
        let id = building.view.id.clone();
        assert_eq!(
            (0..4)
                .map(|lot| g.window_slots(building, Some(lot)).len())
                .collect::<Vec<_>>(),
            vec![15, 13, 17, 9]
        );
        let at = [
            building.view.x + building.view.lots[0].at[0],
            building.view.z + building.view.lots[0].at[1],
        ];
        assert!(!g.passable(at, false));
        for lane in [72., 120., 168., 216.] {
            assert!(
                g.clear_segment([96., lane], [312., lane], 0),
                "firing lane {lane} is obstructed"
            );
        }
        g.command(json!({"type":"training","action":"buy","kind":"short"}));
        let rifle = g.training.as_ref().unwrap().last_purchase.clone().unwrap();
        g.command(json!({"type":"command","commandType":"garrison","unitIds":[rifle],"targetId":id,"destination":{"x":at[0],"y":at[1]}}));
        for _ in 0..600 {
            g.step();
        }
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == rifle)
                .unwrap()
                .view
                .garrison
                .as_ref()
                .is_some_and(|s| s.entered && s.lot == Some(0)),
            "unit at {:?}, path {:?}, garrison {:?}",
            g.entities.iter().find(|e| e.view.id == rifle).map(position),
            g.entities
                .iter()
                .find(|e| e.view.id == rifle)
                .map(|e| &e.path),
            g.entities
                .iter()
                .find(|e| e.view.id == rifle)
                .map(|e| &e.view.garrison)
        );
        let house_hp = g
            .entities
            .iter()
            .find(|e| e.view.id == id)
            .unwrap()
            .view
            .lots[0]
            .max_hp();
        g.impact(&Projectile {
            owner: 1,
            from: [100., 192.],
            to: at,
            born: g.tick,
            impact: g.tick,
            damage: house_hp,
            radius: 0.25,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        g.release_collapsed_garrisons();
        assert_eq!(
            g.entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .lots[0]
                .integrity,
            0.
        );
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == rifle)
                .unwrap()
                .view
                .garrison
                .is_none()
        );
        let mut restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        restored.terrain.hydrate();
        restored.refresh_towns();
        restored.validate().unwrap();
        assert_eq!(
            restored
                .entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .lots[0]
                .integrity,
            0.
        );
        restored.command(json!({"type":"new"}));
        let houses = restored
            .entities
            .iter()
            .find(|e| e.view.label == "PRACTICE HOUSES")
            .unwrap();
        assert!(houses.view.lots.iter().all(|lot| lot.integrity == 1.));
    }
    #[test]
    fn targets_stay_still_and_training_continues_after_they_are_destroyed() {
        let mut g = range();
        let before: Vec<_> = g
            .entities
            .iter()
            .filter(|e| e.view.training_target)
            .map(|e| (e.view.id.clone(), position(e)))
            .collect();
        for _ in 0..120 {
            g.step();
        }
        assert!(g.projectiles.is_empty());
        for (id, at) in &before {
            assert_eq!(
                position(g.entities.iter().find(|e| &e.view.id == id).unwrap()),
                *at
            );
        }
        for e in g.entities.iter_mut().filter(|e| e.view.training_target) {
            e.view.hp = 0.;
        }
        g.step();
        assert!(g.winner.is_none());
        assert_eq!(g.training.as_ref().unwrap().destroyed, 4);
        let tick = g.tick;
        for _ in 0..20 {
            g.step();
        }
        assert_eq!(g.tick, tick + 20);
        command(&mut g, "reset-targets");
        assert_eq!(g.training_summary().unwrap().targets_alive, 4);
    }
    #[test]
    fn buying_uses_real_prices_and_rejects_invalid_and_retired_units() {
        let mut g = range();
        let before = g.supply[0];
        g.command(json!({"type":"training","action":"buy","kind":"medium"}));
        assert_eq!(g.supply[0], before - number(spec("medium"), "price", 0.));
        let id = g.training.as_ref().unwrap().last_purchase.as_ref().unwrap();
        assert!(
            g.entities
                .iter()
                .any(|e| &e.view.id == id && e.view.kind == "medium" && e.view.owner == 0)
        );
        let count = g.entities.len();
        for kind in ["port", "waterTransport", "unknown"] {
            g.command(json!({"type":"training","action":"buy","kind":kind}));
            assert_eq!(g.entities.len(), count);
        }
        g.supply[0] = 0.;
        g.command(json!({"type":"training","action":"buy","kind":"short"}));
        assert_eq!(g.entities.len(), count);
        g.command(json!({"type":"training","action":"buy","kind":"commander"}));
        assert_eq!(g.entities.len(), count + 1);
        assert_eq!(g.supply[0], 0.);
        command(&mut g, "refill");
        assert_eq!(g.supply[0], 5000.);
        let mut normal = NativeGame::default();
        let count = normal.entities.len();
        normal.command(json!({"type":"training","action":"buy","kind":"medium"}));
        assert_eq!(normal.entities.len(), count);
    }
    #[test]
    fn bought_tank_orders_launch_real_rounds_and_damage_targets() {
        let mut g = range();
        g.command(json!({"type":"training","action":"buy","kind":"medium"}));
        let id = g.training.as_ref().unwrap().last_purchase.clone().unwrap();
        g.command(json!({"type":"training","action":"target","kind":"short","destination":{"x":116.,"y":176.}}));
        let target = g
            .training
            .as_ref()
            .unwrap()
            .targets
            .last()
            .unwrap()
            .id
            .clone();
        g.command(
            json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":target}),
        );
        for _ in 0..900 {
            g.step();
        }
        let t = g.training.as_ref().unwrap();
        assert!(t.shots > 0 && t.hits > 0 && t.damage > 0.);
        assert!(t.destroyed > 0);
        assert!(!g.entities.iter().any(|e| e.view.id == target));
        assert!(g.winner.is_none());
        command(&mut g, "fog");
        assert!(g.snapshot().fog_enabled);
        command(&mut g, "fog");
        assert!(!g.snapshot().fog_enabled);
    }
    #[test]
    fn placed_targets_cover_return_fire_and_reset_use_the_actual_simulation() {
        let mut g = range();
        g.command(json!({"type":"training","action":"target","kind":"short","destination":{"x":150.,"y":300.}}));
        assert_eq!(g.training_summary().unwrap().targets_alive, 5);
        g.command(json!({"type":"training","action":"cover","kind":"sandbags","destination":{"x":170.,"y":300.}}));
        assert_eq!(g.cover_level([171., 300.], Some([160., 300.])), 2);
        command(&mut g, "live-fire");
        assert!(g.training.as_ref().unwrap().live_fire);
        assert!(
            g.entities
                .iter()
                .filter(|e| e.view.training_target)
                .all(|e| e.stance == "attack")
        );
        let at = [150., 300.];
        g.impact(&Projectile {
            owner: 0,
            from: [140., 300.],
            to: at,
            born: 0,
            impact: 0,
            damage: 1.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert_eq!(g.training.as_ref().unwrap().hits, 1);
        assert!((g.training.as_ref().unwrap().damage - 1.).abs() < 0.01);
        command(&mut g, "reset-targets");
        assert!(
            g.entities
                .iter()
                .any(|e| e.view.training_target && position(e) == at && e.view.hp == e.view.max_hp)
        );
        assert_eq!(g.training.as_ref().unwrap().hits, 0);
    }
    #[test]
    fn wounds_rescue_and_saved_range_state_survive_a_round_trip() {
        let mut g = range();
        let id = g
            .entities
            .iter()
            .find(|e| e.view.kind == "short" && e.view.owner == 0)
            .unwrap()
            .view
            .id
            .clone();
        g.command(json!({"type":"training","action":"wound","unitIds":[id]}));
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .downed
        );
        for _ in 0..200 {
            g.step();
        }
        assert!(
            !g.entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .downed
        );
        assert_ne!(g.operation_save_path(), save_path());
        let data = serde_json::to_string(&g).unwrap();
        let restored: NativeGame = serde_json::from_str(&data).unwrap();
        assert_eq!(restored.training_summary().unwrap().targets_alive, 4);
        assert!(restored.training.is_some());
    }
    #[test]
    fn capture_practice_does_not_end_training_and_live_targets_really_fire() {
        let mut g = range();
        let post = g
            .entities
            .iter_mut()
            .find(|e| e.view.kind == "objective")
            .unwrap();
        post.view.owner = 0;
        for _ in 0..200 {
            g.objectives();
        }
        assert!(g.winner.is_none());
        let target = g.spawn("short", 1, [90., 144.]);
        let e = g.entities.last_mut().unwrap();
        e.view.training_target = true;
        e.stance = "hold-fire".into();
        command(&mut g, "live-fire");
        for _ in 0..25 {
            g.step();
        }
        assert!(
            g.events
                .iter()
                .any(|e| e.kind == "shot" && e.source == target)
        );
    }
}
