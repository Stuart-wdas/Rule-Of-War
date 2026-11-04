impl NativeGame {
    pub(crate) fn smoke_objective_victory(&mut self, owner: i32) {
        *self = Self::default();
        self.human_opponent = true;
        self.entities.retain(|e| e.view.building);
        let posts: Vec<_> = self
            .entities
            .iter()
            .filter(|e| e.view.kind == "objective")
            .map(position)
            .collect();
        for at in posts {
            let at = self.open_land(at, "short").unwrap();
            self.spawn("short", owner, at);
            self.entities.last_mut().unwrap().stance = "hold-fire".into();
        }
        self.tick = DEPLOYMENT_TICKS;
        for _ in 0..2400 {
            self.step();
        }
        assert_eq!(self.winner, Some(owner));
        self.paused = true;
    }
}
// Behavioural regressions exercise the public command path and complete 20 Hz steps.
#[cfg(test)]
mod order_regressions {
    use super::*;

    fn skirmish(kind: &str, owner: i32) -> (NativeGame, String, String) {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("order-regression", 300, 200);
        g.terrain.authored = vec![0.52; 300 * 200];
        g.terrain.hydrate();
        g.training = Some(TrainingGround::default());
        g.config["fogEnabled"] = json!(false);
        let unit = g.spawn(kind, owner, [35., 80.]);
        let enemy = g.spawn("short", 1 - owner, [57., 80.]);
        for e in &mut g.entities {
            e.view.hp = 10000.;
            e.view.max_hp = 10000.;
        }
        g.entities[1].stance = "hold-fire".into();
        (g, unit, enemy)
    }
    fn command(g: &mut NativeGame, owner: i32, id: &str, kind: &str, goal: Option<Point>) {
        g.command_as(owner, json!({"type":"command","commandType":kind,"unitIds":[id],"destination":goal.map(|p| json!({"x":p[0],"y":p[1]}))}));
        assert!(
            g.notice.starts_with("Orders acknowledged by 1"),
            "{}",
            g.notice
        );
    }
    #[test]
    fn stop_cancels_combat_and_does_not_reacquire_until_a_new_order() {
        for owner in [0, 1] {
            for kind in ["short", "scout", "engineer", "medium", "long"] {
                let (mut g, id, _) = skirmish(kind, owner);
                for _ in 0..140 {
                    g.step();
                }
                command(&mut g, owner, &id, "stop", None);
                let at = position(&g.entities[0]);
                let last_shot = g.entities[0].last_shot;
                for _ in 0..500 {
                    g.step();
                }
                assert!(
                    distance(position(&g.entities[0]), at) < 0.01,
                    "{kind} moved after X"
                );
                assert_eq!(g.entities[0].last_shot, last_shot, "{kind} fired after X");
                assert!(g.entities[0].target.is_none());
                command(&mut g, owner, &id, "attack-stance", None);
                for _ in 0..200 {
                    g.step();
                }
                assert!(
                    g.entities[0].last_shot > last_shot,
                    "{kind} could not resume"
                );
            }
        }
    }
    #[test]
    fn a_player_advance_does_not_flank_a_blocked_incidental_enemy() {
        for kind in ["short", "scout", "engineer"] {
            let (mut g, id, _) = skirmish(kind, 0);
            g.spawn("watchtower", -1, [46., 80.]);
            command(&mut g, 0, &id, "attack-move", Some([35., 40.]));
            for _ in 0..120 {
                g.step();
            }
            assert!(
                g.entities[0].view.z < 74.,
                "{kind} abandoned its commanded route: {:?}",
                position(&g.entities[0])
            );
            assert!(
                g.entities[0].view.x < 36.,
                "{kind} took an unsolicited flank"
            );
        }
    }
    #[test]
    fn a_move_order_overrides_cover_pinning_and_pending_launcher_animation() {
        for kind in ["short", "scout", "engineer", "medic"] {
            let (mut g, id, _) = skirmish(kind, 0);
            let e = &mut g.entities[0];
            e.aa_pose_until = 500;
            e.aa_target = Some("obsolete-aircraft".into());
            e.warfare = InfantryWarfare {
                role: InfantryRole::Pinned,
                until: 500,
                ..Default::default()
            };
            e.threat_until = 500;
            e.threat = Some([57., 80.]);
            e.view.suppression = 0.95;
            command(&mut g, 0, &id, "move", Some([35., 60.]));
            for _ in 0..160 {
                g.step();
            }
            assert!(
                g.entities[0].view.z < 74.,
                "{kind} ignored move: {:?}",
                position(&g.entities[0])
            );
        }
    }
    #[test]
    fn infantry_retries_a_lost_path_instead_of_completing_an_unreached_order() {
        let (mut g, id, _) = skirmish("scout", 0);
        command(&mut g, 0, &id, "move", Some([35., 65.]));
        g.entities[0].path.clear();
        for _ in 0..400 {
            g.step();
        }
        assert!(distance(position(&g.entities[0]), [35., 65.]) < 0.5);
    }
    #[test]
    fn stopped_orders_and_player_priority_survive_save_load_and_queued_moves() {
        let (mut g, id, _) = skirmish("scout", 0);
        command(&mut g, 0, &id, "stop", None);
        let mut loaded: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        for _ in 0..80 {
            g.step();
            loaded.step();
        }
        assert_eq!(
            serde_json::to_value(&g.entities).unwrap(),
            serde_json::to_value(&loaded.entities).unwrap()
        );
        command(&mut loaded, 0, &id, "move", Some([35., 70.]));
        loaded.command(json!({"type":"command","commandType":"move","unitIds":[id],"destination":{"x":35.,"y":60.},"queued":true}));
        for _ in 0..400 {
            loaded.step();
            if !loaded.entities[0].orders.is_empty() {
                assert_eq!(
                    loaded.entities[0].last_shot, 0,
                    "move-only must hold weapons until arrival"
                );
            }
        }
        assert!(distance(position(&loaded.entities[0]), [35., 60.]) < 0.5);
        assert!(
            loaded.entities[0].last_shot > 0,
            "automatic fire resumes after arrival"
        );
    }
    #[test]
    fn shift_queued_first_move_releases_stop_and_blocked_ai_cover_expires() {
        let (mut g, id, _) = skirmish("short", 0);
        command(&mut g, 0, &id, "stop", None);
        g.command(json!({"type":"command","commandType":"move","unitIds":[id],"destination":{"x":35.,"y":60.},"queued":true}));
        for _ in 0..200 {
            g.step();
        }
        assert!(distance(position(&g.entities[0]), [35., 60.]) < 0.5);
        let (mut g, id, _) = skirmish("short", 0);
        g.spawn("watchtower", -1, [46., 80.]);
        g.props.push(BattlefieldProp {
            scar: None,
            id: "cover".into(),
            kind: "sandbags".into(),
            at: [37., 80.],
            heading: std::f32::consts::FRAC_PI_2,
            hp: 100.,
            wreck: None,
        });
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([35., 40.]),
                target: None,
            },
            false,
        );
        g.entities[0].cover_goal = Some([35., 80.]);
        g.entities[0].path.clear();
        g.entities[0].tactical_until = 1;
        g.tick = 121;
        g.tactical_step();
        assert!(g.entities[0].cover_goal.is_none());
        assert!(!g.entities[0].path.is_empty());
        assert!(g.entities[0].cover_retry_after > g.tick);
    }
    #[test]
    fn a_vehicle_keeps_its_reachable_firing_goal_instead_of_replacing_it_each_second() {
        let (mut g, id, enemy) = skirmish("medium", 0);
        g.entities[1].view.x = 165.;
        g.spawn("watchtower", -1, [100., 80.]);
        g.command(json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":enemy}));
        let goal = *g.entities[0]
            .path
            .back()
            .expect("a reachable standoff point");
        assert!(g.infantry_firing_position(0, goal, 1));
        for _ in 0..60 {
            g.step();
        }
        assert_eq!(g.entities[0].path.back(), Some(&goal));
        for _ in 0..800 {
            g.step();
            if g.entities[0].last_shot > 0 {
                break;
            }
        }
        assert!(
            g.entities[0].last_shot > 0,
            "tank never reached a firing position"
        );
    }
    #[test]
    fn stopped_soldiers_can_board_enter_buildings_and_retreat_on_the_next_order() {
        for action in ["load", "garrison", "retreat"] {
            let (mut g, id, _) = skirmish("scout", 0);
            let carrier = g.spawn("troopCarrier", 0, [35., 65.]);
            let house = g.spawn("watchtower", 0, [50., 100.]);
            g.spawn("barracks", 0, [20., 140.]);
            command(&mut g, 0, &id, "stop", None);
            let target = if action == "load" {
                Some(carrier.clone())
            } else if action == "garrison" {
                Some(house)
            } else {
                None
            };
            g.command(
                json!({"type":"command","commandType":action,"unitIds":[id],"targetId":target}),
            );
            let start = position(&g.entities[0]);
            for _ in 0..500 {
                g.step();
            }
            assert!(
                distance(position(&g.entities[0]), start) > 1.,
                "{action} remained halted"
            );
            if action == "load" {
                assert_eq!(g.entities[0].transport.as_ref(), Some(&carrier));
            }
            if action == "garrison" {
                assert!(
                    g.entities[0]
                        .view
                        .garrison
                        .as_ref()
                        .is_some_and(|v| v.entered)
                );
            }
        }
    }
    #[test]
    fn rural_roads_and_farm_access_arrive_for_both_teams_and_all_ground_classes() {
        let mut g = NativeGame {
            human_opponent: true,
            training: Some(TrainingGround::default()),
            ..Default::default()
        };
        g.entities.retain(|e| e.view.building);
        let town = g
            .entities
            .iter()
            .find(|e| e.view.town_plan == crate::settlement_layout::TownPlan::Rural)
            .unwrap();
        let center = position(town);
        let roads = crate::settlement_layout::roads_for(&town.view.kind, town.view.town_plan);
        let buildings = g.entities.len();
        let mut cases = 0;
        for owner in [0, 1] {
            for kind in [
                "short",
                "scout",
                "medic",
                "engineer",
                "medium",
                "long",
                "troopCarrier",
            ] {
                for (n, road) in roads.iter().enumerate() {
                    g.entities.truncate(buildings);
                    let start = g
                        .open_land([center[0] + road.a[0], center[1] + road.a[1]], kind)
                        .unwrap();
                    let goal = g
                        .open_land([center[0] + road.b[0], center[1] + road.b[1]], kind)
                        .unwrap();
                    let id = g.spawn(kind, owner, start);
                    g.entities.last_mut().unwrap().view.heading = if n % 2 == 0 {
                        std::f32::consts::PI
                    } else {
                        -1.4
                    };
                    command(&mut g, owner, &id, "move", Some(goal));
                    // Infantry orders reserve nearby cover at their destination.
                    // Check arrival at the accepted order, including this normal
                    // cover search (8m plus its existing 1.6m edge allowance).
                    let requested = goal;
                    let goal = g.entities[buildings].orders.front().unwrap().destination.unwrap();
                    assert!(distance(requested, goal) <= 9.61, "team {owner} {kind} road {n}: assigned {goal:?} too far from {requested:?}");
                    let mut travelled = 0.;
                    let mut previous = start;
                    for _ in 0..2600 {
                        g.step();
                        let e = &g.entities[buildings];
                        let at = position(e);
                        travelled += distance(previous, at);
                        assert!(
                            g.clear_segment_radius(
                                previous,
                                at,
                                0,
                                body_clearance(kind, e.view.model_scale)
                            ),
                            "{kind} crossed town collision"
                        );
                        previous = at;
                        if e.orders.is_empty() {
                            break;
                        }
                    }
                    let e = &g.entities[buildings];
                    assert!(
                        distance(position(e), goal) < 0.6,
                        "team {owner} {kind} road {n}: {:?} -> {goal:?}, stopped {:?}, orders {}, path {}",
                        start,
                        position(e),
                        e.orders.len(),
                        e.path.len()
                    );
                    assert!(
                        travelled < distance(start, goal) * 1.8 + 10.,
                        "{kind} circled: {travelled}"
                    );
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, roads.len() * 14);
    }
    #[test]
    fn both_teams_can_secure_the_real_objectives_and_contesting_breaks_the_win_clock() {
        for owner in [0, 1] {
            let mut g = NativeGame {
                human_opponent: true,
                ..Default::default()
            };
            g.entities.retain(|e| e.view.building);
            let posts: Vec<_> = g
                .entities
                .iter()
                .filter(|e| e.view.kind == "objective")
                .map(position)
                .collect();
            for at in &posts {
                let at = g.open_land(*at, "short").unwrap();
                g.spawn("short", owner, at);
                g.entities.last_mut().unwrap().stance = "hold-fire".into();
            }
            // Capture is forbidden during deployment.
            g.tick = DEPLOYMENT_TICKS - 10;
            for _ in 0..5 {
                g.step();
            }
            assert!(
                g.entities
                    .iter()
                    .filter(|e| e.view.kind == "objective")
                    .all(|e| e.view.owner < 0)
            );
            for _ in 0..1100 {
                g.step();
            }
            assert!(
                g.entities
                    .iter()
                    .filter(|e| e.view.kind == "objective")
                    .all(|e| e.view.owner == owner)
            );
            assert!(g.winner.is_none());
            let enemy = g.spawn("medium", 1 - owner, posts[0]);
            g.entities.last_mut().unwrap().stance = "hold-fire".into();
            for _ in 0..30 {
                g.step();
            }
            assert_eq!(g.hold[owner as usize], 0.);
            g.entities.retain(|e| e.view.id != enemy);
            for _ in 0..1790 {
                g.step();
            }
            assert!(g.winner.is_none());
            for _ in 0..20 {
                g.step();
            }
            assert_eq!(g.winner, Some(owner));
            let tick = g.tick;
            g.step();
            assert_eq!(g.tick, tick);
        }
    }
}
