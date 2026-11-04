// Included in simulation.rs. Attack orders route to a firing position, never the victim.
impl NativeGame {
    pub fn smoke_infantry_range(&mut self) -> Vec<String> {
        *self = Self::operation(crate::training::ID, 384, 384, default_config());
        self.entities.clear();
        self.props.clear();
        self.bases.clear();
        self.config["fogEnabled"] = json!(false);
        let mut ids = vec![];
        for (kind, at, target_at) in [
            ("short", [160., 150.], [210., 150.]),
            ("scout", [100., 190.], [235., 190.]),
            ("engineer", [160., 235.], [210., 235.]),
        ] {
            let id = self.spawn(kind, 0, at);
            let target = self.spawn("short", 1, target_at);
            let e = self.entities.last_mut().unwrap();
            e.view.hp = 200.;
            e.view.max_hp = 200.;
            e.view.training_target = true;
            e.stance = "hold-fire".into();
            self.issue(
                0,
                std::slice::from_ref(&id),
                Order {
                    kind: "attack".into(),
                    target: Some(target),
                    destination: Some(target_at),
                },
                false,
            );
            ids.push(id);
        }
        for _ in 0..500 {
            self.step();
        }
        for row in self.infantry_range_report() {
            assert!(row["distance"].as_f64().unwrap() >= row["range"].as_f64().unwrap() * 0.93);
            assert!(row["shotsFired"].as_bool().unwrap());
        }
        self.paused = true;
        ids
    }
    pub fn infantry_range_report(&self) -> Vec<Value> {
        self.entities.iter().enumerate().filter(|(_,e)|e.view.owner==0 && e.view.category=="troop")
            .filter_map(|(i,e)| {
                let target=e.orders.front().filter(|o|o.kind=="attack")?.target.as_ref()?;
                let j=self.entities.iter().position(|t|&t.view.id==target)?;
                Some(json!({"kind":e.view.kind,"range":self.infantry_attack_range(i,j),"distance":distance(position(e),position(&self.entities[j])),"shotsFired":e.last_shot>0}))
            }).collect()
    }
    fn infantry_attack_range(&self, index: usize, target: usize) -> f32 {
        let e = &self.entities[index];
        if !self.entities[target].view.airborne
            && !weapon_can_damage(&e.view.kind, &self.entities[target].view)
        {
            return 0.;
        }
        if self.entities[target].view.airborne {
            if flag(spec(&e.view.kind), "antiAirLauncher") {
                number(spec(&e.view.kind), "aaRange", 160.)
            } else {
                0.
            }
        } else {
            self.weapon_range(&e.view.kind)
        }
    }
    fn infantry_firing_position(&self, index: usize, at: Point, target: usize) -> bool {
        let range = self.infantry_attack_range(index, target);
        if range <= 0.
            || distance(
                at,
                aim_point(&self.entities[index], &self.entities[target], at),
            ) > range
        {
            return false;
        }
        if distance(at, position(&self.entities[index])) < 0.001 {
            return self.firing_line(&self.entities[index], &self.entities[target]);
        }
        let mut shooter = self.entities[index].clone();
        shooter.view.x = at[0];
        shooter.view.z = at[1];
        self.firing_line(&shooter, &self.entities[target])
    }
    fn infantry_attack_route(&self, index: usize, start: Point, target: usize) -> VecDeque<Point> {
        let _work = NavigationWork::new(16384);
        let _timing = crate::performance::detailed(
            "infantry.attack_route",
            || json!({"unit":self.entities[index].view.id,"kind":self.entities[index].view.kind,"target":self.entities[target].view.id,"from":start}),
        );
        if !self.visible(self.entities[index].view.owner, &self.entities[target])
            || self.infantry_firing_position(index, start, target)
        {
            return VecDeque::new();
        }
        let range = self.infantry_attack_range(index, target);
        if range <= 0. {
            return VecDeque::new();
        }
        let center = aim_point(&self.entities[index], &self.entities[target], start);
        let d = distance(start, center).max(0.01);
        // A small margin avoids oscillating on the exact range boundary. If a shot is
        // blocked while already in range, move sideways around this ring instead of closer.
        let stand = (range * 0.94).min(d).max(1.);
        let heading = (start[1] - center[1]).atan2(start[0] - center[0]);
        let mut candidates = vec![];
        let clearance = body_clearance(
            &self.entities[index].view.kind,
            self.entities[index].view.model_scale,
        );
        // Check short lateral adjustments first. A blocked window should not
        // send a sniper around the circumference of its entire weapon range.
        let away = [(start[0] - center[0]) / d, (start[1] - center[1]) / d];
        for offset in [2., 4., 8., 12.] {
            for side in [-1., 1.] {
                let p = [
                    start[0] - away[1] * offset * side,
                    start[1] + away[0] * offset * side,
                ];
                if self.passable_clearance(p, 0, clearance)
                    && self.infantry_firing_position(index, p, target)
                {
                    candidates.push(p);
                }
            }
        }
        for turn in [
            0.,
            0.25,
            -0.25,
            0.5,
            -0.5,
            0.85,
            -0.85,
            1.2,
            -1.2,
            1.7,
            -1.7,
            2.4,
            -2.4,
            std::f32::consts::PI,
        ] {
            let angle = heading + turn;
            let p = [
                center[0] + angle.cos() * stand,
                center[1] + angle.sin() * stand,
            ];
            if self.passable_clearance(p, 0, clearance)
                && self.infantry_firing_position(index, p, target)
            {
                candidates.push(p);
            }
        }
        candidates.sort_by(|a, b| distance(start, *a).total_cmp(&distance(start, *b)));
        // Ranking is stable while a route is being followed; time-based angle
        // rotation used to alternate opposite flanks before a soldier arrived.
        for goal in candidates {
            let _candidate_work = NavigationWork::new(4096);
            let path = self.route(index, start, goal);
            // A flank must not cut through the enemy's position on its way around.
            let minimum = d.min(stand) - 0.3;
            if !path.is_empty() && route_outside(&path, start, center, minimum) {
                return path;
            }
            if d <= range && !path.is_empty() {
                let end_angle = (goal[1] - center[1]).atan2(goal[0] - center[0]);
                let sweep = angle_delta(heading, end_angle);
                let steps = (sweep.abs() / 0.16).ceil().max(1.) as usize;
                let mut around = VecDeque::new();
                let mut from = start;
                let mut valid = true;
                for n in 0..=steps + 1 {
                    let p = if n == steps + 1 {
                        goal
                    } else {
                        let angle = heading + sweep * n as f32 / steps as f32;
                        [
                            center[0] + angle.cos() * (stand + 3.),
                            center[1] + angle.sin() * (stand + 3.),
                        ]
                    };
                    let segment = self.route(index, from, p);
                    if segment.is_empty() || !route_outside(&segment, from, center, minimum) {
                        valid = false;
                        break;
                    }
                    around.extend(segment);
                    from = p;
                }
                if valid {
                    return around;
                }
            }
        }
        VecDeque::new()
    }
    fn attack_route_needed(&self, index: usize, target: usize) -> bool {
        self.entities[index]
            .path
            .back()
            .is_none_or(|p| !self.infantry_firing_position(index, *p, target))
    }
    fn infantry_engagement_target(&self, index: usize, visibility: &[[bool; 2]]) -> Option<usize> {
        let e = &self.entities[index];
        if e.view.category != "troop"
            || e.stance != "attack"
            || number(spec(&e.view.kind), "damage", 0.) <= 0.
            || e.orders
                .front()
                .is_some_and(|o| !["attack", "attack-move", "patrol"].contains(&o.kind.as_str()))
        {
            return None;
        }
        let explicit = e
            .orders
            .front()
            .filter(|o| o.kind == "attack")
            .and_then(|o| o.target.as_ref());
        self.entities
            .iter()
            .enumerate()
            .filter(|(j, t)| {
                t.view.owner != e.view.owner
                    && t.view.hp > 0.
                    && !t.view.downed
                    && t.transport.is_none()
                    && (t.view.owner >= 0 || explicit.is_some() && !t.view.lots.is_empty())
                    && t.view.kind != "objective"
                    && explicit.is_none_or(|id| id == &t.view.id)
                    && self.infantry_attack_range(index, *j) > 0.
                    && (explicit.is_some()
                        || distance(position(e), attack_point(t, position(e)))
                            <= self.infantry_attack_range(index, *j))
                    && visibility[*j]
                        .get(e.view.owner as usize)
                        .copied()
                        .unwrap_or(false)
            })
            .min_by(|(_, a), (_, b)| {
                let clear_a = self.firing_line(e, a);
                let clear_b = self.firing_line(e, b);
                let da = distance(position(e), aim_point(e, a, position(e)));
                let db = distance(position(e), aim_point(e, b, position(e)));
                clear_b.cmp(&clear_a).then_with(|| da.total_cmp(&db))
            })
            .map(|(j, _)| j)
    }
}

fn route_outside(path: &VecDeque<Point>, start: Point, center: Point, minimum: f32) -> bool {
    let mut from = start;
    path.iter().all(|p| {
        let safe = segment_distance(center, from, *p) >= minimum;
        from = *p;
        safe
    })
}

#[cfg(test)]
mod infantry_engagement_tests {
    use super::*;
    #[test]
    fn advancing_vehicles_do_not_stop_permanently_behind_a_building() {
        for owner in [0, 1] {
            for kind in ["medium", "long"] {
                let mut g = field(kind, owner, 22.);
                g.spawn("watchtower", -1, [46., 80.]);
                let id = g.entities[0].view.id.clone();
                assert_eq!(
                    g.firing_line(&g.entities[0], &g.entities[1]),
                    kind == "long",
                    "artillery has an indirect firing path; tank fire is obstructed"
                );
                assert_eq!(
                    g.issue(
                        owner,
                        &[id],
                        Order {
                            kind: "attack-move".into(),
                            target: None,
                            destination: Some([100., 80.])
                        },
                        false
                    ),
                    1
                );
                for _ in 0..650 {
                    g.step();
                    if g.entities[0].last_shot > 0 {
                        break;
                    }
                }
                assert!(
                    g.entities[0].last_shot > 0,
                    "team {owner} {kind} stopped behind the obstruction"
                );
                assert!(g.firing_line(&g.entities[0], &g.entities[1]));
            }
        }
    }
    #[test]
    fn attack_commands_release_pending_entry_and_restore_close_range_fire() {
        for kind in ["attack", "attack-stance"] {
            let mut g = field("short", 0, 22.);
            let id = g.entities[0].view.id.clone();
            let enemy = g.entities[1].view.id.clone();
            let building = g.spawn("watchtower", -1, [35., 95.]);
            g.entities[0].view.garrison = Some(Garrison {
                building,
                lot: None,
                window: 0,
                entered: false,
                ready_at: 0,
                door_stage: 0,
                exiting: false,
            });
            g.entities[0].orders = VecDeque::from([Order {
                kind: "garrison".into(),
                target: None,
                destination: Some([35., 98.]),
            }]);
            assert!(!g.firing_line(&g.entities[0], &g.entities[1]));
            g.issue(
                0,
                &[id],
                Order {
                    kind: kind.into(),
                    target: (kind == "attack").then_some(enemy),
                    destination: Some([57., 80.]),
                },
                false,
            );
            assert!(g.entities[0].view.garrison.is_none());
            for _ in 0..100 {
                g.step();
            }
            assert!(
                g.entities[0].last_shot > 0,
                "{kind} did not restore automatic fire"
            );
        }
    }
    #[test]
    fn a_stale_covering_target_does_not_block_a_close_clear_enemy() {
        for owner in [0, 1] {
            let mut g = field("short", owner, 80.);
            g.entities[0].orders.clear();
            g.entities[0].warfare = InfantryWarfare {
                role: InfantryRole::Cover,
                target: Some(g.entities[1].view.id.clone()),
                until: 500,
                ..Default::default()
            };
            let close = g.spawn("short", 1 - owner, [50., 84.]);
            g.entities.last_mut().unwrap().stance = "hold-fire".into();
            for _ in 0..20 {
                g.tick += 1;
                g.movement();
                g.combat();
            }
            assert!(
                g.events
                    .iter()
                    .any(|e| e.kind == "shot" && e.owner == owner && e.from == Some([50., 84.])),
                "stale tactical target suppressed fire for team {owner}"
            );
            assert_eq!(g.entities[0].target.as_ref(), Some(&close));
        }
    }
    #[test]
    fn automatic_engagement_shoots_a_clear_enemy_before_flanking_a_blocked_one() {
        let mut g = field("short", 0, 22.);
        g.entities[0].orders.clear();
        g.spawn("watchtower", -1, [46., 80.]);
        let clear = g.spawn("short", 1, [52., 92.]);
        g.entities.last_mut().unwrap().stance = "hold-fire".into();
        for _ in 0..25 {
            g.tick += 1;
            g.movement();
            g.combat();
        }
        assert_eq!(g.entities[0].target.as_ref(), Some(&clear));
        assert!(g.entities[0].last_shot > 0);
        assert!(distance(position(&g.entities[0]), [35., 80.]) < 0.1);
    }
    #[test]
    fn idle_units_auto_engage_without_orders_for_both_teams() {
        for owner in [0, 1] {
            let mut g = field("short", owner, 22.);
            g.entities[0].orders.clear();
            for _ in 0..100 {
                g.step();
            }
            assert!(g.entities[0].last_shot > 0, "team {owner} stayed idle");
            assert!(distance(position(&g.entities[0]), [35., 80.]) < 0.1);
            assert!(
                g.snapshot()
                    .entities
                    .iter()
                    .find(|p| p.id == g.entities[0].view.id)
                    .unwrap()
                    .auto_attack
            );
        }
    }
    #[test]
    fn idle_units_find_a_clear_shot_but_hold_fire_remains_explicit() {
        let mut g = field("short", 0, 22.);
        g.entities[0].orders.clear();
        g.spawn("watchtower", -1, [46., 80.]);
        assert!(!g.infantry_firing_position(0, [35., 80.], 1));
        for _ in 0..400 {
            g.step();
        }
        assert!(
            g.entities[0].last_shot > 0,
            "idle blocked unit never found a shot"
        );
        assert!(
            distance(position(&g.entities[0]), position(&g.entities[1])) >= 21.65,
            "idle flank ended at {:?}",
            position(&g.entities[0])
        );
        let mut g = field("short", 0, 22.);
        let id = g.entities[0].view.id.clone();
        g.issue(
            0,
            &[id],
            Order {
                kind: "hold-fire".into(),
                target: None,
                destination: None,
            },
            false,
        );
        for _ in 0..100 {
            g.step();
        }
        assert_eq!(g.entities[0].last_shot, 0);
    }
    #[test]
    fn auto_attack_command_releases_move_only_suppression() {
        let mut g = field("short", 0, 22.);
        let id = g.entities[0].view.id.clone();
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "move".into(),
                target: None,
                destination: Some([10., 80.]),
            },
            false,
        );
        for _ in 0..10 {
            g.step();
        }
        assert_eq!(g.entities[0].last_shot, 0);
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-stance".into(),
                target: None,
                destination: None,
            },
            false,
        );
        assert!(g.entities[0].orders.is_empty());
        assert!(g.entities[0].path.is_empty());
        for _ in 0..100 {
            g.step();
        }
        assert!(g.entities[0].last_shot > 0);
    }
    fn field(kind: &str, owner: i32, d: f32) -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("engagement", 300, 200);
        g.terrain.authored = vec![0.52; 300 * 200];
        g.terrain.hydrate();
        g.training = Some(TrainingGround::default());
        g.config["fogEnabled"] = json!(false);
        let id = g.spawn(kind, owner, [35., 80.]);
        let target = g.spawn("short", 1 - owner, [35. + d, 80.]);
        for e in &mut g.entities {
            e.view.hp = 1000.;
            e.view.max_hp = 1000.;
        }
        g.entities[1].stance = "hold-fire".into();
        g.issue(
            owner,
            &[id],
            Order {
                kind: "attack".into(),
                target: Some(target),
                destination: Some([35. + d, 80.]),
            },
            false,
        );
        g
    }
    #[test]
    fn each_armed_class_and_team_stops_at_range_and_fires_without_a_melee_approach() {
        for owner in [0, 1] {
            for kind in ["short", "scout", "engineer"] {
                let range = number(spec(kind), "range", 32.);
                let mut g = field(kind, owner, range + 15.);
                let mut closest = f32::MAX;
                for _ in 0..700 {
                    g.step();
                    closest =
                        closest.min(distance(position(&g.entities[0]), position(&g.entities[1])));
                }
                assert!(
                    closest >= range * 0.93,
                    "{kind} team {owner}: {closest} / {range}"
                );
                assert!(closest <= range + 0.2);
                assert!(g.entities[0].last_shot > 0, "{kind} did not fire");
            }
        }
    }
    #[test]
    fn attack_reacquires_a_moving_target_and_old_paths_cannot_override_the_range_stop() {
        let mut g = field("short", 0, 22.);
        g.entities[0].path = g.route(0, [35., 80.], [57., 80.]);
        for _ in 0..50 {
            g.step();
        }
        assert!(distance(position(&g.entities[0]), [35., 80.]) < 0.1);
        g.entities[1].view.x = 110.;
        for _ in 0..500 {
            g.step();
        }
        let d = distance(position(&g.entities[0]), position(&g.entities[1]));
        let range=g.weapon_range("short");
        assert!((range*0.93..=range+0.1).contains(&d));
        let mut loaded: NativeGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        for _ in 0..40 {
            g.step();
            loaded.step();
        }
        assert_eq!(
            serde_json::to_value(&g.entities).unwrap(),
            serde_json::to_value(&loaded.entities).unwrap()
        );
    }
    #[test]
    fn queued_attacks_and_attack_move_keep_their_orders_when_stopping_to_fire() {
        let mut g = field("short", 0, 70.);
        let id = g.entities[0].view.id.clone();
        let target = g.entities[1].view.id.clone();
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "move".into(),
                destination: Some([45., 80.]),
                target: None,
            },
            false,
        );
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "attack".into(),
                destination: Some([105., 80.]),
                target: Some(target),
            },
            true,
        );
        for _ in 0..500 {
            g.step();
        }
        assert!(distance(position(&g.entities[0]), position(&g.entities[1])) >= 31.5);
        assert!(g.entities[0].last_shot > 0);
        let mut g = field("short", 0, 70.);
        let id = g.entities[0].view.id.clone();
        g.props.push(BattlefieldProp {
            scar: None,
            id: "destination-cover".into(),
            kind: "sandbags".into(),
            at: [180., 81.],
            heading: 0.,
            hp: 5.,
            wreck: None,
        });
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([180., 80.]),
                target: None,
            },
            false,
        );
        for _ in 0..400 {
            g.step();
        }
        assert!(g.entities[0].last_shot > 0);
        assert!(!g.entities[0].orders.is_empty());
        assert!(distance(position(&g.entities[0]), position(&g.entities[1])) >= 31.5);
        g.entities[1].view.hp = 0.;
        for _ in 0..900 {
            g.step();
        }
        assert!(
            g.entities[0].view.x > 150.,
            "advance order must resume after the fight"
        );
    }
    #[test]
    fn neutral_houses_do_not_interrupt_an_attack_move() {
        let mut g = field("short", 0, 70.);
        g.entities[1].view.owner = -1;
        g.spawn("village", -1, [75., 100.]);
        let id = g.entities[0].view.id.clone();
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([100., 80.]),
                target: None,
            },
            false,
        );
        for _ in 0..180 {
            g.step();
        }
        assert!(g.entities[0].view.x > 50.);
        assert_eq!(g.entities[0].last_shot, 0);
    }
    #[test]
    fn blocked_shots_use_a_lateral_firing_position_instead_of_walking_into_the_target() {
        let mut g = field("short", 0, 22.);
        let building = g.spawn("watchtower", -1, [46., 80.]);
        assert!(g.entities.iter().any(|e| e.view.id == building));
        let path = g.infantry_attack_route(0, [35., 80.], 1);
        assert!(!path.is_empty());
        assert!(path.iter().all(|p| distance(*p, [57., 80.]) >= 21.7));
        assert!(g.infantry_firing_position(0, *path.back().unwrap(), 1));
        for _ in 0..400 {
            g.step();
            assert!(distance(position(&g.entities[0]), position(&g.entities[1])) >= 21.65);
        }
        assert!(
            g.entities[0].last_shot > 0,
            "flanking must produce a real clear shot"
        );
    }
}
