// Included in simulation.rs. The presentation scene uses real entities and damage.
impl NativeGame {
    pub fn setup_visual_demo(&mut self) {
        *self = Self::new();
        self.entities.clear();
        self.jobs.clear();
        self.props.clear();
        self.bases.clear();
        self.projectiles.clear();
        self.guided_missiles.clear();
        self.events.clear();
        self.terrain = crate::terrain::Terrain::new(crate::visual_demo::ID, 192, 192);
        self.terrain.authored_scale = 12.;
        self.terrain.authored = vec![0.52; 192 * 192];
        self.terrain.hydrate();
        self.training = Some(TrainingGround::default());
        self.human_opponent = true;
        self.config["fogEnabled"] = json!(false);
        self.paused = false;
        self.winner = None;
        self.tick = 0;
        self.rng = 74129;
        self.spawn("village", -1, [96., 96.]);
        self.entities[0].view.label = "SAINT-ORNE / VISUAL STUDY".into();
        self.entities[0].view.lots = [-17., 17.]
            .into_iter()
            .enumerate()
            .flat_map(|(row, z)| {
                [-22., 0., 22.]
                    .into_iter()
                    .enumerate()
                    .map(move |(col, x)| crate::settlement_layout::Lot {
                        at: [x, z],
                        width: 9.2,
                        depth: 8.,
                        height: 3.6,
                        yaw: if row == 0 { 0. } else { std::f32::consts::PI },
                        style: [0, 3, 6, 7, 8, 11][row * 3 + col],
                        ground: 0.,
                        integrity: 1.,
                    })
            })
            .collect();
        self.refresh_towns();
        for (kind, owner, at) in [
            ("medium", 0, [80., 96.]),
            ("medium", 1, [116., 96.]),
            ("short", 0, [78., 102.]),
            ("short", 0, [82., 103.]),
            ("scout", 0, [87., 102.]),
            ("medic", 0, [75., 102.]),
            ("engineer", 0, [85., 103.]),
            ("short", 1, [122., 102.]),
        ] {
            self.spawn(kind, owner, at);
            self.entities.last_mut().unwrap().stance = "hold-fire".into();
        }
        let walkers: Vec<_> = self
            .entities
            .iter()
            .filter(|e| e.view.kind == "short" && e.view.owner == 0)
            .map(|e| e.view.id.clone())
            .collect();
        for (i, id) in walkers.iter().enumerate() {
            let e = self.entities.iter_mut().find(|e| &e.view.id == id).unwrap();
            e.view.z = 100.6 + i as f32;
            self.issue(
                0,
                std::slice::from_ref(id),
                Order {
                    kind: "patrol".into(),
                    destination: Some([90. + i as f32 * 2., 100.6 + i as f32]),
                    target: None,
                },
                false,
            );
        }
        self.notice = "VISUAL DEMONSTRATION / 1 impact / 2 artillery / 3 vehicle / R reset".into();
    }
    pub fn visual_demo_strike(&mut self, kind: usize) -> Point {
        let mut at = [[91., 96.], [101., 83.], [116., 96.], [91., 83.]][kind.min(3)];
        if kind == 1 {
            let args: Vec<_> = std::env::args().collect();
            if let Some(check) = args
                .iter()
                .position(|a| a == "--demo-occlusion")
                .and_then(|i| args.get(i + 1))
            {
                at = match check.as_str() {
                    "behind" => [116., 107.],
                    "inside" => [118., 113.],
                    "front" => [124., 119.],
                    _ => at,
                };
            }
        }
        if kind == 2 {
            if let Some(e) = self
                .entities
                .iter_mut()
                .find(|e| e.view.owner == 1 && e.view.kind == "medium")
            {
                e.view.hp = 0.;
                self.record_wrecks();
                self.entities.retain(|e| e.view.hp > 0.);
            } else {
                self.events.push(BattleEvent {
                    hit: None,
                    target: None,
                    tick: self.tick,
                    kind: "crash-impact".into(),
                    at,
                    from: None,
                    owner: 1,
                    radius: 3.5,
                    weapon: "visual-vehicle".into(),
                    source: format!("demo-{}", self.tick),
                    duration: 0.,
                    release_height: None,
                });
            }
        } else {
            self.impact(&Projectile {
                owner: 0,
                from: [70., 96.],
                to: at,
                born: self.tick,
                impact: self.tick,
                damage: if kind == 0 { 0.15 } else { 2.5 },
                radius: match kind { 0 => 1.2, 3 => 2.4, _ => 3.5 },
                hit: true,
                weapon: match kind { 0 => "medium", 3 => "heli-rocket", _ => "long" }.into(),
                ground_hit: true,
            });
        }
        at
    }
}

#[cfg(test)]
mod visual_demo_tests {
    use super::*;
    #[test]
    fn demo_uses_damageable_houses_and_vehicle_wrecks_without_finishing_a_match() {
        let mut g = NativeGame::new();
        g.setup_visual_demo();
        assert_eq!(g.entities[0].view.lots.len(), 6);
        assert_eq!(g.terrain.id, crate::visual_demo::ID);
        assert_ne!(g.operation_save_path(), save_path());
        assert_ne!(
            g.operation_save_path(),
            save_path().with_file_name("native-training.json")
        );
        g.tick = 10;
        g.visual_demo_strike(2);
        assert!(g.props.iter().any(|p| p.wreck.is_some()));
        assert!(g.events.iter().any(|e| e.kind == "destruction"));
        for _ in 0..40 {
            g.step();
        }
        assert_eq!(g.winner, None);
        g.setup_visual_demo();
        assert!(g.props.is_empty());
        g.tick = 10;
        g.visual_demo_strike(1);
        assert!(g.entities[0].view.lots.iter().any(|l| l.integrity < 1.));
        assert_eq!(
            g.entities
                .iter()
                .filter(|e| e.view.kind == "medium")
                .count(),
            2
        );
        g.command_as(0, json!({"type":"new"}));
        assert_eq!(g.terrain.id, crate::visual_demo::ID);
        assert_eq!(g.entities[0].view.lots.len(), 6);
    }
}
