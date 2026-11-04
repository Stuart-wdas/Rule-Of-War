// Included in simulation.rs: cached, spatially indexed collision geometry.
// A failed query must never monopolize a simulation step. Nested scopes share
// the allowance across every candidate/arc in one firing-position decision.
thread_local! { static SEARCH_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) }; }
struct NavigationWork {
    previous: usize,
    initial: usize,
}
impl NavigationWork {
    fn new(limit: usize) -> Self {
        SEARCH_WORK.with(|b| {
            let previous = b.get();
            let initial = previous.min(limit);
            b.set(initial);
            Self { previous, initial }
        })
    }
    fn remaining() -> usize {
        SEARCH_WORK.with(|b| b.get())
    }
    fn expand() -> bool {
        SEARCH_WORK.with(|b| {
            let n = b.get();
            if n == 0 {
                false
            } else {
                b.set(n - 1);
                true
            }
        })
    }
}
impl Drop for NavigationWork {
    fn drop(&mut self) {
        SEARCH_WORK.with(|b| {
            let spent = self.initial.saturating_sub(b.get());
            b.set(if self.previous == usize::MAX {
                usize::MAX
            } else {
                self.previous.saturating_sub(spent)
            });
        });
    }
}
#[derive(Hash, PartialEq, Eq)]
struct FailedRoute {
    start: (i32, i32),
    goal: (i32, i32),
    mode: u8,
    radius: u32,
    terrain: u64,
    allowance: usize,
}
struct FailedSearch {
    until: u64,
    work: usize,
}
type CachedNavigation = Option<(u64, std::sync::Arc<NavigationWorld>)>;
#[derive(Default)]
struct NavigationCache(std::sync::Mutex<CachedNavigation>);
impl Clone for NavigationCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}
struct NavObstacle {
    at: Point,
    half: Point,
    sin: f32,
    cos: f32,
}
impl NavObstacle {
    fn local(&self, p: Point) -> Point {
        let d = [p[0] - self.at[0], p[1] - self.at[1]];
        [
            self.cos * d[0] - self.sin * d[1],
            self.sin * d[0] + self.cos * d[1],
        ]
    }
    fn signed_distance(&self, p: Point) -> f32 {
        let p = self.local(p);
        let q = [p[0].abs() - self.half[0], p[1].abs() - self.half[1]];
        q[0].max(0.).hypot(q[1].max(0.)) + q[0].max(q[1]).min(0.)
    }
    fn crosses(&self, a: Point, b: Point, radius: f32) -> bool {
        let a = self.local(a);
        let b = self.local(b);
        let crosses_box = |half: Point| {
            let mut lo: f32 = 0.;
            let mut hi: f32 = 1.;
            for axis in 0..2 {
                let delta = b[axis] - a[axis];
                if delta.abs() < 0.000001 {
                    if a[axis].abs() >= half[axis] {
                        return false;
                    }
                } else {
                    let x = (-half[axis] - a[axis]) / delta;
                    let y = (half[axis] - a[axis]) / delta;
                    lo = lo.max(x.min(y));
                    hi = hi.min(x.max(y));
                    if hi < lo {
                        return false;
                    }
                }
            }
            true
        };
        if !crosses_box([self.half[0] + radius, self.half[1] + radius]) {
            return false;
        }
        if radius == 0. || crosses_box(self.half) {
            return true;
        }
        let point_distance = |p: Point| {
            let x = (p[0].abs() - self.half[0]).max(0.);
            let z = (p[1].abs() - self.half[1]).max(0.);
            x * x + z * z
        };
        let mut squared = point_distance(a).min(point_distance(b));
        for x in [-self.half[0], self.half[0]] {
            for z in [-self.half[1], self.half[1]] {
                squared = squared.min(segment_distance([x, z], a, b).powi(2));
            }
        }
        squared < radius * radius
    }
}
#[derive(Default)]
struct NavigationWorld {
    obstacles: Vec<NavObstacle>,
    buckets: std::collections::HashMap<(i32, i32), Vec<usize>>,
    window_capacity: std::collections::HashMap<String, usize>,
    failed_routes: std::sync::Mutex<std::collections::HashMap<FailedRoute, FailedSearch>>,
}
impl NavigationWorld {
    fn add(&mut self, at: Point, half: Point, yaw: f32) {
        let (sin, cos) = yaw.sin_cos();
        let bound = [
            cos.abs() * half[0] + sin.abs() * half[1],
            sin.abs() * half[0] + cos.abs() * half[1],
        ];
        let id = self.obstacles.len();
        for z in
            ((at[1] - bound[1]) / 16.).floor() as i32..=((at[1] + bound[1]) / 16.).floor() as i32
        {
            for x in ((at[0] - bound[0]) / 16.).floor() as i32
                ..=((at[0] + bound[0]) / 16.).floor() as i32
            {
                self.buckets.entry((x, z)).or_default().push(id);
            }
        }
        self.obstacles.push(NavObstacle { at, half, sin, cos });
    }
    fn nearby(&self, a: Point, b: Point, radius: f32) -> Vec<usize> {
        let mut ids = vec![];
        for z in ((a[1].min(b[1]) - radius) / 16.).floor() as i32
            ..=((a[1].max(b[1]) + radius) / 16.).floor() as i32
        {
            for x in ((a[0].min(b[0]) - radius) / 16.).floor() as i32
                ..=((a[0].max(b[0]) + radius) / 16.).floor() as i32
            {
                if let Some(bucket) = self.buckets.get(&(x, z)) {
                    for id in bucket {
                        if !ids.contains(id) {
                            ids.push(*id);
                        }
                    }
                }
            }
        }
        ids
    }
    fn blocked(&self, p: Point, radius: f32) -> bool {
        self.any_nearby(p, p, radius, |o| o.signed_distance(p) < radius)
    }
    fn crosses(&self, a: Point, b: Point, radius: f32) -> bool {
        self.any_nearby(a, b, radius, |o| o.crosses(a, b, radius))
    }
    fn any_nearby(
        &self,
        a: Point,
        b: Point,
        radius: f32,
        mut test: impl FnMut(&NavObstacle) -> bool,
    ) -> bool {
        // Boolean queries can short-circuit directly in the spatial buckets.
        // Repeated membership is harmless; allocating and deduplicating a vector
        // on every A* edge was costly in towns.
        for z in ((a[1].min(b[1]) - radius) / 16.).floor() as i32
            ..=((a[1].max(b[1]) + radius) / 16.).floor() as i32
        {
            for x in ((a[0].min(b[0]) - radius) / 16.).floor() as i32
                ..=((a[0].max(b[0]) + radius) / 16.).floor() as i32
            {
                if let Some(bucket) = self.buckets.get(&(x, z))
                    && bucket.iter().any(|i| test(&self.obstacles[*i]))
                {
                    return true;
                }
            }
        }
        false
    }
    fn overlap(&self, p: Point, radius: f32) -> f32 {
        self.nearby(p, p, radius)
            .into_iter()
            .map(|i| (radius - self.obstacles[i].signed_distance(p)).max(0.))
            .sum()
    }
    fn escaping(&self, from: Point, to: Point, radius: f32) -> bool {
        self.nearby(from, to, radius).into_iter().all(|i| {
            let obstacle = &self.obstacles[i];
            let before = obstacle.signed_distance(from);
            if before < radius {
                // A legacy overlap may leave this obstacle, but must never deepen
                // it or enter a different obstacle while recovering.
                obstacle.signed_distance(to) >= before - 0.0001
            } else {
                !obstacle.crosses(from, to, radius)
            }
        })
    }
}
fn body_clearance(kind: &str, scale: f32) -> f32 {
    if spec(kind)["category"] == "vehicle" {
        1.08_f32.hypot(1.60) * scale + 0.2
    } else {
        0.
    }
}
impl NativeGame {
    pub fn smoke_runway_navigation(&mut self) -> String {
        *self = Self::default();
        self.entities
            .retain(|e| e.view.building && e.view.owner == 0);
        self.bases.retain(|b| b.owner == 0);
        self.props.clear();
        self.human_opponent = true;
        self.config["fogEnabled"] = json!(false);
        let field = self
            .entities
            .iter()
            .find(|e| e.view.kind == "airfield")
            .unwrap()
            .view
            .clone();
        let start = crate::airfield_layout::world(&field, [-37., 42.]);
        let goal = crate::airfield_layout::world(&field, [9., 42.]);
        let id = self.spawn("long", 0, start);
        let index = self.entities.len() - 1;
        self.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "move".into(),
                destination: Some(goal),
                target: None,
            },
            false,
        );
        for _ in 0..4000 {
            self.tick += 1;
            self.movement();
            assert_eq!(self.vehicle_clearance_violations(), 0);
            if distance(position(&self.entities[index]), goal) < 0.6 {
                break;
            }
        }
        assert!(
            distance(position(&self.entities[index]), goal) < 0.6,
            "support gun failed to get around the runway hangar"
        );
        self.paused = true;
        id
    }
    fn navigation_world(&self) -> std::sync::Arc<NavigationWorld> {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let mut key_float = |v: f32| v.to_bits().hash(&mut hash);
        for e in self
            .entities
            .iter()
            .filter(|e| e.view.building && e.view.kind != "objective")
        {
            for v in [
                e.view.x,
                e.view.z,
                e.view.heading,
                e.view.model_scale,
                f32::from(e.view.hp > 0.),
            ] {
                key_float(v);
            }
            for lot in &e.view.lots {
                for v in [
                    lot.at[0],
                    lot.at[1],
                    lot.width,
                    lot.depth,
                    lot.yaw,
                    lot.style as f32,
                    f32::from(lot.integrity > 0.),
                ] {
                    key_float(v);
                }
            }
            if let Some(end) = e.bridge_end {
                key_float(end[0]);
                key_float(end[1]);
            }
        }
        for p in &self.props {
            for v in [
                p.at[0],
                p.at[1],
                p.heading,
                f32::from(p.hp > 0.),
                f32::from(p.grounded()),
            ] {
                key_float(v);
            }
            if let Some(w) = &p.wreck {
                for v in w.half_extents {
                    key_float(v);
                }
            }
        }
        for e in self.entities.iter().filter(|e| e.view.building) {
            e.view.id.hash(&mut hash);
            e.view.kind.hash(&mut hash);
            e.blocking.len().hash(&mut hash);
        }
        for p in &self.props {
            p.kind.hash(&mut hash);
            p.wreck.as_ref().map(|w| w.piece.building).hash(&mut hash);
        }
        let key = hash.finish();
        let mut cache = self.navigation.0.lock().unwrap();
        if let Some((old, world)) = cache.as_ref()
            && *old == key
        {
            return world.clone();
        }
        let mut world = NavigationWorld::default();
        for e in self.entities.iter().filter(|e| {
            e.view.building
                && e.view.hp > 0.
                && !["objective", "bridge"].contains(&e.view.kind.as_str())
        }) {
            // Window geometry is static until a building/lot changes. Do not
            // reconstruct every town window on every rendered HUD snapshot.
            let capacity = if e.view.lots.is_empty() {
                facility_windows(&e.view.kind).len()
            } else {
                e.view
                    .lots
                    .iter()
                    .filter(|l| l.integrity > 0.)
                    .map(|l| l.windows().len())
                    .sum()
            };
            world.window_capacity.insert(e.view.id.clone(), capacity);
            if !e.view.lots.is_empty() {
                for lot in e.view.lots.iter().filter(|l| l.integrity > 0.) {
                    for wing in lot.wings() {
                        let at = lot.to_world(wing.at);
                        world.add(
                            building_world(e, at),
                            [
                                wing.size[0] * e.view.model_scale * 0.5 + 0.25,
                                wing.size[1] * e.view.model_scale * 0.5 + 0.25,
                            ],
                            lot.yaw + e.view.heading,
                        );
                    }
                }
            } else if e.view.kind == "airfield" {
                let (s, c) = e.view.heading.sin_cos();
                for (at, half) in crate::airfield_layout::SOLIDS {
                    world.add(
                        [
                            e.view.x + (c * at[0] + s * at[1]) * e.view.model_scale,
                            e.view.z + (-s * at[0] + c * at[1]) * e.view.model_scale,
                        ],
                        [
                            half[0] * e.view.model_scale + 0.25,
                            half[1] * e.view.model_scale + 0.25,
                        ],
                        e.view.heading,
                    );
                }
            } else if !e.blocking.is_empty() {
                for at in &e.blocking {
                    world.add(*at, [0.51, 0.51], 0.);
                }
            } else {
                world.add(
                    position(e),
                    [
                        number(spec(&e.view.kind), "footprintWidth", 3.6)
                            * e.view.model_scale
                            * 0.5
                            + 0.25,
                        number(spec(&e.view.kind), "footprintDepth", 3.6)
                            * e.view.model_scale
                            * 0.5
                            + 0.25,
                    ],
                    e.view.heading,
                );
            }
        }
        for p in &self.props {
            if let Some(w) = p
                .wreck
                .as_ref()
                .filter(|w| w.crash.is_none() && !w.piece.building)
            {
                world.add(
                    p.at,
                    [w.half_extents[0] + 0.2, w.half_extents[1] + 0.2],
                    p.heading,
                );
            } else if p.kind == "wall" && p.hp > 0. {
                world.add(p.at, [2.1, 0.4], p.heading);
            }
        }
        let world = std::sync::Arc::new(world);
        *cache = Some((key, world.clone()));
        world
    }
    fn ground_passable(&self, p: Point, mode: u8) -> bool {
        self.passable(p, true)
            && if mode == 2 {
                self.terrain.cell(p[0], p[1]).water()
            } else {
                !self.terrain.cell(p[0], p[1]).water() || self.on_bridge(p)
            }
    }
    fn passable_clearance(&self, p: Point, mode: u8, radius: f32) -> bool {
        if mode == 1 {
            return self.passable(p, true);
        }
        self.ground_passable(p, mode) && !self.navigation_world().blocked(p, radius)
    }
    fn clear_segment_radius(&self, a: Point, b: Point, mode: u8, radius: f32) -> bool {
        if mode == 1 {
            return self.passable(b, true);
        }
        let world = self.navigation_world();
        self.navigation_segment(&world, a, b, mode, radius)
    }
    fn navigation_segment(
        &self,
        world: &NavigationWorld,
        a: Point,
        b: Point,
        mode: u8,
        radius: f32,
    ) -> bool {
        if world.crosses(a, b, radius) {
            return false;
        }
        let steps = (distance(a, b) * 3.).ceil().max(1.) as usize;
        (0..=steps).all(|i| {
            let t = i as f32 / steps as f32;
            self.ground_passable([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], mode)
        })
    }
    fn clearance_destination(&self, goal: Point, radius: f32) -> Option<Point> {
        let world = self.navigation_world();
        let fits = |p: Point| self.ground_passable(p, 0) && !world.blocked(p, radius);
        if fits(goal) {
            return Some(goal);
        }
        for ring in 1..=28 {
            for n in 0..32 {
                let angle = n as f32 * std::f32::consts::TAU / 32.;
                let d = ring as f32 * 0.5;
                let p = [goal[0] + angle.cos() * d, goal[1] + angle.sin() * d];
                if fits(p) {
                    return Some(p);
                }
            }
        }
        None
    }
    fn recover_vehicle_clearance(&mut self, index: usize, radius: f32) -> bool {
        if radius == 0. {
            return false;
        }
        let at = position(&self.entities[index]);
        let world = self.navigation_world();
        let overlap = world.overlap(at, radius);
        if overlap < 0.001 {
            return false;
        }
        let Some(goal) = self.clearance_destination(at, radius + 0.1) else {
            return true;
        };
        let d = distance(at, goal).max(0.001);
        let desired = (goal[1] - at[1]).atan2(goal[0] - at[0]);
        let e = &mut self.entities[index];
        let limit = turn_rate(&e.view.kind, e.view.model_scale) * DT;
        e.view.heading += angle_delta(e.view.heading, desired).clamp(-limit, limit);
        let heading = e.view.heading;
        let alignment = angle_delta(heading, desired).cos();
        e.speed = 0.;
        let step = if alignment > 0.85 {
            (number(spec(&e.view.kind), "speed", 2.) * DT * 0.4).min(d)
        } else {
            0.
        };
        let next = [at[0] + heading.cos() * step, at[1] + heading.sin() * step];
        if self.ground_passable(next, 0)
            && world.escaping(at, next, radius)
            && world.overlap(next, radius) < overlap
        {
            let e = &mut self.entities[index];
            e.view.x = next[0];
            e.view.z = next[1];
            e.view.velocity = [(next[0] - at[0]) / DT, (next[1] - at[1]) / DT];
            e.speed = step / DT;
        }
        true
    }
    pub(crate) fn vehicle_clearance_violations(&self) -> usize {
        let world = self.navigation_world();
        self.entities
            .iter()
            .filter(|e| {
                e.view.category == "vehicle"
                    && world.blocked(
                        position(e),
                        body_clearance(&e.view.kind, e.view.model_scale) - 0.001,
                    )
            })
            .count()
    }
    pub(crate) fn safe_vehicle_prediction(&self, p: &Piece, target: Point) -> bool {
        self.clear_segment_radius(
            [p.x, p.z],
            target,
            0,
            body_clearance(&p.kind, p.model_scale),
        )
    }
}

#[cfg(test)]
impl NativeGame {
    fn clear_segment(&self, a: Point, b: Point, mode: u8) -> bool {
        self.clear_segment_radius(a, b, mode, 0.)
    }
}
#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn long_diagonal_move_replaces_the_old_destination_and_displays_the_complete_route() {
        for kind in ["short", "medium"] {
            let mut g = NativeGame::new();
            g.entities.clear();
            g.props.clear();
            g.bases.clear();
            g.config["fogEnabled"] = json!(false);
            g.terrain = crate::terrain::Terrain::new("long-order-test", 1024, 1024);
            g.terrain.authored = vec![0.52; 1024 * 1024];
            g.terrain.authored_scale = 12.;
            g.terrain.hydrate();
            g.spawn("village", -1, [350., 350.]);
            g.entities[0].view.lots = vec![crate::settlement_layout::Lot {
                at: [0., 0.],
                width: 10.,
                depth: 10.,
                height: 3.2,
                yaw: 0.,
                style: 0,
                ground: 0.,
                integrity: 1.,
            }];
            g.refresh_towns();
            let id = g.spawn(kind, 0, [30., 30.]);
            let index = g.entities.iter().position(|e| e.view.id == id).unwrap();
            assert!(!g.clear_segment([30., 30.], [900., 900.], 0));
            for goal in [[40., 60.], [900., 900.]] {
                g.command(
                    json!({"type":"command", "commandType":"move", "unitIds":[id],
                    "destination":{"x":goal[0], "y":goal[1]}}),
                );
                assert_eq!(g.entities[index].orders.len(), 1, "{kind}: {}", g.notice);
                assert_eq!(
                    g.entities[index].orders[0].destination,
                    Some(goal),
                    "{kind}: {}",
                    g.notice
                );
                assert_eq!(g.entities[index].path.back(), Some(&goal), "{kind}");
                let snapshot = g.snapshot();
                let unit = snapshot.entities.iter().find(|p| p.id == id).unwrap();
                assert_eq!(unit.orders, vec![goal]);
                assert_eq!(unit.waypoints.last(), Some(&goal));
                let mut from = position(&g.entities[index]);
                for to in &g.entities[index].path {
                    assert!(g.clear_segment(from, *to, 0));
                    from = *to;
                }
            }
            // A genuinely invalid destination keeps the accepted route and
            // explains why it was rejected, rather than acknowledging zero units.
            g.command(
                json!({"type":"command", "commandType":"move", "unitIds":[id],
                "destination":{"x":1200., "y":1200.}}),
            );
            assert_eq!(g.entities[index].orders[0].destination, Some([900., 900.]));
            assert!(g.notice.contains("Cannot reach"), "{}", g.notice);
        }
    }
    #[test]
    fn failed_path_memoization_keeps_the_same_decision_budget_after_save_load() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        for (n, (at, heading)) in [
            ([50., 48.], 0.),
            ([50., 52.], 0.),
            ([48., 50.], std::f32::consts::FRAC_PI_2),
            ([52., 50.], std::f32::consts::FRAC_PI_2),
        ]
        .into_iter()
        .enumerate()
        {
            g.props.push(BattlefieldProp {
                scar: None,
                id: format!("memo-wall-{n}"),
                kind: "wall".into(),
                at,
                heading,
                hp: 100.,
                wreck: None,
            });
        }
        assert!(g.path([40., 50.], [50., 50.], false).is_empty());
        let loaded: NativeGame = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        let cached_work = {
            let _scope = NavigationWork::new(32768);
            assert!(g.path([40., 50.], [50., 50.], false).is_empty());
            NavigationWork::remaining()
        };
        let fresh_work = {
            let _scope = NavigationWork::new(32768);
            assert!(loaded.path([40., 50.], [50., 50.], false).is_empty());
            NavigationWork::remaining()
        };
        assert_eq!(
            cached_work, fresh_work,
            "a CPU cache must not change simulation decisions after restoring a save"
        );
    }
    #[test]
    fn fractional_goal_beside_a_wall_routes_to_the_exact_point_without_searching_a_blocked_grid_node()
     {
        let mut g = fixture();
        g.entities[0].view.lots[0].yaw = 0.;
        g.refresh_towns();
        let start = [55., 70.];
        let goal = [74.46, 70.];
        assert!(g.passable(goal, false));
        assert!(!g.passable([goal[0].round(), goal[1].round()], false));
        let path = g.path(start, goal, false);
        assert_eq!(path.back(), Some(&goal));
        let mut from = start;
        for to in path {
            assert!(g.clear_segment(from, to, 0));
            from = to;
        }
    }
    #[test]
    fn unreachable_large_map_goal_has_bounded_work_and_reopens_after_a_wall_is_destroyed() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new(crate::battlefield::ID, 1024, 1024);
        for (n, (at, heading)) in [
            ([300., 298.], 0.),
            ([300., 302.], 0.),
            ([298., 300.], std::f32::consts::FRAC_PI_2),
            ([302., 300.], std::f32::consts::FRAC_PI_2),
        ]
        .into_iter()
        .enumerate()
        {
            g.props.push(BattlefieldProp {
                scar: None,
                id: format!("closed-{n}"),
                kind: "wall".into(),
                at,
                heading,
                hp: 100.,
                wreck: None,
            });
        }
        let _work = NavigationWork::new(16384);
        assert!(g.path([290., 300.], [300., 300.], false).is_empty());
        assert_eq!(NavigationWork::remaining(), 0);
        let before = NavigationWork::remaining();
        assert!(g.path([290., 300.], [300., 300.], false).is_empty());
        assert_eq!(NavigationWork::remaining(), before);
        drop(_work);
        g.props[2].hp = 0.;
        assert_eq!(
            g.path([290., 300.], [300., 300.], false).back(),
            Some(&[300., 300.])
        );
    }
    #[test]
    fn shared_visibility_matches_detection_after_movement_casualties_and_transport() {
        let mut g = NativeGame::new();
        g.config["fogEnabled"] = json!(true);
        for turn in 0..5 {
            if turn == 1 {
                g.entities[3].view.x += 60.;
            }
            if turn == 2 {
                g.entities[3].view.downed = true;
            }
            if turn == 3 {
                g.entities[4].transport = Some("carrier".into());
            }
            if turn == 4 {
                g.config["fogEnabled"] = json!(false);
            }
            let observed = g.visibility_mask();
            for (i, e) in g.entities.iter().enumerate() {
                for owner in [0, 1] {
                    assert_eq!(observed[i][owner as usize], g.visible(owner, e));
                }
            }
        }
    }
    #[test]
    fn saved_support_gun_inside_a_runway_building_drives_out_without_losing_its_order() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("runway-recovery", 384, 384);
        g.terrain.authored = vec![0.52; 384 * 384];
        g.terrain.hydrate();
        g.deploy_base(0);
        let base = g
            .entities
            .iter()
            .find(|e| e.view.kind == "airfield")
            .unwrap()
            .view
            .clone();
        let start = crate::airfield_layout::world(&base, crate::airfield_layout::HANGAR_AT);
        let goal = crate::airfield_layout::world(&base, [12., 70.]);
        let id = g.spawn("long", 0, start);
        let index = g.entities.len() - 1;
        assert_eq!(
            g.issue(
                0,
                &[id],
                Order {
                    kind: "move".into(),
                    destination: Some(goal),
                    target: None
                },
                false
            ),
            1
        );
        let radius = body_clearance("long", g.entities[index].view.model_scale);
        let mut overlap = g.navigation_world().overlap(start, radius);
        for _ in 0..4000 {
            g.tick += 1;
            g.movement();
            let next = g
                .navigation_world()
                .overlap(position(&g.entities[index]), radius);
            assert!(
                next <= overlap + 0.001,
                "recovery must not move deeper into a building"
            );
            overlap = next;
            if distance(position(&g.entities[index]), goal) < 0.6 {
                break;
            }
        }
        assert!(
            distance(position(&g.entities[index]), goal) < 0.6,
            "saved support gun stayed trapped at {:?}",
            position(&g.entities[index])
        );
    }
    #[test]
    fn support_guns_drive_around_runway_hangars_and_offices_for_both_teams() {
        for owner in [0, 1] {
            let mut g = NativeGame::new();
            g.entities.clear();
            g.props.clear();
            g.bases.clear();
            g.terrain = crate::terrain::Terrain::new("runway-navigation", 384, 384);
            g.terrain.authored = vec![0.52; 384 * 384];
            g.terrain.hydrate();
            g.deploy_base(owner);
            let base = g
                .entities
                .iter()
                .find(|e| e.view.kind == "airfield")
                .unwrap()
                .view
                .clone();
            for (center, _) in crate::airfield_layout::SOLIDS {
                let start = crate::airfield_layout::world(&base, [center[0] - 23., center[1]]);
                let goal = crate::airfield_layout::world(&base, [center[0] + 23., center[1]]);
                let id = g.spawn("long", owner, start);
                let index = g.entities.len() - 1;
                let radius = body_clearance("long", g.entities[index].view.model_scale);
                assert!(g.passable_clearance(start, 0, radius));
                assert_eq!(
                    g.issue(
                        owner,
                        &[id],
                        Order {
                            kind: "move".into(),
                            destination: Some(goal),
                            target: None
                        },
                        false
                    ),
                    1
                );
                for _ in 0..4000 {
                    g.tick += 1;
                    g.movement();
                    assert!(g.passable_clearance(position(&g.entities[index]), 0, radius - 0.001));
                    if distance(position(&g.entities[index]), goal) < 0.6 {
                        break;
                    }
                }
                assert!(
                    distance(position(&g.entities[index]), goal) < 0.6,
                    "support gun stuck: team={owner}, building={center:?}, at={:?}, goal={goal:?}, path={:?}",
                    position(&g.entities[index]),
                    g.entities[index].path
                );
                g.entities.remove(index);
            }
        }
    }
    #[test]
    fn local_search_widens_to_find_a_route_around_a_long_barrier() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.terrain = crate::terrain::Terrain::new("wide-detour", 200, 200);
        g.terrain.authored = vec![0.52; 200 * 200];
        g.terrain.hydrate();
        for z in (10..=130).step_by(4) {
            g.props.push(BattlefieldProp {
                scar: None,
                id: format!("barrier-{z}"),
                kind: "wall".into(),
                at: [30., z as f32],
                heading: std::f32::consts::FRAC_PI_2,
                hp: 5.,
                wreck: None,
            });
        }
        let path = g.path([25., 70.], [35., 70.], false);
        assert!(path.len() > 110);
        assert!(path.iter().all(|p| g.passable(*p, false)));
    }
    fn fixture() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("clearance-test", 160, 160);
        g.terrain.authored = vec![0.52; 160 * 160];
        g.terrain.authored_scale = 12.;
        g.terrain.hydrate();
        g.spawn("village", -1, [70., 70.]);
        g.entities[0].view.lots = vec![crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 10.,
            depth: 10.,
            height: 3.2,
            yaw: 0.45,
            style: 0,
            ground: 0.,
            integrity: 1.,
        }];
        g.refresh_towns();
        g
    }
    #[test]
    fn tank_routes_steering_and_separation_keep_the_full_hull_outside_rotated_walls() {
        let mut g = fixture();
        let unit = g.spawn("medium", 0, [49., 60.]);
        g.spawn("medium", 0, [85., 59.]);
        let index = 1;
        let radius = body_clearance("medium", g.entities[index].view.model_scale);
        assert_eq!(
            g.issue(
                0,
                &[unit],
                Order {
                    kind: "move".into(),
                    destination: Some([90., 77.]),
                    target: None
                },
                false
            ),
            1
        );
        let mut arrived = false;
        for _ in 0..2400 {
            g.tick += 1;
            g.movement();
            let p = &g.entities[index].view;
            assert!(
                !g.navigation_world().blocked([p.x, p.z], radius - 0.001),
                "hull crossed a wall at {:?}",
                [p.x, p.z]
            );
            if distance([p.x, p.z], [90., 77.]) < 0.6 {
                arrived = true;
                break;
            }
        }
        assert!(
            arrived,
            "vehicle stalled at {:?}, path={:?}",
            position(&g.entities[index]),
            g.entities[index].path
        );
    }
    #[test]
    fn point_clearance_does_not_admit_a_tank_and_cache_updates_after_a_house_collapses() {
        let mut g = fixture();
        g.entities[0].view.lots[0].yaw = 0.;
        g.refresh_towns();
        let at = [76., 70.];
        assert!(g.passable(at, false));
        assert!(!g.passable_clearance(at, 0, body_clearance("medium", 2.25)));
        let old = g.navigation_world();
        g.entities[0].view.lots[0].integrity = 0.;
        assert!(g.passable_clearance(at, 0, body_clearance("medium", 2.25)));
        assert!(!std::sync::Arc::ptr_eq(&old, &g.navigation_world()));
    }
    #[test]
    fn partially_overlapping_saved_tanks_drive_clear_and_keep_their_move_order() {
        let mut g = fixture();
        g.entities[0].view.lots[0].yaw = 0.;
        g.refresh_towns();
        let unit = g.spawn("medium", 0, [76., 70.]);
        let radius = body_clearance("medium", 2.25);
        assert_eq!(
            g.issue(
                0,
                &[unit],
                Order {
                    kind: "move".into(),
                    destination: Some([90., 80.]),
                    target: None
                },
                false
            ),
            1
        );
        let mut overlap = g
            .navigation_world()
            .overlap(position(&g.entities[1]), radius);
        for _ in 0..1000 {
            g.tick += 1;
            g.movement();
            let next = g
                .navigation_world()
                .overlap(position(&g.entities[1]), radius);
            assert!(next <= overlap + 0.001);
            overlap = next;
            if distance(position(&g.entities[1]), [90., 80.]) < 0.6 {
                break;
            }
        }
        assert_eq!(overlap, 0.);
        assert!(distance(position(&g.entities[1]), [90., 80.]) < 0.6);
    }
}

// Preserve corners in the displayed route without sending every metre-long A* node.
fn display_route(start: Point, path: &VecDeque<Point>) -> Vec<Point> {
    let mut out = Vec::new();
    let mut previous = start;
    let mut points = path.iter().copied().peekable();
    while let Some(point) = points.next() {
        let corner = points.peek().is_none_or(|next| {
            let a = [point[0] - previous[0], point[1] - previous[1]];
            let b = [next[0] - point[0], next[1] - point[1]];
            (a[0] * b[1] - a[1] * b[0]).abs() > 0.0001 || a[0] * b[0] + a[1] * b[1] < 0.
        });
        if corner {
            out.push(point);
            if out.len() == 128 {
                break;
            }
        }
        previous = point;
    }
    out
}

#[cfg(test)]
mod displayed_route_tests {
    use super::*;
    #[test]
    fn displayed_route_keeps_town_corners_and_the_actual_door_endpoint() {
        let path = VecDeque::from([[1., 0.], [2., 0.], [3., 0.], [3., 1.], [3., 2.], [4., 2.]]);
        assert_eq!(
            display_route([0., 0.], &path),
            vec![[3., 0.], [3., 2.], [4., 2.]]
        );
    }
    #[test]
    fn small_tank_splash_increase_hits_the_new_outer_band_only() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.spawn("medium", 1, [53.8, 50.]);
        g.spawn("medium", 1, [54.2, 50.]);
        for e in &mut g.entities {
            e.view.hp = 20.;
            e.view.max_hp = 20.;
        }
        let radius = number(spec("medium"), "damageRadius", 0.);
        assert_eq!(radius, 4.);
        g.impact(&Projectile {
            owner: 0,
            from: [40., 50.],
            to: [50., 50.],
            born: 0,
            impact: 0,
            damage: 2.,
            radius,
            hit: true,
            weapon: "medium".into(),
            ground_hit: false,
        });
        assert!(g.entities[0].view.hp < 20.);
        assert_eq!(g.entities[1].view.hp, 20.);
    }
}
