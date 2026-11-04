// Included by simulation.rs. All lock, countermeasure and kill rolls are authoritative
// and saved, so local play, reloads and multiplayer resolve the same interception.
const MANPADS_HIT_CHANCE: f32 = 0.60;
const MANPADS_FLARED_HIT_CHANCE: f32 = 0.20;
fn initial_flare_salvos() -> u8 {
    3
}
fn missile_hits(roll: f32, flared: bool) -> bool {
    roll < if flared {
        MANPADS_FLARED_HIT_CHANCE
    } else {
        MANPADS_HIT_CHANCE
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct GuidedMissile {
    #[serde(default)]
    hit: Option<bool>,
    source: String,
    target: String,
    owner: i32,
    from: Point,
    born: u64,
    impact: u64,
}
impl NativeGame {
    pub fn smoke_infantry_pose(&mut self, id: &str, launcher: bool) {
        if let Some(e) = self.entities.iter_mut().find(|e| e.view.id == id) {
            e.stance = "hold-fire".into();
            if launcher {
                e.aa_pose_until = 100000;
            }
        }
    }
    pub fn smoke_support_attack(&mut self, hit_check: bool) {
        self.rng = if hit_check { 1 } else { 12345 };
        self.terrain = crate::terrain::Terrain::new(
            crate::training::ID,
            crate::training::SIZE,
            crate::training::SIZE,
        );
        self.training = Some(TrainingGround::default());
        self.jobs.clear();
        self.notice = "SUPPORT AA / AUTOMATIC FLARES / INTERCEPTION DEMONSTRATION".into();
        self.entities.clear();
        self.bases.clear();
        self.props.clear();
        self.projectiles.clear();
        self.guided_missiles.clear();
        self.config["fogEnabled"] = json!(false);
        self.human_opponent = true;
        self.winner = None;
        self.tick = 10;
        self.spawn("engineer", 0, [152., 168.]);
        self.spawn("plane", 1, [247., 168.]);
        self.entities[1].view.airborne = true;
        self.entities[1].view.altitude = Some(32.);
        self.entities[1].view.training_target = true;
        self.entities[1].stance = "hold-fire".into();
        for _ in 0..26 {
            self.tick += 1;
            self.combat();
        }
        self.paused = true;
    }
    fn deploy_flares(&mut self, i: usize) {
        let e = &mut self.entities[i];
        if !e.view.airborne || e.aircraft.flares == 0 || self.tick < e.aircraft.next_flare {
            return;
        }
        let s = spec(&e.view.kind);
        e.aircraft.flares -= 1;
        e.aircraft.next_flare = self.tick + (number(s, "flareCooldownSeconds", 12.) / DT) as u64;
        e.aircraft.decoy_until = self.tick + (number(s, "flareProtectionSeconds", 3.) / DT) as u64;
        self.events.push(BattleEvent {
            hit: None,
            target: None,
            tick: self.tick,
            kind: "flares".into(),
            at: position(e),
            from: Some([e.view.heading.cos(), e.view.heading.sin()]),
            owner: e.view.owner,
            radius: 2.,
            weapon: e.view.kind.clone(),
            source: e.view.id.clone(),
            duration: 2.8,
            release_height: Some(e.view.elevation + e.view.flight_height()),
        });
    }
    fn support_anti_air(&mut self) {
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            let s = spec(&e.view.kind);
            if !flag(s, "antiAirLauncher") {
                continue;
            }
            let explicit = e
                .orders
                .front()
                .filter(|o| o.kind == "attack")
                .and_then(|o| o.target.as_deref());
            let allowed = e.view.hp > 0.
                && !e.view.downed
                && e.transport.is_none()
                && e.view.garrison.is_none()
                && e.view.suppression < 0.8
                && !["hold-fire", "flee", "retreat"].contains(&e.stance.as_str())
                && e.orders
                    .front()
                    .is_none_or(|o| !["move", "board"].contains(&o.kind.as_str()))
                && self.tick >= e.aa_reload_until;
            let target = if allowed {
                self.entities
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        t.view.hp > 0.
                            && t.view.airborne
                            && t.view.owner >= 0
                            && t.view.owner != e.view.owner
                            && t.transport.is_none()
                            && explicit.is_none_or(|id| id == t.view.id)
                            && distance(position(e), position(t)) <= number(s, "aaRange", 160.)
                            && self.visible(e.view.owner, t)
                            && self.building_line_clear(
                                position(e),
                                position(t),
                                None,
                                Some(&t.view.id),
                            )
                    })
                    .min_by(|(_, a), (_, b)| {
                        distance(position(e), position(a))
                            .total_cmp(&distance(position(e), position(b)))
                    })
                    .map(|(index, t)| (index, t.view.id.clone(), position(t)))
            } else {
                None
            };
            let Some((target_index, id, to)) = target else {
                self.entities[i].aa_target = None;
                self.entities[i].aa_lock_until = 0;
                continue;
            };
            let at = position(&self.entities[i]);
            let e = &mut self.entities[i];
            e.target = Some(id.clone());
            e.view.heading = (to[1] - at[1]).atan2(to[0] - at[0]);
            // Holding a lock requires a stationary, unobstructed launcher.
            e.speed = 0.;
            e.view.velocity = [0., 0.];
            if e.aa_target.as_ref() != Some(&id) {
                e.aa_target = Some(id.clone());
                e.aa_lock_until = self.tick + (number(s, "aaLockSeconds", 1.25) / DT) as u64;
                continue;
            }
            if self.tick < e.aa_lock_until {
                continue;
            }
            e.aa_reload_until = self.tick + (number(s, "aaReloadSeconds", 12.) / DT) as u64;
            e.aa_pose_until = self.tick + 16;
            e.last_shot = self.tick;
            e.aa_target = None;
            let source = e.view.id.clone();
            let owner = e.view.owner;
            let height = self.entities[target_index].view.elevation
                + self.entities[target_index].view.flight_height();
            let slant = distance(at, to).hypot(height - self.terrain.height(at[0], at[1]));
            let travel = (slant / number(s, "aaMissileSpeed", 55.) / DT)
                .ceil()
                .max(1.) as u64;
            self.deploy_flares(target_index);
            let flared = self.entities[target_index].aircraft.decoy_until > self.tick;
            let hit = missile_hits(self.random(), flared);
            self.guided_missiles.push(GuidedMissile {
                hit: Some(hit),
                source: source.clone(),
                target: id.clone(),
                owner,
                from: at,
                born: self.tick,
                impact: self.tick + travel,
            });
            self.events.push(BattleEvent {
                hit: Some(hit),
                target: Some(id),
                tick: self.tick,
                kind: "shot".into(),
                at,
                from: Some(to),
                owner,
                radius: 0.3,
                weapon: "manpads".into(),
                source,
                duration: travel as f32 * DT,
                release_height: None,
            });
            if owner == 0
                && let Some(t) = &mut self.training
            {
                t.shots += 1;
            }
        }
        let arrivals: Vec<_> = self
            .guided_missiles
            .iter()
            .filter(|m| m.impact <= self.tick)
            .cloned()
            .collect();
        self.guided_missiles.retain(|m| m.impact > self.tick);
        for m in arrivals {
            let Some(i) = self.entities.iter().position(|e| {
                e.view.id == m.target
                    && e.view.hp > 0.
                    && e.view.category == "aircraft"
                    && e.view.owner != m.owner
            }) else {
                continue;
            };
            let protected = self.entities[i].aircraft.decoy_until > self.tick;
            let hit = m
                .hit
                .unwrap_or_else(|| missile_hits(self.random(), protected));
            let e = &mut self.entities[i];
            if hit {
                let hp = e.view.hp;
                e.view.hp = 0.;
                e.view.strength = 0;
                if m.owner == 0
                    && e.view.training_target
                    && let Some(t) = &mut self.training
                {
                    t.hits += 1;
                    t.damage += hp;
                }
                // record_wrecks emits the fireball at the aircraft's actual altitude.
            } else {
                let miss = crate::runtime::missile_destination(&e.view, true);
                self.events.push(BattleEvent {
                    hit: Some(false),
                    target: None,
                    tick: self.tick,
                    kind: "missile-miss".into(),
                    at: [miss.x, miss.z],
                    from: Some(m.from),
                    owner: m.owner,
                    radius: 0.7,
                    weapon: "manpads".into(),
                    source: m.source,
                    duration: 0.,
                    release_height: Some(miss.y),
                });
            }
        }
    }
}

#[cfg(test)]
mod infantry_class_tests {
    use super::*;
    fn range() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(false);
        g.tick = 10;
        g.human_opponent = true;
        g
    }
    fn aircraft(g: &mut NativeGame, kind: &str, at: Point) -> String {
        let id = g.spawn(kind, 1, at);
        let e = g.entities.last_mut().unwrap();
        e.view.airborne = true;
        e.view.altitude = Some(40.);
        e.aircraft.flares = 3;
        id
    }
    fn combat(g: &mut NativeGame, n: u64) {
        for _ in 0..n {
            g.tick += 1;
            g.combat();
        }
    }
    #[test]
    fn four_recruitable_infantry_classes_and_distinct_sniper_role() {
        let roles: std::collections::HashSet<_> = catalog()["units"]
            .as_object()
            .unwrap()
            .values()
            .filter(|s| {
                s["category"] == "troop" && s["recruitable"] != false && !flag(s, "retired")
            })
            .map(|s| s["infantryClass"].as_str().unwrap())
            .collect();
        assert_eq!(
            roles,
            std::collections::HashSet::from(["assault", "sniper", "medic", "support"])
        );
        assert!(number(spec("scout"), "range", 0.) > number(spec("short"), "range", 0.) * 2.);
        assert!(number(spec("scout"), "reloadSeconds", 0.) >= 4.);
        assert!(flag(spec("medic"), "medical"));
    }
    #[test]
    fn dice_boundaries_are_sixty_and_twenty_percent() {
        assert!(missile_hits(0.5999, false));
        assert!(!missile_hits(0.60, false));
        assert!(missile_hits(0.1999, true));
        assert!(!missile_hits(0.20, true));
    }
    #[test]
    fn shoulder_missiles_travel_at_half_the_previous_speed_for_both_teams() {
        for owner in [0, 1] {
            let mut g = range();
            g.spawn("engineer", owner, [70., 70.]);
            aircraft(&mut g, "helicopter", [150., 70.]);
            g.entities[1].view.owner = 1 - owner;
            combat(&mut g, 30);
            let shot = g.events.iter().find(|e| e.weapon == "manpads").unwrap();
            let height = g.entities[1].view.elevation + g.entities[1].view.flight_height()
                - g.terrain.height(70., 70.);
            let expected = (80_f32.hypot(height) / 55. / DT).ceil() as u64;
            let missile = &g.guided_missiles[0];
            assert_eq!(missile.impact - missile.born, expected);
            assert_eq!(shot.duration, expected as f32 * DT);
            assert!((number(spec("engineer"), "aaMissileSpeed", 0.) - 55.).abs() < 0.001);
        }
    }
    #[test]
    fn sniper_can_engage_beyond_rifle_range_but_fires_slowly() {
        let mut g = range();
        g.spawn("scout", 0, [30., 30.]);
        g.spawn("short", 0, [30., 32.]);
        g.spawn("short", 1, [120., 30.]);
        g.entities[2].view.hp = 100.;
        g.entities[2].view.max_hp = 100.;
        combat(&mut g, 200);
        let shots: Vec<_> = g
            .events
            .iter()
            .filter(|e| e.kind == "shot" && e.owner == 0)
            .collect();
        assert!((2..=3).contains(&shots.len()));
        assert!(shots.iter().all(|e| e.weapon == "scout"));
        assert!(shots.windows(2).all(|w| w[1].tick - w[0].tick >= 90));
    }
    #[test]
    fn support_keeps_its_rifle_and_hold_fire_blocks_missile_launches() {
        let mut g = range();
        g.spawn("engineer", 0, [70., 70.]);
        g.spawn("short", 1, [80., 70.]);
        combat(&mut g, 35);
        assert!(
            g.events
                .iter()
                .any(|e| e.kind == "shot" && e.weapon == "engineer")
        );
        aircraft(&mut g, "helicopter", [90., 70.]);
        g.entities[0].stance = "hold-fire".into();
        combat(&mut g, 40);
        assert!(!g.events.iter().any(|e| e.weapon == "manpads"));
        assert_eq!(g.entities[2].aircraft.flares, 3);
    }
    #[test]
    fn rifles_never_attack_airborne_targets_and_support_locks_both_aircraft() {
        for kind in ["plane", "helicopter"] {
            let mut g = range();
            g.spawn("short", 0, [70., 70.]);
            aircraft(&mut g, kind, [80., 70.]);
            combat(&mut g, 80);
            assert!(!g.events.iter().any(|e| e.kind == "shot"));
            g.spawn("engineer", 0, [70., 70.]);
            combat(&mut g, 30);
            assert_eq!(
                g.events
                    .iter()
                    .filter(|e| e.weapon == "manpads" && e.kind == "shot")
                    .count(),
                1
            );
            assert_eq!(g.entities[1].aircraft.flares, 2);
            assert!(g.events.iter().any(|e| e.kind == "flares"));
        }
    }
    #[test]
    fn support_honors_ground_orders_then_air_orders_and_lock_breaks_out_of_range() {
        let mut g = range();
        let unit = g.spawn("engineer", 0, [70., 70.]);
        let air = aircraft(&mut g, "plane", [90., 70.]);
        let ground = g.spawn("short", 1, [75., 70.]);
        g.entities[0].orders.push_back(Order {
            kind: "attack".into(),
            target: Some(ground),
            destination: None,
        });
        combat(&mut g, 30);
        assert!(g.guided_missiles.is_empty());
        g.entities[0].orders.clear();
        g.entities[0].orders.push_back(Order {
            kind: "attack".into(),
            target: Some(air),
            destination: None,
        });
        combat(&mut g, 12);
        assert_eq!(g.entities[0].view.id, unit);
        assert!(g.entities[0].aa_target.is_some());
        g.entities[1].view.x = 300.;
        combat(&mut g, 30);
        assert!(g.guided_missiles.is_empty());
        assert_eq!(g.entities[1].aircraft.flares, 3);
    }
    #[test]
    fn flare_supply_and_cooldown_are_real_and_old_saves_receive_three_salvos() {
        let mut g = range();
        aircraft(&mut g, "plane", [90., 70.]);
        for expected in [2, 1, 0] {
            g.deploy_flares(0);
            assert_eq!(g.entities[0].aircraft.flares, expected);
            g.deploy_flares(0);
            assert_eq!(g.entities[0].aircraft.flares, expected);
            g.tick += 240;
        }
        let count = g.events.len();
        g.deploy_flares(0);
        assert_eq!(g.events.len(), count);
        let mut saved = serde_json::to_value(&g.entities[0].aircraft).unwrap();
        saved.as_object_mut().unwrap().remove("flares");
        let old: AircraftState = serde_json::from_value(saved).unwrap();
        assert_eq!(old.flares, 3);
    }
    #[test]
    fn tracking_missile_save_reload_preserves_interception_roll_and_cooldowns() {
        let mut g = range();
        g.spawn("engineer", 0, [70., 70.]);
        aircraft(&mut g, "plane", [150., 70.]);
        combat(&mut g, 27);
        assert_eq!(g.guided_missiles.len(), 1);
        let mut loaded: NativeGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        g.entities[1].view.z += 25.;
        loaded.entities[1].view.z += 25.;
        combat(&mut g, 60);
        combat(&mut loaded, 60);
        assert_eq!(g.entities[1].view.hp, loaded.entities[1].view.hp);
        assert_eq!(g.rng, loaded.rng);
        assert!(g.guided_missiles.is_empty());
        assert!(g.entities[0].aa_reload_until > g.tick);
    }
    #[test]
    fn an_interception_tracks_its_aircraft_and_never_splashes_ground_units() {
        for kind in ["plane", "helicopter"] {
            let mut g = range();
            g.spawn("engineer", 0, [70., 70.]);
            let id = aircraft(&mut g, kind, [100., 70.]);
            g.spawn("short", 1, [100., 70.]);
            let target = &mut g.entities[1];
            target.aircraft.flares = 0;
            g.guided_missiles.push(GuidedMissile {
                hit: None,
                source: g.entities[0].view.id.clone(),
                target: id.clone(),
                owner: 0,
                from: [70., 70.],
                born: 9,
                impact: 11,
            });
            g.rng = 1; // xorshift's next roll is below 0.6.
            g.entities[1].view.x = 120.;
            combat(&mut g, 1);
            assert!(g.entities[1].view.hp <= 0.);
            assert_eq!(g.entities[2].view.hp, g.entities[2].view.max_hp);
            g.record_wrecks();
            let wreck = g.props.iter().find_map(|p| p.wreck.as_ref()).unwrap();
            assert!(wreck.crash.is_some());
            assert!(
                g.events
                    .iter()
                    .any(|e| e.kind == "aircraft-hit" && e.release_height.unwrap_or(0.) > 30.)
            );
        }
    }
    #[test]
    fn confirmed_missile_hits_destroy_full_health_aircraft_without_a_second_roll() {
        for kind in ["plane", "helicopter"] {
            let mut g = range();
            let id = aircraft(&mut g, kind, [90., 70.]);
            g.entities[0].view.hp = 1000.;
            g.entities[0].view.max_hp = 1000.;
            g.entities[0].aircraft.decoy_until = 1000;
            g.guided_missiles.push(GuidedMissile {
                hit: Some(true),
                source: "launcher".into(),
                target: id,
                owner: 0,
                from: [70., 70.],
                born: 9,
                impact: 11,
            });
            let rng = g.rng;
            combat(&mut g, 1);
            assert_eq!(g.entities[0].view.hp, 0.);
            assert_eq!(g.rng, rng);
            g.record_wrecks();
            assert!(g.props[0].wreck.as_ref().unwrap().crash.is_some());
        }
    }
    #[test]
    fn landing_before_an_incoming_missile_arrives_does_not_cancel_a_confirmed_hit() {
        let mut g = range();
        let id = aircraft(&mut g, "plane", [90., 70.]);
        g.guided_missiles.push(GuidedMissile {
            hit: Some(true),
            source: "launcher".into(),
            target: id,
            owner: 0,
            from: [70., 70.],
            born: 9,
            impact: 11,
        });
        g.entities[0].view.airborne = false;
        g.entities[0].view.altitude = Some(0.);
        combat(&mut g, 1);
        assert_eq!(g.entities[0].view.hp, 0.);
    }
    #[test]
    fn evaded_missiles_airburst_clear_of_the_airframe_and_cannot_damage_it() {
        let mut g = range();
        let id = aircraft(&mut g, "plane", [90., 70.]);
        let hp = g.entities[0].view.hp;
        let center = g.entities[0].view.position();
        g.guided_missiles.push(GuidedMissile {
            hit: Some(false),
            source: "launcher".into(),
            target: id,
            owner: 0,
            from: [70., 70.],
            born: 9,
            impact: 11,
        });
        let rng = g.rng;
        combat(&mut g, 1);
        assert_eq!(g.rng, rng);
        assert_eq!(g.entities[0].view.hp, hp);
        let burst = g.events.iter().find(|e| e.kind == "missile-miss").unwrap();
        let at = bevy::prelude::Vec3::new(burst.at[0], burst.release_height.unwrap(), burst.at[1]);
        assert!(at.distance(center) > 13.);
        assert_eq!(
            at,
            crate::runtime::missile_destination(&g.entities[0].view, true)
        );
        assert_eq!(
            crate::runtime::missile_destination(&g.entities[0].view, false),
            center
        );
    }
}
