#[cfg(test)]
mod simulation_mode_tests {
    use super::*;

    fn field() -> NativeGame {
        let mut game = NativeGame::new();
        game.entities.clear();
        game.props.clear();
        game.bases.clear();
        game.terrain = crate::terrain::Terrain::new("simulation-test", 256, 160);
        game.terrain.authored = vec![0.52; 256 * 160];
        game.terrain.hydrate();
        game.simulation = true;
        game.config["fogEnabled"] = json!(true);
        game.supply = [100., 100.];
        game
    }

    #[test]
    fn simulation_commands_and_recruits_for_both_teams_but_solo_only_for_red() {
        let mut game = field();
        game.spawn("objective", -1, [128., 80.]);
        for (owner, x) in [(0, 22.), (1, 234.)] {
            game.spawn("barracks", owner, [x, 60.]);
            game.spawn("short", owner, [x, 80.]);
        }
        let mut solo = game.clone();
        solo.simulation = false;
        game.step();
        solo.step();
        for owner in [0, 1] {
            assert!(
                game.entities
                    .iter()
                    .any(|e| e.view.owner == owner && !e.orders.is_empty())
            );
            assert!(game.jobs.iter().any(|job| job.owner == owner));
            assert!(game.supply[owner as usize] < 100.);
        }
        assert!(
            solo.entities
                .iter()
                .filter(|e| e.view.owner == 0)
                .all(|e| e.orders.is_empty())
        );
        assert!(!solo.jobs.iter().any(|job| job.owner == 0));
        assert!(solo.jobs.iter().any(|job| job.owner == 1));
    }

    #[test]
    fn simulation_reveals_both_armies_and_effects_without_changing_ai_sight() {
        let mut game = field();
        game.spawn("short", 0, [20., 80.]);
        let enemy = game.spawn("short", 1, [234., 80.]);
        game.events.push(BattleEvent {
            tick: 0,
            kind: "impact".into(),
            at: [234., 80.],
            from: None,
            owner: 1,
            radius: 1.,
            weapon: "short".into(),
            source: enemy.clone(),
            duration: 0.1,
            release_height: None,
            hit: None,
            target: None,
        });
        assert!(!game.visible(0, &game.entities[1]));
        let snapshot = game.snapshot();
        assert!(snapshot.simulation);
        assert!(!snapshot.fog_enabled);
        assert_eq!(snapshot.team_supply, game.supply);
        assert_eq!(snapshot.entities.len(), 2);
        assert_eq!(snapshot.events.len(), 1);
        game.simulation = false;
        assert!(!game.snapshot().entities.iter().any(|p| p.id == enemy));
        assert!(game.snapshot().events.is_empty());
    }

    #[test]
    fn spectators_cannot_order_or_purchase_but_can_pause() {
        let mut game = field();
        let soldier = game.spawn("short", 0, [20., 80.]);
        let barracks = game.spawn("barracks", 0, [20., 60.]);
        game.command(json!({"type":"command", "unitIds":[soldier], "commandType":"move", "destination":{"x":100,"y":80}}));
        game.command(json!({"type":"recruit", "unitType":"short", "producerId":barracks}));
        game.command(json!({"type":"build", "buildingType":"mine", "destination":{"x":60,"y":60}}));
        assert!(game.entities[0].orders.is_empty());
        assert!(game.jobs.is_empty());
        assert_eq!(game.supply, [100., 100.]);
        game.command(json!({"type":"pause"}));
        game.step();
        assert!(game.paused);
        assert_eq!(game.tick, 0);
        game.command(json!({"type":"pause"}));
        game.step();
        assert_eq!(game.tick, 1);
    }

    #[test]
    fn simulation_armies_exchange_fire_and_stop_at_victory() {
        let mut game = field();
        for (owner, x) in [(0, 120.), (1, 130.)] {
            game.spawn("medium", owner, [x, 80.]);
            let unit = game.entities.last_mut().unwrap();
            unit.view.hp = 10000.;
            unit.view.max_hp = 10000.;
            // Tanks need infantry spotters under the normal combat rules.
            game.spawn("short", owner, [x, 84.]);
            let spotter = game.entities.last_mut().unwrap();
            spotter.view.hp = 10000.;
            spotter.view.max_hp = 10000.;
        }
        game.tick = DEPLOYMENT_TICKS + 1;
        let mut fired = [false; 2];
        for _ in 0..80 {
            game.step();
            for owner in [0, 1] {
                fired[owner as usize] |= game
                    .events
                    .iter()
                    .any(|event| event.kind == "shot" && event.owner == owner);
            }
        }
        assert_eq!(fired, [true, true]);
        game.entities.retain(|e| e.view.owner == 0);
        game.step();
        assert_eq!(game.winner, Some(0));
        let ended_at = game.tick;
        game.step();
        assert_eq!(game.tick, ended_at);
    }

    #[test]
    fn simulation_save_restores_automation_and_old_saves_default_to_solo() {
        let mut game = field();
        game.spawn("short", 0, [20., 80.]);
        game.spawn("short", 1, [234., 80.]);
        assert_eq!(
            game.operation_save_path().file_name().unwrap(),
            "native-simulation.json"
        );
        assert_ne!(game.operation_save_path(), save_path());
        let path = std::env::temp_dir().join(format!("row-simulation-{}.json", std::process::id()));
        game.save_to(&path).unwrap();
        let mut restored = NativeGame::load_from(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(restored.simulation);
        restored.step();
        assert!(
            restored
                .entities
                .iter()
                .filter(|e| !e.view.building)
                .all(|e| !e.orders.is_empty())
        );
        let mut legacy = serde_json::to_value(&game).unwrap();
        legacy.as_object_mut().unwrap().remove("simulation");
        assert!(
            !serde_json::from_value::<NativeGame>(legacy)
                .unwrap()
                .simulation
        );
    }
}
