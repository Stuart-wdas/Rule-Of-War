// Included in simulation.rs to share catalog, orders and production state.
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0. {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.
    }
    .clamp(0., 1.);
    distance(p, [a[0] + t * d[0], a[1] + t * d[1]])
}
fn damage(p: &mut Piece, amount: f32) {
    let amount = amount.max(0.);
    p.hp -= amount;
    let maximum = number(spec(&p.kind), "formationStrength", 1.) as u32;
    if maximum > 1 {
        let lost = (maximum as f32 * amount / p.max_hp.max(1.)).ceil().max(1.) as u32;
        p.strength = p.strength.saturating_sub(lost);
    }
}
fn heal(p: &mut Piece, amount: f32) {
    let amount = amount.max(0.).min((p.max_hp - p.hp).max(0.));
    if amount == 0. {
        return;
    }
    p.hp += amount;
    let maximum = number(spec(&p.kind), "formationStrength", 1.) as u32;
    if maximum > 1 {
        let restored = (maximum as f32 * amount / p.max_hp.max(1.)).ceil().max(1.) as u32;
        p.strength = p.strength.saturating_add(restored).min(maximum);
    }
}
impl NativeGame {
    fn bridge_site(&self, a: Point, b: Point) -> bool {
        if !a.iter().chain(&b).all(|n| n.is_finite())
            || distance(a, b) < 2.
            || distance(a, b) > 256.
            || !self.passable(a, false)
            || !self.passable(b, false)
        {
            return false;
        }
        let steps = (distance(a, b) * 2.).ceil() as usize;
        let mut crosses_water = false;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if !self.passable(p, true)
                || self.on_bridge(p)
                || self.entities.iter().any(|e| {
                    e.view.hp > 0.
                        && e.view.building
                        && e.view.kind != "objective"
                        && distance(position(e), p) < 2.
                })
                || self
                    .jobs
                    .iter()
                    .any(|j| j.end.is_some_and(|end| segment_distance(p, j.at, end) < 2.))
            {
                return false;
            }
            crosses_water |= self.terrain.cell(p[0], p[1]).water();
        }
        crosses_water
    }
    fn build_bridge(&mut self, owner: i32, a: Option<Point>, b: Option<Point>) -> String {
        let (Some(a), Some(b)) = (a, b) else {
            return "Choose both bridge banks.".into();
        };
        let a = a.map(f32::round);
        let b = b.map(f32::round);
        // The original catalog restricts bridge construction to commanders.
        if !self.entities.iter().any(|e| {
            e.view.owner == owner
                && e.view.kind == "commander"
                && e.view.hp > 0.
                && e.transport.is_none()
                && distance(position(e), a) <= 9.
        }) {
            return "Start the bridge within 9 metres of a commander.".into();
        }
        if !self.bridge_site(a, b) {
            return "Bridge needs two clear dry banks and a water crossing.".into();
        }
        let cost = (distance(a, b) * number(spec("bridge"), "pricePerMeter", 3.))
            .round()
            .max(1.);
        if self.supply[owner as usize] < cost {
            return "Insufficient supply.".into();
        }
        self.supply[owner as usize] -= cost;
        let id = self.id();
        self.jobs.push(Job {
            id,
            owner,
            kind: "building".into(),
            unit_type: "bridge".into(),
            producer: None,
            at: a,
            end: Some(b),
            cost,
            progress: 0,
            duration: 200,
        });
        "Constructing bridge.".into()
    }
}

#[cfg(test)]
mod specialist_tests {
    use super::*;
    #[test]
    fn bridge_build_save_cross_and_destruction() {
        let mut g = NativeGame::new();
        g.spawn("commander", 0, [72., 50.]);
        g.supply[0] = 100.;
        assert!(g.path([74., 50.], [86., 50.], false).len() > 12);
        assert_eq!(
            g.build_bridge(0, Some([74., 50.]), Some([86., 50.])),
            "Constructing bridge."
        );
        assert_eq!(g.supply[0], 64.);
        for _ in 0..200 {
            g.production();
        }
        assert_eq!(g.path([74., 50.], [86., 50.], false).len(), 12);
        let saved = serde_json::to_vec(&g).unwrap();
        let mut restored: NativeGame = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.path([74., 50.], [86., 50.], false).len(), 12);
        restored
            .entities
            .iter_mut()
            .find(|e| e.view.kind == "bridge")
            .unwrap()
            .view
            .hp = 0.;
        assert!(restored.path([74., 50.], [86., 50.], false).len() > 12);
    }
    #[test]
    fn bridge_rejects_land_enemy_builder_and_reserves_site() {
        let mut g = NativeGame::new();
        g.spawn("commander", 0, [72., 50.]);
        g.supply[0] = 100.;
        assert!(
            !g.build_bridge(1, Some([74., 50.]), Some([86., 50.]))
                .contains("Constructing")
        );
        assert!(
            !g.build_bridge(0, Some([65., 50.]), Some([74., 50.]))
                .contains("Constructing")
        );
        g.build_bridge(0, Some([74., 50.]), Some([86., 50.]));
        assert!(
            !g.build_bridge(0, Some([74., 50.]), Some([86., 50.]))
                .contains("Constructing")
        );
        assert_eq!(g.supply[0], 64.);
    }
    #[test]
    fn patrol_repeats_and_stop_clears_it() {
        let mut g = NativeGame::new();
        g.entities.clear();
        let id = g.spawn("scout", 0, [30., 30.]);
        g.command(json!({"type":"command","unitIds":[id],"commandType":"patrol","destination":{"x":35.,"y":30.}}));
        let mut visited_end = false;
        let mut returned = false;
        for _ in 0..400 {
            g.tick += 1;
            g.movement();
            let x = g.entities[0].view.x;
            visited_end |= x > 34.8;
            returned |= visited_end && x < 30.2;
        }
        assert!(visited_end && returned);
        g.command(json!({"type":"command","unitIds":[id],"commandType":"stop"}));
        assert!(g.entities[0].orders.is_empty() && g.entities[0].patrol_origin.is_none());
    }
    #[test]
    fn vehicle_formation_casualties_and_healing_match_web() {
        let mut g = NativeGame::new();
        let id = g.spawn("medium", 0, [30., 30.]);
        let p = &mut g
            .entities
            .iter_mut()
            .find(|e| e.view.id == id)
            .unwrap()
            .view;
        let maximum = p.strength;
        damage(p, 1.);
        assert_eq!(
            p.strength,
            maximum - (maximum as f32 / p.max_hp).ceil() as u32
        );
        heal(p, 1.);
        assert_eq!(p.strength, maximum);
        assert_eq!(p.hp, p.max_hp);
    }
    #[test]
    fn guest_commands_and_snapshots_use_guest_ownership() {
        let mut g = NativeGame::new();
        g.config["fogEnabled"] = json!(false);
        let friendly = g.spawn("scout", 0, [30., 30.]);
        let guest = g.spawn("scout", 1, [40., 30.]);
        g.command_as(1,json!({"type":"command","unitIds":[friendly,guest],"commandType":"move","destination":{"x":45.,"y":30.}}));
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == friendly)
                .unwrap()
                .orders
                .is_empty()
        );
        assert_eq!(
            g.entities
                .iter()
                .find(|e| e.view.id == guest)
                .unwrap()
                .orders
                .len(),
            1
        );
        let before = g.tick;
        g.command_as(1, json!({"type":"new"}));
        assert_eq!(g.tick, before);
        g.supply = [42., 17.];
        assert_eq!(g.snapshot_for(1).supply, 17.);
    }
}
