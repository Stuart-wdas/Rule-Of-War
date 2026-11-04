// Included in simulation.rs. Repeatable engineering battles on the shipping battlefield.
pub const BATTLE_BENCH_CASES: &[&str] = &[
    "rifles",
    "urban",
    "armour",
    "artillery",
    "air",
    "combined",
    "siege",
    "airstrike",
];
pub fn mock_battle_enabled() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    *ACTIVE.get_or_init(|| std::env::args().any(|a| a == "--mock-battle"))
}
#[derive(Default)]
pub struct BattleBenchStats {
    last_tick: u64,
    weapons: BTreeMap<String, u64>,
    peak_entities: usize,
    peak_projectiles: usize,
    wrecks: std::collections::BTreeSet<String>,
    pub samples: Vec<f64>,
}
impl BattleBenchStats {
    pub fn observe(&mut self, g: &NativeGame) {
        for e in g
            .events
            .iter()
            .filter(|e| e.tick > self.last_tick && e.tick <= g.tick && e.kind == "shot")
        {
            *self.weapons.entry(e.weapon.clone()).or_default() += 1;
        }
        self.last_tick = g.tick;
        self.peak_entities = self.peak_entities.max(g.entities.len());
        self.peak_projectiles = self
            .peak_projectiles
            .max(g.projectiles.len() + g.guided_missiles.len());
        for p in &g.props {
            if let Some(w) = &p.wreck {
                self.wrecks.insert(w.piece.id.clone());
            }
        }
    }
    pub fn report(&self, g: &NativeGame) -> Value {
        let mut sorted = self.samples.clone();
        sorted.sort_by(f64::total_cmp);
        let percentile = |q: f64| {
            sorted
                .get(((sorted.len().saturating_sub(1)) as f64 * q).ceil() as usize)
                .copied()
                .unwrap_or(0.)
        };
        json!({"map":g.terrain.id,"size":[g.terrain.cols,g.terrain.rows],"tick":g.tick,"unitsAlive":g.entities.iter().filter(|e|!e.view.building).count(),"peakEntities":self.peak_entities,"peakProjectiles":self.peak_projectiles,"weaponEvents":self.weapons,"wrecks":self.wrecks.len(),"damagedLots":g.entities.iter().flat_map(|e|&e.view.lots).filter(|l|l.integrity<1.).count(),"collapsedLots":g.entities.iter().flat_map(|e|&e.view.lots).filter(|l|l.integrity<=0.).count(),"meanTickMs":self.samples.iter().sum::<f64>()/self.samples.len().max(1) as f64,"p95TickMs":percentile(0.95),"p99TickMs":percentile(0.99),"maxTickMs":percentile(1.),"ticksOver50Ms":self.samples.iter().filter(|v|**v>50.).count(),"ticksOver1000Ms":self.samples.iter().filter(|v|**v>1000.).count()})
    }
}
impl NativeGame {
    pub fn setup_mock_battle(&mut self, case: &str) {
        assert!(BATTLE_BENCH_CASES.contains(&case));
        *self = Self::default();
        self.rng = 0x73a4b529;
        self.tick = DEPLOYMENT_TICKS;
        self.human_opponent = true;
        self.config["fogEnabled"] = json!(true);
        self.entities.retain(|e| e.view.building);
        self.props.retain(|p| p.wreck.is_none());
        self.battle_bench_wave(case, true);
        self.paused = false;
    }
    fn bench_spawn(&mut self, kind: &str, owner: i32, at: Point) -> Option<String> {
        let at = if flag(spec(kind), "airUnit") {
            Some(at)
        } else {
            self.open_land(at, kind)
        }?;
        Some(self.spawn(kind, owner, at))
    }
    pub fn battle_bench_wave(&mut self, case: &str, initial: bool) {
        let _timing = crate::performance::span("benchmark.wave");
        let town = self
            .entities
            .iter()
            .find(|e| e.view.town_plan == crate::settlement_layout::TownPlan::Rural)
            .map(position)
            .unwrap_or([512., 512.]);
        let urban = case != "rifles";
        let z = if urban { town[1] } else { town[1] + 190. };
        let infantry = matches!(
            case,
            "rifles" | "urban" | "armour" | "air" | "combined" | "airstrike"
        );
        for owner in [0, 1] {
            let side = if owner == 0 { -1. } else { 1. };
            let goal = [town[0] - side * 70., z];
            let active = self
                .entities
                .iter()
                .filter(|e| {
                    e.view.owner == owner
                        && !e.view.building
                        && e.view.category == "troop"
                        && e.view.hp > 0.
                })
                .count();
            if infantry && (initial || active < 12) {
                for group in 0..2 {
                    let center = town[0] + if group == 0 { -84. } else { 84. };
                    for soldier in 0..8 {
                        let kind = if case == "rifles" {
                            "short"
                        } else if case == "airstrike" {
                            [
                                "short", "short", "scout", "short", "medic", "short", "short",
                                "short",
                            ][soldier]
                        } else {
                            [
                                "short", "engineer", "scout", "short", "medic", "short",
                                "engineer", "short",
                            ][soldier]
                        };
                        let at = [center + side * 17., z - 4.5 + soldier as f32 * 1.3];
                        if let Some(id) = self.bench_spawn(kind, owner, at) {
                            self.issue(
                                owner,
                                &[id],
                                Order {
                                    kind: "attack-move".into(),
                                    destination: Some(goal),
                                    target: None,
                                },
                                false,
                            );
                        }
                    }
                }
            }
            if initial
                && matches!(
                    case,
                    "armour" | "artillery" | "air" | "combined" | "siege" | "airstrike"
                )
            {
                let kinds: &[&str] = match case {
                    "armour" => &[
                        "medium",
                        "medium",
                        "medium",
                        "antiAir",
                        "troopCarrier",
                        "long",
                    ],
                    "artillery" => &["long", "long", "xtralong", "xtralong", "medium", "medium"],
                    "air" => &["medium", "medium", "antiAir"],
                    "airstrike" => &["medium", "medium"],
                    "siege" => &["medium", "medium", "long", "long", "xtralong", "xtralong"],
                    _ => &[
                        "medium",
                        "medium",
                        "long",
                        "xtralong",
                        "antiAir",
                        "troopCarrier",
                    ],
                };
                for (slot, kind) in kinds.iter().enumerate() {
                    let at = [
                        town[0] + side * (100. + (slot % 2) as f32 * 18.),
                        town[1] - 32. + (slot / 2) as f32 * 30.,
                    ];
                    if let Some(id) = self.bench_spawn(kind, owner, at) {
                        let destination = if ["long", "xtralong"].contains(kind) {
                            [town[0] - side * 40., town[1]]
                        } else {
                            goal
                        };
                        self.issue(
                            owner,
                            &[id],
                            Order {
                                kind: if case == "siege" {
                                    "attack"
                                } else {
                                    "attack-move"
                                }
                                .into(),
                                destination: Some(destination),
                                target: if case == "siege" {
                                    self.entities
                                        .iter()
                                        .find(|e| {
                                            e.view.town_plan
                                                == crate::settlement_layout::TownPlan::Rural
                                        })
                                        .map(|e| e.view.id.clone())
                                } else {
                                    None
                                },
                            },
                            false,
                        );
                    }
                }
            }
            if initial
                && matches!(case, "air" | "combined" | "airstrike")
                && let Some(field) = self
                    .entities
                    .iter()
                    .find(|e| e.view.kind == "airfield" && e.view.owner == owner)
                    .map(|e| e.view.id.clone())
            {
                for kind in ["plane", "helicopter"] {
                    for slot in 0..2 {
                        let id = self.spawn(kind, owner, [town[0] + side * 220., town[1]]);
                        let i = self.entities.len() - 1;
                        self.park_aircraft(i, &field, slot);
                        self.issue(
                            owner,
                            &[id],
                            Order {
                                kind: "attack-move".into(),
                                destination: Some([town[0] - side * 65., town[1]]),
                                target: None,
                            },
                            false,
                        );
                    }
                }
            }

            if initial
                && matches!(case, "urban" | "combined")
                && let Some(building) = self
                    .entities
                    .iter()
                    .find(|e| e.view.town_plan == crate::settlement_layout::TownPlan::Rural)
                    .map(|e| e.view.id.clone())
            {
                let ids: Vec<_> = self
                    .entities
                    .iter()
                    .filter(|e| e.view.owner == owner && e.view.kind == "short")
                    .take(2)
                    .map(|e| e.view.id.clone())
                    .collect();
                self.enter_building(
                    owner,
                    &ids,
                    &building,
                    Some([town[0] + side * 70., town[1] - 12.]),
                );
            }
        }
    }
    pub fn benchmark_battle(mut self, case: &str, ticks: u64) -> Value {
        crate::performance::reset();
        let setup = std::time::Instant::now();
        self.setup_mock_battle(case);
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.;
        let mut stats = BattleBenchStats::default();
        let start = self.tick;
        for n in 0..ticks {
            let before = std::time::Instant::now();
            if n > 0 && n % 400 == 0 {
                self.battle_bench_wave(case, false);
            }
            self.step();
            stats.samples.push(before.elapsed().as_secs_f64() * 1000.);
            stats.observe(&self);
            if n % 20 == 0 {
                eprintln!(
                    "battle={case} tick={n}/{ticks} lastMs={:.2}",
                    stats.samples.last().unwrap()
                );
            }
        }
        let mut report = stats.report(&self);
        report["case"] = json!(case);
        report["simulatedTicks"] = json!(self.tick - start);
        report["setupMs"] = json!(setup_ms);
        report["timings"] = crate::performance::report();
        report
    }
}
