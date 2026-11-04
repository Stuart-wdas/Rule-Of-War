// Included in simulation.rs; layouts and blocking offsets are embedded web data.
fn town_blocks(seed: &str, count: usize) -> Vec<Value> {
    let mut state = seed
        .chars()
        .fold(2166136261u32, |hash, c| {
            let mut utf16 = [0u16; 2];
            let code = c.encode_utf16(&mut utf16)[0];
            (hash ^ code as u32).wrapping_mul(16777619)
        })
        .max(1);
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state as f64 / 4294967296.
    };
    let mut blocks = vec![json!({"gridX":0,"gridY":0,"rotation":0,"variant":0,"role":"civic"})];
    let mut occupied = std::collections::BTreeSet::from([(0i32, 0i32)]);
    while blocks.len() < count.clamp(1, 8) {
        let mut edges = vec![];
        for block in &blocks {
            let (x, y) = (
                block["gridX"].as_i64().unwrap() as i32,
                block["gridY"].as_i64().unwrap() as i32,
            );
            for (dx, dy, opposite) in [
                (0, -1, "south"),
                (1, 0, "west"),
                (0, 1, "north"),
                (-1, 0, "east"),
            ] {
                if !occupied.contains(&(x + dx, y + dy)) {
                    edges.push((x + dx, y + dy, opposite));
                }
            }
        }
        let (x, y, opposite) = edges[(random() * edges.len() as f64).floor() as usize];
        let rotation = (random() * 4.).floor() as i32;
        let variant = (random() * 4.).floor() as i32;
        let role = ["residential", "commercial", "industrial", "residential"]
            [(random() * 4.).floor() as usize];
        blocks.push(json!({"gridX":x,"gridY":y,"rotation":rotation,"variant":variant,"role":role,"attachedVia":opposite}));
        occupied.insert((x, y));
    }
    blocks
}
impl NativeGame {
    /// Migrate old saved house dimensions once, preserving damage and every occupant.
    pub(crate) fn upgrade_house_layouts(&mut self) {
        if self.version != 1
            || !(10..=crate::terrain::MAX_SIZE).contains(&self.terrain.cols)
            || !(10..=crate::terrain::MAX_SIZE).contains(&self.terrain.rows)
        {
            return;
        }
        let mut changed = vec![];
        for building in &mut self.entities {
            if !["town", "village"].contains(&building.view.kind.as_str()) {
                continue;
            }
            let old = building.view.lots.clone();
            if old.is_empty()
                || old.iter().all(|l| {
                    l.width == crate::settlement_layout::HOUSE_PLOT
                        && l.depth == crate::settlement_layout::HOUSE_PLOT
                        && l.height == crate::settlement_layout::HOUSE_HEIGHT
                })
            {
                continue;
            }
            if old.iter().any(|l| {
                !l.width.is_finite()
                    || !l.depth.is_finite()
                    || !l.height.is_finite()
                    || !l.integrity.is_finite()
            }) {
                continue;
            }
            building.view.lots = if building.view.label == "PRACTICE HOUSES" {
                crate::training::practice_houses()
            } else if old.len() == 1 {
                old.clone()
            } else {
                crate::settlement_layout::lots(&building.view.kind, &building.view.id)
            };
            for (i, lot) in building.view.lots.iter_mut().enumerate() {
                lot.width = crate::settlement_layout::HOUSE_PLOT;
                lot.depth = crate::settlement_layout::HOUSE_PLOT;
                lot.height = crate::settlement_layout::HOUSE_HEIGHT;
                lot.integrity = old.get(i).map(|l| l.integrity).unwrap_or(1.);
            }
            changed.push((
                building.view.id.clone(),
                building.view.x,
                building.view.z,
                building.view.lots.clone(),
            ));
        }
        for unit in &mut self.entities {
            let Some(g) = unit.view.garrison.as_ref() else {
                continue;
            };
            let Some((_, x, z, lots)) = changed.iter().find(|b| b.0 == g.building) else {
                continue;
            };
            let lot = &lots[g.lot.unwrap_or(0).min(lots.len() - 1)];
            let (door, n) = lot.door();
            unit.view.x = (x + door[0] + n[0] * 1.5).clamp(1., self.terrain.cols as f32 - 2.);
            unit.view.z = (z + door[1] + n[1] * 1.5).clamp(1., self.terrain.rows as f32 - 2.);
            unit.view.garrison = None;
            unit.view.velocity = [0., 0.];
            unit.speed = 0.;
            unit.path.clear();
            unit.orders.clear();
        }
    }
    pub fn refresh_towns(&mut self) {
        for e in &mut self.entities {
            if !["town", "village"].contains(&e.view.kind.as_str()) {
                continue;
            }
            if e.view.lots.is_empty() {
                e.view.lots =
                    crate::settlement_layout::lots_for(&e.view.kind, &e.view.id, e.view.town_plan);
            }
            for lot in &mut e.view.lots {
                lot.width = crate::settlement_layout::HOUSE_PLOT;
                lot.depth = crate::settlement_layout::HOUSE_PLOT;
                lot.height = crate::settlement_layout::HOUSE_HEIGHT;
            }
            let base = self.terrain.height(e.view.x, e.view.z);
            e.view.max_hp = e.view.lots.iter().map(|l| l.max_hp()).sum();
            e.view.hp = e
                .view
                .lots
                .iter()
                .map(|l| l.max_hp() * l.integrity)
                .sum::<f32>()
                .max(0.01);
            for lot in &mut e.view.lots {
                lot.ground = self
                    .terrain
                    .height(e.view.x + lot.at[0], e.view.z + lot.at[1])
                    - base;
            }
            e.blocking.clear();
            // Preserve the legacy baked footprint for compatibility maps. The joined
            // town uses its lot polygons directly instead of a truncated fixed box.
            if !matches!(e.view.town_plan, crate::settlement_layout::TownPlan::Joined | crate::settlement_layout::TownPlan::Rural) {
                for z in -50..=50 {
                    for x in -50..=50 {
                        if e.view
                            .lots
                            .iter()
                            .any(|l| l.contains([x as f32, z as f32], 0.25))
                        {
                            e.blocking.push([e.view.x + x as f32, e.view.z + z as f32]);
                        }
                    }
                }
            }
        }
    }
    fn grade_town_plots(&mut self, at: Point, lots: &[crate::settlement_layout::Lot]) {
        for lot in lots {
            let center = [at[0] + lot.at[0], at[1] + lot.at[1]];
            let target = self.terrain.cell(center[0], center[1]).surface;
            let (cx, cz) = (center[0].round() as i32, center[1].round() as i32);
            for z in cz - 7..=cz + 7 {
                for x in cx - 7..=cx + 7 {
                    let weight = ((7.
                        - (x as f32 - center[0])
                            .abs()
                            .max((z as f32 - center[1]).abs()))
                        / 1.4)
                        .clamp(0., 1.);
                    let old = self.terrain.cell(x as f32, z as f32);
                    let height = old.surface + (target - old.surface) * weight;
                    self.terrain.edits.insert(
                        format!("{x},{z}"),
                        crate::terrain::Cell {
                            elevation: height,
                            surface: height,
                            moisture: old.moisture,
                            kind: 3,
                        },
                    );
                }
            }
        }
        self.terrain.revision += 1;
    }
    fn town_site(&self, at: Point) -> bool {
        const EDGE: f32 = 47.;
        if at[0] < EDGE + 2.
            || at[1] < EDGE + 2.
            || at[0] > self.terrain.cols as f32 - EDGE - 2.
            || at[1] > self.terrain.rows as f32 - EDGE - 2.
        {
            return false;
        }
        if self
            .entities
            .iter()
            .filter(|e| e.view.building && e.view.kind != "objective")
            .any(|e| {
                let half = if !e.view.lots.is_empty() {
                    [47., 47.]
                } else {
                    [
                        number(spec(&e.view.kind), "footprintWidth", 8.) * e.view.model_scale * 0.5,
                        number(spec(&e.view.kind), "footprintDepth", 8.) * e.view.model_scale * 0.5,
                    ]
                };
                (at[0] - e.view.x).abs() < EDGE + half[0] + 4.
                    && (at[1] - e.view.z).abs() < EDGE + half[1] + 4.
            })
        {
            return false;
        }
        (-40..=40).step_by(5).all(|z| {
            (-40..=40).step_by(5).all(|x| {
                !self
                    .terrain
                    .cell(at[0] + x as f32, at[1] + z as f32)
                    .water()
            })
        })
    }
    fn add_rural_town(&mut self) {
        let at = [
            self.terrain.cols as f32 * 0.5,
            self.terrain.rows as f32 * 0.5,
        ];
        let plan = crate::settlement_layout::TownPlan::Rural;
        let seed = format!("rural-town:{}", self.rng);
        let lots = crate::settlement_layout::lots_for("town", &seed, plan);
        // Grade the compact village-and-fields platform, feathering the edge
        // into the existing rolling terrain outside the reference footprint.
        let target = self.terrain.cell(at[0], at[1]).surface;
        for z in -crate::rural_layout::BOUND..=crate::rural_layout::BOUND {
            for x in -crate::rural_layout::BOUND..=crate::rural_layout::BOUND {
                let p = crate::rural_layout::local([x as f32, z as f32]);
                let weight = crate::rural_layout::coverage(p);
                if weight <= 0. { continue; }
                let wx = at[0] as i32 + x;
                let wz = at[1] as i32 + z;
                let old = self.terrain.cell(wx as f32, wz as f32);
                // A shallow common platform avoids foundation steps while
                // preserving relief at the woodland transition.
                let h = old.surface + (target - old.surface) * weight;
                self.terrain.edits.insert(
                    format!("{wx},{wz}"),
                    crate::terrain::Cell {
                        elevation: h,
                        surface: h,
                        moisture: old.moisture,
                        kind: old.kind,
                    },
                );
            }
        }
        self.terrain.revision += 1;
        let id = self.spawn("town", -1, at);
        let e = self.entities.iter_mut().find(|e| e.view.id == id).unwrap();
        e.view.label = "VILLAGE & FARMS".into();
        e.view.town_plan = plan;
        e.view.lots = lots;
        e.view.town_blocks = town_blocks(&seed, 8);
        self.refresh_towns();
        for plaza in plan.plazas() {
            self.spawn("objective", -1, [at[0] + plaza[0], at[1] + plaza[1]]);
        }
        if self.terrain.id==crate::battlefield::ID {
            for (index,(site,farm)) in crate::battlefield::additional_sites(self.terrain.cols,self.terrain.rows).into_iter().enumerate() {
                let lots=crate::rural_layout::site_lots(index);
                self.grade_town_plots(site,&lots);
                let kind=if farm {"village"} else {"town"};
                let id=self.spawn(kind,-1,site);
                let entity=self.entities.iter_mut().find(|e|e.view.id==id).unwrap();
                entity.view.label=["WEST HAMLET","EAST HAMLET","NORTH FARM","SOUTH FARM"][index].into();
                entity.view.town_plan=crate::rural_layout::site_plan(index);
                entity.view.lots=lots;
                if !farm {
                    let plaza=crate::rural_layout::site_plan(index).plaza();
                    self.spawn("objective",-1,[site[0]+plaza[0],site[1]+plaza[1]]);
                }
            }
            self.refresh_towns();
        }
    }
    fn add_settlements(&mut self) {
        if self.terrain.cols.min(self.terrain.rows) >= 700 {
            self.add_rural_town();
            return;
        }
        // On the active battlefield every command post is housed in a town square.
        if self.terrain.cols.min(self.terrain.rows) >= 300 {
            let mut plans = [
                crate::settlement_layout::TownPlan::H,
                crate::settlement_layout::TownPlan::L,
                crate::settlement_layout::TownPlan::E,
            ];
            for i in (1..plans.len()).rev() {
                let j = (self.random() * (i + 1) as f32) as usize;
                plans.swap(i, j.min(i));
            }
            for (plan, fraction) in plans.into_iter().zip([0.25, 0.5, 0.75]) {
                let mut chosen = None;
                for _ in 0..120 {
                    let at = [
                        self.terrain.cols as f32 * (0.5 + (self.random() - 0.5) * 0.36),
                        self.terrain.rows as f32 * (fraction + (self.random() - 0.5) * 0.07),
                    ];
                    if self.town_site(at) {
                        chosen = Some(at);
                        break;
                    }
                }
                if chosen.is_none() {
                    chosen = (46..self.terrain.rows - 46)
                        .step_by(10)
                        .flat_map(|z| {
                            (46..self.terrain.cols - 46)
                                .step_by(10)
                                .map(move |x| [x as f32, z as f32])
                        })
                        .filter(|p| self.town_site(*p))
                        .min_by(|a, b| {
                            (a[1] - self.terrain.rows as f32 * fraction)
                                .abs()
                                .total_cmp(&(b[1] - self.terrain.rows as f32 * fraction).abs())
                        });
                }
                if let Some(at) = chosen {
                    let town = self.spawn("town", -1, at);
                    let seed = format!("{}:{}:{}", self.rng, at[0], at[1]);
                    let lots = crate::settlement_layout::lots_for("town", &seed, plan);
                    self.grade_town_plots(at, &lots);
                    let e = self
                        .entities
                        .iter_mut()
                        .find(|e| e.view.id == town)
                        .unwrap();
                    e.view.lots = lots;
                    e.view.town_plan = plan;
                    e.view.town_blocks = town_blocks(&seed, 4);
                    self.refresh_towns();
                    let plaza = plan.plaza();
                    self.spawn("objective", -1, [at[0] + plaza[0], at[1] + plaza[1]]);
                } else if let Some(at) = self.open_land(
                    [
                        self.terrain.cols as f32 * 0.5,
                        self.terrain.rows as f32 * fraction,
                    ],
                    "objective",
                ) {
                    self.spawn("objective", -1, at);
                }
            }
        } else {
            // Small compatibility maps retain useful flag locations without overlapping bases.
            for fraction in [0.25, 0.5, 0.75] {
                if let Some(at) = self.open_land(
                    [
                        self.terrain.cols as f32 * 0.5,
                        self.terrain.rows as f32 * fraction,
                    ],
                    "objective",
                ) {
                    self.spawn("objective", -1, at);
                }
            }
        }
    }
}
#[cfg(test)]
mod settlement_tests {
    use super::*;
    #[test]
    fn extended_battlefield_has_three_towns_two_farmsteads_and_five_open_capture_areas() {
        let g = NativeGame::default();
        let towns: Vec<_> = g
            .entities
            .iter()
            .filter(|e| e.view.kind == "town")
            .collect();
        let flags: Vec<_> = g
            .entities
            .iter()
            .filter(|e| e.view.kind == "objective")
            .collect();
        assert_eq!(towns.len(), 3);
        assert_eq!(flags.len(), 5);
        let snapshot=g.snapshot();
        for entity in g.entities.iter().filter(|e|!e.view.lots.is_empty()) {
            assert_eq!(snapshot.entities.iter().find(|p|p.id==entity.view.id).unwrap().label,entity.view.label);
        }
        assert_eq!(g.entities.iter().filter(|e|e.view.kind=="village").count(),2);
        let town = towns.iter().find(|e|e.view.town_plan==crate::settlement_layout::TownPlan::Rural).unwrap();
        for entity in g.entities.iter().filter(|e|!e.view.lots.is_empty()) {
            for road in crate::settlement_layout::roads_for(&entity.view.kind,entity.view.town_plan) {
                for step in 0..=20 {
                    let f=step as f32/20.;
                    let point=[entity.view.x+road.a[0]+(road.b[0]-road.a[0])*f,
                        entity.view.z+road.a[1]+(road.b[1]-road.a[1])*f];
                    assert!(g.passable(point,false),"blocked settlement street at {point:?}");
                }
            }
        }
        assert_eq!(
            town.view.town_plan,
            crate::settlement_layout::TownPlan::Rural
        );
        assert_eq!(town.view.lots.len(), 18);
        assert!(g.validate().is_ok(), "rural farm lots must be valid save data");
        let restored: NativeGame = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert!(restored.validate().is_ok());
        assert_eq!(restored.entities.iter().find(|e| e.view.kind == "town").unwrap().view.lots, town.view.lots);
        for plaza in town.view.town_plan.plazas() {
            let at = [town.view.x + plaza[0], town.view.z + plaza[1]];
            assert!(flags.iter().any(|f| distance(at, position(f)) < 0.01));
            assert!(g.passable(at, false));
        }
        for road in crate::settlement_layout::roads_for("town", town.view.town_plan) {
            for step in 0..=100 {
                let t = step as f32 / 100.;
                let p = [
                    town.view.x + road.a[0] + (road.b[0] - road.a[0]) * t,
                    town.view.z + road.a[1] + (road.b[1] - road.a[1]) * t,
                ];
                assert!(g.passable(p, false), "blocked street {p:?}");
            }
        }
        for lot in &town.view.lots {
            for x in [-5., 5.] {
                for z in [-5., 5.] {
                    assert!(
                        (g.terrain
                            .height(town.view.x + lot.at[0] + x, town.view.z + lot.at[1] + z)
                            - g.terrain
                                .height(town.view.x + lot.at[0], town.view.z + lot.at[1]))
                        .abs()
                            < 0.001
                    );
                }
            }
        }
    }
    #[test]
    fn capture_zone_includes_street_fights_and_equal_opposition_contests_it() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.tick = DEPLOYMENT_TICKS;
        g.spawn("objective", -1, [60., 60.]);
        g.spawn("short", 0, [85., 60.]);
        for _ in 0..8 {
            g.objectives();
        }
        let progress = g.entities[0].capture_progress;
        assert!(progress > 0.);
        g.spawn("short", 1, [35., 60.]);
        for _ in 0..8 {
            g.objectives();
        }
        assert_eq!(g.entities[0].capture_progress, progress);
        g.entities[2].view.x = 25.;
        g.objectives();
        assert!(g.entities[0].capture_progress > progress);
        let progress = g.entities[0].capture_progress;
        g.entities[1].view.x = 60. + CAPTURE_RADIUS + 0.1;
        g.objectives();
        assert!(g.entities[0].capture_progress < progress);
    }
    #[test]
    fn deployment_infantry_security_neutralization_and_contesting_prevent_flag_rushes() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.spawn("objective", -1, [60., 60.]);
        g.spawn("short", 0, [65., 60.]);
        for _ in 0..200 {
            g.objectives();
        }
        assert_eq!(g.entities[0].view.owner, -1);
        assert_eq!(g.entities[0].capture_progress, 0.);
        g.tick = DEPLOYMENT_TICKS;
        for _ in 0..99 {
            g.objectives();
        }
        assert_eq!(g.entities[0].view.owner, -1);
        let progress = g.entities[0].capture_progress;
        g.spawn("medium", 1, [55., 60.]);
        for _ in 0..40 {
            g.objectives();
        }
        assert_eq!(g.entities[0].capture_progress, progress);
        g.entities.pop();
        g.objectives();
        assert_eq!(g.entities[0].view.owner, 0);
        g.entities[1].view.owner = 1;
        for _ in 0..100 {
            g.objectives();
        }
        assert_eq!(g.entities[0].view.owner, -1);
        for _ in 0..100 {
            g.objectives();
        }
        assert_eq!(g.entities[0].view.owner, 1);
        g.entities[1].view.kind = "medium".into();
        g.entities[1].view.category = "vehicle".into();
        g.entities[0].view.owner = -1;
        for _ in 0..200 {
            g.objectives();
        }
        assert_eq!(g.entities[0].view.owner, -1);
        g.entities[0].view.owner = 1;
        g.hold[1] = 89.75;
        g.spawn("short", 0, [55., 60.]);
        g.objectives();
        assert_eq!(g.hold[1], 0.);
        assert!(g.winner.is_none());
    }
    #[test]
    fn upgrading_a_legacy_house_preserves_damage_and_releases_its_occupants_safely() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        let house = g.spawn("village", -1, [40., 40.]);
        g.entities[0].view.lots = vec![crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 6.,
            depth: 4.,
            height: 5.4,
            yaw: 0.7,
            style: 0,
            ground: 0.,
            integrity: 0.4,
        }];
        g.spawn("short", 0, [40., 40.]);
        g.entities[1].view.hp = 0.7;
        g.entities[1].view.garrison = Some(Garrison {
            building: house,
            lot: Some(0),
            window: 19,
            entered: true,
            ready_at: 0,
            door_stage: 2,
            exiting: false,
        });
        let mut restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        restored.upgrade_house_layouts();
        restored.terrain.hydrate();
        restored.refresh_towns();
        let house = &restored.entities[0].view.lots[0];
        assert_eq!([house.width, house.depth, house.height], [10., 10., 3.2]);
        assert_eq!(house.integrity, 0.4);
        let unit = &restored.entities[1];
        assert_eq!(unit.view.hp, 0.7);
        assert!(unit.view.garrison.is_none());
        assert!(restored.passable(position(unit), false));
        restored.validate().unwrap();
        let before = serde_json::to_value(&restored).unwrap();
        restored.upgrade_house_layouts();
        assert_eq!(before, serde_json::to_value(&restored).unwrap());
    }
    #[test]
    fn freestanding_houses_block_their_footprints_and_leave_the_lane_open() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.spawn("village", -1, [50., 50.]);
        assert!(g.passable([50., 50.], false));
        for lot in &g.entities[0].view.lots {
            for wing in lot.wings() {
                let at = lot.to_world(wing.at);
                assert!(!g.passable([50. + at[0], 50. + at[1]], false));
            }
        }
        let piece = &g.entities[0].view;
        let targets: Vec<_> = piece.pick_positions().collect();
        assert_eq!(
            targets.len(),
            piece
                .lots
                .iter()
                .map(|l| l.wings().len() + 1)
                .sum::<usize>()
        ); // Roof/wall centre and the doorway.
        assert!(targets.iter().all(|p| !g.passable([p.x, p.z], false)));
        let restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(g.entities[0].view.lots, restored.entities[0].view.lots);
    }
    #[test]
    fn town_sequences_match_web_and_keep_streets_open() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/web-settlements.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                &town_blocks(case["seed"].as_str().unwrap(), 7),
                case["blocks"].as_array().unwrap()
            );
        }
        let mut g = NativeGame::new();
        g.config["fogEnabled"] = json!(false);
        g.entities.clear();
        g.spawn("village", -1, [40., 40.]);
        assert!(g.passable([40., 40.], false));
        let cell = g.entities[0].blocking[0];
        assert!(!g.passable(cell, false));
        let snapshot = g.snapshot();
        assert_eq!(snapshot.entities[0].kind, "village");
    }
}

#[cfg(test)]
mod destruction_tests {
    use super::*;
    #[test]
    fn house_damage_clears_its_footprint_preserves_neighbours_and_survives_restore() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(false);
        g.spawn("village", -1, [40., 40.]);
        let lot = g.entities[0].view.lots[0].clone();
        let at = [40. + lot.at[0], 40. + lot.at[1]];
        assert!(!g.passable(at, false));
        g.impact(&Projectile {
            owner: 0,
            from: [20., 40.],
            to: at,
            born: 0,
            impact: 0,
            damage: lot.max_hp(),
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert_eq!(g.entities[0].view.lots[0].integrity, 0.);
        assert!(
            g.entities[0]
                .view
                .lots
                .iter()
                .skip(1)
                .all(|l| l.integrity == 1.)
        );
        assert!(g.passable(at, false));
        assert_ne!(attack_point(&g.entities[0], [20., 40.]), at);
        let encoded = serde_json::to_vec(&g).unwrap();
        let mut restored: NativeGame = serde_json::from_slice(&encoded).unwrap();
        restored.terrain.hydrate();
        restored.refresh_towns();
        assert_eq!(restored.entities[0].view.lots[0].integrity, 0.);
        assert!(restored.passable(at, false));
        assert!(restored.validate().is_ok());
    }
    #[test]
    fn devastated_town_stays_capturable_but_stops_producing_supply() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.spawn("town", 0, [40., 40.]);
        for lot in &mut g.entities[0].view.lots {
            lot.integrity = 0.;
        }
        g.entities[0].view.hp = 0.01;
        let before = g.supply;
        g.economy_support();
        assert_eq!(g.supply, before);
        assert!(!g.snapshot().entities[0].lots.is_empty());
    }
}
