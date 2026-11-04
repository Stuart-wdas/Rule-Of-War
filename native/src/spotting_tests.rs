#[cfg(test)]
mod spotting_tests {
    use super::*;
    // Radius/ordering regressions need an open field; physical occlusion is tested below.
    pub(super) fn open_sight(g: &mut NativeGame) {
        g.terrain.authored = vec![0.52; (g.terrain.cols * g.terrain.rows) as usize];
        g.terrain.authored_scale = 12.;
        g.terrain.edits.clear();
        g.terrain.hydrate();
        plants(g, vec![]);
    }
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("spotting-check", 512, 256);
        g.terrain.authored = vec![0.52; 512 * 256];
        g.terrain.hydrate();
        g.config["fogEnabled"] = json!(true);
        g.tick = 100;
        plants(&g, vec![]);
        g
    }
    fn plants(g: &NativeGame, plants: Vec<SightPlant>) {
        let mut vegetation = SightVegetation::default();
        for p in plants {
            vegetation
                .cells
                .entry(((p.at[0] / 8.).floor() as i32, (p.at[1] / 8.).floor() as i32))
                .or_default()
                .push(p);
        }
        *g.spotting.0.lock().unwrap() = Some((
            crate::terrain::render_source_key(&g.terrain),
            Arc::new(vegetation),
        ));
    }
    fn house(g: &mut NativeGame) {
        g.spawn("village", -1, [70., 70.]);
        let ground = 0.;
        g.entities.last_mut().unwrap().view.lots = vec![crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 10.,
            depth: 10.,
            height: 3.2,
            yaw: 0.,
            style: 0,
            ground,
            integrity: 1.,
        }];
    }
    #[test]
    fn requested_shooting_ranges_preserve_every_authored_sight_radius() {
        let mut g = field();
        g.terrain.id = crate::battlefield::ID.into();
        g.terrain.cols = 2048;
        g.terrain.rows = 1024;
        for (kind, weapon, sight) in [
            ("short", 45., 64.),
            ("medium", 100., 144.),
            ("long", 200., 288.),
            ("xtralong", 310., 420.),
            ("plane", 200., 200.),
            ("antiAir", 200., 224.),
            ("engineer", 32., 192.),
            ("scout", 110., 224.),
            ("troopCarrier", 0., 160.),
        ] {
            g.spawn(kind, 0, [50., 50.]);
            assert_eq!(g.weapon_range(kind), weapon, "{kind}");
            assert_eq!(g.unit_sight(g.entities.last().unwrap()), sight, "{kind}");
        }
    }
    #[test]
    fn default_save_ranges_upgrade_once_without_overwriting_custom_balance() {
        let mut g = field();
        g.config
            .as_object_mut()
            .unwrap()
            .remove("weaponRangeRevision");
        for (key, old) in [
            ("shortRange", 32.),
            ("mediumRange", 72.),
            ("longRange", 144.),
            ("xtralongRange", 210.),
        ] {
            g.config[key] = json!(old);
        }
        g.config["mediumRange"] = json!(95.);
        g.upgrade_weapon_ranges();
        assert_eq!(g.weapon_range("short"), 45.);
        assert_eq!(g.weapon_range("medium"), 95.);
        assert_eq!(g.weapon_range("long"), 200.);
        assert_eq!(g.weapon_range("xtralong"), 310.);
        g.config["shortRange"] = json!(32.);
        g.upgrade_weapon_ranges();
        assert_eq!(g.weapon_range("short"), 32.);
    }
    #[test]
    fn both_teams_use_the_new_weapon_limits_in_actual_combat() {
        for owner in [0, 1] {
            for (kind, range) in [
                ("short", 45.),
                ("medium", 100.),
                ("long", 200.),
                ("xtralong", 310.),
            ] {
                for (offset, shoots) in [(-1., true), (1., false)] {
                    let mut g = field();
                    g.config["fogEnabled"] = json!(false);
                    g.spawn(kind, owner, [40., 90.]);
                    g.spawn("short", 1 - owner, [40. + range + offset, 90.]);
                    g.entities[0].stationary = 1000;
                    g.entities[1].stance = "hold-fire".into();
                    g.entities[1].view.hp = 10000.;
                    for _ in 0..80 {
                        g.tick += 1;
                        g.combat();
                    }
                    assert_eq!(
                        g.events
                            .iter()
                            .any(|e| e.kind == "shot" && e.owner == owner),
                        shoots,
                        "{kind} at {}m",
                        range + offset
                    );
                }
            }
        }
    }
    #[test]
    fn buildings_hide_infantry_and_vehicles_until_the_sight_line_clears() {
        for owner in [0, 1] {
            for kind in ["short", "medium"] {
                let mut g = field();
                house(&mut g);
                g.spawn(kind, owner, [56., 70.]);
                let enemy = g.spawn(kind, 1 - owner, [84., 70.]);
                assert!(!g.visible(owner, &g.entities[2]), "{kind}");
                assert!(!g.point_visible(owner, [84., 70.]));
                assert!(!g.snapshot_for(owner).entities.iter().any(|p| p.id == enemy));
                assert!(!g.firing_line(&g.entities[1], &g.entities[2]));
                g.entities[2].last_shot = g.tick;
                assert!(
                    !g.visible(owner, &g.entities[2]),
                    "firing cannot reveal through a house"
                );
                g.entities[0].view.lots[0].integrity = 0.;
                assert!(g.visible(owner, &g.entities[2]));
                assert!(g.firing_line(&g.entities[1], &g.entities[2]));
            }
        }
    }
    #[test]
    fn hills_block_ground_spotting_and_direct_fire_but_aircraft_can_see_over_them() {
        let mut g = field();
        for z in 0..256 {
            for x in 68..=72 {
                g.terrain.authored[z * 512 + x] = 0.70;
            }
        }
        g.spawn("medium", 0, [56., 70.]);
        g.spawn("short", 1, [84., 70.]);
        assert!(!g.visible(0, &g.entities[1]));
        assert!(!g.firing_line(&g.entities[0], &g.entities[1]));
        g.spawn("helicopter", 0, [56., 70.]);
        g.entities[2].view.airborne = true;
        g.entities[2].view.altitude = Some(80.);
        assert!(g.visible(0, &g.entities[1]));
        g.entities[0].view.kind = "long".into();
        assert!(g.firing_line(&g.entities[0], &g.entities[1]));
    }
    #[test]
    fn an_elevated_observer_can_see_over_a_roof() {
        let mut g = field();
        house(&mut g);
        g.spawn("helicopter", 0, [56., 70.]);
        g.spawn("short", 1, [84., 70.]);
        g.entities[1].view.airborne = true;
        g.entities[1].view.altitude = Some(30.);
        assert!(g.visible(0, &g.entities[2]));
        g.entities[1].view.altitude = Some(1.);
        assert!(!g.visible(0, &g.entities[2]));
    }
    #[test]
    fn hedges_conceal_infantry_but_do_not_stop_bullets_or_hide_taller_vehicles() {
        for owner in [0, 1] {
            let mut g = field();
            plants(
                &g,
                vec![SightPlant {
                    at: [70., 70.],
                    radius: 1.3,
                    bottom: 0.,
                    top: 2.,
                    density: 1.1,
                }],
            );
            g.spawn("short", owner, [56., 70.]);
            g.spawn("short", 1 - owner, [84., 70.]);
            assert!(!g.visible(owner, &g.entities[1]));
            assert!(g.firing_line(&g.entities[0], &g.entities[1]));
            g.spawn("short", owner, [84., 76.]);
            assert!(g.visible(owner, &g.entities[1]));
            g.entities.pop();
            g.entities[0].view.kind = "medium".into();
            g.entities[0].view.category = "vehicle".into();
            g.entities[0].view.model_scale = 2.25;
            g.entities[1].view.kind = "medium".into();
            g.entities[1].view.category = "vehicle".into();
            g.entities[1].view.model_scale = 2.25;
            assert!(g.visible(owner, &g.entities[1]));
        }
    }
    #[test]
    fn movement_and_firing_make_partly_concealed_units_easier_to_spot() {
        let mut g = field();
        plants(
            &g,
            vec![SightPlant {
                at: [70., 70.],
                radius: 1.1,
                bottom: 0.,
                top: 2.,
                density: 0.8,
            }],
        );
        g.spawn("short", 0, [56., 70.]);
        g.spawn("short", 1, [84., 70.]);
        assert!(!g.visible(0, &g.entities[1]));
        g.entities[1].speed = 1.;
        assert!(g.visible(0, &g.entities[1]));
        g.entities[1].speed = 0.;
        g.entities[1].last_shot = g.tick;
        assert!(g.visible(0, &g.entities[1]));
        g.tick += 61;
        assert!(!g.visible(0, &g.entities[1]));
    }
    #[test]
    fn dense_woodland_can_hide_a_vehicle_without_extending_any_observers_radius() {
        let mut g = field();
        plants(
            &g,
            (64..=76)
                .step_by(3)
                .map(|x| SightPlant {
                    at: [x as f32, 70.],
                    radius: 2.,
                    bottom: 0.6,
                    top: 6.,
                    density: 0.32,
                })
                .collect(),
        );
        g.spawn("medium", 0, [56., 70.]);
        g.spawn("medium", 1, [84., 70.]);
        assert!(!g.visible(0, &g.entities[1]));
        plants(&g, vec![]);
        assert!(g.visible(0, &g.entities[1]));
        g.entities[1].view.x = 129.;
        assert!(!g.visible(0, &g.entities[1]));
    }
    #[test]
    fn enemy_fire_behind_a_house_is_clipped_before_it_reaches_hidden_ground() {
        let mut g = field();
        house(&mut g);
        g.spawn("short", 0, [56., 70.]);
        let observation = g.observation();
        assert!(!observation.sees(0, [80., 70.]));
        let (start, end) = observation
            .segment_window(0, [80., 70.], [56., 70.])
            .unwrap();
        assert!(start > 0.3 && end > 0.99);
        assert!(
            observation
                .segment_window(0, [84., 70.], [80., 70.])
                .is_none()
        );
    }
}
