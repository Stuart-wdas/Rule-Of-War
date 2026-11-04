// Included in simulation.rs so this scenario uses the native gameplay systems.
impl NativeGame {
    fn gis_prototype(config: Value) -> Self {
        let mut game = Self::new();
        game.entities.clear();
        game.bases.clear();
        game.counter = 0;
        game.config = config;
        game.config["cols"] = json!(crate::gis_prototype::COLS);
        game.config["rows"] = json!(crate::gis_prototype::ROWS);
        game.config["fogEnabled"] = json!(false);
        game.terrain = crate::terrain::Terrain::new(
            crate::gis_prototype::ID,
            crate::gis_prototype::COLS,
            crate::gis_prototype::ROWS,
        );
        game.rng = 74129;
        game.supply = [150.; 2];
        // Group nearby houses only for scene batching; each retains its own
        // garrison windows, collision shape and independent lot integrity.
        let mut groups: std::collections::BTreeMap<
            (i32, i32),
            Vec<&crate::gis_prototype::Building>,
        > = std::collections::BTreeMap::new();
        for b in &crate::gis_prototype::map().buildings {
            groups
                .entry(((b.at[0] / 128.) as i32, (b.at[1] / 128.) as i32))
                .or_default()
                .push(b);
        }
        for ((x, z), buildings) in groups {
            let at = [x as f32 * 128. + 64., z as f32 * 128. + 64.];
            let lots: Vec<_> = buildings
                .into_iter()
                .map(|b| crate::settlement_layout::Lot {
                    at: [b.at[0] - at[0], b.at[1] - at[1]],
                    width: 10.,
                    depth: 10.,
                    height: 3.2,
                    yaw: b.yaw,
                    // Square/straight houses keep close source plots predictable.
                    style: if b.osm_id % 3 == 0 { 3 } else { 0 },
                    ground: 0.,
                    integrity: 1.,
                })
                .collect();
            game.grade_town_plots(at, &lots);
            let id = game.spawn("village", -1, at);
            let e = game.entities.iter_mut().find(|e| e.view.id == id).unwrap();
            e.view.model_scale = 1.;
            e.view.heading = 0.;
            e.view.label = format!("HOTTOT / BLOCK {}-{}", x + 1, z + 1);
            e.view.town_plan = crate::settlement_layout::TownPlan::Rural;
            e.view.lots = lots;
            e.view.town_blocks.clear();
        }
        game.refresh_towns();
        // Scenario deployments and objectives are authored gameplay additions.
        // No large airbase or town-wide grading overwrites the source landscape.
        for owner in [0, 1] {
            let x = if owner == 0 { 95. } else { 1950. };
            let z = if owner == 0 { 470. } else { 560. };
            for (kind, dx, dz) in [
                ("barracks", 0., -45.),
                ("vehicleDepot", 0., 0.),
                ("fieldHospital", 0., 45.),
            ] {
                if let Some(at) = game.open_land([x + dx, z + dz], kind) {
                    game.spawn(kind, owner, at);
                }
            }
            for (i, kind) in [
                "commander",
                "short",
                "short",
                "short",
                "short",
                "short",
                "short",
                "scout",
                "engineer",
                "medic",
                "medium",
                "troopCarrier",
            ]
            .into_iter()
            .enumerate()
            {
                let direction = if owner == 0 { 1. } else { -1. };
                let at = [
                    x + direction * (38. + (i % 3) as f32 * 12.),
                    z - 50. + (i / 3) as f32 * 22.,
                ];
                if let Some(at) = game.open_land(at, kind) {
                    game.spawn(kind, owner, at);
                    game.entities.last_mut().unwrap().view.heading =
                        if owner == 0 { 0. } else { std::f32::consts::PI };
                }
            }
        }
        for at in [[650., 580.], [1020., 360.], [1260., 420.]] {
            if let Some(at) = game.open_land(at, "objective") {
                game.spawn("objective", -1, at);
            }
        }
        game.paused = true;
        game.notice = "HOTTOT / REAL-WORLD PROTOTYPE | Space: start/pause · F2: reset · F5/F9: separate save/load · Fog off for inspection".into();
        game
    }
}

#[cfg(test)]
mod gis_gameplay_tests {
    use super::*;
    #[test]
    fn gis_map_supports_navigation_garrison_damage_restart_and_separate_saves() {
        let mut game =
            NativeGame::operation(crate::gis_prototype::ID, 2048, 1024, default_config());
        for entity in &game.entities {
            assert!(
                spec(&entity.view.kind).is_object(),
                "invalid scenario unit: {}",
                entity.view.kind
            );
        }
        game.validate().unwrap();
        assert_eq!(
            game.entities
                .iter()
                .map(|e| e.view.lots.len())
                .sum::<usize>(),
            crate::gis_prototype::map().buildings.len()
        );
        assert_eq!(
            game.operation_save_path().file_name().unwrap(),
            crate::gis_prototype::SAVE
        );
        assert_ne!(game.operation_save_path(), save_path());
        let tank = game
            .entities
            .iter()
            .position(|e| e.view.owner == 0 && e.view.kind == "medium")
            .unwrap();
        let start = position(&game.entities[tank]);
        let goal = game.open_land([420., 470.], "medium").unwrap();
        assert!(
            !game.route(tank, start, goal).is_empty(),
            "tanks must navigate imported ground"
        );
        let building = game
            .entities
            .iter()
            .find(|e| !e.view.lots.is_empty() && e.view.x > 500. && e.view.x < 1500.)
            .unwrap()
            .view
            .clone();
        let lot = &building.lots[0];
        let (door, normal) = lot.door();
        let at = [
            building.x + door[0] + normal[0] * 2.,
            building.z + door[1] + normal[1] * 2.,
        ];
        let soldier = game.spawn("short", 0, at);
        assert_eq!(
            game.enter_building(
                0,
                std::slice::from_ref(&soldier),
                &building.id,
                Some([building.x + lot.at[0], building.z + lot.at[1]])
            ),
            1
        );
        assert!(
            game.entities
                .iter()
                .find(|e| e.view.id == soldier)
                .unwrap()
                .view
                .garrison
                .is_some()
        );
        for _ in 0..400 {
            game.tick += 1;
            game.movement();
            game.garrison_step();
            if game
                .entities
                .iter()
                .find(|e| e.view.id == soldier)
                .unwrap()
                .view
                .garrison
                .as_ref()
                .is_some_and(|g| g.entered)
            {
                break;
            }
        }
        assert!(
            game.entities
                .iter()
                .find(|e| e.view.id == soldier)
                .unwrap()
                .view
                .garrison
                .as_ref()
                .is_some_and(|g| g.entered),
            "infantry must physically cross the imported house's doorway"
        );
        let target = [building.x + lot.at[0], building.z + lot.at[1]];
        game.impact(&Projectile {
            owner: 0,
            from: at,
            to: target,
            born: 0,
            impact: 0,
            damage: lot.max_hp() * 2.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        game.garrison_step();
        assert_eq!(
            game.entities
                .iter()
                .find(|e| e.view.id == building.id)
                .unwrap()
                .view
                .lots[0]
                .integrity,
            0.
        );
        let path = std::env::temp_dir().join(format!("row-gis-{}.json", std::process::id()));
        game.save_to(&path).unwrap();
        let restored = NativeGame::load_from(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(restored.terrain.id, crate::gis_prototype::ID);
        assert!(
            (restored.terrain.height(1024., 512.) - game.terrain.height(1024., 512.)).abs() < 0.001
        );
        assert_eq!(
            restored
                .entities
                .iter()
                .map(|e| e.view.lots.len())
                .sum::<usize>(),
            crate::gis_prototype::map().buildings.len()
        );
        assert_eq!(
            restored
                .entities
                .iter()
                .find(|e| e.view.id == building.id)
                .unwrap()
                .view
                .lots[0]
                .integrity,
            0.
        );
        game.command(json!({"type":"new"}));
        assert_eq!(game.terrain.id, crate::gis_prototype::ID);
        assert!(game.paused);
        // The default operation is still selected independently.
        assert_eq!(NativeGame::default().terrain.id, crate::battlefield::ID);
    }
}
