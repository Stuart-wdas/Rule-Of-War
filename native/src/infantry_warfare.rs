// Included in simulation.rs. Local fire-and-movement decisions are authoritative,
// deterministic and saved. Orders remain intact while soldiers execute a bound.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum InfantryRole {
    #[default]
    None,
    Cover,
    Bound,
    Flank,
    Assault,
    Recover,
    Pinned,
    AirDefence,
}
impl InfantryRole {
    fn mobile(self) -> bool {
        matches!(
            self,
            Self::Bound | Self::Flank | Self::Assault | Self::Recover
        )
    }
    fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Cover => "covering-fire",
            Self::Bound => "bounding",
            Self::Flank => "flanking",
            Self::Assault => "assaulting",
            Self::Recover => "taking-cover",
            Self::Pinned => "pinned-down",
            Self::AirDefence => "air-defence",
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct InfantryWarfare {
    role: InfantryRole,
    until: u64,
    next_decision: u64,
    target: Option<String>,
    partner: Option<String>,
    goal: Option<Point>,
}
impl InfantryWarfare {
    fn active(&self, tick: u64) -> bool {
        self.role != InfantryRole::None && self.until > tick
    }
}
// Reuse faction observation and weapon facts for this decision pass. Positions and
// visibility cannot change while roles are assigned; nothing survives the pass.
struct InfantryContact {
    range: f32,
    armed: bool,
    visible: [bool; 2],
}
fn infantry_available(e: &Entity, tick: u64) -> bool {
    (0..=1).contains(&e.view.owner)
        && e.view.category == "troop"
        && e.view.hp > 0.
        && !e.view.downed
        && !e.view.retreating
        && !e.view.training_target
        && e.transport.is_none()
        && e.stance == "attack"
        && !e.player_ordered
        && !e.halted
        && e.medical_until <= tick
        && e.aa_target.is_none()
        && e.aa_pose_until <= tick
        && number(spec(&e.view.kind), "damage", 0.) > 0.
        && e.orders
            .front()
            .is_none_or(|o| ["attack", "attack-move", "patrol"].contains(&o.kind.as_str()))
}
impl NativeGame {
    pub fn warfare_benchmark(mut self) -> Value {
        self.entities.clear();
        self.bases.clear();
        self.props.clear();
        self.terrain = crate::terrain::Terrain::new("warfare-benchmark", 1024, 1024);
        self.terrain.authored = vec![0.52; 1024 * 1024];
        self.terrain.hydrate();
        self.training = Some(TrainingGround::default());
        self.human_opponent = true;
        self.tick = 0;
        self.config["fogEnabled"] = json!(true);
        for group in 0..4 {
            let x = 130. + group as f32 * 220.;
            for owner in [0, 1] {
                let mut ids = vec![];
                for soldier in 0..6 {
                    let at = [
                        x + if owner == 0 { -15. } else { 15. },
                        150. + soldier as f32 * 4.,
                    ];
                    ids.push(self.spawn("short", owner, at));
                    let e = self.entities.last_mut().unwrap();
                    e.view.hp = 100.;
                    e.view.max_hp = 100.;
                }
                self.issue(
                    owner,
                    &ids,
                    Order {
                        kind: "attack-move".into(),
                        destination: Some([x + if owner == 0 { 35. } else { -35. }, 160.]),
                        target: None,
                    },
                    false,
                );
                self.props.push(BattlefieldProp {
                    scar: None,
                    id: format!("cover-{group}-{owner}"),
                    kind: "sandbags".into(),
                    at: [x + if owner == 0 { -13. } else { 13. }, 160.],
                    heading: std::f32::consts::FRAC_PI_2,
                    hp: 100.,
                    wreck: None,
                });
            }
        }
        let mut samples = Vec::new();
        let mut roles = std::collections::BTreeSet::new();
        crate::performance::reset();
        for _ in 0..300 {
            let start = std::time::Instant::now();
            self.step();
            samples.push(start.elapsed().as_secs_f64() * 1000.);
            for e in &self.entities {
                if e.warfare.active(self.tick) {
                    roles.insert(e.warfare.role.label());
                }
            }
        }
        samples.sort_by(f64::total_cmp);
        json!({"units":48,"ticks":300,"meanTickMs":samples.iter().sum::<f64>()/samples.len() as f64,
            "p95TickMs":samples[(samples.len() as f32*0.95) as usize],"maxTickMs":samples.last(),"rolesObserved":roles,"timings":crate::performance::report()})
    }
    fn warfare_contact(&self, index: usize, awareness: &[InfantryContact]) -> Option<usize> {
        let e = &self.entities[index];
        let at = position(e);
        let explicit = e
            .orders
            .front()
            .filter(|o| o.kind == "attack")
            .and_then(|o| o.target.as_deref());
        self.entities
            .iter()
            .enumerate()
            .filter(|(j, t)| {
                t.view.owner >= 0
                    && t.view.owner != e.view.owner
                    && t.view.hp > 0.
                    && !t.view.downed
                    && !t.view.building
                    && !t.view.airborne
                    && weapon_can_damage(&e.view.kind, &t.view)
                    && t.transport.is_none()
                    && explicit.is_none_or(|id| id == t.view.id)
                    && distance(at, position(t))
                        <= awareness[index].range.max(awareness[*j].range).min(110.) + 18.
                    && awareness[*j].visible[e.view.owner as usize]
            })
            .min_by(|(_, a), (_, b)| {
                distance(at, position(a)).total_cmp(&distance(at, position(b)))
            })
            .map(|(i, _)| i)
    }
    fn end_infantry_plan(&mut self, index: usize) {
        let e = &mut self.entities[index];
        e.warfare = InfantryWarfare {
            next_decision: self.tick + 10,
            ..Default::default()
        };
        e.tactical_until = 0;
        if !e.cover_ordered {
            e.cover_goal = None;
            e.path.clear();
        }
        let at = position(e);
        let goal = e.orders.front().and_then(|o| o.destination);
        if let Some(goal) = goal
            && distance(at, goal) > 0.8
        {
            self.entities[index].path = self.route(index, at, goal);
        }
    }
    fn infantry_recover(&mut self, index: usize, threat: Point) {
        let e = &self.entities[index];
        let at = position(e);
        // A soldier already behind effective cover stays there instead of zigzagging.
        if self.cover_level(at, Some(threat)) > 0 && e.speed < 0.1 {
            let e = &mut self.entities[index];
            e.path.clear();
            e.cover_goal = Some(at);
            e.warfare = InfantryWarfare {
                role: InfantryRole::Pinned,
                until: self.tick + 60,
                next_decision: self.tick + 20,
                ..Default::default()
            };
            return;
        }
        let occupied: Vec<_> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(j, e)| *j != index && e.view.hp > 0.)
            .map(|(_, e)| e.warfare.goal.or(e.cover_goal).unwrap_or(position(e)))
            .collect();
        let goal = self.nearby_cover_slot(index, at, 8., &occupied);
        if let Some(goal) = goal {
            let path = self.route(index, at, goal);
            let until = self.tick
                + ((distance(at, goal) / number(spec(&e.view.kind), "speed", 0.8).max(0.1) / DT)
                    as u64
                    + 60)
                    .min(600);
            let e = &mut self.entities[index];
            e.path = path;
            e.cover_goal = Some(goal);
            e.cover_ordered = false;
            e.tactical_until = until;
            e.warfare = InfantryWarfare {
                role: InfantryRole::Recover,
                until,
                next_decision: self.tick + 30,
                goal: Some(goal),
                ..Default::default()
            };
        } else {
            // With no safe cover, pinned men crouch and stop chasing a kill.
            let e = &mut self.entities[index];
            e.path.clear();
            e.speed = 0.;
            e.warfare = InfantryWarfare {
                role: InfantryRole::Pinned,
                until: self.tick + 40,
                next_decision: self.tick + 20,
                ..Default::default()
            };
        }
    }
    fn infantry_warfare(&mut self) {
        if !self.tick.is_multiple_of(10) {
            return;
        }
        let _timing = crate::performance::span("infantry.warfare");
        let air_response = self.infantry_air_response();
        let visibility = self.visibility_mask();
        let awareness: Vec<_> = self
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| InfantryContact {
                range: self.weapon_range(&e.view.kind),
                armed: number(spec(&e.view.kind), "damage", 0.) > 0.,
                visible: visibility[i],
            })
            .collect();
        for i in 0..self.entities.len() {
            if air_response.contains(&i) {
                continue;
            }
            if self.entities[i].warfare.role == InfantryRole::None {
                continue;
            }
            let e = &self.entities[i];
            if !infantry_available(e, self.tick) {
                self.end_infantry_plan(i);
                continue;
            }
            let plan = e.warfare.clone();
            let contact = self.warfare_contact(i, &awareness);
            let threatened = e.threat_until > self.tick || e.view.suppression > 0.25;
            if plan.role == InfantryRole::Pinned && threatened {
                self.entities[i].warfare.until = self.tick + 40;
                continue;
            }
            if !plan.active(self.tick) || (contact.is_none() && !threatened) {
                self.end_infantry_plan(i);
                continue;
            }
            if plan.role.mobile()
                && (plan.goal.is_some_and(|g| distance(position(e), g) < 0.8)
                    || matches!(
                        plan.role,
                        InfantryRole::Bound | InfantryRole::Flank | InfantryRole::Assault
                    ) && contact
                        .is_some_and(|j| self.infantry_firing_position(i, position(e), j)))
            {
                let partner = plan.partner.clone();
                let e = &mut self.entities[i];
                e.path.clear();
                e.speed = 0.;
                e.tactical_until = 0;
                e.warfare.role = InfantryRole::Cover;
                e.warfare.partner = None;
                e.warfare.goal = None;
                e.warfare.until = self.tick + 100;
                if let Some(j) =
                    partner.and_then(|id| self.entities.iter().position(|e| e.view.id == id))
                {
                    self.entities[j].warfare.until = self.tick;
                }
            }
        }
        for i in 0..self.entities.len() {
            if air_response.contains(&i) {
                continue;
            }
            let e = &self.entities[i];
            if !infantry_available(e, self.tick)
                || e.view.garrison.is_some()
                || e.warfare.next_decision > self.tick
            {
                continue;
            }
            let Some(target) = self.warfare_contact(i, &awareness) else {
                if e.view.suppression >= 0.65
                    && e.threat_until > self.tick
                    && let Some(threat) = e.threat
                {
                    self.infantry_recover(i, threat);
                }
                continue;
            };
            let at = position(e);
            let enemy = position(&self.entities[target]);
            let incoming = e.threat_until > self.tick;
            let friends = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, a)| {
                    a.view.owner == e.view.owner
                        && a.view.hp > 0.
                        && !a.view.downed
                        && a.transport.is_none()
                        && awareness[*j].armed
                        && distance(at, position(a)) < 28.
                })
                .count();
            let foes = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, t)| {
                    t.view.owner >= 0
                        && t.view.owner != e.view.owner
                        && t.view.hp > 0.
                        && !t.view.downed
                        && !t.view.building
                        && !t.view.airborne
                        && awareness[*j].armed
                        && distance(at, position(t)) <= awareness[*j].range + 8.
                        && awareness[*j].visible[e.view.owner as usize]
                })
                .count();
            let endangered = foes >= friends + 2
                || e.view.suppression >= 0.65
                || (incoming && e.view.hp / e.view.max_hp.max(0.01) < 0.35);
            let lost_cover = matches!(
                e.warfare.role,
                InfantryRole::Bound | InfantryRole::Flank | InfantryRole::Assault
            ) && e.warfare.partner.as_ref().is_some_and(|id| {
                !self.entities.iter().any(|a| {
                    &a.view.id == id
                        && a.view.hp > 0.
                        && !a.view.downed
                        && a.view.suppression < 0.65
                        && a.warfare.role == InfantryRole::Cover
                })
            });
            if e.warfare.role == InfantryRole::Recover
                && e.warfare.active(self.tick)
                && !e.path.is_empty()
                && e.warfare
                    .goal
                    .is_some_and(|p| self.cover_level(p, Some(enemy)) > 0)
            {
                continue;
            }
            if endangered || lost_cover {
                self.infantry_recover(i, enemy);
                continue;
            }
            if e.warfare.active(self.tick) {
                continue;
            }
            // Do not pull manually positioned troops out of a quiet defensive position.
            if e.orders.is_empty() && !incoming {
                continue;
            }
            if number(spec(&e.view.kind), "speed", 0.) <= 0.
                || e.view.suppression > 0.4
                || e.view.hp / e.view.max_hp.max(0.01) < 0.45
            {
                continue;
            }
            let direct = e.orders.front().is_some_and(|o| o.kind == "attack");
            let hostile = &self.entities[target];
            if self.infantry_firing_position(i, at, target) {
                continue;
            }
            let vulnerable = friends >= foes
                && (hostile.view.suppression >= 0.65
                    || hostile.view.hp / hostile.view.max_hp.max(0.01) < 0.35);
            let flank = self.cover_level(enemy, Some(at)) > 0 || !self.firing_line(e, hostile);
            if direct && !flank && !vulnerable {
                continue;
            }
            let buddy = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, a)| {
                    *j != i
                        && a.view.owner == e.view.owner
                        && infantry_available(a, self.tick)
                        && a.view.suppression < 0.4
                        && a.view.hp / a.view.max_hp.max(0.01) >= 0.45
                        && distance(at, position(a)) < 28.
                        && distance(position(a), enemy) <= awareness[*j].range
                        && self.firing_line(a, hostile)
                        && a.warfare.partner.is_none()
                        && !a.warfare.role.mobile()
                        && (a.warfare.role != InfantryRole::Pinned)
                        && a.orders
                            .front()
                            .filter(|o| o.kind == "attack")
                            .and_then(|o| o.target.as_ref())
                            .is_none_or(|id| id == &hostile.view.id)
                })
                .min_by(|(_, a), (_, b)| {
                    let score = |a: &Entity| {
                        a.speed * 10.
                            - self.cover_level(position(a), Some(enemy)) as f32 * 4.
                            - if a.view.kind == "scout" { 3. } else { 0. }
                    };
                    score(a).total_cmp(&score(b))
                })
                .map(|(j, _)| j);
            let Some(buddy) = buddy else {
                continue;
            };
            let d = distance(at, enemy).max(0.01);
            if d < 12. && !vulnerable {
                continue;
            }
            let forward = [(enemy[0] - at[0]) / d, (enemy[1] - at[1]) / d];
            let right = [-forward[1], forward[0]];
            let role = if vulnerable && d < 26. {
                InfantryRole::Assault
            } else if flank {
                InfantryRole::Flank
            } else {
                InfantryRole::Bound
            };
            let step = if role == InfantryRole::Assault {
                8.
            } else {
                (d - self.weapon_range(&e.view.kind) * 0.94).clamp(0., 8.)
            };
            let options = if role == InfantryRole::Flank {
                vec![
                    [at[0] + right[0] * 10., at[1] + right[1] * 10.],
                    [at[0] - right[0] * 10., at[1] - right[1] * 10.],
                ]
            } else {
                vec![[at[0] + forward[0] * step, at[1] + forward[1] * step]]
            };
            let occupied: Vec<_> = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, e)| *j != i && e.view.hp > 0.)
                .map(|(_, e)| e.warfare.goal.or(e.cover_goal).unwrap_or(position(e)))
                .collect();
            let mut chosen = None;
            for anchor in options {
                let cover = if role == InfantryRole::Assault {
                    None
                } else {
                    self.nearby_cover_slot(i, anchor, 4., &occupied)
                };
                let goal = cover.unwrap_or(anchor);
                if distance(at, goal) < 2.
                    || distance(goal, enemy) < d.min(self.weapon_range(&e.view.kind) * 0.94) - 0.25
                    || !self.passable(goal, false)
                    || occupied.iter().any(|p| distance(*p, goal) < 1.35)
                {
                    continue;
                }
                // Avoid a flank directly into another observed defender.
                if self.entities.iter().enumerate().any(|(j, t)| {
                    t.view.owner >= 0
                        && t.view.owner != e.view.owner
                        && t.view.hp > 0.
                        && !t.view.downed
                        && !t.view.building
                        && !t.view.airborne
                        && distance(position(t), goal) < 6.
                        && awareness[j].visible[e.view.owner as usize]
                }) {
                    continue;
                }
                let path = self.route(i, at, goal);
                if !path.is_empty() {
                    chosen = Some((goal, cover, path));
                    break;
                }
            }
            let Some((goal, cover, path)) = chosen else {
                self.entities[i].warfare.next_decision = self.tick + 40;
                continue;
            };
            let until = self.tick
                + ((distance(at, goal) / number(spec(&e.view.kind), "speed", 0.8).max(0.1) / DT)
                    as u64
                    + 60)
                    .min(600);
            let mover = self.entities[i].view.id.clone();
            let covering = self.entities[buddy].view.id.clone();
            let enemy_id = self.entities[target].view.id.clone();
            self.entities[i].path = path;
            self.entities[i].cover_goal = cover;
            self.entities[i].cover_ordered = false;
            self.entities[i].tactical_until = until;
            self.entities[i].warfare = InfantryWarfare {
                role,
                until,
                next_decision: self.tick + 10,
                target: Some(enemy_id.clone()),
                partner: Some(covering),
                goal: Some(goal),
            };
            let a = &mut self.entities[buddy];
            a.path.clear();
            a.speed = 0.;
            a.view.velocity = [0., 0.];
            a.warfare = InfantryWarfare {
                role: InfantryRole::Cover,
                until,
                next_decision: self.tick + 10,
                target: Some(enemy_id),
                partner: Some(mover),
                goal: None,
            };
        }
    }
}

#[cfg(test)]
mod warfare_tests {
    use super::*;
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.bases.clear();
        g.props.clear();
        g.terrain = crate::terrain::Terrain::new("warfare-test", 200, 200);
        g.terrain.authored = vec![0.52; 200 * 200];
        g.terrain.authored_scale = 12.;
        g.terrain.hydrate();
        g.training = Some(TrainingGround::default());
        g.human_opponent = true;
        g.config["fogEnabled"] = json!(false);
        g.spawn("short", 0, [40., 50.]);
        g.spawn("short", 0, [40., 54.]);
        g.spawn("short", 1, [62., 50.]);
        for e in &mut g.entities {
            e.view.hp = 100.;
            e.view.max_hp = 100.;
        }
        let ids = g.entities[..2]
            .iter()
            .map(|e| e.view.id.clone())
            .collect::<Vec<_>>();
        g.issue(
            0,
            &ids,
            Order {
                kind: "attack-move".into(),
                destination: Some([110., 50.]),
                target: None,
            },
            false,
        );
        g.tick = 100;
        g
    }
    #[test]
    fn direct_attack_holds_firing_range_even_against_a_suppressed_defender() {
        let mut g = field();
        let id = g.entities[0].view.id.clone();
        let target = g.entities[2].view.id.clone();
        g.entities[2].stance = "hold-fire".into();
        g.entities[2].view.suppression = 0.8;
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "attack".into(),
                target: Some(target),
                destination: Some([62., 50.]),
            },
            false,
        );
        let initial = distance(position(&g.entities[0]), position(&g.entities[2]));
        let mut closest = initial;
        for _ in 0..180 {
            g.step();
            closest = closest.min(distance(position(&g.entities[0]), position(&g.entities[2])));
        }
        assert!(
            closest >= initial - 0.25,
            "an in-range soldier closed from {initial}m to {closest}m"
        );
        assert!(g.entities[0].last_shot > 100);
    }
    #[test]
    fn a_bound_keeps_one_soldier_firing_and_the_other_moving_with_orders_intact() {
        let mut g = field();
        g.entities[2].view.x = 95.;
        g.entities[1].view.x = 65.;
        g.infantry_warfare();
        assert_eq!(g.entities[0].warfare.role, InfantryRole::Bound);
        assert_eq!(g.entities[1].warfare.role, InfantryRole::Cover);
        let moving = position(&g.entities[0]);
        let covering = position(&g.entities[1]);
        let source = g.entities[1].view.id.clone();
        let goal = g.entities[0].orders.front().unwrap().destination;
        for _ in 0..10 {
            g.tick += 1;
            g.tactical_step();
            g.movement();
            g.combat();
        }
        assert!(distance(moving, position(&g.entities[0])) > 0.5);
        assert!(distance(covering, position(&g.entities[1])) < 0.01);
        assert!(
            g.events
                .iter()
                .any(|e| e.kind == "shot" && e.source == source)
        );
        assert_eq!(g.entities[0].orders.front().unwrap().destination, goal);
    }
    #[test]
    fn fireable_defenders_do_not_trigger_point_blank_assaults() {
        let mut g = field();
        g.props.push(BattlefieldProp {
            scar: None,
            id: "enemy-cover".into(),
            kind: "sandbags".into(),
            at: [60., 50.],
            heading: std::f32::consts::FRAC_PI_2,
            hp: 5.,
            wreck: None,
        });
        g.infantry_warfare();
        assert!(!g.entities[0].warfare.role.mobile());
        let mut g = field();
        g.entities[2].view.suppression = 0.8;
        g.infantry_warfare();
        assert!(!g.entities[0].warfare.role.mobile());
    }
    #[test]
    fn heavy_suppression_interrupts_kill_chasing_and_loss_of_the_covering_man_aborts_a_bound() {
        let mut g = field();
        let id = g.entities[0].view.id.clone();
        let target = g.entities[2].view.id.clone();
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "attack".into(),
                target: Some(target.clone()),
                destination: None,
            },
            false,
        );
        g.entities[0].view.suppression = 0.9;
        g.entities[0].threat = Some([62., 50.]);
        g.entities[0].threat_until = 200;
        g.infantry_warfare();
        assert_eq!(g.entities[0].warfare.role, InfantryRole::Pinned);
        assert_eq!(g.entities[0].orders.front().unwrap().target, Some(target));
        g.combat();
        assert!(!g.events.iter().any(|e| e.kind == "shot" && e.source == id));
        let mut g = field();
        g.entities[2].view.x = 95.;
        g.entities[1].view.x = 65.;
        g.infantry_warfare();
        g.entities[1].view.hp = 0.;
        g.tick += 10;
        g.infantry_warfare();
        assert!(matches!(
            g.entities[0].warfare.role,
            InfantryRole::Recover | InfantryRole::Pinned
        ));
    }
    #[test]
    fn manual_moves_hold_fire_and_unseen_enemies_do_not_start_fireteam_plans() {
        let mut g = field();
        let ids = g.entities[..2]
            .iter()
            .map(|e| e.view.id.clone())
            .collect::<Vec<_>>();
        g.issue(
            0,
            &ids,
            Order {
                kind: "move".into(),
                destination: Some([35., 80.]),
                target: None,
            },
            false,
        );
        g.infantry_warfare();
        assert!(
            g.entities[..2]
                .iter()
                .all(|e| e.warfare.role == InfantryRole::None)
        );
        let mut g = field();
        g.entities[1].stance = "hold-fire".into();
        g.infantry_warfare();
        assert_eq!(g.entities[0].warfare.role, InfantryRole::None);
        let mut g = field();
        g.config["fogEnabled"] = json!(true);
        g.entities[2].view.x = 120.;
        g.infantry_warfare();
        assert!(
            g.entities[..2]
                .iter()
                .all(|e| e.warfare.role == InfantryRole::None)
        );
    }
    #[test]
    fn badly_outnumbered_soldiers_seek_protection_before_a_suicidal_bound() {
        let mut g = field();
        for z in [46., 54., 58.] {
            g.spawn("short", 1, [63., z]);
        }
        g.infantry_warfare();
        assert!(matches!(
            g.entities[0].warfare.role,
            InfantryRole::Recover | InfantryRole::Pinned
        ));
        assert!(matches!(
            g.entities[1].warfare.role,
            InfantryRole::Recover | InfantryRole::Pinned
        ));
    }
    #[test]
    fn saved_fireteams_resume_the_same_roles_positions_and_targeting() {
        let mut g = field();
        g.entities[2].view.x = 78.;
        g.entities[1].view.x = 48.;
        g.infantry_warfare();
        let mut loaded: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        for _ in 0..30 {
            g.step();
            loaded.step();
        }
        assert_eq!(
            serde_json::to_value(&g.entities).unwrap(),
            serde_json::to_value(&loaded.entities).unwrap()
        );
    }
    #[test]
    fn supply_is_fixed_slow_equal_and_stops_while_paused() {
        let mut g = field();
        g.supply = [10., 10.];
        g.tick = 0;
        for x in [10., 20., 30.] {
            g.spawn("mine", 0, [x, 150.]);
        }
        g.spawn("town", 0, [100., 150.]);
        g.config["mineIncome"] = json!(999);
        for _ in 0..99 {
            g.step();
        }
        assert_eq!(g.supply, [10., 10.]);
        g.step();
        assert_eq!(g.supply, [11., 11.]);
        g.paused = true;
        for _ in 0..200 {
            g.step();
        }
        assert_eq!(g.supply, [11., 11.]);
        assert_eq!(g.tick, 100);
        g.paused = false;
        for _ in 0..100 {
            g.step();
        }
        assert_eq!(g.supply, [12., 12.]);
        let mut loaded: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        for _ in 0..100 {
            g.step();
            loaded.step();
        }
        assert_eq!(loaded.supply, g.supply);
        assert_eq!(g.supply, [13., 13.]);
    }
}
