// Included in simulation.rs. Saved attack/breakaway cycles use faction observation.
#[derive(Clone, Default, Serialize, Deserialize)]
struct HelicopterTactics {
    target: Option<String>,
    next_decision: u64,
    evade_until: u64,
    evade_goal: Option<Point>,
    burst_left: u8,
    weapon: String,
    next_round: u64,
    next_attack: u64,
}
impl NativeGame {
    pub fn smoke_helicopter(&mut self, gun: bool) -> String {
        *self = Self::operation(crate::training::ID, 384, 384, default_config());
        self.entities
            .retain(|e| e.view.kind == "airfield" && e.view.owner == 0);
        self.props.clear();
        self.config["fogEnabled"] = json!(false);
        self.command(json!({"type":"training","action":"buy","kind":"helicopter"}));
        let id = self
            .training
            .as_ref()
            .unwrap()
            .last_purchase
            .clone()
            .unwrap();
        let target = self.spawn(if gun { "short" } else { "medium" }, 1, [190., 210.]);
        let e = self.entities.last_mut().unwrap();
        e.view.hp = 100.;
        e.view.max_hp = 100.;
        e.view.training_target = true;
        e.stance = "hold-fire".into();
        self.command(
            json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":target}),
        );
        for _ in 0..1800 {
            self.step();
            if self.training.as_ref().unwrap().shots > 0 {
                break;
            }
        }
        assert!(self.training.as_ref().unwrap().shots > 0);
        self.paused = true;
        id
    }
    pub fn smoke_rocket_trails(&mut self) -> String {
        *self = Self::operation(crate::training::ID, 384, 384, default_config());
        self.entities.clear();
        self.props.clear();
        self.bases.clear();
        self.config["fogEnabled"] = json!(false);
        let id = self.spawn("xtralong", 0, [100., 180.]);
        let target = self.spawn("medium", 1, [230., 180.]);
        self.entities[1].stance = "hold-fire".into();
        self.entities[1].view.hp = 100.;
        self.entities[1].view.training_target = true;
        self.command(
            json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":target}),
        );
        for _ in 0..200 {
            self.step();
            if self.training.as_ref().unwrap().shots > 0 {
                break;
            }
        }
        assert!(self.training.as_ref().unwrap().shots > 0);
        self.paused = true;
        id
    }
    fn infantry_air_response(&mut self) -> Vec<usize> {
        let mut responding = vec![];
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if e.view.category != "troop"
                || e.view.hp <= 0.
                || e.view.downed
                || e.view.retreating
                || e.view.training_target
                || e.transport.is_some()
                || e.view.garrison.is_some()
                || e.stance != "attack"
                || e.player_ordered
                || e.halted
                || e.medical_until > self.tick
                || e.orders.front().is_some_and(|o| {
                    !["attack", "attack-move", "patrol"].contains(&o.kind.as_str())
                })
            {
                continue;
            }
            let at = position(e);
            let threat = self
                .entities
                .iter()
                .filter(|h| {
                    h.view.kind == "helicopter"
                        && h.view.airborne
                        && h.view.hp > 0.
                        && h.view.owner >= 0
                        && h.view.owner != e.view.owner
                        && distance(at, position(h)) <= 105.
                        && self.visible(e.view.owner, h)
                })
                .min_by(|a, b| distance(at, position(a)).total_cmp(&distance(at, position(b))))
                .map(|h| (h.view.id.clone(), position(h)));
            let Some((id, threat)) = threat else {
                continue;
            };
            responding.push(i);
            let explicit = e
                .orders
                .front()
                .filter(|o| o.kind == "attack")
                .and_then(|o| o.target.as_ref());
            let aa = flag(spec(&e.view.kind), "antiAirLauncher")
                && e.view.suppression < 0.65
                && (e.aa_reload_until <= self.tick || e.aa_target.is_some())
                && explicit.is_none_or(|t| *t == id)
                && self.building_line_clear(at, threat, None, Some(&id));
            if aa {
                let e = &mut self.entities[i];
                e.path.clear();
                e.cover_goal = None;
                e.tactical_until = 0;
                e.warfare = InfantryWarfare {
                    role: InfantryRole::AirDefence,
                    target: Some(id),
                    until: self.tick + 20,
                    next_decision: self.tick + 20,
                    ..Default::default()
                };
                continue;
            }
            if e.warfare.active(self.tick)
                && matches!(e.warfare.role, InfantryRole::Recover | InfantryRole::Pinned)
            {
                continue;
            }
            let building =
                self.entities
                    .iter()
                    .filter(|b| {
                        b.view.building
                            && b.view.hp > 0.
                            && (b.view.owner == e.view.owner || b.view.owner < 0)
                    })
                    .filter(|b| {
                        if b.view.lots.is_empty() {
                            distance(at, position(b)) < 14.
                        } else {
                            b.view.lots.iter().any(|l| {
                                distance(at, [b.view.x + l.at[0], b.view.z + l.at[1]]) < 14.
                            })
                        }
                    })
                    .min_by(|a, b| distance(at, position(a)).total_cmp(&distance(at, position(b))))
                    .map(|b| b.view.id.clone());
            let owner = e.view.owner;
            let soldier = e.view.id.clone();
            if let Some(building) = building
                && self.enter_building(owner, &[soldier], &building, Some(threat)) > 0
            {
                continue;
            }
            self.infantry_recover(i, threat);
            if self.entities[i].warfare.role == InfantryRole::Pinned
                && self.cover_level(at, Some(threat)) == 0
            {
                // In open ground, break contact rather than standing to duel an aircraft with a rifle.
                let d = distance(at, threat).max(0.1);
                let away = [(at[0] - threat[0]) / d, (at[1] - threat[1]) / d];
                let goal = [
                    (at[0] + away[0] * 14. - away[1] * 4.).clamp(2., self.terrain.cols as f32 - 2.),
                    (at[1] + away[1] * 14. + away[0] * 4.).clamp(2., self.terrain.rows as f32 - 2.),
                ];
                let path = self.route(i, at, goal);
                if !path.is_empty() {
                    let e = &mut self.entities[i];
                    e.path = path;
                    e.warfare = InfantryWarfare {
                        role: InfantryRole::Recover,
                        until: self.tick + 180,
                        next_decision: self.tick + 20,
                        goal: Some(goal),
                        ..Default::default()
                    };
                }
            }
        }
        responding
    }
    fn helicopter_evade(&mut self, i: usize, threat: Point) {
        if self.entities[i].helicopter.evade_until > self.tick {
            return;
        }
        let at = position(&self.entities[i]);
        let d = distance(at, threat).max(0.1);
        let away = [(at[0] - threat[0]) / d, (at[1] - threat[1]) / d];
        let side = if i.is_multiple_of(2) { 1. } else { -1. };
        let goal = [
            (at[0] + away[0] * 28. - away[1] * side * 35.).clamp(3., self.terrain.cols as f32 - 3.),
            (at[1] + away[1] * 28. + away[0] * side * 35.).clamp(3., self.terrain.rows as f32 - 3.),
        ];
        let e = &mut self.entities[i];
        e.helicopter.evade_goal = Some(goal);
        e.helicopter.evade_until = self.tick + 70;
        e.helicopter.burst_left = 0;
        e.helicopter.next_attack = self.tick + 80;
    }
    fn helicopter_engagement(&mut self, i: usize) -> bool {
        let e = &self.entities[i];
        if e.view.training_target {
            return false;
        }
        if e.orders.front().is_some_and(|o| o.kind == "move") || e.stance != "attack" {
            let e = &mut self.entities[i];
            e.helicopter.target = None;
            e.helicopter.burst_left = 0;
            e.target = None;
            return false;
        }
        if e.view.hp / e.view.max_hp.max(0.01) < 0.3 {
            self.return_aircraft(i, true);
            return true;
        }
        let missile = self
            .guided_missiles
            .iter()
            .find(|m| m.target == e.view.id && m.impact > self.tick)
            .map(|m| m.from);
        if let Some(threat) = missile {
            self.helicopter_evade(i, threat);
        }
        if self.entities[i].helicopter.evade_until > self.tick {
            if let Some(goal) = self.entities[i].helicopter.evade_goal {
                self.fly_aircraft(i, goal, 14., false);
            }
            let e = &mut self.entities[i];
            let ground = self.terrain.height(e.view.x, e.view.z);
            e.aircraft.world_y += (ground + 22. - e.aircraft.world_y).clamp(-3. * DT, 3. * DT);
            e.view.flight_bank += (0.18 - e.view.flight_bank).clamp(-0.8 * DT, 0.8 * DT);
            return true;
        }
        if self.entities[i].helicopter.next_decision <= self.tick {
            let e = &self.entities[i];
            let at = position(e);
            let explicit = e
                .orders
                .front()
                .filter(|o| o.kind == "attack")
                .and_then(|o| o.target.clone());
            let candidate = self
                .entities
                .iter()
                .filter(|t| {
                    (t.view.owner >= 0
                        || explicit.as_deref() == Some(t.view.id.as_str())
                            && !t.view.lots.is_empty())
                        && t.view.owner != e.view.owner
                        && t.view.hp > 0.
                        && !t.view.downed
                        && !t.view.airborne
                        && t.transport.is_none()
                        && t.view.kind != "objective"
                        && (!t.view.building
                            || t.view.lots.is_empty()
                            || explicit.as_deref() == Some(t.view.id.as_str()))
                        && ordered_lot(t, e.orders.front()).is_none_or(|lot| lot.integrity > 0.)
                        && explicit.as_deref().is_none_or(|id| id == t.view.id)
                        && (explicit.is_some() || distance(at, position(t)) <= 155.)
                        && self.visible(e.view.owner, t)
                })
                .min_by(|a, b| {
                    let score = |t: &Entity| {
                        distance(at, position(t))
                            - if flag(spec(&t.view.kind), "antiAirLauncher")
                                || flag(spec(&t.view.kind), "antiAir")
                            {
                                90.
                            } else if t.view.category == "vehicle" {
                                25.
                            } else {
                                0.
                            }
                            - if e.helicopter.target.as_ref() == Some(&t.view.id) {
                                15.
                            } else {
                                0.
                            }
                    };
                    score(a).total_cmp(&score(b))
                })
                .map(|t| t.view.id.clone());
            let e = &mut self.entities[i];
            if e.helicopter.target != candidate {
                e.helicopter.burst_left = 0;
            }
            e.helicopter.target = candidate.clone();
            e.target = candidate;
            e.helicopter.next_decision = self.tick + 10;
            if explicit.is_some() && e.helicopter.target.is_none() {
                // A hidden target is never followed through fog. A destroyed target completes its order.
                let id = e.orders.front().and_then(|o| o.target.clone());
                if !self
                    .entities
                    .iter()
                    .any(|t| Some(&t.view.id) == id.as_ref() && t.view.hp > 0.)
                {
                    self.entities[i].orders.pop_front();
                }
            }
        }
        let Some(j) = self.entities[i]
            .helicopter
            .target
            .as_ref()
            .and_then(|id| self.entities.iter().position(|t| &t.view.id == id))
        else {
            return false;
        };
        let at = position(&self.entities[i]);
        let target = aim_point(&self.entities[i], &self.entities[j], at);
        let d = distance(at, target).max(0.01);
        let range = if self.entities[j].view.category == "troop" {
            85.
        } else {
            120.
        };
        let stand = range * 0.78;
        if !self.firing_line(&self.entities[i], &self.entities[j]) {
            let side = if i.is_multiple_of(2) { 1. } else { -1. };
            let away = [(at[0] - target[0]) / d, (at[1] - target[1]) / d];
            let goal = [
                (target[0] + away[0] * stand - away[1] * stand * 0.7 * side)
                    .clamp(3., self.terrain.cols as f32 - 3.),
                (target[1] + away[1] * stand + away[0] * stand * 0.7 * side)
                    .clamp(3., self.terrain.rows as f32 - 3.),
            ];
            self.fly_aircraft(i, goal, 12., false);
        } else if d > range * 0.9 || d < stand * 0.65 {
            let goal = [
                target[0] + (at[0] - target[0]) / d * stand,
                target[1] + (at[1] - target[1]) / d * stand,
            ];
            self.fly_aircraft(i, goal, 12., false);
        } else {
            let e = &mut self.entities[i];
            e.speed = (e.speed - 5. * DT).max(0.);
            e.view.velocity = [0., 0.];
            e.view.heading +=
                angle_delta(e.view.heading, (target[1] - at[1]).atan2(target[0] - at[0]))
                    .clamp(-1.1 * DT, 1.1 * DT);
        }
        let e = &mut self.entities[i];
        let ground = self.terrain.height(e.view.x, e.view.z);
        e.aircraft.world_y += (ground + 16. - e.aircraft.world_y).clamp(-3. * DT, 3. * DT);
        e.view.flight_bank += (0. - e.view.flight_bank).clamp(-0.8 * DT, 0.8 * DT);
        true
    }
    fn helicopter_combat(&mut self) {
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if e.view.kind != "helicopter"
                || e.view.training_target
                || e.view.hp <= 0.
                || !e.view.airborne
                || e.aircraft.phase != FlightPhase::Hover
                || e.stance != "attack"
                || e.helicopter.evade_until > self.tick
                || e.orders.front().is_some_and(|o| o.kind == "move")
            {
                continue;
            }
            let Some(j) = e.helicopter.target.as_ref().and_then(|id| {
                self.entities.iter().position(|t| {
                    &t.view.id == id
                        && t.view.hp > 0.
                        && !t.view.downed
                        && !t.view.airborne
                        && self.visible(e.view.owner, t)
                })
            }) else {
                continue;
            };
            let at = position(e);
            let to = aim_point(e, &self.entities[j], at);
            let gun = self.entities[j].view.category == "troop";
            let range = number(
                spec("helicopter"),
                if gun { "gunRange" } else { "rocketRange" },
                85.,
            );
            if distance(at, to) > range
                || !self.firing_line(e, &self.entities[j])
                || angle_delta(e.view.heading, (to[1] - at[1]).atan2(to[0] - at[0])).abs()
                    > if gun { 1.2 } else { 0.4 }
            {
                continue;
            }
            if e.helicopter.burst_left == 0 {
                if e.helicopter.next_attack > self.tick {
                    continue;
                }
                let e = &mut self.entities[i];
                e.helicopter.burst_left = if gun { 5 } else { 2 };
                e.helicopter.weapon = if gun { "heli-gun" } else { "heli-rocket" }.into();
            }
            if self.entities[i].helicopter.next_round > self.tick {
                continue;
            }
            let hit = self.random() < if gun { 0.8 } else { 0.9 };
            let spread = if hit { 0.35 } else { 3.5 };
            let angle = self.random() * std::f32::consts::TAU;
            let end = [to[0] + angle.cos() * spread, to[1] + angle.sin() * spread];
            let speed = if gun { 300. } else { 95. };
            let height = self.entities[i].aircraft.world_y;
            let travel = (distance(at, end).hypot(height - self.terrain.height(end[0], end[1]))
                / speed
                / DT)
                .ceil()
                .max(1.) as u64;
            let owner = self.entities[i].view.owner;
            let weapon = if gun { "heli-gun" } else { "heli-rocket" }.to_owned();
            let p = Projectile {
                owner,
                from: at,
                to: end,
                born: self.tick,
                impact: self.tick + travel,
                damage: number(
                    spec("helicopter"),
                    if gun { "gunDamage" } else { "rocketDamage" },
                    0.28,
                ),
                radius: if gun { 0.8 } else { 2.5 },
                hit,
                weapon: weapon.clone(),
                ground_hit: !gun,
            };
            self.events.push(BattleEvent {
                hit: None,
                target: None,
                tick: self.tick,
                kind: "shot".into(),
                at,
                from: Some(end),
                owner,
                radius: 0.3,
                weapon,
                source: self.entities[i].view.id.clone(),
                duration: travel as f32 * DT,
                release_height: None,
            });
            self.warn_incoming(&p);
            self.projectiles.push(p);
            if let Some(t) = &mut self.training {
                t.shots += 1;
            }
            let e = &mut self.entities[i];
            e.last_shot = self.tick;
            e.helicopter.burst_left -= 1;
            e.helicopter.next_round = self.tick + if gun { 3 } else { 6 };
            if e.helicopter.burst_left == 0 {
                self.helicopter_evade(i, to);
            }
        }
    }
}

#[cfg(test)]
mod helicopter_tactics_tests {
    use super::*;
    fn engagement(kind: &str) -> (NativeGame, String, String) {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("heli-tactics", 256, 256);
        g.terrain.authored = vec![0.52; 256 * 256];
        g.terrain.hydrate();
        g.config["fogEnabled"] = json!(false);
        let h = g.spawn("helicopter", 0, [80., 80.]);
        let t = g.spawn(kind, 1, [140., 80.]);
        g.entities[1].stance = "hold-fire".into();
        g.entities[1].view.hp = 100.;
        let e = &mut g.entities[0];
        e.aircraft.phase = FlightPhase::Hover;
        e.view.airborne = true;
        e.aircraft.world_y = g.terrain.height(80., 80.) + 16.;
        e.view.heading = 0.;
        (g, h, t)
    }
    #[test]
    fn helicopter_selects_the_right_weapon_deals_damage_and_breaks_away() {
        for kind in ["short", "medium"] {
            let (mut g, _, _) = engagement(kind);
            let mut weapons = vec![];
            let start = position(&g.entities[0]);
            for _ in 0..150 {
                g.tick += 1;
                g.helicopter_engagement(0);
                g.combat();
                for e in g
                    .events
                    .iter()
                    .filter(|e| e.tick == g.tick && e.kind == "shot")
                {
                    weapons.push(e.weapon.clone());
                }
            }
            let expected = if kind == "short" {
                "heli-gun"
            } else {
                "heli-rocket"
            };
            assert!(weapons.iter().any(|w| w == expected), "{kind}: {weapons:?}");
            assert!(weapons.iter().all(|w| w == expected));
            assert!(g.entities[1].view.hp < 100.);
            assert!(distance(start, position(&g.entities[0])) > 5.);
            assert!(g.entities[0].helicopter.next_attack > 0);
        }
    }
    #[test]
    fn explicit_target_wins_over_aa_priority_and_hold_fire_and_fog_are_respected() {
        let (mut g, _, target) = engagement("medium");
        g.spawn("engineer", 1, [120., 80.]);
        g.entities[0].orders.push_back(Order {
            kind: "attack".into(),
            target: Some(target.clone()),
            destination: None,
        });
        g.helicopter_engagement(0);
        assert_eq!(g.entities[0].helicopter.target.as_ref(), Some(&target));
        g.entities[0].stance = "hold-fire".into();
        g.helicopter_engagement(0);
        g.combat();
        assert!(g.projectiles.is_empty());
        g.entities[0].stance = "attack".into();
        g.entities[0].orders.clear();
        g.entities[0].helicopter.next_decision = 0;
        g.helicopter_engagement(0);
        assert_eq!(
            g.entities[0].helicopter.target.as_ref(),
            Some(&g.entities[2].view.id)
        );
        g.config["fogEnabled"] = json!(true);
        g.entities[1].view.x = 900.;
        g.entities[2].view.x = 900.;
        g.entities[0].helicopter.next_decision = 0;
        g.helicopter_engagement(0);
        assert!(g.entities[0].helicopter.target.is_none());
    }
    #[test]
    fn rifles_keep_advancing_past_aircraft_they_cannot_shoot() {
        let (mut g, _, _) = engagement("scout");
        g.entities[0].view.owner = 1;
        g.entities[0].view.x = 35.;
        g.entities[1].view.owner = 0;
        g.entities[1].stance = "attack".into();
        let id = g.entities[1].view.id.clone();
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([190., 80.]),
                target: None,
            },
            false,
        );
        let before = position(&g.entities[1]);
        g.tick = 1;
        g.movement();
        assert!(distance(before, position(&g.entities[1])) > 0.);
    }
    #[test]
    fn helicopter_cycle_survives_save_load() {
        let (mut a, _, _) = engagement("medium");
        for _ in 0..30 {
            a.tick += 1;
            a.helicopter_engagement(0);
            a.combat();
        }
        let mut b: NativeGame = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        for _ in 0..90 {
            for g in [&mut a, &mut b] {
                g.tick += 1;
                g.helicopter_engagement(0);
                g.combat();
            }
        }
        assert_eq!(
            serde_json::to_value(&a.entities).unwrap(),
            serde_json::to_value(&b.entities).unwrap()
        );
        assert_eq!(a.rng, b.rng);
    }
    #[test]
    fn training_purchase_accepts_attack_takes_off_fires_and_evades() {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.config["fogEnabled"] = json!(false);
        g.command(json!({"type":"training","action":"buy","kind":"helicopter"}));
        let id = g.training.as_ref().unwrap().last_purchase.clone().unwrap();
        let target = g.spawn("medium", 1, [190., 210.]);
        g.entities.last_mut().unwrap().view.hp = 100.;
        g.entities.last_mut().unwrap().stance = "hold-fire".into();
        assert_eq!(
            g.issue(
                0,
                std::slice::from_ref(&id),
                Order {
                    kind: "attack".into(),
                    target: Some(target.clone()),
                    destination: None
                },
                false
            ),
            1
        );
        let mut fired = false;
        let mut evaded = false;
        for _ in 0..900 {
            g.step();
            fired |= g
                .events
                .iter()
                .any(|e| e.weapon == "heli-rocket" && e.kind == "shot");
            evaded |= g
                .entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .helicopter
                .evade_until
                > g.tick;
            if fired
                && evaded
                && g.entities
                    .iter()
                    .find(|e| e.view.id == target)
                    .unwrap()
                    .view
                    .hp
                    < 100.
            {
                break;
            }
        }
        assert!(fired && evaded);
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == target)
                .unwrap()
                .view
                .hp
                < 100.
        );
    }
    #[test]
    fn infantry_withdraws_from_helicopters_and_support_prepares_air_defence() {
        let (mut g, _, _) = engagement("short");
        // Helicopter is hostile to these soldiers; the rifleman cannot fight it.
        g.entities[0].view.owner = 1;
        g.entities[1].view.owner = 0;
        g.entities[1].stance = "attack".into();
        g.spawn("engineer", 0, [145., 85.]);
        g.tick = 100;
        g.infantry_warfare();
        assert_eq!(g.entities[1].warfare.role, InfantryRole::Recover);
        assert_eq!(g.entities[2].warfare.role, InfantryRole::AirDefence);
        assert!(!g.entities[1].path.is_empty());
        assert!(g.entities[2].path.is_empty());
        let goal = g.entities[1].warfare.goal.unwrap();
        assert!(distance(goal, [80., 80.]) > distance([140., 80.], [80., 80.]));
        for _ in 0..28 {
            g.tick += 1;
            g.infantry_warfare();
            g.support_anti_air();
        }
        assert!(
            g.guided_missiles
                .iter()
                .any(|m| m.target == g.entities[0].view.id)
        );
        g.entities[0].view.owner = 0;
        g.entities[1].view.owner = 1;
        g.entities[1].orders = VecDeque::from([Order {
            kind: "move".into(),
            destination: Some([160., 80.]),
            target: None,
        }]);
        g.entities[1].warfare = InfantryWarfare::default();
        g.tick = 140;
        g.infantry_warfare();
        assert_eq!(g.entities[1].warfare.role, InfantryRole::None);
    }
}
