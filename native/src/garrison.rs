// Included in simulation.rs: saved occupancy, exclusive window reservations and fire arcs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Garrison {
    pub building: String,
    pub lot: Option<usize>,
    pub window: usize,
    pub entered: bool,
    #[serde(default)]
    pub ready_at: u64,
    #[serde(default)]
    pub door_stage: u8,
    #[serde(default)]
    pub exiting: bool,
}

#[cfg(test)]
mod garrison_tests {
    use super::*;
    #[test]
    fn explicit_house_clicks_do_not_overflow_into_a_neighbour() {
        let mut g = field();
        g.terrain = crate::terrain::Terrain::new("entry-overflow", 128, 128);
        g.terrain.authored = vec![0.52; 128 * 128];
        g.terrain.hydrate();
        let building = house(&mut g);
        let mut neighbour = g.entities[0].view.lots[0].clone();
        neighbour.at = [14., 0.];
        g.entities[0].view.lots.push(neighbour);
        let slots = g.window_slots(&g.entities[0], Some(0)).len();
        for window in 0..slots {
            g.spawn("short", 0, [50., 50.]);
            g.entities.last_mut().unwrap().view.garrison = Some(Garrison {
                building: building.clone(),
                lot: Some(0),
                window,
                entered: true,
                ready_at: 0,
                door_stage: 2,
                exiting: false,
            });
        }
        let id = g.spawn("short", 0, [60., 65.]);
        assert_eq!(
            g.enter_building(0, std::slice::from_ref(&id), &building, Some([70., 70.])),
            0
        );
        assert!(g.entities.last().unwrap().view.garrison.is_none());
        assert_eq!(g.enter_building(0, &[id], &building, Some([84., 70.])), 1);
        assert_eq!(
            g.entities
                .last()
                .unwrap()
                .view
                .garrison
                .as_ref()
                .unwrap()
                .lot,
            Some(1)
        );
    }
    #[test]
    fn rotated_scaled_town_entry_keeps_its_house_and_cancels_old_tactics() {
        let mut g = field();
        let building = house(&mut g);
        let mut neighbour = g.entities[0].view.lots[0].clone();
        neighbour.at = [14., 0.];
        g.entities[0].view.lots.push(neighbour);
        g.entities[0].view.heading = 0.7;
        g.entities[0].view.model_scale = 1.3;
        let click = building_world(&g.entities[0], [14., 0.]);
        let reservation = Garrison {
            building: building.clone(),
            lot: Some(1),
            window: 0,
            entered: false,
            ready_at: 0,
            door_stage: 0,
            exiting: false,
        };
        let (door, _, _) = g.garrison_door(&reservation).unwrap();
        let id = g.spawn("short", 0, [door[0] + 6., door[1] + 6.]);
        g.entities[1].warfare = InfantryWarfare {
            role: InfantryRole::Flank,
            until: 1000,
            goal: Some([150., 150.]),
            ..Default::default()
        };
        g.entities[1].tactical_until = 1000;
        assert_eq!(g.enter_building(0, &[id], &building, Some(click)), 1);
        assert_eq!(g.entities[1].warfare.role, InfantryRole::None);
        assert_eq!(g.entities[1].path.back(), Some(&door));
        let path = g.entities[1].path.clone();
        let mut from = position(&g.entities[1]);
        for to in path {
            assert!(g.clear_segment_radius(from, to, 0, 0.));
            from = to;
        }
        for _ in 0..800 {
            g.tick += 1;
            g.movement();
            g.garrison_step();
            let state = g.entities[1].view.garrison.as_ref().unwrap();
            assert_eq!(state.lot, Some(1));
            if state.entered {
                return;
            }
        }
        panic!("soldier did not cross the clicked house door");
    }
    #[test]
    fn hostile_interiors_are_private_for_both_sides_even_with_fog_disabled() {
        let mut g = field();
        g.config["fogEnabled"] = json!(false);
        let building = house(&mut g);
        let soldier = g.spawn("short", 1, [70., 78.]);
        g.enter_building(1, std::slice::from_ref(&soldier), &building, None);
        settle(&mut g);
        assert!(g.entities[1].view.garrison.as_ref().unwrap().entered);
        assert!(!g.visible(0, &g.entities[1])); // A tank alone cannot discover a window occupant.
        let hostile = g.snapshot_for(0);
        assert!(!hostile.entities.iter().any(|e| e.id == soldier));
        assert_eq!(
            hostile
                .entities
                .iter()
                .find(|e| e.id == building)
                .unwrap()
                .occupants,
            0
        );
        let own = g.snapshot_for(1);
        assert_eq!(own.viewer, 1);
        assert!(own.entities.iter().any(|e| e.id == soldier));
        assert_eq!(
            own.entities
                .iter()
                .find(|e| e.id == building)
                .unwrap()
                .occupants,
            1
        );
        g.leave_building(1, false);
        settle(&mut g);
        assert!(g.snapshot_for(0).entities.iter().any(|e| e.id == soldier));
    }
    #[test]
    fn another_wing_of_the_occupied_h_house_blocks_fire_across_its_courtyard() {
        let mut g = field();
        let building = house(&mut g);
        g.entities[0].view.lots[0].style = 2;
        g.refresh_towns();
        let lot = &g.entities[0].view.lots[0];
        let window = lot
            .windows()
            .into_iter()
            .find(|(p, _, n)| n[0] > 0.9 && p[0] < 0. && p[1] > 0.)
            .unwrap();
        let from = [70. + window.0[0], 70. + window.0[1]];
        assert!(!g.building_line_clear(from, [80., from[1]], Some(&building), None));
        assert!(g.building_line_clear(from, [70., 82.], Some(&building), None));
    }
    #[test]
    fn an_infantryman_approaching_from_behind_walks_around_and_crosses_the_front_door() {
        let mut g = field();
        let building = house(&mut g);
        g.entities[0].view.lots[0].yaw = 0.7;
        g.refresh_towns();
        let rifle = g.spawn("short", 0, [70., 60.]);
        assert_eq!(g.enter_building(0, &[rifle], &building, None), 1);
        let reservation = g.entities[1].view.garrison.as_ref().unwrap().clone();
        let (outside, inside, _) = g.garrison_door(&reservation).unwrap();
        assert_eq!(g.entities[1].path.back(), Some(&outside));
        let mut crossed = false;
        for _ in 0..600 {
            g.tick += 1;
            g.movement();
            let before = position(&g.entities[1]);
            g.garrison_step();
            let state = g.entities[1].view.garrison.as_ref().unwrap();
            if state.door_stage == 1 && !state.entered {
                crossed = true;
                assert!(segment_distance(position(&g.entities[1]), outside, inside) < 0.05);
                assert!(distance(before, position(&g.entities[1])) <= 2.4 * DT + 0.001);
            }
            if state.entered {
                break;
            }
        }
        assert!(
            crossed && g.entities[1].view.garrison.as_ref().unwrap().entered,
            "at={:?} door={:?} path={:?} state={:?}",
            position(&g.entities[1]),
            outside,
            g.entities[1].path,
            g.entities[1].view.garrison
        );
        let destination = [50., 85.];
        assert_eq!(
            g.issue(
                0,
                &[g.entities[1].view.id.clone()],
                Order {
                    kind: "move".into(),
                    destination: Some(destination),
                    target: None
                },
                false
            ),
            1
        );
        for _ in 0..600 {
            g.tick += 1;
            g.movement();
            g.garrison_step();
        }
        assert!(g.entities[1].view.garrison.is_none());
        assert!(distance(position(&g.entities[1]), destination) < 1.);
    }
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.config["fogEnabled"] = json!(false);
        g.human_opponent = true;
        g
    }
    #[test]
    fn a_tank_shell_damages_multiple_units_at_its_impact() {
        let mut g = field();
        g.spawn("medium", 0, [30., 30.]);
        g.spawn("medium", 1, [35., 30.]);
        g.spawn("medium", 1, [37., 30.]);
        for e in &mut g.entities {
            e.view.hp = 20.;
            e.view.max_hp = 20.;
            if e.view.owner == 1 {
                e.stance = "hold-fire".into();
            }
        }
        for _ in 0..280 {
            g.tick += 1;
            g.combat();
        }
        assert!(g.entities[1].view.hp < 20.);
        assert!(g.entities[2].view.hp < 20.);
        assert!(g.entities[1].view.hp < g.entities[2].view.hp);
    }
    #[test]
    fn wounded_occupants_leave_the_window_alive_for_medic_rescue() {
        let mut g = field();
        let building = house(&mut g);
        let rifle = g.spawn("short", 1, [70., 78.]);
        g.enter_building(1, &[rifle], &building, None);
        settle(&mut g);
        g.entities[1].view.hp = 0.1;
        let at = position(&g.entities[1]);
        g.impact(&Projectile {
            owner: 0,
            from: [70., 90.],
            to: at,
            born: 0,
            impact: 0,
            damage: 1.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        g.release_collapsed_garrisons();
        assert!(g.entities[1].view.downed && g.entities[1].view.hp > 0.);
        assert!(g.entities[1].view.garrison.is_none());
    }
    #[test]
    fn other_houses_in_the_same_settlement_block_an_occupied_window() {
        let mut g = field();
        let building = house(&mut g);
        let mut other = g.entities[0].view.lots[0].clone();
        other.at = [0., 10.];
        g.entities[0].view.lots.push(other);
        g.refresh_towns();
        assert!(!g.building_line_clear([70., 74.38], [70., 90.], Some(&building), None));
        g.entities[0].view.lots[1].integrity = 0.;
        assert!(g.building_line_clear([70., 74.38], [70., 90.], Some(&building), None));
    }
    fn house(g: &mut NativeGame) -> String {
        let id = g.spawn("village", -1, [70., 70.]);
        g.entities[0].view.lots = vec![crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 6.,
            depth: 4.,
            height: 3.2,
            yaw: 0.,
            style: 0,
            ground: 0.,
            integrity: 1.,
        }];
        g.refresh_towns();
        id
    }
    fn settle(g: &mut NativeGame) {
        for _ in 0..240 {
            g.tick += 1;
            g.movement();
            g.garrison_step();
        }
    }
    #[test]
    fn capacity_uses_one_floor_and_each_soldier_reserves_a_window() {
        let mut g = field();
        let building = house(&mut g);
        let capacity = g.window_slots(&g.entities[0], Some(0)).len();
        let mut small = g.entities[0].view.lots[0].clone();
        small.width = 4.8;
        assert!(small.windows().len() < capacity);
        small.width = crate::settlement_layout::HOUSE_PLOT;
        small.height = 5.4;
        assert_eq!(small.windows().len(), capacity);
        assert!(small.windows().iter().all(|s| s.1 == 1.5));
        let ids: Vec<_> = (0..capacity + 2)
            .map(|n| g.spawn("short", 0, [66. + n as f32, 78.]))
            .collect();
        let tank = g.spawn("medium", 0, [80., 80.]);
        assert_eq!(
            g.enter_building(0, &ids, &building, Some([70., 70.])),
            capacity
        );
        assert_eq!(g.enter_building(0, &[tank], &building, None), 0);
        let enemy = g.spawn("short", 1, [70., 60.]);
        assert_eq!(g.enter_building(1, &[enemy], &building, None), 0);
        let seats: std::collections::BTreeSet<_> = g
            .entities
            .iter()
            .filter_map(|e| e.view.garrison.as_ref().map(|g| g.window))
            .collect();
        assert_eq!(seats.len(), capacity);
        g.validate().unwrap();
    }
    #[test]
    fn occupants_switch_to_a_free_window_facing_the_enemy_and_survive_save_load() {
        let mut g = field();
        let building = house(&mut g);
        let rifle = g.spawn("short", 0, [70., 78.]);
        assert_eq!(g.enter_building(0, &[rifle], &building, None), 1);
        settle(&mut g);
        let old = g.entities[1].view.garrison.clone().unwrap();
        assert!(old.entered);
        g.spawn("short", 1, [70., 54.]);
        assert!(!g.firing_line(&g.entities[1], &g.entities[2]));
        g.tick = 260;
        g.garrison_step();
        let new = g.entities[1].view.garrison.clone().unwrap();
        assert_ne!(old.window, new.window);
        assert!(!g.firing_line(&g.entities[1], &g.entities[2]));
        g.tick = new.ready_at;
        g.garrison_step();
        assert!(g.firing_line(&g.entities[1], &g.entities[2]));
        let bytes = serde_json::to_vec(&g).unwrap();
        let mut restored: NativeGame = serde_json::from_slice(&bytes).unwrap();
        restored.terrain.hydrate();
        restored.refresh_towns();
        restored.validate().unwrap();
        assert_eq!(
            restored.entities[1].view.garrison,
            g.entities[1].view.garrison
        );
        assert!(restored.leave_building(1, false));
        settle(&mut restored);
        assert!(restored.entities[1].view.garrison.is_none());
        assert!(restored.passable(position(&restored.entities[1]), false));
    }
    #[test]
    fn houses_and_facilities_take_repeated_shells_but_sufficient_damage_collapses_in_one_hit() {
        let mut g = field();
        let building = house(&mut g);
        let rifle = g.spawn("short", 1, [70., 78.]);
        g.enter_building(1, &[rifle], &building, None);
        settle(&mut g);
        let mut shell = Projectile {
            owner: 0,
            from: [50., 70.],
            to: [70., 70.],
            born: 0,
            impact: 0,
            damage: 2.,
            radius: 0.3,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        };
        g.impact(&shell);
        assert!(g.entities[0].view.lots[0].integrity > 0.);
        shell.damage = g.entities[0].view.lots[0].max_hp();
        g.impact(&shell);
        g.release_collapsed_garrisons();
        assert_eq!(g.entities[0].view.lots[0].integrity, 0.);
        assert!(g.entities[1].view.garrison.is_none());
        assert!(g.passable(position(&g.entities[1]), false));
        g.entities.clear();
        g.spawn("barracks", 1, [70., 70.]);
        shell.damage = 2.;
        shell.radius = 3.;
        g.impact(&shell);
        assert!(g.entities[0].view.hp > 0. && g.entities[0].view.hp < g.entities[0].view.max_hp);
        shell.damage = 100.;
        g.impact(&shell);
        assert!(g.entities[0].view.hp <= 0.);
    }
    #[test]
    fn direct_fire_is_blocked_by_a_house_and_artillery_can_fire_over_it() {
        let mut g = field();
        house(&mut g);
        g.spawn("short", 0, [70., 60.]);
        g.spawn("short", 1, [70., 80.]);
        assert!(!g.firing_line(&g.entities[1], &g.entities[2]));
        g.entities[1].view.kind = "long".into();
        assert!(g.firing_line(&g.entities[1], &g.entities[2]));
    }
    #[test]
    fn sight_is_independent_of_configured_weapon_range_and_fog_only_hides_enemy_units() {
        let mut g = field();
        spotting_tests::open_sight(&mut g);
        g.config["fogEnabled"] = json!(true);
        g.config["shortRange"] = json!(50.);
        g.spawn("short", 0, [30., 30.]);
        g.spawn("short", 1, [81., 30.]);
        g.spawn("barracks", 1, [120., 120.]);
        assert_eq!(g.unit_sight(&g.entities[0]), 32.);
        assert_eq!(g.snapshot().entities[0].sight, 32.);
        assert!(!g.visible(0, &g.entities[1]));
        assert!(g.visible(0, &g.entities[2]));
        g.entities[1].view.x = 60.;
        assert!(g.visible(0, &g.entities[1]));
        assert_eq!(g.snapshot().props.len(), g.props.len());
        let building = g.entities[2].view.id.clone();
        let occupant = g.spawn("short", 1, [120., 130.]);
        assert_eq!(g.enter_building(1, &[occupant], &building, None), 1);
        assert_eq!(
            g.snapshot()
                .entities
                .iter()
                .find(|e| e.id == building)
                .unwrap()
                .occupants,
            0
        );
    }
    #[test]
    fn mixed_squads_persist_and_cannot_assign_enemy_units() {
        let mut g = field();
        let tank = g.spawn("medium", 0, [30., 30.]);
        let medic = g.spawn("medic", 0, [35., 30.]);
        let engineer = g.spawn("engineer", 0, [40., 30.]);
        let enemy = g.spawn("short", 1, [60., 30.]);
        g.command_as(0,json!({"type":"command","commandType":"assign-squad","squad":2,"unitIds":[tank,medic,engineer,enemy]}));
        assert!(g.entities[..3].iter().all(|e| e.view.squad == Some(2)));
        assert_eq!(g.entities[3].view.squad, None);
        let restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert!(
            restored.entities[..3]
                .iter()
                .all(|e| e.view.squad == Some(2))
        );
    }
    #[test]
    fn turning_is_bounded_by_vehicle_type_and_infantry_falls_back_without_losing_orders() {
        let mut g = field();
        let tank = g.spawn("medium", 0, [30., 30.]);
        g.issue(
            0,
            &[tank],
            Order {
                kind: "move".into(),
                destination: Some([20., 30.]),
                target: None,
            },
            false,
        );
        g.movement();
        assert!(
            g.entities[0].view.heading.abs()
                <= turn_rate("medium", g.entities[0].view.model_scale) * DT + 0.001
        );
        assert!(distance(position(&g.entities[0]), [30., 30.]) < 0.001);
        assert!(turn_rate("xtralong", 2.25) < turn_rate("troopCarrier", 2.25));
        g.entities.clear();
        let rifle = g.spawn("short", 0, [40., 40.]);
        g.spawn("short", 1, [30., 40.]);
        g.issue(
            0,
            &[rifle],
            Order {
                kind: "attack-move".into(),
                destination: Some([30., 40.]),
                target: None,
            },
            false,
        );
        g.entities[0].view.suppression = 0.9;
        g.entities[0].threat = Some([30., 40.]);
        g.entities[0].threat_until = 200;
        g.tick = 100;
        g.infantry_manoeuvres();
        assert_eq!(g.entities[0].orders.front().unwrap().kind, "attack-move");
        assert!(g.entities[0].tactical_until > g.tick);
        assert!(distance(*g.entities[0].path.back().unwrap(), [30., 40.]) > 10.);
    }
}

type WindowSlot = (Point, f32, Point);
type Doorway = (Point, Point, f32);

/// Ground-level entries. Watchtower occupants climb the front ladder.
pub fn facility_door(kind: &str) -> Option<Point> {
    match kind {
        "barracks" => Some([-2.82, 3.2]),
        "fieldHospital" => Some([-2.52, 3.7]),
        "vehicleDepot" => Some([-2.8, 5.2]),
        "mine" => Some([-2.22, 3.2]),
        "watchtower" => Some([0., 1.7]),
        "airfield" => Some([-14., -28.6]),
        _ => None,
    }
}

/// Model-local facility windows, matching military_models.rs.
pub fn facility_windows(kind: &str) -> Vec<WindowSlot> {
    let mut slots = vec![];
    if kind == "airfield" {
        for x in [-9., -5.] {
            for dx in [-0.28, 0., 0.28] {
                slots.push(([-14. + (x + dx * 2.7 + 7.) * 1.6, -28.312], 2.88, [0., 1.]));
            }
        }
        return slots;
    }
    if kind == "watchtower" {
        for side in [-1., 1.] {
            for dx in [-0.28, 0., 0.28] {
                slots.push(([dx * 3., side * 1.78], 5.6, [0., side]));
            }
        }
        return slots;
    }
    let (w, d) = match kind {
        "barracks" => (9.4, 6.4),
        "fieldHospital" => (8.4, 7.4),
        "vehicleDepot" => (11.4, 10.4),
        "mine" => (7.4, 6.4),
        _ => return slots,
    };
    for side in [-1., 1.] {
        if kind != "vehicleDepot" || side < 0. {
            for dx in [-0.28, 0., 0.28] {
                slots.push((
                    [w * 0.16 + dx * w * 0.65, side * (d * 0.5 + 0.18)],
                    1.7,
                    [0., side],
                ));
            }
        }
        for z in [-d * 0.28, 0., d * 0.28] {
            slots.push(([side * (w * 0.5 + 0.18), z], 1.7, [side, 0.]));
        }
    }
    slots
}

impl NativeGame {
    fn can_target_structure(&self, shooter: &Entity, target: &Entity) -> bool {
        if !target.view.building
            || target.view.lots.is_empty()
            || shooter.view.category != "vehicle"
        {
            return true;
        }
        shooter
            .orders
            .front()
            .is_some_and(|o| o.kind == "attack" && o.target.as_deref() == Some(&target.view.id))
    }
    pub fn smoke_town_entry(&mut self, settle: bool) -> Vec<String> {
        *self = Self::default();
        self.entities.retain(|e| e.view.kind == "town");
        self.props.clear();
        self.bases.clear();
        self.training = Some(TrainingGround::default());
        self.human_opponent = true;
        self.config["fogEnabled"] = json!(false);
        let building = self.entities[0].view.id.clone();
        let mut selected = vec![];
        let mut expected = vec![];
        for lot in 0..self.entities[0].view.lots.len() {
            if selected.len() == 3 {
                break;
            }
            let reservation = Garrison {
                building: building.clone(),
                lot: Some(lot),
                window: 0,
                entered: false,
                ready_at: 0,
                door_stage: 0,
                exiting: false,
            };
            let Some((outside, inside, _)) = self.garrison_door(&reservation) else {
                continue;
            };
            let d = distance(outside, inside);
            let normal = [(outside[0] - inside[0]) / d, (outside[1] - inside[1]) / d];
            let start = [outside[0] - normal[0] * 14., outside[1] - normal[1] * 14.];
            if !self.passable(start, false) {
                continue;
            }
            let click = building_world(&self.entities[0], self.entities[0].view.lots[lot].at);
            let id = self.spawn("short", 0, start);
            if self.enter_building(0, std::slice::from_ref(&id), &building, Some(click)) == 0 {
                self.entities.pop();
                continue;
            }
            let unit = self.entities.last().unwrap();
            assert_eq!(unit.view.garrison.as_ref().unwrap().lot, Some(lot));
            let mut from = start;
            for &to in &unit.path {
                assert!(self.clear_segment_radius(from, to, 0, 0.));
                from = to;
            }
            expected.push((id.clone(), lot));
            selected.push(id);
        }
        assert_eq!(
            selected.len(),
            3,
            "actual town must offer three reachable selected houses"
        );
        for _ in 0..if settle { 1600 } else { 20 } {
            self.step();
        }
        for (id, lot) in expected {
            let state = self
                .entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .garrison
                .as_ref()
                .unwrap();
            assert_eq!(state.lot, Some(lot));
            if settle {
                assert!(state.entered, "town soldier failed to enter house {lot}");
            }
        }
        self.paused = true;
        selected
    }
    fn unit_sight(&self, e: &Entity) -> f32 {
        // Shared by AI detection, player snapshots, sight rings and the HUD.
        // Existing saves gain the same proportional observation on their maps.
        let span=if self.terrain.id==crate::battlefield::ID || self.terrain.id==crate::gis_prototype::ID {self.terrain.cols.min(self.terrain.rows)} else {self.terrain.cols.max(self.terrain.rows)};
        let scale = (span as f32 / 512.).clamp(1., 4.);
        sight_range(&e.view.kind) * scale
    }
    fn window_slots(&self, building: &Entity, lot: Option<usize>) -> Vec<WindowSlot> {
        if building.view.hp <= 0. {
            return vec![];
        }
        if let Some(lot) = lot {
            let Some(lot) = building.view.lots.get(lot).filter(|l| l.integrity > 0.) else {
                return vec![];
            };
            return lot
                .windows()
                .into_iter()
                .map(|(p, y, n)| {
                    (
                        building_world(building, p),
                        building.view.elevation + (lot.ground + y) * building.view.model_scale,
                        {
                            let (s, c) = building.view.heading.sin_cos();
                            [c * n[0] + s * n[1], -s * n[0] + c * n[1]]
                        },
                    )
                })
                .collect();
        }
        let (s, c) = building.view.heading.sin_cos();
        let scale = building.view.model_scale;
        facility_windows(&building.view.kind)
            .into_iter()
            .map(|(p, y, n)| {
                (
                    [
                        building.view.x + (c * p[0] + s * p[1]) * scale,
                        building.view.z + (-s * p[0] + c * p[1]) * scale,
                    ],
                    building.view.elevation + y * scale,
                    [c * n[0] + s * n[1], -s * n[0] + c * n[1]],
                )
            })
            .collect()
    }
    fn garrison_slot(&self, g: &Garrison) -> Option<WindowSlot> {
        let building = self.entities.iter().find(|e| e.view.id == g.building)?;
        self.window_slots(building, g.lot).get(g.window).copied()
    }
    fn window_free(
        &self,
        building: &str,
        lot: Option<usize>,
        window: usize,
        except: usize,
    ) -> bool {
        !self.entities.iter().enumerate().any(|(i, e)| {
            i != except
                && e.view.hp > 0.
                && !e.view.downed
                && e.view
                    .garrison
                    .as_ref()
                    .is_some_and(|g| g.building == building && g.lot == lot && g.window == window)
        })
    }
    fn garrison_door(&self, g: &Garrison) -> Option<Doorway> {
        let b = self
            .entities
            .iter()
            .find(|e| e.view.id == g.building && e.view.hp > 0.)?;
        let (at, normal, floor) = if let Some(lot) = g.lot {
            let lot = b.view.lots.get(lot).filter(|l| l.integrity > 0.)?;
            let (at, n) = lot.door();
            (
                building_world(b, at),
                {
                    let (s, c) = b.view.heading.sin_cos();
                    [c * n[0] + s * n[1], -s * n[0] + c * n[1]]
                },
                b.view.elevation + (lot.ground + 0.2) * b.view.model_scale,
            )
        } else {
            let p = facility_door(&b.view.kind)?;
            let (s, c) = b.view.heading.sin_cos();
            let scale = b.view.model_scale;
            (
                [
                    b.view.x + (c * p[0] + s * p[1]) * scale,
                    b.view.z + (-s * p[0] + c * p[1]) * scale,
                ],
                [s, c],
                b.view.elevation + 0.2 * scale,
            )
        };
        Some((
            [at[0] + normal[0] * 1.5, at[1] + normal[1] * 1.5],
            [at[0] - normal[0] * 0.8, at[1] - normal[1] * 0.8],
            floor,
        ))
    }
    fn garrison_entry(&self, index: usize, g: &Garrison) -> Option<(Point, VecDeque<Point>)> {
        let (goal, _, _) = self.garrison_door(g)?;
        if !self.passable(goal, false) {
            return None;
        }
        let mut path = self.route(index, position(&self.entities[index]), goal);
        if path.is_empty() && distance(position(&self.entities[index]), goal) > 0.3 {
            return None;
        }
        if !path.is_empty() && path.back().is_some_and(|last| distance(*last, goal) > 0.01) {
            path.push_back(goal);
        }
        Some((goal, path))
    }
    fn enter_building(
        &mut self,
        owner: i32,
        ids: &[String],
        target: &str,
        at: Option<Point>,
    ) -> usize {
        let Some(b) = self.entities.iter().position(|e| {
            e.view.id == target
                && e.view.building
                && e.view.hp > 0.
                && (e.view.owner == owner || e.view.owner < 0)
        }) else {
            return 0;
        };
        let click = at
            .or_else(|| {
                self.entities
                    .iter()
                    .find(|e| ids.contains(&e.view.id))
                    .map(position)
            })
            .unwrap_or(position(&self.entities[b]));
        let local = building_local(&self.entities[b], click);
        let mut lots: Vec<_> = if self.entities[b].view.lots.is_empty() {
            vec![None]
        } else {
            (0..self.entities[b].view.lots.len()).map(Some).collect()
        };
        lots.sort_by(|a, b_lot| {
            let d = |lot: Option<usize>| {
                lot.map(|i| distance(local, self.entities[b].view.lots[i].nearest(local)))
                    .unwrap_or(0.)
            };
            d(*a).total_cmp(&d(*b_lot))
        });
        // An explicit house click never spills the squad into other houses.
        if at.is_some() {
            lots.truncate(1);
        }
        let mut count = 0;
        for id in ids {
            let Some(i) = self.entities.iter().position(|e| {
                &e.view.id == id
                    && e.view.owner == owner
                    && e.view.category == "troop"
                    && !e.view.downed
                    && e.view.hp > 0.
                    && e.transport.is_none()
                    && number(spec(&e.view.kind), "speed", 0.) > 0.
            }) else {
                continue;
            };
            if self.entities[i].view.garrison.is_some() && !self.leave_building(i, false) {
                continue;
            }
            let mut chosen = None;
            for &lot in &lots {
                if self.entities.iter().any(|e| {
                    e.view.owner != owner
                        && e.view.hp > 0.
                        && e.view
                            .garrison
                            .as_ref()
                            .is_some_and(|g| g.building == target && g.lot == lot)
                }) {
                    continue;
                }
                let slots = self.window_slots(&self.entities[b], lot);
                let mut windows: Vec<_> = (0..slots.len()).collect();
                windows.sort_by(|a, b| {
                    distance(slots[*a].0, position(&self.entities[i]))
                        .total_cmp(&distance(slots[*b].0, position(&self.entities[i])))
                });
                for window in windows {
                    if self.window_free(target, lot, window, i)
                        && let Some((goal, path)) = self.garrison_entry(
                            i,
                            &Garrison {
                                building: target.into(),
                                lot,
                                window,
                                entered: false,
                                ready_at: 0,
                                door_stage: 0,
                                exiting: false,
                            },
                        )
                    {
                        chosen = Some((lot, window, goal, path));
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let Some((lot, window, goal, path)) = chosen else {
                continue;
            };
            let e = &mut self.entities[i];
            e.cancel_local_actions();
            e.release_stop();
            e.view.garrison = Some(Garrison {
                building: target.into(),
                lot,
                window,
                entered: false,
                ready_at: 0,
                door_stage: 0,
                exiting: false,
            });
            e.view.retreating = false;
            e.cover_goal = None;
            e.cover_ordered = false;
            e.warfare = InfantryWarfare::default();
            e.tactical_until = 0;
            e.medical_until = 0;
            e.treatment_target = None;
            e.patrol_origin = None;
            e.speed = 0.;
            e.target = None;
            e.stance = "attack".into();
            e.orders = VecDeque::from([Order {
                kind: "garrison".into(),
                destination: Some(goal),
                target: None,
            }]);
            e.path = path;
            count += 1;
        }
        count
    }
    fn leave_building(&mut self, index: usize, collapsed: bool) -> bool {
        let Some(g) = self.entities[index].view.garrison.clone() else {
            return true;
        };
        if !g.entered && g.door_stage == 1 && !collapsed {
            let e = &mut self.entities[index];
            e.view.garrison.as_mut().unwrap().exiting = true;
            e.release_stop();
            e.orders.clear();
            e.path.clear();
            e.target = None;
            e.speed = 0.;
            return true;
        }
        if g.entered
            && !collapsed
            && !self.entities[index].view.downed
            && self.entities[index].view.hp > 0.
        {
            if self.garrison_door(&g).is_none() {
                return false;
            }
            let e = &mut self.entities[index];
            e.release_stop();
            let g = e.view.garrison.as_mut().unwrap();
            g.exiting = true;
            g.door_stage = 2;
            e.orders.clear();
            e.path.clear();
            e.target = None;
            e.speed = 0.;
            return true;
        }
        let origin = position(&self.entities[index]);
        let exit = if !g.entered {
            Some(origin)
        } else {
            self.garrison_door(&g)
                .map(|d| d.0)
                .filter(|p| self.passable(*p, false))
                .or_else(|| self.open_land(origin, &self.entities[index].view.kind))
        };
        let Some(exit) = exit else {
            return false;
        };
        let e = &mut self.entities[index];
        e.view.garrison = None;
        e.view.x = exit[0];
        e.view.z = exit[1];
        e.view.elevation = self.terrain.height(exit[0], exit[1]);
        e.orders.clear();
        e.path.clear();
        e.speed = 0.;
        e.view.velocity = [0., 0.];
        if collapsed {
            e.view.suppression = 1.;
            e.last_hit = self.tick;
            damage(&mut e.view, 1.);
            if e.view.hp <= 0.
                && !e.view.downed
                && !e.view.training_target
                && ["short", "scout", "engineer"].contains(&e.view.kind.as_str())
            {
                e.view.downed = true;
                e.view.hp = 0.01;
                e.view.strength = 0;
                e.downed_until = self.tick + 400;
            }
        }
        true
    }
    fn release_collapsed_garrisons(&mut self) {
        for i in 0..self.entities.len() {
            if self.entities[i].view.garrison.as_ref().is_some_and(|g| {
                self.garrison_slot(g).is_none()
                    || self.entities[i].view.downed
                    || self.entities[i].view.hp <= 0.
            }) {
                let collapsed = self.entities[i]
                    .view
                    .garrison
                    .as_ref()
                    .is_some_and(|g| self.garrison_slot(g).is_none());
                self.leave_building(i, collapsed);
            }
        }
    }
    fn garrison_step(&mut self) {
        self.release_collapsed_garrisons();
        for i in 0..self.entities.len() {
            let Some(mut g) = self.entities[i].view.garrison.clone() else {
                continue;
            };
            if g.exiting {
                let Some((outside, inside, floor)) = self.garrison_door(&g) else {
                    continue;
                };
                let goal = if g.door_stage == 2 { inside } else { outside };
                let height = if g.door_stage == 2 {
                    floor
                } else {
                    self.terrain.height(outside[0], outside[1])
                };
                if self.walk_doorway(i, goal, height) {
                    if g.door_stage == 2 {
                        g.door_stage = 1;
                    } else {
                        self.entities[i].view.garrison = None;
                        continue;
                    }
                }
                self.entities[i].view.garrison = Some(g);
                continue;
            }
            if !g.entered {
                let Some((outside, inside, floor)) = self.garrison_door(&g) else {
                    continue;
                };
                if g.door_stage == 0 && distance(position(&self.entities[i]), outside) > 0.4 {
                    if self.entities[i].path.is_empty() && (self.tick + i as u64).is_multiple_of(20)
                    {
                        self.entities[i].path = self.route(i, position(&self.entities[i]), outside);
                    }
                    continue;
                }
                if g.door_stage == 0 && !self.entities[i].path.is_empty() {
                    continue;
                }
                if g.door_stage == 0
                    && self.entities.iter().enumerate().any(|(j, e)| {
                        j != i
                            && e.view.garrison.as_ref().is_some_and(|other| {
                                other.building == g.building
                                    && other.lot == g.lot
                                    && other.door_stage == 1
                            })
                    })
                {
                    continue;
                }
                g.door_stage = 1;
                if !self.walk_doorway(i, inside, floor) {
                    self.entities[i].view.garrison = Some(g);
                    continue;
                }
                g.door_stage = 2;
                g.entered = true;
                g.ready_at = self.tick
                    + self
                        .garrison_slot(&g)
                        .map(|s| {
                            (distance(position(&self.entities[i]), s.0) / 2.4 / DT)
                                .ceil()
                                .max(20.) as u64
                        })
                        .unwrap_or(20);
                self.entities[i].orders.clear();
            }
            let Some(mut slot) = self.garrison_slot(&g) else {
                continue;
            };
            if self.tick >= g.ready_at && self.tick.is_multiple_of(20) {
                let target = self.entity_threat(&self.entities[i]);
                if let Some(target) = target {
                    let b = self
                        .entities
                        .iter()
                        .find(|e| e.view.id == g.building)
                        .unwrap();
                    let slots = self.window_slots(b, g.lot);
                    let score = |s: WindowSlot| {
                        let d = distance(s.0, target).max(0.01);
                        let facing =
                            ((target[0] - s.0[0]) * s.2[0] + (target[1] - s.0[1]) * s.2[1]) / d;
                        if facing < 0.15
                            || !self.building_line_clear(s.0, target, Some(&g.building), None)
                        {
                            return -1000.;
                        }
                        facing * 10. - d * 0.01
                    };
                    let best = slots
                        .iter()
                        .enumerate()
                        .filter(|(w, _)| self.window_free(&g.building, g.lot, *w, i))
                        .max_by(|a, b| score(*a.1).total_cmp(&score(*b.1)));
                    if let Some((w, s)) = best
                        && score(*s) > score(slot) + 1.
                    {
                        g.window = w;
                        g.ready_at = self.tick
                            + (distance(position(&self.entities[i]), s.0) / 2.4 / DT)
                                .ceil()
                                .max(20.) as u64;
                        slot = *s;
                    }
                }
            }
            // Movement between firing positions takes time; no firing while changing windows.
            let e = &mut self.entities[i];
            let at = position(e);
            let d = distance(at, slot.0);
            let t = if self.tick >= g.ready_at {
                1.
            } else {
                (2.4 * DT / d.max(0.01)).min(1.)
            };
            e.view.x += (slot.0[0] - at[0]) * t;
            e.view.z += (slot.0[1] - at[1]) * t;
            e.view.velocity = [(e.view.x - at[0]) / DT, (e.view.z - at[1]) / DT];
            e.view.elevation = slot.1 - 1.27 * e.view.model_scale;
            e.view.heading = slot.2[1].atan2(slot.2[0]);
            e.speed = 0.;
            e.stationary += 1;
            e.view.garrison = Some(g);
        }
    }
    fn walk_doorway(&mut self, index: usize, goal: Point, floor: f32) -> bool {
        let e = &mut self.entities[index];
        let at = position(e);
        let d = distance(at, goal);
        let t = (2.4 * DT / d.max(0.001)).min(1.);
        e.view.x += (goal[0] - at[0]) * t;
        e.view.z += (goal[1] - at[1]) * t;
        e.view.velocity = [(e.view.x - at[0]) / DT, (e.view.z - at[1]) / DT];
        e.view.elevation += (floor - e.view.elevation).clamp(-2.4 * DT, 2.4 * DT);
        if d > 0.01 {
            e.view.heading = (goal[1] - at[1]).atan2(goal[0] - at[0]);
        }
        e.speed = 2.4;
        e.stationary = 0;
        d <= 2.4 * DT + 0.01
    }
    fn building_line_clear(
        &self,
        from: Point,
        to: Point,
        source: Option<&str>,
        target: Option<&str>,
    ) -> bool {
        !self.entities.iter().any(|e| {
            if !e.view.building
                || e.view.hp <= 0.
                || (Some(e.view.id.as_str()) == source && e.view.lots.is_empty())
                || Some(e.view.id.as_str()) == target && e.view.lots.is_empty()
                || ["objective", "bridge"].contains(&e.view.kind.as_str())
            {
                return false;
            }
            if !e.view.lots.is_empty() {
                let a = building_local(e, from);
                let b = building_local(e, to);
                return e.view.lots.iter().any(|l| {
                    !(Some(e.view.id.as_str()) == target && distance(b, l.nearest(b)) < 0.05)
                        && l.intersects_segment(a, b, 0.)
                });
            }
            let (sin, cos) = e.view.heading.sin_cos();
            let scale = e.view.model_scale.max(0.1);
            let crosses = |at, half| NavObstacle { at, half, sin, cos }.crosses(from, to, 0.);
            if e.view.kind == "airfield" {
                crate::airfield_layout::SOLIDS
                    .into_iter()
                    .any(|(at, half)| {
                        crosses(
                            crate::airfield_layout::world(&e.view, at),
                            half.map(|v| v * scale),
                        )
                    })
            } else {
                crosses(
                    position(e),
                    [
                        number(spec(&e.view.kind), "footprintWidth", 3.6) * scale * 0.5,
                        number(spec(&e.view.kind), "footprintDepth", 3.6) * scale * 0.5,
                    ],
                )
            }
        })
    }
    fn firing_line(&self, shooter: &Entity, target: &Entity) -> bool {
        if shooter
            .view
            .garrison
            .as_ref()
            .is_some_and(|g| !g.entered || g.exiting)
        {
            return false;
        }
        let at = position(shooter);
        let to = aim_point(shooter, target, at);
        let g = shooter.view.garrison.as_ref().filter(|g| g.entered);
        if let Some(g) = g {
            if self.tick < g.ready_at {
                return false;
            }
            let Some(slot) = self.garrison_slot(g) else {
                return false;
            };
            if ((to[0] - at[0]) * slot.2[0] + (to[1] - at[1]) * slot.2[1])
                / distance(at, to).max(0.01)
                < 0.15
            {
                return false;
            }
        }
        ["long", "xtralong", "plane"].contains(&shooter.view.kind.as_str())
            || (self.terrain_sight_clear(Vec3::new(at[0],self.sight_height(shooter,true),at[1]),Vec3::new(to[0],self.sight_height(target,false),to[1])) && self.building_line_clear(
                at,
                to,
                g.map(|g| g.building.as_str()),
                Some(&target.view.id),
            ))
    }
}
