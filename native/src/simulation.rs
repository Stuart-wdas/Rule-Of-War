//! Deterministic 20 Hz native RTS simulation. No rendering or JavaScript calls.
use crate::runtime::{Piece, Snapshot};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, VecDeque},
    sync::OnceLock,
};

const DT: f32 = 0.05;
pub const SUPPLY_INTERVAL_TICKS: u64 = 100;
pub const SUPPLY_PER_TICK: f32 = 1.;
pub const CAPTURE_RADIUS: f32 = 32.;
pub const DEPLOYMENT_TICKS: u64 = 120 * 20;
pub const SECURE_SECONDS: f32 = 25.;
pub const VICTORY_HOLD_SECONDS: f32 = 90.;
const BUILDS: [&str; 6] = [
    "mine",
    "barracks",
    "vehicleDepot",
    "airfield",
    "fieldHospital",
    "watchtower",
];
type Point = [f32; 2];
fn distance(a: Point, b: Point) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
fn position(e: &Entity) -> Point {
    [e.view.x, e.view.z]
}
fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
fn turn_rate(kind: &str, scale: f32) -> f32 {
    let rate = match kind {
        "medium" => 0.65,
        "long" => 0.45,
        "xtralong" => 0.35,
        "troopCarrier" => 1.15,
        "antiAir" => 0.85,
        "plane" => 0.3,
        "helicopter" | "huey" => 1.1,
        _ => 0.5,
    };
    rate * (2.25 / scale.max(0.1)).sqrt()
}
fn number(v: &Value, key: &str, fallback: f32) -> f32 {
    v[key].as_f64().map(|n| n as f32).unwrap_or(fallback)
}
fn flag(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap_or(false)
}
pub fn transport_capacity(kind: &str) -> usize {
    number(spec(kind), "transportCapacity", 0.) as usize
}
pub(crate) fn catalog() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../catalog.json")).unwrap())
}
pub fn active_kind(kind: &str) -> bool {
    spec(kind).is_object() && !flag(spec(kind), "retired")
}
pub fn active_catalog() -> Value {
    let mut result = catalog().clone();
    for group in ["units", "buildings"] {
        result[group]
            .as_object_mut()
            .unwrap()
            .retain(|_, v| !flag(v, "retired"));
    }
    result
}
fn spec(kind: &str) -> &'static Value {
    let c = catalog();
    if c["weapons"][kind].is_object() {
        &c["weapons"][kind]
    } else if c["units"][kind].is_object() {
        &c["units"][kind]
    } else {
        &c["buildings"][kind]
    }
}
/// Authored observation radius before map scaling, independent of weapon reach.
pub fn sight_range(kind: &str) -> f32 {
    number(
        spec(kind),
        "visionRange",
        if spec(kind)["category"] == "vehicle" {
            90.
        } else {
            42.
        },
    )
}
/// Starting a new battlefield resets old map-specific balance while keeping preferences.
pub fn core_config(previous: &Value) -> Value {
    let mut config = default_config();
    config["fogEnabled"] = previous["fogEnabled"].as_bool().unwrap_or(true).into();
    config
}
pub fn water(x: f32, z: f32) -> bool {
    (x - 80.).abs() < 4. && ![38., 80., 122.].iter().any(|v| (z - v).abs() < 4.)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Order {
    kind: String,
    destination: Option<Point>,
    target: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Entity {
    pub view: Piece,
    orders: VecDeque<Order>,
    path: VecDeque<Point>,
    stance: String,
    cooldown: u32,
    windup: u32,
    target: Option<String>,
    speed: f32,
    stationary: u32,
    fuel: f32,
    home: Option<String>,
    #[serde(default)]
    aircraft: AircraftState,
    #[serde(default)]
    gunship: Option<GunshipMission>,
    transport: Option<String>,
    #[serde(default)]
    helicopter: HelicopterTactics,
    capture_owner: i32,
    capture_progress: f32,
    #[serde(default)]
    patrol_origin: Option<Point>,
    #[serde(default)]
    pub bridge_end: Option<Point>,
    #[serde(default)]
    blocking: Vec<Point>,
    #[serde(default)]
    pub(crate) downed_until: u64,
    #[serde(default)]
    last_hit: u64,
    #[serde(default)]
    threat: Option<Point>,
    #[serde(default)]
    threat_until: u64,
    #[serde(default)]
    cover_goal: Option<Point>,
    #[serde(default)]
    cover_ordered: bool,
    #[serde(default)]
    last_shot: u64,
    #[serde(default)]
    medical_until: u64,
    #[serde(default)]
    treatment_target: Option<String>,
    #[serde(default)]
    treatment_start: u64,
    #[serde(default)]
    tactical_until: u64,
    #[serde(default)]
    gun_heading: Option<f32>,
    #[serde(default)]
    warfare: InfantryWarfare,
    #[serde(default)]
    aa_target: Option<String>,
    #[serde(default)]
    aa_lock_until: u64,
    #[serde(default)]
    aa_reload_until: u64,
    #[serde(default)]
    aa_pose_until: u64,
    /// A human destination/position owns locomotion until replaced. Local AI may
    /// still fire, but cannot substitute a cover, flank or medical destination.
    #[serde(default)]
    player_ordered: bool,
    #[serde(default)]
    halted: bool,
    #[serde(default)]
    cover_retry_after: u64,
    #[serde(default)]
    attack_route_after: u64,
}
impl Entity {
    fn cancel_local_actions(&mut self) {
        self.cover_retry_after = 0;
        self.attack_route_after = 0;
        self.warfare = InfantryWarfare::default();
        self.tactical_until = 0;
        self.cover_goal = None;
        self.cover_ordered = false;
        self.medical_until = 0;
        self.treatment_target = None;
        self.aa_target = None;
        self.aa_lock_until = 0;
        self.aa_pose_until = 0;
        self.target = None;
        self.windup = 0;
    }
    fn release_stop(&mut self) {
        if self.halted {
            self.stance = "attack".into();
        }
        self.halted = false;
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Job {
    id: String,
    owner: i32,
    kind: String,
    unit_type: String,
    producer: Option<String>,
    at: Point,
    cost: f32,
    progress: u32,
    duration: u32,
    #[serde(default)]
    end: Option<Point>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Projectile {
    owner: i32,
    from: Point,
    to: Point,
    born: u64,
    impact: u64,
    damage: f32,
    radius: f32,
    hit: bool,
    #[serde(default)]
    weapon: String,
    #[serde(default)]
    ground_hit: bool,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct NativeGame {
    #[serde(default)]
    pub terrain: crate::terrain::Terrain,
    #[serde(default)]
    pub bases: Vec<BasePlan>,
    #[serde(default)]
    pub world: crate::world::World,
    #[serde(default)]
    pub sector: Option<usize>,
    #[serde(default = "default_config")]
    pub config: Value,
    version: u32,
    pub tick: u64,
    pub paused: bool,
    pub winner: Option<i32>,
    pub entities: Vec<Entity>,
    jobs: Vec<Job>,
    projectiles: Vec<Projectile>,
    #[serde(default)]
    guided_missiles: Vec<GuidedMissile>,
    pub supply: [f32; 2],
    hold: [f32; 2],
    rng: u32,
    counter: u64,
    pub notice: String,
    #[serde(default)]
    pub human_opponent: bool,
    #[serde(default)]
    pub simulation: bool,
    #[serde(default)]
    pub training: Option<TrainingGround>,
    #[serde(default)]
    pub props: Vec<BattlefieldProp>,
    #[serde(default)]
    events: Vec<BattleEvent>,
    #[serde(skip)]
    navigation: NavigationCache,
    #[serde(skip)]
    spotting: SpottingCache,
}
impl Default for NativeGame {
    fn default() -> Self {
        Self::operation(
            crate::battlefield::ID,
            crate::battlefield::COLS,
            crate::battlefield::ROWS,
            default_config(),
        )
    }
}
impl NativeGame {
    pub fn new() -> Self {
        let mut s = Self {
            terrain: crate::terrain::Terrain::default(),
            bases: vec![],
            world: crate::world::World::default(),
            sector: None,
            config: default_config(),
            version: 1,
            tick: 0,
            paused: false,
            winner: None,
            entities: vec![],
            jobs: vec![],
            projectiles: vec![],
            guided_missiles: vec![],
            supply: [80., 80.],
            hold: [0., 0.],
            rng: 719381,
            counter: 0,
            notice:
                "Deploy for 2 minutes, secure posts with infantry, then hold all for 90 seconds."
                    .into(),
            human_opponent: false,
            simulation: false,
            training: None,
            props: vec![],
            events: vec![],
            navigation: NavigationCache::default(),
            spotting: SpottingCache::default(),
        };
        for owner in [0, 1] {
            let x = if owner == 0 { 18. } else { 141. };
            for (kind, z) in [("barracks", 78.), ("mine", 86.), ("vehicleDepot", 70.)] {
                s.spawn(kind, owner, [x, z]);
            }
            for (i, kind) in [
                "commander",
                "short",
                "short",
                "medium",
                "scout",
                "engineer",
                "medic",
            ]
            .iter()
            .enumerate()
            {
                s.spawn(
                    kind,
                    owner,
                    [x + if owner == 0 { 7. } else { -7. }, 67. + i as f32 * 4.],
                );
            }
        }
        for z in [38., 80., 122.] {
            s.spawn("objective", -1, [80., z]);
        }
        s
    }
    fn id(&mut self) -> String {
        self.counter += 1;
        format!("native-{}", self.counter)
    }
    pub fn operation(id: &str, cols: i32, rows: i32, config: Value) -> Self {
        if id == crate::gis_prototype::ID {
            return Self::gis_prototype(config);
        }
        if id == crate::training::ID {
            return Self::training_ground(config);
        }
        Self::operation_terrain(crate::terrain::Terrain::new(id, cols, rows), config)
    }
    pub fn simulation(config: Value) -> Self {
        let mut game = Self::operation(
            crate::battlefield::ID,
            crate::battlefield::COLS,
            crate::battlefield::ROWS,
            config,
        );
        game.simulation = true;
        game.notice = "SIMULATION / AI vs AI · Space: pause · WASD / drag: pan · Wheel: zoom · F2: restart · Esc: menu".into();
        game
    }
    fn operation_terrain(terrain: crate::terrain::Terrain, config: Value) -> Self {
        let id = terrain.id.clone();
        let mut s = Self::new();
        s.terrain = terrain;
        s.config = config;
        s.config["cols"] = json!(s.terrain.cols);
        s.config["rows"] = json!(s.terrain.rows);
        s.entities.clear();
        s.counter = 0;
        let (cols, rows) = (s.terrain.cols, s.terrain.rows);
        for owner in [0, 1] {
            let prefix = if owner == 0 { "player" } else { "enemy" };
            s.supply[owner as usize] = number(&s.config, &format!("{prefix}Supply"), 25.);
            let base = s.deploy_base(owner);
            let mut infantry_slot = 0;
            let mut vehicle_slot = 0;
            for (kind, suffix, fallback) in [
                ("commander", "Commander", 1.),
                ("short", "Short", 1.),
                ("medic", "Medic", 1.),
                ("engineer", "Engineer", 1.),
                ("scout", "Scout", 1.),
                ("medium", "Medium", 1.),
                ("long", "Long", 0.),
                ("troopCarrier", "TroopCarrier", 1.),
                ("xtralong", "Xtralong", 1.),
            ] {
                let count = number(&s.config, &format!("{prefix}{suffix}"), fallback) as usize;
                for _ in 0..count {
                    let direction = if owner == 0 { 1. } else { -1. };
                    let scale = s.bases.last().unwrap().scale;
                    let vehicle = spec(kind)["category"] == "vehicle";
                    let target = if vehicle {
                        let p = [
                            base[0] + direction * 16. * scale,
                            base[1] + direction * (-29. + vehicle_slot as f32 * 9.) * scale,
                        ];
                        vehicle_slot += 1;
                        p
                    } else {
                        let p = [
                            base[0] - direction * 5. * scale,
                            base[1] + direction * (-7. + infantry_slot as f32 * 7.) * scale,
                        ];
                        infantry_slot += 1;
                        p
                    };
                    let target = [
                        target[0].clamp(1., cols as f32 - 2.),
                        target[1].clamp(1., rows as f32 - 2.),
                    ];
                    if let Some(at) = s.open_land(target, kind) {
                        s.spawn(kind, owner, at);
                        s.entities.last_mut().unwrap().view.heading =
                            if owner == 0 { 0. } else { std::f32::consts::PI };
                    }
                }
            }
        }
        let mut seed_bytes = [0_u8; 4];
        if getrandom::fill(&mut seed_bytes).is_ok() {
            s.rng = u32::from_le_bytes(seed_bytes).max(1);
        }
        s.add_settlements();
        s.refresh_towns();
        s.seed_cover();
        s.notice = format!("{} | {} x {} operation established", id, cols, rows);
        s
    }
    fn open_land(&self, at: Point, kind: &str) -> Option<Point> {
        for radius in 0..self.terrain.cols.max(self.terrain.rows) {
            for (dx, dz) in [
                (radius, 0),
                (-radius, 0),
                (0, radius),
                (0, -radius),
                (radius, radius),
                (-radius, -radius),
                (radius, -radius),
                (-radius, radius),
            ] {
                let p = [at[0] + dx as f32, at[1] + dz as f32];
                if self.passable_clearance(
                    p,
                    0,
                    body_clearance(kind, number(spec(kind), "worldScale", 1.)),
                ) && !self.entities.iter().any(|e| distance(position(e), p) < 4.)
                    && (!catalog()["buildings"][kind].is_object() || self.clear_site(p))
                {
                    return Some(p);
                }
            }
        }
        None
    }
    pub fn start_sector(&mut self, index: usize) -> Result<(), String> {
        if !self.world.can_contest(index, 0) {
            return Err("Choose a hostile sector adjacent to friendly territory.".into());
        }
        let mut world = self.world.clone();
        world.sectors[index].contested = true;
        world.sectors[index].battle_id = Some(format!("battle-{}", world.turn));
        let terrain = crate::terrain::Terrain::new_with_seed(
            "custom",
            self.terrain.cols,
            self.terrain.rows,
            Some(world.sectors[index].terrain_seed.clone()),
        );
        let mut next = Self::operation_terrain(terrain, self.config.clone());
        next.terrain
            .apply_modifications(&world.sectors[index].modifications);
        next.redeploy();
        next.world = world;
        next.sector = Some(index);
        *self = next;
        Ok(())
    }
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f64 / 4294967296.) as f32
    }
    fn weapon_range(&self, kind: &str) -> f32 {
        let key = match kind {
            "short" => "shortRange",
            "medium" => "mediumRange",
            "long" => "longRange",
            "xtralong" => "xtralongRange",
            _ => "",
        };
        number(&self.config, key, number(spec(kind), "range", 0.))
    }
    pub fn spawn(&mut self, kind: &str, owner: i32, at: Point) -> String {
        let s = spec(kind);
        let id = self.id();
        self.entities.push(Entity {
            view: Piece {
                id: id.clone(),
                kind: kind.into(),
                owner,
                x: at[0],
                z: at[1],
                elevation: self.terrain.height(at[0], at[1]),
                hp: number(s, "lives", 1.),
                max_hp: number(s, "lives", 1.),
                heading: 0.,
                model_scale: number(s, "worldScale", 1.),
                building: catalog()["buildings"][kind].is_object(),
                category: s["category"].as_str().unwrap_or("building").into(),
                airborne: false,
                label: s["label"].as_str().unwrap_or(kind).into(),
                strength: number(s, "formationStrength", 1.) as u32,
                bridge_end: None,
                town_blocks: vec![],
                ..Default::default()
            },
            orders: VecDeque::new(),
            path: VecDeque::new(),
            stance: "attack".into(),
            cooldown: 0,
            windup: 0,
            target: None,
            speed: 0.,
            stationary: 0,
            fuel: number(s, "fuelCapacity", 0.),
            home: None,
            aircraft: AircraftState {
                flares: initial_flare_salvos(),
                ..Default::default()
            },
            gunship: None,
            transport: None,
            helicopter: HelicopterTactics::default(),
            capture_owner: -1,
            capture_progress: 0.,
            patrol_origin: None,
            bridge_end: None,
            downed_until: 0,
            last_hit: 0,
            threat: None,
            threat_until: 0,
            cover_goal: None,
            cover_ordered: false,
            last_shot: 0,
            medical_until: 0,
            treatment_target: None,
            treatment_start: 0,
            tactical_until: 0,
            gun_heading: None,
            warfare: InfantryWarfare::default(),
            aa_target: None,
            aa_lock_until: 0,
            aa_reload_until: 0,
            aa_pose_until: 0,
            player_ordered: false,
            halted: false,
            cover_retry_after: 0,
            attack_route_after: 0,
            blocking: s["blockingOffsets"]
                .as_array()
                .map(|offsets| {
                    offsets
                        .iter()
                        .filter_map(|p| {
                            Some([at[0] + p[0].as_f64()? as f32, at[1] + p[1].as_f64()? as f32])
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
        if ["town", "village"].contains(&kind) {
            self.refresh_towns();
        }
        id
    }
    pub fn visible(&self, owner: i32, e: &Entity) -> bool {
        self.observation().sees_entity(owner, e)
    }
    /// Share observation results within a simulation phase. Recomputing every
    /// observer's sight radius for every candidate of every unit made movement cubic.
    fn visibility_mask(&self) -> Vec<[bool; 2]> {
        self.observation().mask(&self.entities)
    }
    /// Window occupants remain combat targets, but only their owner sees the interior.
    fn snapshot_visible(&self, owner: i32, e: &Entity, visible: bool) -> bool {
        visible
            && (e.view.owner == owner
                || !e
                    .view
                    .garrison
                    .as_ref()
                    .is_some_and(|g| g.entered || g.door_stage == 1))
    }
    fn passable(&self, at: Point, air: bool) -> bool {
        if !at.iter().all(|v| v.is_finite())
            || at[0] < 1.
            || at[1] < 1.
            || at[0] > self.terrain.cols as f32 - 2.
            || at[1] > self.terrain.rows as f32 - 2.
        {
            return false;
        }
        if air {
            return true;
        }
        (!self.terrain.cell(at[0], at[1]).water() || self.on_bridge(at))
            && !self.props.iter().any(|p| {
                if p.blocks(at) {
                    return true;
                }
                if p.kind != "wall" || p.hp <= 0. {
                    return false;
                }
                let (dx, dz) = (at[0] - p.at[0], at[1] - p.at[1]);
                (dx * p.heading.cos() + dz * p.heading.sin()).abs() < 2.1
                    && (-dx * p.heading.sin() + dz * p.heading.cos()).abs() < 0.4
            })
            && !self.entities.iter().any(|e| {
                e.view.building
                    && e.view.kind != "objective"
                    && e.view.kind != "bridge"
                    && e.view.hp > 0.
                    && if !e.view.lots.is_empty() {
                        e.view.lots.iter().any(|lot| {
                            lot.contains(building_local(e, at), 0.25 / e.view.model_scale.max(0.1))
                        })
                    } else if e.blocking.is_empty() {
                        self.building_contains(e, at, 0.25)
                    } else {
                        e.blocking
                            .iter()
                            .any(|p| (p[0] - at[0]).abs() < 0.51 && (p[1] - at[1]).abs() < 0.51)
                    }
            })
    }
    /// Weighted four-neighbour A*: never cuts through river banks or structures.
    fn on_bridge(&self, at: Point) -> bool {
        self.entities.iter().any(|e| {
            e.view.hp > 0.
                && e.bridge_end.is_some_and(|end| {
                    segment_distance(at, position(e), end)
                        <= number(spec("bridge"), "width", 2.) * 0.5
                })
        })
    }
    #[cfg(test)]
    pub fn path(&self, start: Point, goal: Point, air: bool) -> VecDeque<Point> {
        self.path_mode(start, goal, if air { 1 } else { 0 })
    }
    fn sea_passable(&self, at: Point) -> bool {
        self.passable(at, true) && self.terrain.cell(at[0], at[1]).water()
    }
    fn route(&self, index: usize, start: Point, goal: Point) -> VecDeque<Point> {
        let s = spec(&self.entities[index].view.kind);
        let mode = if flag(s, "airUnit") {
            1
        } else if flag(s, "waterUnit") {
            2
        } else {
            0
        };
        let radius = body_clearance(
            &self.entities[index].view.kind,
            self.entities[index].view.model_scale,
        );
        let path = self.path_mode_clearance(start, goal, mode, radius);
        // Steer towards visible corners, not every metre of the A* grid. Dense
        // waypoints make slow-turning vehicles orbit a corner they just missed.
        let world = self.navigation_world();
        let points: Vec<_> = path.into_iter().collect();
        let mut result = VecDeque::new();
        let mut from = start;
        let mut first = 0;
        while first < points.len() {
            let last = (first..points.len().min(first + 128))
                .rev()
                .find(|n| self.navigation_segment(&world, from, points[*n], mode, radius))
                .unwrap_or(first);
            result.push_back(points[last]);
            from = points[last];
            first = last + 1;
        }
        result
    }
    pub(crate) fn navigation_benchmark(&mut self) -> Value {
        self.entities.clear();
        self.props.clear();
        self.bases.clear();
        self.spawn("town", -1, [192., 192.]);
        let unit = self.spawn("medium", 0, [130., 130.]);
        let index = self
            .entities
            .iter()
            .position(|e| e.view.id == unit)
            .unwrap();
        let mut timings = vec![];
        for (start, goal) in [
            ([140., 140.], [245., 245.]),
            ([140., 170.], [240., 218.]),
            ([125., 192.], [260., 192.]),
            ([160., 146.], [226., 237.]),
        ] {
            let begin = std::time::Instant::now();
            let path = self.route(index, start, goal);
            timings.push(json!({"from":start,"to":goal,"waypoints":path.len(),"ms":begin.elapsed().as_secs_f64()*1000.}));
        }
        json!(timings)
    }
    #[cfg(test)]
    fn path_mode(&self, start: Point, goal: Point, mode: u8) -> VecDeque<Point> {
        self.path_mode_clearance(start, goal, mode, 0.)
    }
    fn path_mode_clearance(
        &self,
        start: Point,
        goal: Point,
        mode: u8,
        radius: f32,
    ) -> VecDeque<Point> {
        let _query = crate::performance::detailed(
            "navigation.query",
            || json!({"from":start,"to":goal,"mode":mode,"radius":radius}),
        );
        let _work = NavigationWork::new(16384);
        let world = self.navigation_world();
        let goal = if mode == 0 && radius > 0. {
            self.clearance_destination(goal, radius).unwrap_or(goal)
        } else {
            goal
        };
        let _timing = crate::performance::span("navigation.search");
        let passable = |p: Point| {
            if mode == 1 {
                self.passable(p, true)
            } else {
                self.ground_passable(p, mode) && !world.blocked(p, radius)
            }
        };
        let segment = |a: Point, b: Point| self.navigation_segment(&world, a, b, mode, radius);
        if !passable(goal) {
            return VecDeque::new();
        }
        if mode == 1 {
            return VecDeque::from([goal]);
        }
        // Open-ground orders avoid allocating a map-sized A* search.
        if segment(start, goal) {
            let steps = distance(start, goal).ceil().max(1.) as usize;
            return (1..=steps)
                .map(|i| {
                    let t = i as f32 / steps as f32;
                    [
                        start[0] + (goal[0] - start[0]) * t,
                        start[1] + (goal[1] - start[1]) * t,
                    ]
                })
                .collect();
        }
        // A valid fractional point can round into a wall or wreck. Searching for
        // that blocked grid node used to exhaust the entire map for 5–10 seconds.
        // Pick a nearby clear anchor connected to the real endpoint instead.
        let anchor = |at: Point| {
            let center = (at[0].round() as i32, at[1].round() as i32);
            let mut nodes: Vec<_> = (-1..=1)
                .flat_map(|z| (-1..=1).map(move |x| (center.0 + x, center.1 + z)))
                .collect();
            nodes.sort_by(|a, b| {
                distance(at, [a.0 as f32, a.1 as f32])
                    .total_cmp(&distance(at, [b.0 as f32, b.1 as f32]))
                    .then_with(|| a.cmp(b))
            });
            nodes.into_iter().find(|n| {
                let point = [n.0 as f32, n.1 as f32];
                passable(point) && segment(at, point)
            })
        };
        let (Some(a), Some(b)) = (anchor(start), anchor(goal)) else {
            return VecDeque::new();
        };
        let failure = FailedRoute {
            start: a,
            goal: b,
            mode,
            radius: radius.to_bits(),
            terrain: self.terrain.revision,
            allowance: NavigationWork::remaining(),
        };
        if let Some(previous) = world.failed_routes.lock().unwrap().get(&failure)
            && previous.until > self.tick
        {
            // Charge the same logical work on a cache hit. Reloading a save
            // (with an empty CPU cache) must leave identical planning allowance.
            SEARCH_WORK.with(|b| b.set(b.get().saturating_sub(previous.work)));
            return VecDeque::new();
        }
        let inside = |p: (i32, i32)| {
            p.0 >= 0 && p.1 >= 0 && p.0 < self.terrain.cols && p.1 < self.terrain.rows
        };
        if !inside(a) || !inside(b) {
            return VecDeque::new();
        }
        if a == b {
            return VecDeque::from([[a.0 as f32, a.1 as f32], goal]);
        }
        let search = |margin: i32| {
            let left = (a.0.min(b.0) - margin).max(0);
            let top = (a.1.min(b.1) - margin).max(0);
            let right = (a.0.max(b.0) + margin).min(self.terrain.cols - 1);
            let bottom = (a.1.max(b.1) + margin).min(self.terrain.rows - 1);
            let width = (right - left + 1) as usize;
            let inside = |(x, z): (i32, i32)| x >= left && x <= right && z >= top && z <= bottom;
            let key = |(x, z): (i32, i32)| (z - top) as usize * width + (x - left) as usize;
            let count = width * (bottom - top + 1) as usize;
            // Dense arrays avoid millions of tree allocations; tile costs are sampled once.
            let mut costs = vec![u32::MAX; count];
            let mut previous = vec![u32::MAX; count];
            let mut closed = vec![false; count];
            let mut tile_cost = vec![0u8; count];
            let mut open = BinaryHeap::new();
            costs[key(a)] = 0;
            open.push(Reverse((0u32, 0u32, a)));
            while let Some(Reverse((_, _, p))) = open.pop() {
                let current_key = key(p);
                if closed[current_key] {
                    continue;
                }
                if !NavigationWork::expand() {
                    break;
                }
                closed[current_key] = true;
                if p == b {
                    let mut out = VecDeque::new();
                    let mut current = key(b);
                    while current != key(a) {
                        out.push_front([
                            (current % width) as f32 + left as f32,
                            (current / width) as f32 + top as f32,
                        ]);
                        let parent = previous[current];
                        if parent == u32::MAX {
                            return VecDeque::new();
                        }
                        current = parent as usize;
                    }
                    // A rounded start must be reached before following grid edges around a corner.
                    let first = [a.0 as f32, a.1 as f32];
                    if distance(start, first) > 0.01 && passable(first) && segment(start, first) {
                        out.push_front(first);
                    }
                    if out
                        .back()
                        .is_some_and(|last| distance(*last, goal) > 0.01 && segment(*last, goal))
                    {
                        out.push_back(goal);
                    }
                    return out;
                }
                for n in [
                    (p.0 - 1, p.1),
                    (p.0 + 1, p.1),
                    (p.0, p.1 - 1),
                    (p.0, p.1 + 1),
                ] {
                    if !inside(n) {
                        continue;
                    }
                    let next_key = key(n);
                    if closed[next_key] {
                        continue;
                    }
                    if tile_cost[next_key] == 0 {
                        let point = [n.0 as f32, n.1 as f32];
                        tile_cost[next_key] = if !passable(point) {
                            255
                        } else if mode == 2 {
                            10
                        } else {
                            let cell = self.terrain.cell(point[0], point[1]);
                            if cell.water() {
                                10
                            } else {
                                (cell.cost() * 10.) as u8
                            }
                        };
                    }
                    if tile_cost[next_key] == 255 {
                        continue;
                    }
                    // A clear grid node can still lie across a rotated wall corner.
                    if !segment([p.0 as f32, p.1 as f32], [n.0 as f32, n.1 as f32]) {
                        continue;
                    }
                    let next = costs[current_key] + tile_cost[next_key] as u32;
                    if next < costs[next_key] {
                        costs[next_key] = next;
                        previous[next_key] = current_key as u32;
                        let remaining = ((n.0 - b.0).abs() + (n.1 - b.1).abs()) as u32 * 10;
                        // Equal-cost routes should advance toward the goal.
                        // Coordinate-only ties flooded the entire rectangle on
                        // long diagonal orders, exhausting the work allowance.
                        open.push(Reverse((next + remaining, remaining, n)));
                    }
                }
            }
            VecDeque::new()
        };
        for margin in [24, 96, self.terrain.cols.max(self.terrain.rows)] {
            if NavigationWork::remaining() == 0 {
                break;
            }
            let path = search(margin);
            if !path.is_empty() {
                return path;
            }
        }
        let mut failed = world.failed_routes.lock().unwrap();
        if failed.len() >= 512 {
            failed.retain(|_, previous| previous.until > self.tick);
            if failed.len() >= 512 {
                failed.clear();
            }
        }
        let spent = failure
            .allowance
            .saturating_sub(NavigationWork::remaining());
        failed.insert(
            failure,
            FailedSearch {
                until: self.tick + 20,
                work: spent,
            },
        );
        VecDeque::new()
    }
    fn issue(&mut self, owner: i32, ids: &[String], order: Order, queued: bool) -> usize {
        self.issue_controlled(owner, ids, order, queued, false)
    }
    fn issue_controlled(
        &mut self,
        owner: i32,
        ids: &[String],
        order: Order,
        queued: bool,
        player: bool,
    ) -> usize {
        let eligible: Vec<_> = ids
            .iter()
            .filter(|id| {
                self.entities.iter().any(|e| {
                    &e.view.id == *id
                        && e.view.owner == owner
                        && e.view.hp > 0.
                        && !e.view.building
                        && !e.view.downed
                        && e.transport.is_none()
                })
            })
            .collect();
        let mut accepted = 0;
        let mut destinations = vec![];
        for (slot, id) in eligible.iter().enumerate() {
            let id = *id;
            let Some(i) = self.entities.iter().position(|e| {
                &e.view.id == id
                    && e.view.owner == owner
                    && e.view.hp > 0.
                    && !e.view.building
                    && e.transport.is_none()
            }) else {
                continue;
            };
            let activate = !queued || self.entities[i].orders.is_empty();
            if flag(spec(&self.entities[i].view.kind), "airUnit") {
                let mut air_order = order.clone();
                if matches!(self.entities[i].view.kind.as_str(), "helicopter" | "huey")
                    && air_order.kind != "attack"
                    && let Some(goal) = air_order.destination
                {
                    let goal =
                        self.formation_destination(i, goal, slot, eligible.len(), &destinations);
                    air_order.destination = Some(goal);
                    destinations.push(goal);
                }
                accepted += self.aircraft_order(i, &air_order, queued) as usize;
                continue;
            }
            if order.kind == "retreat" {
                self.entities[i].warfare = InfantryWarfare::default();
                accepted += self.retreat(i) as usize;
                continue;
            }
            // Combat/stop commands must release an unfinished entry reservation.
            // Otherwise firing_line rejects every shot while an obsolete garrison
            // route continues to own the unit, even after a direct attack click.
            if !queued
                && ["attack", "attack-stance", "stop", "hold", "flee"]
                    .contains(&order.kind.as_str())
                && self.entities[i]
                    .view
                    .garrison
                    .as_ref()
                    .is_some_and(|g| !g.entered && !g.exiting)
            {
                self.leave_building(i, false);
            }
            if !queued
                && ["move", "attack-move", "patrol"].contains(&order.kind.as_str())
                && !self.leave_building(i, false)
            {
                continue;
            }
            self.entities[i].view.retreating = false;
            if ["attack", "attack-move"].contains(&order.kind.as_str()) {
                self.entities[i].stance = "attack".into();
            }
            if activate {
                self.entities[i].cancel_local_actions();
            }
            if self.entities[i].stance == "retreat" {
                self.entities[i].stance = "attack".into();
            }
            if order.kind == "stop" || order.kind == "hold" {
                self.entities[i].orders.clear();
                self.entities[i].path.clear();
                self.entities[i].target = None;
                self.entities[i].patrol_origin = None;
                self.entities[i].speed = 0.;
                self.entities[i].windup = 0;
                self.entities[i].view.velocity = [0., 0.];
                self.entities[i].halted = order.kind == "stop";
                self.entities[i].player_ordered = player;
                self.entities[i].stance = "hold-fire".into();
                accepted += 1;
                continue;
            }
            if ["attack-stance", "flee", "hold-fire"].contains(&order.kind.as_str()) {
                let e = &mut self.entities[i];
                e.halted = false;
                e.player_ordered = false;
                e.stance = if order.kind == "attack-stance" {
                    "attack".into()
                } else {
                    order.kind.clone()
                };
                e.target = None;
                if order.kind == "attack-stance"
                    && e.orders.front().is_some_and(|o| o.kind == "move")
                {
                    e.orders.clear();
                    e.path.clear();
                    e.speed = 0.;
                    e.view.velocity = [0., 0.];
                }
                if order.kind != "attack-stance" {
                    e.orders.clear();
                    e.path.clear();
                    e.patrol_origin = None;
                    e.speed = 0.;
                    e.windup = 0;
                }
                accepted += 1;
                continue;
            }
            if !["move", "attack-move", "attack", "patrol"].contains(&order.kind.as_str()) {
                continue;
            }
            if number(spec(&self.entities[i].view.kind), "speed", 0.) == 0.
                && order.kind != "attack"
            {
                continue;
            }
            let mut unit_order = order.clone();
            if let Some(goal) = unit_order.destination
                && unit_order.kind != "attack"
            {
                let goal = self.formation_destination(i, goal, slot, eligible.len(), &destinations);
                unit_order.destination = Some(goal);
            }
            let goal = unit_order.destination.or_else(|| {
                order.target.as_ref().and_then(|id| {
                    self.entities
                        .iter()
                        .find(|e| &e.view.id == id)
                        .map(position)
                })
            });
            if order.kind == "attack"
                && (order.target.is_none()
                    || goal.is_none()
                    || number(spec(&self.entities[i].view.kind), "damage", 0.) <= 0.)
            {
                continue;
            }
            let start = if queued {
                self.entities[i]
                    .orders
                    .back()
                    .and_then(|o| o.destination)
                    .unwrap_or(
                        self.entities[i]
                            .view
                            .garrison
                            .as_ref()
                            .filter(|g| g.exiting)
                            .and_then(|g| self.garrison_door(g))
                            .map(|d| d.0)
                            .unwrap_or(position(&self.entities[i])),
                    )
            } else {
                self.entities[i]
                    .view
                    .garrison
                    .as_ref()
                    .filter(|g| g.exiting)
                    .and_then(|g| self.garrison_door(g))
                    .map(|d| d.0)
                    .unwrap_or(position(&self.entities[i]))
            };
            let path = if order.kind == "attack"
                && !flag(spec(&self.entities[i].view.kind), "waterUnit")
            {
                order
                    .target
                    .as_ref()
                    .and_then(|id| self.entities.iter().position(|t| &t.view.id == id))
                    .map(|j| {
                        let previous = std::mem::replace(
                            &mut self.entities[i].orders,
                            VecDeque::from([unit_order.clone()]),
                        );
                        let path = self.infantry_attack_route(i, start, j);
                        self.entities[i].orders = previous;
                        path
                    })
                    .unwrap_or_default()
            } else {
                goal.map(|g| self.route(i, start, g)).unwrap_or_default()
            };
            if order.kind != "attack"
                && path.is_empty()
                && goal.is_none_or(|g| distance(start, g) > 0.2)
                && !(self.entities[i].view.category == "vehicle"
                    && self.ground_passable(start, 0)
                    && self.navigation_world().blocked(
                        start,
                        body_clearance(
                            &self.entities[i].view.kind,
                            self.entities[i].view.model_scale,
                        ),
                    ))
            {
                self.notice = "Cannot reach that destination; previous orders kept.".into();
                continue;
            }
            if let Some(goal) = goal {
                destinations.push(goal);
            }
            let ordered_cover = self.entities[i].view.category == "troop"
                && ["move", "attack-move"].contains(&unit_order.kind.as_str())
                && unit_order.destination.is_some_and(|goal| {
                    self.cover_level(goal, self.entity_threat(&self.entities[i])) > 0
                });
            let e = &mut self.entities[i];
            if activate {
                e.player_ordered = player;
                e.release_stop();
            }
            if !queued {
                e.orders.clear();
                e.path.clear();
                e.patrol_origin = (order.kind == "patrol").then_some([e.view.x, e.view.z]);
            }
            if e.orders.is_empty() {
                if unit_order.kind == "attack" {
                    e.attack_route_after = self.tick + if path.is_empty() { 40 } else { 20 };
                }
                e.path = path;
                if e.view.category == "troop"
                    && ["move", "attack-move"].contains(&unit_order.kind.as_str())
                    && let Some(goal) = unit_order.destination
                    && ordered_cover
                {
                    e.cover_goal = Some(goal);
                    e.cover_ordered = true;
                }
            }
            e.orders.push_back(unit_order);
            accepted += 1;
        }
        accepted
    }
    pub fn command(&mut self, msg: Value) {
        self.command_as(0, msg);
    }
    pub fn command_as(&mut self, owner: i32, msg: Value) {
        if !(0..=1).contains(&owner) {
            return;
        }
        let kind = msg["type"].as_str().unwrap_or("");
        if owner != 0 && !["command", "recruit", "build", "call-in"].contains(&kind) {
            return;
        }
        match kind {
            "map" => {
                let id = msg["id"].as_str().unwrap_or("custom");
                let cols = msg["cols"].as_i64().unwrap_or(128) as i32;
                let rows = msg["rows"].as_i64().unwrap_or(128) as i32;
                *self = Self::operation(id, cols, rows, self.config.clone());
                return;
            }
            "new" => {
                if self.simulation {
                    *self = Self::simulation(self.config.clone());
                    return;
                }
                if self.terrain.id == crate::visual_demo::ID {
                    self.setup_visual_demo();
                    return;
                }
                *self = Self::operation(
                    &self.terrain.id,
                    self.terrain.cols,
                    self.terrain.rows,
                    self.config.clone(),
                );
                return;
            }
            "pause" => {
                self.paused = !self.paused;
                return;
            }
            "save" => {
                self.notice = match self.save_to(&self.operation_save_path()) {
                    Ok(()) => "Operation saved.".into(),
                    Err(e) => format!("Save failed: {e}"),
                };
                return;
            }
            "load" => {
                match Self::load_from(&self.operation_save_path()) {
                    Ok(game) => {
                        *self = game;
                        self.notice = "Operation restored.".into();
                    }
                    Err(e) => self.notice = format!("Load failed: {e}"),
                };
                return;
            }
            _ => {}
        }
        // Spectators control playback and files; commanders issue their own orders.
        if self.simulation {
            return;
        }
        if kind == "training" {
            self.notice = self.training_command(owner, &msg);
            return;
        }
        if self.winner.is_some() {
            self.notice = "Operation ended. Press F2 to start again.".into();
            return;
        }
        self.notice = match kind {
            "call-in" if msg["unitType"] == "c130" => {
                self.call_gunship(owner, read_point(&msg["destination"]))
            }
            "cancel-production" => {
                let id = msg["jobId"].as_str().unwrap_or("");
                if let Some(i) = self
                    .jobs
                    .iter()
                    .position(|j| j.id == id && j.owner == owner && j.kind == "recruit")
                {
                    let job = self.jobs.remove(i);
                    self.supply[owner as usize] += job.cost;
                    "Production cancelled; supply refunded.".into()
                } else {
                    "No owned production job selected.".into()
                }
            }
            "recruit" => self.recruit(
                msg["unitType"].as_str().unwrap_or(""),
                msg["producerId"].as_str().unwrap_or(""),
                owner,
            ),
            "build" if msg["buildingType"] == "bridge" => self.build_bridge(
                owner,
                read_point(&msg["destination"]),
                read_point(&msg["end"]),
            ),
            "build" => self.build_as(
                owner,
                msg["buildingType"].as_str().unwrap_or(""),
                read_point(&msg["destination"]),
            ),
            "command" => {
                let ids: Vec<String> = msg["unitIds"]
                    .as_array()
                    .map(|v| {
                        v.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let command = msg["commandType"].as_str().unwrap_or("");
                if command == "assign-squad" {
                    if let Some(squad) = msg["squad"].as_u64().filter(|n| *n < 9) {
                        for e in &mut self.entities {
                            if e.view.owner == owner && !e.view.building {
                                if ids.contains(&e.view.id) {
                                    e.view.squad = Some(squad as u8);
                                } else if e.view.squad == Some(squad as u8) {
                                    e.view.squad = None;
                                }
                            }
                        }
                        format!("Squad {} assigned.", squad + 1)
                    } else {
                        "Invalid squad number.".into()
                    }
                } else if command == "garrison" {
                    let target = msg["targetId"].as_str().map(str::to_owned).or_else(|| {
                        let unit = self.entities.iter().find(|e| {
                            ids.contains(&e.view.id)
                                && e.view.owner == owner
                                && e.view.category == "troop"
                        })?;
                        self.entities
                            .iter()
                            .filter(|e| {
                                e.view.building
                                    && e.view.hp > 0.
                                    && (e.view.owner == owner || e.view.owner < 0)
                            })
                            .flat_map(|e| {
                                if e.view.lots.is_empty() {
                                    self.window_slots(e, None)
                                } else {
                                    (0..e.view.lots.len())
                                        .flat_map(|l| self.window_slots(e, Some(l)))
                                        .collect()
                                }
                                .into_iter()
                                .map(move |slot| {
                                    (e.view.id.clone(), distance(position(unit), slot.0))
                                })
                            })
                            .filter(|(_, d)| *d <= 30.)
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .map(|(id, _)| id)
                    });
                    let count = self.enter_building(
                        owner,
                        &ids,
                        target.as_deref().unwrap_or(""),
                        read_point(&msg["destination"]),
                    );
                    format!("{count} soldier(s) ordered into free building windows.")
                } else if command == "leave-building" {
                    let mut count = 0;
                    for id in &ids {
                        if let Some(i) = self.entities.iter().position(|e| {
                            &e.view.id == id && e.view.owner == owner && e.view.garrison.is_some()
                        }) {
                            count += self.leave_building(i, false) as usize;
                        }
                    }
                    format!("{count} soldier(s) leaving buildings.")
                } else if command == "load" || command == "unload" {
                    if command == "unload" {
                        for id in &ids {
                            if let Some(i) = self.entities.iter().position(|e| {
                                &e.view.id == id
                                    && e.view.owner == owner
                                    && e.view.garrison.is_some()
                            }) {
                                self.leave_building(i, false);
                            }
                        }
                    }
                    self.transport_order(owner, &ids, msg["targetId"].as_str(), command == "load")
                } else {
                    let target = msg["targetId"].as_str().map(str::to_owned);
                    if target.as_ref().is_some_and(|id| {
                        !self.entities.iter().any(|e| {
                            &e.view.id == id
                                && (e.view.owner >= 0 || !e.view.lots.is_empty())
                                && e.view.owner != owner
                                && self.visible(owner, e)
                        })
                    }) {
                        "Target is not visible.".into()
                    } else {
                        self.notice.clear();
                        let count = self.issue_controlled(
                            owner,
                            &ids,
                            Order {
                                kind: command.into(),
                                destination: read_point(&msg["destination"]),
                                target,
                            },
                            flag(&msg, "queued"),
                            true,
                        );
                        if count == 0 && !self.notice.is_empty() {
                            self.notice.clone()
                        } else {
                            format!("Orders acknowledged by {count} unit(s).")
                        }
                    }
                }
            }
            _ => "Unknown command.".into(),
        };
    }
    fn recruit(&mut self, kind: &str, producer: &str, owner: i32) -> String {
        let s = spec(kind);
        if !active_kind(kind) {
            return "Naval units are unavailable in ground operations.".into();
        }
        let Some(p) = self
            .entities
            .iter()
            .find(|e| e.view.id == producer && e.view.owner == owner && e.view.hp > 0.)
        else {
            return "Select an owned production building.".into();
        };
        let compatible = s["productionBuildings"]
            .as_array()
            .map(|a| a.iter().any(|v| v.as_str() == Some(&p.view.kind)))
            .unwrap_or_else(|| {
                s["productionBuilding"].as_str().unwrap_or("barracks") == p.view.kind
            });
        if !catalog()["units"][kind].is_object()
            || s["recruitable"] == false
            || !compatible
            || number(s, "price", 0.) <= 0.
        {
            return "This unit cannot be recruited here.".into();
        }
        if self
            .jobs
            .iter()
            .filter(|j| j.producer.as_deref() == Some(producer))
            .count()
            >= 5
        {
            return "Production queue full (5 units).".into();
        }
        if flag(s, "airUnit") {
            let reserved = self
                .entities
                .iter()
                .filter(|e| {
                    e.home.as_deref() == Some(producer)
                        && crate::airfield_layout::shares_stands(&e.view.kind, kind)
                        && e.view.hp > 0.
                })
                .count()
                + self
                    .jobs
                    .iter()
                    .filter(|j| {
                        j.producer.as_deref() == Some(producer)
                            && crate::airfield_layout::shares_stands(&j.unit_type, kind)
                    })
                    .count();
            if reserved >= crate::airfield_layout::stands(kind).len() {
                return "Airfield capacity reached.".into();
            }
        }
        if flag(s, "airUnit") && self.free_aircraft_stand(producer, kind, None).is_none() {
            return "Airfield capacity reached.".into();
        }
        let cost = number(s, "price", 0.);
        if self.supply[owner as usize] < cost {
            return "Insufficient supply.".into();
        }
        self.supply[owner as usize] -= cost;
        let id = self.id();
        self.jobs.push(Job {
            id,
            owner,
            kind: "recruit".into(),
            unit_type: kind.into(),
            producer: Some(producer.into()),
            at: [0., 0.],
            cost,
            progress: 0,
            duration: ((if flag(s, "airUnit") {
                18.
            } else {
                8. + cost * 0.2
            }) * 20.)
                .round() as u32,
            end: None,
        });
        format!("Recruiting {}.", s["label"].as_str().unwrap_or(kind))
    }
    fn clear_site(&self, at: Point) -> bool {
        [-2., 0., 2.].iter().all(|dx| {
            [-2., 0., 2.]
                .iter()
                .all(|dz| self.passable([at[0] + dx, at[1] + dz], false))
        }) && !self
            .jobs
            .iter()
            .any(|j| j.kind == "building" && distance(j.at, at) < 5.)
            && !self
                .entities
                .iter()
                .any(|e| !e.view.airborne && distance(position(e), at) < 2.5)
    }
    #[cfg(test)]
    fn build(&mut self, kind: &str, at: Option<Point>) -> String {
        self.build_as(0, kind, at)
    }
    fn build_as(&mut self, owner: i32, kind: &str, at: Option<Point>) -> String {
        let Some(at) = at.map(|p| [p[0].round(), p[1].round()]) else {
            return "Invalid site.".into();
        };
        if kind == "machineGunner" {
            let eligible = self.entities.iter().any(|e| {
                e.view.owner == owner
                    && e.view.hp > 0.
                    && e.transport.is_none()
                    && e.view.kind == "engineer"
                    && distance(position(e), at) <= number(spec("engineer"), "movementRange", 10.)
            });
            let cost = number(spec(kind), "price", 16.);
            if !eligible || !self.passable(at, false) || self.supply[owner as usize] < cost {
                return "Machine gun nests require a nearby engineer, dry ground and supply."
                    .into();
            }
            self.supply[owner as usize] -= cost;
            self.spawn(kind, owner, at);
            return "Machine gun nest deployed.".into();
        }
        if !BUILDS.contains(&kind) || !active_kind(kind) {
            return "Invalid building.".into();
        }
        let s = spec(kind);
        if !self.entities.iter().any(|e| {
            e.view.owner == owner
                && e.view.hp > 0.
                && e.transport.is_none()
                && ((e.view.kind == "commander" && !flag(s, "engineerOnly"))
                    || (e.view.kind == "engineer" && !flag(s, "commanderOnly")))
                && distance(position(e), at) <= 9.
        }) {
            return "Build within 9 metres of a commander or engineer.".into();
        }
        if !self.clear_site(at) {
            return "Construction needs clear, dry ground.".into();
        }
        if kind == "port"
            && !(-4..=4).any(|dx| {
                (-4..=4).any(|dz| {
                    self.terrain
                        .cell(at[0] + dx as f32, at[1] + dz as f32)
                        .water()
                })
            })
        {
            return "A port must be adjacent to navigable water.".into();
        }
        let cost = number(s, "price", 0.);
        if self.supply[owner as usize] < cost {
            return "Insufficient supply.".into();
        }
        self.supply[owner as usize] -= cost;
        let id = self.id();
        self.jobs.push(Job {
            id,
            owner,
            kind: "building".into(),
            unit_type: kind.into(),
            producer: None,
            at,
            cost,
            progress: 0,
            duration: if kind == "watchtower" { 100 } else { 200 },
            end: None,
        });
        format!("Constructing {}.", s["label"].as_str().unwrap_or(kind))
    }
    pub fn step(&mut self) {
        crate::performance::tick(self.tick);
        if self.paused {
            return;
        }
        if self.winner.is_some() {
            // A final aircraft kill must still reach the ground after victory is decided.
            if self
                .props
                .iter()
                .any(|p| p.wreck.as_ref().is_some_and(|w| w.crash.is_some()))
            {
                self.tick += 1;
                {
                    let _timing = crate::performance::span("simulation.aircraft_crashes");
                    self.aircraft_crashes();
                }
                {
                    let _timing = crate::performance::span("simulation.record_wrecks");
                    self.record_wrecks();
                }
                self.entities.retain(|e| e.view.hp > 0.);
            }
            return;
        }
        if self.training.is_none() && !self.human_opponent && self.tick.is_multiple_of(100) {
            self.ai();
        }
        self.tick += 1;
        {
            let _timing = crate::performance::span("simulation.tactics");
            self.tactical_step();
        }
        {
            let _timing = crate::performance::span("simulation.production");
            self.production();
        }
        {
            let _timing = crate::performance::span("simulation.boarding_step");
            self.boarding_step();
        }
        {
            let _timing = crate::performance::span("simulation.movement");
            self.movement();
        }
        {
            let _timing = crate::performance::span("simulation.aircraft_crashes");
            self.aircraft_crashes();
        }
        {
            let _timing = crate::performance::span("simulation.garrison_step");
            self.garrison_step();
        }
        {
            let _timing = crate::performance::span("simulation.combat");
            self.combat();
        }
        {
            let _timing = crate::performance::span("simulation.release_collapsed_garrisons");
            self.release_collapsed_garrisons();
        }
        {
            let _timing = crate::performance::span("simulation.record_wrecks");
            self.record_wrecks();
        }
        if self.tick.is_multiple_of(SUPPLY_INTERVAL_TICKS) {
            for supply in &mut self.supply {
                *supply += SUPPLY_PER_TICK;
            }
        }
        if self.tick.is_multiple_of(20) {
            {
                let _timing = crate::performance::span("simulation.economy_support");
                self.economy_support();
            }
        }
        if self.tick.is_multiple_of(5) {
            {
                let _timing = crate::performance::span("simulation.objectives");
                self.objectives();
            }
        }
        let dead: Vec<String> = self
            .entities
            .iter()
            .filter(|e| e.view.hp <= 0.)
            .map(|e| e.view.id.clone())
            .collect();
        self.entities.retain(|e| {
            e.view.hp > 0. && !e.transport.as_ref().is_some_and(|id| dead.contains(id))
        });
        if let Some(training) = &mut self.training {
            training.destroyed += dead
                .iter()
                .filter(|id| training.targets.iter().any(|t| &t.id == *id))
                .count() as u64;
            return;
        }
        let alive = [0, 1].map(|owner| {
            self.entities
                .iter()
                .any(|e| e.view.owner == owner && (!e.view.building || e.view.kind == "barracks"))
        });
        match alive {
            [false, false] => self.winner = Some(-1),
            [true, false] => self.winner = Some(0),
            [false, true] => self.winner = Some(1),
            _ => {}
        }
        if let (Some(index), Some(winner)) = (self.sector, self.winner) {
            self.world.sectors[index].modifications = self.terrain.modifications();
            self.world.outcome(index, winner);
            self.world.sectors[index].contested = false;
            self.world.sectors[index].battle_id = None;
            self.sector = None;
        }
    }
    fn ai(&mut self) {
        if self.simulation {
            self.ai_for(0);
        }
        self.ai_for(1);
    }
    fn ai_for(&mut self, owner: i32) {
        let base = self
            .bases
            .iter()
            .find(|b| b.owner == owner)
            .map(|b| b.origin)
            .unwrap_or([
                if owner == 0 {
                    22.
                } else {
                    self.terrain.cols as f32 - 22.
                },
                self.terrain.rows as f32 * 0.5,
            ]);
        let objective = self
            .entities
            .iter()
            .filter(|e| e.view.kind == "objective" && e.view.owner != owner)
            .min_by(|a, b| distance(base, position(a)).total_cmp(&distance(base, position(b))))
            .or_else(|| self.entities.iter().find(|e| e.view.kind == "objective"))
            .map(position)
            .unwrap_or(base);
        let approach =
            bevy::math::Vec2::from_array([base[0] - objective[0], base[1] - objective[1]])
                .normalize_or_zero();
        let rally = [
            base[0] * 0.45 + objective[0] * 0.55,
            base[1] * 0.45 + objective[1] * 0.55,
        ];
        let ready = self
            .entities
            .iter()
            .filter(|e| {
                e.view.owner == owner
                    && matches!(e.view.kind.as_str(), "short" | "scout")
                    && e.view.hp > 0.
                    && !e.view.downed
                    && !e.view.retreating
                    && distance(position(e), rally) < 28.
            })
            .count()
            >= 3;
        let advancing = self.entities.iter().any(|e| {
            e.view.owner == owner
                && e.view.kind == "short"
                && e.view.hp > 0.
                && distance(position(e), objective) < CAPTURE_RADIUS + 20.
        });
        let deploy = self.tick < DEPLOYMENT_TICKS;
        let idle: Vec<_> = self
            .entities
            .iter()
            .filter(|e| {
                e.view.owner == owner
                    && !e.view.building
                    && e.view.hp > 0.
                    && !e.view.downed
                    && !e.view.retreating
                    && e.orders.is_empty()
                    && e.view.garrison.is_none()
            })
            .map(|e| (e.view.id.clone(), e.view.kind.clone()))
            .collect();
        for (id, kind) in idle {
            let assault = matches!(kind.as_str(), "short" | "medium");
            let goal = if deploy || assault && !ready && !advancing {
                rally
            } else if assault {
                objective
            } else {
                // Guns and support establish a second line behind the assault infantry.
                let setback = if matches!(kind.as_str(), "long" | "xtralong" | "scout") {
                    58.
                } else {
                    38.
                };
                [
                    objective[0] + approach.x * setback,
                    objective[1] + approach.y * setback,
                ]
            };
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
        if self
            .entities
            .iter()
            .filter(|e| e.view.owner == owner && !e.view.building)
            .count()
            < 24
        {
            let producer = self
                .entities
                .iter()
                .find(|e| e.view.owner == owner && e.view.kind == "barracks")
                .map(|e| e.view.id.clone());
            if let Some(id) = producer
                && !self.jobs.iter().any(|j| j.producer.as_ref() == Some(&id))
            {
                let count = |kind: &str| {
                    self.entities
                        .iter()
                        .filter(|e| e.view.owner == owner && e.view.kind == kind && e.view.hp > 0.)
                        .count()
                };
                let recruits = if count("medic") == 0 {
                    "medic"
                } else if count("engineer") == 0 {
                    "engineer"
                } else if count("scout") == 0 {
                    "scout"
                } else {
                    "short"
                };
                self.recruit(recruits, &id, owner);
            }
        }
    }
    fn production(&mut self) {
        let mut valid = Vec::with_capacity(self.jobs.len());
        for job in self.jobs.drain(..) {
            if job.kind == "recruit"
                && !self.entities.iter().any(|e| {
                    Some(&e.view.id) == job.producer.as_ref()
                        && e.view.hp > 0.
                        && e.view.owner == job.owner
                })
            {
                self.supply[job.owner as usize] += job.cost;
            } else {
                valid.push(job);
            }
        }
        self.jobs = valid;
        let mut active = std::collections::HashSet::new();
        for j in &mut self.jobs {
            if j.producer.as_ref().is_none_or(|p| active.insert(p.clone())) {
                j.progress += 1;
            }
        }
        let completed: Vec<Job> = self
            .jobs
            .iter()
            .filter(|j| j.progress >= j.duration)
            .cloned()
            .collect();
        self.jobs.retain(|j| j.progress < j.duration);
        for j in completed {
            if j.unit_type == "bridge" {
                if let Some(end) = j.end.filter(|end| self.bridge_site(j.at, *end)) {
                    self.spawn("bridge", j.owner, j.at);
                    self.entities.last_mut().unwrap().bridge_end = Some(end);
                } else {
                    self.supply[j.owner as usize] += j.cost;
                    self.notice = "Bridge construction blocked; supply refunded.".into();
                }
                continue;
            }
            if j.kind == "building" {
                if self.clear_site(j.at) {
                    self.spawn(&j.unit_type, j.owner, j.at);
                } else {
                    self.supply[j.owner as usize] += j.cost;
                    self.notice = "Construction blocked; supply refunded.".into();
                }
                continue;
            }
            if flag(spec(&j.unit_type), "airUnit") {
                if let Some((base, slot)) = j.producer.as_deref().and_then(|id| {
                    self.free_aircraft_stand(id, &j.unit_type, None)
                        .map(|slot| (id.to_owned(), slot))
                }) {
                    let id = self.spawn(&j.unit_type, j.owner, j.at);
                    let i = self.entities.iter().position(|e| e.view.id == id).unwrap();
                    self.park_aircraft(i, &base, slot);
                } else {
                    self.supply[j.owner as usize] += j.cost;
                    self.notice = "No aircraft stand available; supply refunded.".into();
                }
                continue;
            }
            let producer = self
                .entities
                .iter()
                .find(|e| Some(&e.view.id) == j.producer.as_ref() && e.view.hp > 0.)
                .map(|e| {
                    if e.view.kind == "airfield" && flag(spec(&j.unit_type), "airUnit") {
                        let (sn, c) = e.view.heading.sin_cos();
                        [
                            e.view.x + (6. * c - 15. * sn) * e.view.model_scale,
                            e.view.z + (-6. * sn - 15. * c) * e.view.model_scale,
                        ]
                    } else {
                        position(e)
                    }
                });
            let mut spawn = None;
            if let Some(at) = producer {
                if flag(spec(&j.unit_type), "airUnit") {
                    spawn = Some(at);
                } else {
                    for radius in 3..=14 {
                        for n in 0..12 {
                            let angle = n as f32 * std::f32::consts::PI / 6.;
                            let p = [
                                (at[0] + angle.cos() * radius as f32).round(),
                                (at[1] + angle.sin() * radius as f32).round(),
                            ];
                            if spawn.is_none()
                                && (if flag(spec(&j.unit_type), "waterUnit") {
                                    self.sea_passable(p)
                                } else {
                                    self.passable_clearance(
                                        p,
                                        0,
                                        body_clearance(
                                            &j.unit_type,
                                            number(spec(&j.unit_type), "worldScale", 1.),
                                        ),
                                    )
                                })
                                && !self.entities.iter().any(|e| distance(position(e), p) < 0.9)
                            {
                                spawn = Some(p);
                            }
                        }
                    }
                }
            }
            if let Some(at) = spawn {
                self.spawn(&j.unit_type, j.owner, at);
                if flag(spec(&j.unit_type), "airUnit") {
                    self.entities.last_mut().unwrap().home = j.producer;
                }
            } else {
                self.supply[j.owner as usize] += j.cost;
                self.notice = "Recruitment blocked; supply refunded.".into();
            }
        }
    }
    fn movement(&mut self) {
        let visibility = self.visibility_mask();
        for i in 0..self.entities.len() {
            self.entities[i].view.velocity = [0., 0.];
            if self.entities[i].view.building
                || self.entities[i].transport.is_some()
                || self.entities[i]
                    .view
                    .garrison
                    .as_ref()
                    .is_some_and(|g| g.entered || g.door_stage == 1)
            {
                continue;
            }
            if self.entities[i].view.downed {
                continue;
            }
            if self.entities[i].aa_target.is_some()
                || self.entities[i].aa_pose_until > self.tick
                || self.entities[i].medical_until > self.tick
            {
                self.entities[i].speed = 0.;
                continue;
            }
            let s = spec(&self.entities[i].view.kind);
            let air = flag(s, "airUnit");
            if air {
                if self.entities[i].view.kind == "c130" {
                    self.gunship_movement(i);
                } else {
                    self.aircraft_movement(i);
                }
                continue;
            }
            if self.entities[i].halted {
                self.entities[i].speed = 0.;
                self.entities[i].stationary += 1;
                continue;
            }
            let clearance = body_clearance(
                &self.entities[i].view.kind,
                self.entities[i].view.model_scale,
            );
            if self.recover_vehicle_clearance(i, clearance) {
                continue;
            }
            let at = position(&self.entities[i]);
            let owner = self.entities[i].view.owner;
            if self.entities[i].path.len() > 2 && self.tick.is_multiple_of(5) {
                let e = &self.entities[i];
                let target = e
                    .orders
                    .front()
                    .filter(|o| o.kind == "attack")
                    .and_then(|o| o.target.as_ref())
                    .or_else(|| {
                        e.warfare
                            .active(self.tick)
                            .then_some(e.warfare.target.as_ref())
                            .flatten()
                    });
                let standoff = target
                    .and_then(|id| self.entities.iter().position(|t| &t.view.id == id))
                    .and_then(|j| {
                        e.path.back().map(|p| {
                            let center = aim_point(e, &self.entities[j], at);
                            (
                                center,
                                distance(*p, center).min(self.infantry_attack_range(i, j) * 0.94)
                                    - 0.3,
                            )
                        })
                    });
                let mode = if air {
                    1
                } else if flag(s, "waterUnit") {
                    2
                } else {
                    0
                };
                if let Some(skip) = (1..self.entities[i].path.len().min(10)).rev().find(|n| {
                    self.clear_segment_radius(at, self.entities[i].path[*n], mode, clearance)
                        && standoff.is_none_or(|(center, minimum)| {
                            segment_distance(center, at, self.entities[i].path[*n]) >= minimum
                        })
                }) {
                    for _ in 0..skip {
                        self.entities[i].path.pop_front();
                    }
                }
            }
            if self.entities[i].path.is_empty()
                && !self.entities[i].warfare.active(self.tick)
                && self.entities[i].tactical_until <= self.tick
                && let Some(goal) = self.entities[i].cover_goal.or_else(|| {
                    self.entities[i]
                        .orders
                        .front()
                        .filter(|o| ["move", "attack-move", "patrol"].contains(&o.kind.as_str()))
                        .and_then(|o| o.destination)
                })
                && distance(at, goal) > 0.5
            {
                if (self.tick + i as u64).is_multiple_of(20) {
                    self.entities[i].path = self.route(i, at, goal);
                }
                if self.entities[i].path.is_empty() {
                    self.entities[i].speed = 0.;
                    continue;
                }
            }
            let range = if flag(s, "antiAirLauncher")
                && self.entities[i]
                    .orders
                    .front()
                    .and_then(|o| o.target.as_ref())
                    .is_some_and(|id| {
                        self.entities
                            .iter()
                            .any(|t| &t.view.id == id && t.view.airborne)
                    }) {
                number(s, "aaRange", 160.)
            } else {
                self.weapon_range(&self.entities[i].view.kind)
            };
            let threat = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, e)| {
                    e.view.owner >= 0
                        && e.view.owner != owner
                        && e.view.kind != "objective"
                        && e.view.hp > 0.
                        && !e.view.downed
                        && e.transport.is_none()
                        && self.can_target_structure(&self.entities[i], e)
                        && (!e.view.airborne
                            || flag(s, "antiAir")
                            || flag(s, "antiAirLauncher")
                            || self.entities[i].stance == "flee")
                        && visibility[*j].get(owner as usize).copied().unwrap_or(false)
                })
                .map(|(_, e)| {
                    let d = distance(at, position(e));
                    let clear = d <= range && self.firing_line(&self.entities[i], e);
                    (e, d, clear)
                })
                .min_by(|(_, da, ca), (_, db, cb)| cb.cmp(ca).then_with(|| da.total_cmp(db)));
            let threat = threat.map(|(e, _, clear)| (position(e), clear));
            let mut fire_stop = false;
            if let Some((t, line_clear)) = threat {
                if self.entities[i].stance == "flee"
                    && self.tick.is_multiple_of(20)
                    && distance(at, t) < number(s, "acquisitionRange", range)
                {
                    let d = distance(at, t).max(0.01);
                    let goal = [
                        (at[0] + (at[0] - t[0]) / d * 8.).clamp(1., self.terrain.cols as f32 - 2.),
                        (at[1] + (at[1] - t[1]) / d * 8.).clamp(1., self.terrain.rows as f32 - 2.),
                    ];
                    let path = self.route(i, at, goal);
                    self.entities[i].path = path;
                }
                fire_stop = line_clear
                    && self.entities[i].view.kind != "medium"
                    && number(s, "damage", 0.) > 0.
                    && self.entities[i].stance != "flee"
                    && self.entities[i].stance != "hold-fire"
                    && distance(at, t) <= range
                    && self.entities[i].orders.front().is_some_and(|o| {
                        ["attack-move", "attack", "patrol"].contains(&o.kind.as_str())
                    });
            }
            let seeking_cover = self.entities[i].cover_goal.is_some()
                || self.entities[i].warfare.active(self.tick)
                || self.entities[i].tactical_until > self.tick;
            if seeking_cover
                && self.entities[i].cover_ordered
                && self.entities[i].path.is_empty()
                && self.entities[i]
                    .cover_goal
                    .is_some_and(|g| distance(at, g) < 0.8)
            {
                self.entities[i].cover_ordered = false;
                self.entities[i].orders.pop_front();
            }
            if seeking_cover {
                fire_stop = false;
            }
            if let Some(order) = self.entities[i].orders.front().cloned()
                && !seeking_cover
            {
                if order.kind == "attack" {
                    // A direct order must not stop for a nearer opportunistic target.
                    fire_stop = false;
                    let target = self
                        .entities
                        .iter()
                        .find(|e| {
                            Some(&e.view.id) == order.target.as_ref()
                                && ordered_lot(e, Some(&order)).is_none_or(|lot| lot.integrity > 0.)
                                && e.view.hp > 0.
                                && (e.view.lots.is_empty()
                                    || e.view.lots.iter().any(|l| l.integrity > 0.))
                        })
                        .map(|e| {
                            (
                                aim_point(&self.entities[i], e, at),
                                self.visible(owner, e),
                                self.firing_line(&self.entities[i], e),
                                e.view.id.clone(),
                            )
                        });
                    if let Some((_, false, _, _)) = target {
                        // Keep the order through temporary loss of sight, without tracking
                        // hidden movement or falling back to a different enemy.
                        self.entities[i].path.clear();
                        fire_stop = true;
                    } else if let Some((to, true, line_clear, id)) = target {
                        fire_stop = distance(at, to) <= range && line_clear;
                        if self.entities[i].view.category == "vehicle" {
                            if fire_stop {
                                self.entities[i].path.clear();
                            } else if self.tick >= self.entities[i].attack_route_after
                                && (self.tick + i as u64).is_multiple_of(20)
                            {
                                let target_index =
                                    self.entities.iter().position(|e| e.view.id == id).unwrap();
                                if self.attack_route_needed(i, target_index) {
                                    let path = self.infantry_attack_route(i, at, target_index);
                                    self.entities[i].attack_route_after =
                                        self.tick + if path.is_empty() { 40 } else { 20 };
                                    self.entities[i].path = path;
                                }
                            }
                        }
                    } else {
                        self.entities[i].orders.pop_front();
                        self.entities[i].path.clear();
                    }
                } else if !["board", "garrison"].contains(&order.kind.as_str())
                    && self.entities[i].path.is_empty()
                    && self.entities[i].stance != "flee"
                {
                    if order.kind == "patrol" && self.entities[i].orders.len() == 1 {
                        if let (Some(origin), Some(destination)) =
                            (self.entities[i].patrol_origin, order.destination)
                        {
                            self.entities[i].patrol_origin = Some(destination);
                            self.entities[i].orders.front_mut().unwrap().destination = Some(origin);
                            self.entities[i].path = self.route(i, at, origin);
                        }
                    } else {
                        self.entities[i].orders.pop_front();
                        if self.entities[i].view.category == "troop"
                            && ["move", "attack-move"].contains(&order.kind.as_str())
                            && order
                                .destination
                                .is_some_and(|goal| distance(at, goal) < 0.5)
                            && self.cover_level(at, None) > 0
                        {
                            self.entities[i].cover_goal = Some(at);
                            self.entities[i].cover_ordered = false;
                            self.entities[i].path.clear();
                            continue;
                        }
                        if let Some(next) =
                            self.entities[i].orders.front().and_then(|o| o.destination)
                        {
                            self.entities[i].path = self.route(i, at, next);
                        }
                    }
                }
            }
            if self.entities[i].warfare.active(self.tick) {
                if self.entities[i].warfare.role.mobile() {
                    fire_stop = false;
                } else if matches!(
                    self.entities[i].warfare.role,
                    InfantryRole::Cover | InfantryRole::Pinned
                ) {
                    fire_stop = true;
                }
            }
            if let Some(target) = self.infantry_engagement_target(i, &visibility)
                && self.entities[i].warfare.role != InfantryRole::Recover
                && self.entities[i].warfare.role != InfantryRole::Pinned
            {
                if self.infantry_firing_position(i, at, target) {
                    fire_stop = true;
                    if self.entities[i]
                        .orders
                        .front()
                        .is_some_and(|o| o.kind == "attack")
                        || self.entities[i].warfare.active(self.tick)
                            && self.entities[i].warfare.role.mobile()
                    {
                        self.entities[i].path.clear();
                    }
                } else if self.entities[i]
                    .orders
                    .front()
                    .is_some_and(|o| o.kind == "attack")
                    || self.entities[i].orders.is_empty()
                        && !self.entities[i].player_ordered
                        && !self.entities[i].warfare.active(self.tick)
                {
                    fire_stop = false;
                    if self.tick >= self.entities[i].attack_route_after
                        && (self.tick + i as u64).is_multiple_of(10)
                        && self.attack_route_needed(i, target)
                    {
                        let path = self.infantry_attack_route(i, at, target);
                        self.entities[i].attack_route_after =
                            self.tick + if path.is_empty() { 40 } else { 20 };
                        if self.entities[i]
                            .orders
                            .front()
                            .is_none_or(|o| o.kind != "attack")
                        {
                            let goal = path.back().copied();
                            self.entities[i].warfare = InfantryWarfare {
                                role: InfantryRole::Flank,
                                until: self.tick + 400,
                                next_decision: self.tick + 10,
                                target: Some(self.entities[target].view.id.clone()),
                                goal,
                                ..Default::default()
                            };
                            self.entities[i].cover_ordered = false;
                        }
                        self.entities[i].path = path;
                    }
                }
            }
            let Some(goal) = self.entities[i].path.front().copied() else {
                self.entities[i].speed = 0.;
                self.entities[i].stationary += 1;
                continue;
            };
            if fire_stop {
                self.entities[i].speed = 0.;
                self.entities[i].stationary += 1;
                continue;
            }
            let d = distance(at, goal);
            if self.entities[i].view.category == "vehicle"
                && d < 0.25
                && self.clear_segment_radius(at, goal, 0, clearance)
            {
                let e = &mut self.entities[i];
                e.view.x = goal[0];
                e.view.z = goal[1];
                e.path.pop_front();
                e.speed = 0.;
                continue;
            }
            let desired_heading = (goal[1] - at[1]).atan2(goal[0] - at[0]);
            let vehicle = self.entities[i].view.category != "troop";
            let alignment = if vehicle && d > 0.001 {
                let e = &mut self.entities[i];
                let turn = angle_delta(e.view.heading, desired_heading);
                let limit = turn_rate(&e.view.kind, e.view.model_scale) * DT;
                e.view.heading += turn.clamp(-limit, limit);
                angle_delta(e.view.heading, desired_heading).cos().max(0.)
            } else {
                1.
            };
            let speed = (self.entities[i].speed + number(s, "acceleration", 2.) * DT)
                .min(number(s, "speed", 0.) * alignment.powi(2));
            let cost = if air || flag(s, "waterUnit") || self.on_bridge(at) {
                1.
            } else {
                self.terrain.cell(at[0], at[1]).cost()
            };
            let tactical = self.movement_factor(i);
            let step = (speed * tactical * DT / cost.max(1.)).min(d);
            let next = if d < 0.001 {
                goal
            } else if vehicle {
                let heading = self.entities[i].view.heading;
                [at[0] + heading.cos() * step, at[1] + heading.sin() * step]
            } else {
                [
                    at[0] + (goal[0] - at[0]) / d * step,
                    at[1] + (goal[1] - at[1]) / d * step,
                ]
            };
            let can_move = if flag(s, "waterUnit") {
                self.sea_passable(next)
            } else {
                self.clear_segment_radius(at, next, 0, clearance)
                    && self.passable_clearance(next, 0, clearance)
            };
            if can_move {
                let e = &mut self.entities[i];
                e.view.x = next[0];
                e.view.z = next[1];
                e.view.velocity = [(next[0] - at[0]) / DT, (next[1] - at[1]) / DT];
                e.speed = speed;
                if step > 0.0001 {
                    e.stationary = 0;
                } else {
                    e.stationary += 1;
                }
                if !vehicle && step > 0.001 {
                    e.view.heading = desired_heading;
                }
                if air {
                    e.view.airborne = true;
                    e.fuel -= step * number(s, "fuelBurnPerDistance", 1.);
                }
                if d <= step + 0.02 {
                    e.path.pop_front();
                }
            } else {
                self.entities[i].speed = 0.;
                self.entities[i].stationary += 1;
                if (self.tick + i as u64).is_multiple_of(20) {
                    // Preserve a tactical/firing goal rather than replanning to
                    // the enemy's centre or an unrelated original destination.
                    if let Some(goal) = self.entities[i].path.back().copied() {
                        self.entities[i].path = self.route(i, at, goal);
                    }
                }
            }
        }
        // Departing support aircraft leave without becoming destroyed wrecks.
        self.entities.retain(|e| {
            e.gunship
                .as_ref()
                .is_none_or(|m| m.phase != GunshipPhase::Departed)
        });
        self.separate_units();
        let positions: BTreeMap<String, Point> = self
            .entities
            .iter()
            .map(|e| (e.view.id.clone(), position(e)))
            .collect();
        for e in &mut self.entities {
            if let Some(at) = e.transport.as_ref().and_then(|id| positions.get(id)) {
                e.view.x = at[0];
                e.view.z = at[1];
            }
        }
    }
    fn combat(&mut self) {
        self.support_anti_air();
        self.helicopter_combat();
        let visibility = self.visibility_mask();
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            let s = spec(&e.view.kind);
            if e.view.building
                || (e.view.category == "troop" && e.view.suppression >= 0.85)
                || (e.warfare.active(self.tick) && e.warfare.role.mobile() && !e.path.is_empty())
                || e.aa_target.is_some()
                || e.aa_pose_until > self.tick
                || e.view.category == "aircraft"
                || e.view.hp <= 0.
                || e.view.downed
                || e.transport.is_some()
                || e.stance == "hold-fire"
                || e.stance == "flee"
                || e.stance == "retreat"
                || e.halted
                || number(s, "damage", 0.) <= 0.
                || e.orders
                    .front()
                    .is_some_and(|o| ["move", "board"].contains(&o.kind.as_str()))
            {
                continue;
            }
            if flag(s, "airUnit") && !e.view.airborne {
                continue;
            }
            // Hull-mounted guns cannot rotate the chassis against an active
            // navigation turn. Stop to engage before aiming the whole vehicle.
            if e.view.category == "vehicle"
                && !["medium", "antiAir"].contains(&e.view.kind.as_str())
                && e.speed > 0.01
                && !e.path.is_empty()
            {
                continue;
            }
            if flag(s, "requiresStationaryToFire")
                && (e.speed > 0.01 || e.stationary < (number(s, "setupSeconds", 0.) * 20.) as u32)
            {
                continue;
            }
            let at = position(e);
            let owner = e.view.owner;
            let range = self.weapon_range(&e.view.kind);
            let explicit = e
                .orders
                .front()
                .filter(|o| o.kind == "attack")
                .and_then(|o| o.target.as_deref())
                .or_else(|| {
                    e.warfare
                        .active(self.tick)
                        .then_some(e.warfare.target.as_deref())
                        .flatten()
                        .filter(|id| {
                            self.entities.iter().enumerate().any(|(j, t)| {
                                t.view.id == *id
                                    && t.view.hp > 0.
                                    && !t.view.downed
                                    && distance(at, aim_point(e, t, at)) <= range
                                    && visibility[j].get(owner as usize).copied().unwrap_or(false)
                                    && self.firing_line(e, t)
                            })
                        })
                });
            let target = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, t)| {
                    explicit.is_none_or(|id| id == t.view.id)
                        && ordered_lot(t, e.orders.front()).is_none_or(|lot| lot.integrity > 0.)
                        && (t.view.owner >= 0
                            || (Some(t.view.id.as_str()) == explicit && !t.view.lots.is_empty()))
                        && t.view.owner != owner
                        && t.view.kind != "objective"
                        && (!t.view.airborne || flag(s, "antiAir"))
                        && weapon_can_damage(&e.view.kind, &t.view)
                        && t.view.hp > 0.
                        && (t.view.lots.is_empty() || t.view.lots.iter().any(|l| l.integrity > 0.))
                        && !t.view.downed
                        && t.transport.is_none()
                        && distance(at, aim_point(e, t, at)) <= range
                        && visibility[*j].get(owner as usize).copied().unwrap_or(false)
                        && self.can_target_structure(e, t)
                        && self.firing_line(e, t)
                })
                .min_by(|(_, a), (_, b)| {
                    let da = if Some(a.view.id.as_str()) == explicit {
                        -1.
                    } else {
                        distance(at, aim_point(e, a, at))
                    };
                    let db = if Some(b.view.id.as_str()) == explicit {
                        -1.
                    } else {
                        distance(at, aim_point(e, b, at))
                    };
                    da.total_cmp(&db)
                })
                .map(|(_, t)| (t.view.id.clone(), aim_point(e, t, at)));
            self.entities[i].cooldown = self.entities[i].cooldown.saturating_sub(1);
            let Some((id, to)) = target else {
                self.entities[i].target = None;
                self.entities[i].windup = 0;
                continue;
            };
            if self.entities[i].target.as_ref() != Some(&id) {
                self.entities[i].target = Some(id.clone());
                self.entities[i].windup = (number(s, "windupSeconds", 0.16) * 20.) as u32;
            }
            if self.entities[i].view.category == "vehicle" {
                let e = &mut self.entities[i];
                let desired = (to[1] - at[1]).atan2(to[0] - at[0]);
                let turret = ["medium", "antiAir"].contains(&e.view.kind.as_str());
                let heading = if turret {
                    e.gun_heading.unwrap_or(e.view.heading)
                } else {
                    e.view.heading
                };
                let rate = if turret {
                    if e.view.kind == "antiAir" { 1.5 } else { 0.95 }
                } else {
                    turn_rate(&e.view.kind, e.view.model_scale)
                };
                let next = heading + angle_delta(heading, desired).clamp(-rate * DT, rate * DT);
                e.gun_heading = Some(next);
                if !turret {
                    e.view.heading = next;
                }
                if angle_delta(next, desired).abs() > 0.08 {
                    continue;
                }
            }
            if self.entities[i].cooldown > 0 {
                continue;
            }
            if self.entities[i].windup > 0 {
                self.entities[i].windup -= 1;
                continue;
            }
            self.entities[i].cooldown = (number(s, "reloadSeconds", 1.) * 20.).max(1.) as u32;
            self.entities[i].windup = (number(s, "windupSeconds", 0.16) * 20.) as u32;
            if self.entities[i].view.category == "troop" {
                self.entities[i].view.heading = (to[1] - at[1]).atan2(to[0] - at[0]);
            }
            let hit = self.random()
                < number(s, "hitChance", 1.) * (1. - self.entities[i].view.suppression * 0.5);
            let target_piece = self.entities.iter().find(|e| e.view.id == id).unwrap();
            let ground_hit = !hit
                || (!target_piece.view.building
                    && !target_piece.view.airborne
                    && target_piece.view.category == "troop"
                    && target_piece.view.garrison.is_none());
            let clearance = if target_piece.view.building {
                number(spec(&target_piece.view.kind), "footprintWidth", 8.).max(number(
                    spec(&target_piece.view.kind),
                    "footprintDepth",
                    8.,
                )) * target_piece.view.model_scale
                    * 0.6
            } else if target_piece.view.category == "vehicle" {
                target_piece.view.model_scale * 3.5
            } else {
                0.
            };
            let to = if !hit && number(s, "terrainDamageRadius", 0.) > 0. {
                let angle = self.random() * std::f32::consts::TAU;
                let scatter =
                    clearance + number(s, "terrainDamageRadius", 0.) + 1. + self.random() * 3.;
                [
                    (to[0] + angle.cos() * scatter).clamp(1., self.terrain.cols as f32 - 2.),
                    (to[1] + angle.sin() * scatter).clamp(1., self.terrain.rows as f32 - 2.),
                ]
            } else {
                to
            };
            let projectile_speed = if s["category"] == "troop" {
                number(s, "projectileSpeed", 1000.).max(160.)
            } else {
                number(s, "projectileSpeed", 1000.).max(1.)
            };
            let travel = (distance(at, to) / projectile_speed * 20.).ceil().max(1.) as u64;
            let projectile = Projectile {
                owner,
                from: at,
                to,
                born: self.tick,
                impact: self.tick + travel,
                damage: number(s, "damage", 1.),
                radius: number(s, "damageRadius", 0.2).max(0.05),
                hit,
                weapon: self.entities[i].view.kind.clone(),
                ground_hit,
            };
            self.warn_incoming(&projectile);
            self.projectiles.push(projectile);
            if owner == 0
                && let Some(training) = &mut self.training
            {
                training.shots += 1;
            }
            self.entities[i].last_shot = self.tick;
            self.events.push(BattleEvent {
                hit: None,
                target: None,
                tick: self.tick,
                kind: "shot".into(),
                at,
                from: Some(to),
                owner,
                radius: 0.4,
                weapon: self.entities[i].view.kind.clone(),
                source: self.entities[i].view.id.clone(),
                duration: travel as f32 * DT,
                release_height: None,
            });
        }
        let impacts: Vec<_> = self
            .projectiles
            .iter()
            .filter(|p| p.impact <= self.tick)
            .cloned()
            .collect();
        for p in impacts {
            if p.weapon != "plane"
                || (p.to[0] >= 0.
                    && p.to[1] >= 0.
                    && p.to[0] < self.terrain.cols as f32
                    && p.to[1] < self.terrain.rows as f32)
            {
                self.impact(&p);
            }
        }
        self.projectiles.retain(|p| p.impact > self.tick);
    }
    fn economy_support(&mut self) {
        let healers: Vec<(usize, Point, i32, f32, f32)> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.view.hp > 0.
                    && !e.halted
                    && e.transport.is_none()
                    && (e.view.building || (e.path.is_empty() && e.orders.is_empty()))
                    && (flag(spec(&e.view.kind), "medical")
                        || flag(spec(&e.view.kind), "medicalFacility"))
            })
            .map(|(index, e)| {
                (
                    index,
                    position(e),
                    e.view.owner,
                    number(spec(&e.view.kind), "supportRange", 3.),
                    number(spec(&e.view.kind), "healing", 1.),
                )
            })
            .collect();
        for (healer, at, owner, range, amount) in healers {
            let target = self
                .entities
                .iter()
                .enumerate()
                .filter(|(i, e)| {
                    *i != healer
                        && !e.view.building
                        && e.view.owner == owner
                        && e.view.hp > 0.
                        && e.view.hp < e.view.max_hp
                        && e.transport.is_none()
                        && distance(position(e), at) <= range
                })
                .min_by(|(_, a), (_, b)| a.view.hp.total_cmp(&b.view.hp))
                .map(|(i, _)| i);
            let Some(target) = target else {
                self.entities[healer].treatment_target = None;
                continue;
            };
            let target_id = self.entities[target].view.id.clone();
            if self.entities[healer].treatment_target.as_ref() != Some(&target_id) {
                self.entities[healer].treatment_target = Some(target_id);
                self.entities[healer].treatment_start = self.tick;
            }
            self.entities[healer].medical_until = self.tick + 24;
            self.entities[healer].view.aim = Some(position(&self.entities[target]));
            if !self.entities[healer].view.building {
                self.entities[healer].view.heading = (self.entities[target].view.z - at[1])
                    .atan2(self.entities[target].view.x - at[0]);
            }
            if self.entities[target].view.downed
                && !self.entities[healer].view.building
                && self
                    .tick
                    .saturating_sub(self.entities[healer].treatment_start)
                    < 30
            {
                continue;
            }
            let e = &mut self.entities[target];
            if e.view.downed {
                e.view.downed = false;
                e.downed_until = 0;
                e.view.hp = (e.view.max_hp * 0.35).max(0.1);
                e.view.strength =
                    (number(spec(&e.view.kind), "formationStrength", 1.) as u32 / 3).max(1);
                self.events.push(BattleEvent {
                    hit: None,
                    target: None,
                    tick: self.tick,
                    kind: "rescue".into(),
                    at: position(e),
                    from: None,
                    owner,
                    radius: 1.,
                    weapon: "medic".into(),
                    source: self.entities[healer].view.id.clone(),
                    duration: 0.,
                    release_height: None,
                });
            } else {
                heal(&mut e.view, amount);
            }
        }
    }
    fn objectives(&mut self) {
        let locked = self.training.is_none() && self.tick < DEPLOYMENT_TICKS;
        let mut contested_posts = false;
        for i in 0..self.entities.len() {
            let post = self.entities[i].view.kind == "objective";
            if !post && !flag(spec(&self.entities[i].view.kind), "capturable") {
                continue;
            }
            let at = position(&self.entities[i]);
            let mut presence = [false; 2];
            let mut infantry = [false; 2];
            for e in &self.entities {
                if !e.view.building
                    && (0..=1).contains(&e.view.owner)
                    && e.view.hp > 0.
                    && !e.view.downed
                    && !e.view.retreating
                    && e.transport.is_none()
                    && !e.view.airborne
                    && distance(position(e), at) <= if post { CAPTURE_RADIUS } else { 2.75 }
                {
                    let side = e.view.owner as usize;
                    // Armour can deny a zone; infantry must occupy and secure it.
                    presence[side] = true;
                    infantry[side] |= e.view.category == "troop"
                        && number(spec(&e.view.kind), "damage", 0.) > 0.
                        && e.view.suppression < 0.6;
                }
            }
            let e = &mut self.entities[i];
            if post
                && (presence[0] && presence[1]
                    || e.view.owner >= 0 && presence[1 - e.view.owner as usize])
            {
                contested_posts = true;
            }
            if locked {
                e.capture_progress = 0.;
                e.capture_owner = -1;
                continue;
            }
            if presence[0] && presence[1] {
                continue;
            }
            let owner = if infantry[0] {
                0
            } else if infantry[1] {
                1
            } else {
                -1
            };
            if owner < 0 || owner == e.view.owner {
                e.capture_progress = (e.capture_progress - 0.5).max(0.);
                if e.capture_progress == 0. {
                    e.capture_owner = -1;
                }
                continue;
            }
            if e.capture_owner != owner && e.capture_progress > 0. {
                // Reverse the previous claimant's work before starting a new capture.
                e.capture_progress = (e.capture_progress - 0.5).max(0.);
                continue;
            }
            e.capture_owner = owner;
            // More units provide security, never a faster capture clock.
            e.capture_progress += 0.25;
            if e.capture_progress >= SECURE_SECONDS {
                let neutralizing = e.view.owner >= 0;
                e.view.owner = if neutralizing { -1 } else { owner };
                e.capture_progress = 0.;
                self.notice = format!(
                    "{} {} a command post.",
                    if owner == 0 {
                        "Field Command"
                    } else {
                        "Iron Front"
                    },
                    if neutralizing {
                        "neutralized"
                    } else {
                        "secured"
                    }
                );
            }
        }
        for owner in [0, 1] {
            let objectives: Vec<_> = self
                .entities
                .iter()
                .filter(|e| e.view.kind == "objective")
                .collect();
            self.hold[owner as usize] = if !locked
                && !contested_posts
                && !objectives.is_empty()
                && objectives.iter().all(|e| e.view.owner == owner)
            {
                self.hold[owner as usize] + 0.25
            } else {
                0.
            };
            if self.hold[owner as usize] >= VICTORY_HOLD_SECONDS && self.training.is_none() {
                self.winner = Some(owner);
            }
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot_for(0)
    }
    pub fn snapshot_for(&self, owner: i32) -> Snapshot {
        let _timing = crate::performance::span("simulation.snapshot");
        let geometry = self.navigation_world();
        let observation = self.observation();
        let observed = observation.mask(&self.entities);
        Snapshot {simulation:self.simulation,team_supply:if self.simulation {self.supply} else {[0.;2]},viewer:owner,training:self.training_summary(),fog_enabled:!self.simulation && flag(&self.config,"fogEnabled"),sights:self.entities.iter().filter(|e|e.view.owner==owner && e.view.hp>0. && !e.view.downed && e.transport.is_none()).map(|e|crate::runtime::Sight{at:position(e),radius:self.unit_sight(e)}).collect(),bases:self.bases.clone(),tick:self.tick,paused:self.paused,winner:self.winner.map(|w|json!(w)).unwrap_or(Value::Null),supply:self.supply[owner as usize],notice:self.notice.clone(),victory_progress:self.hold[owner as usize],objective_hold:self.hold,outcome_reason:self.winner.map(|w| if w>=0 && self.hold[w as usize]>=VICTORY_HOLD_SECONDS {"objectives"} else {"elimination"}).unwrap_or("").into(),
            entities:self.entities.iter().enumerate().filter(|(i,e)|e.transport.is_none() && (self.simulation || self.snapshot_visible(owner,e,observed[*i][owner as usize]))).map(|(_,e)|{let mut p=e.view.clone();
                p.under_fire=e.threat_until>self.tick;
                p.sight=self.unit_sight(e);
                p.auto_attack=e.stance=="attack";
                p.waypoints=if p.owner==owner {display_route(position(e), &e.path)} else {vec![]};
                p.home_airfield=e.home.clone();
                p.strafe_target=if p.kind=="plane" && e.aircraft.strafing && !matches!(e.aircraft.phase,FlightPhase::Parked|FlightPhase::Unassigned) && !e.aircraft.phase.returning() {self.aircraft_cannon_target(e)} else {None};
                p.strafe_heading=p.strafe_target.map(|_|e.aircraft.run_heading);
                p.strafe_locked=p.kind=="plane" && Self::strafe_locked(e);
                p.rotor_power=e.aircraft.rotor_power;
                p.gunship_target=e.gunship.as_ref().filter(|m| matches!(m.phase,GunshipPhase::Ingress|GunshipPhase::Orbit)).map(|m|m.target);
                p.gunship_laps=e.gunship.as_ref().map_or(0,|m|(m.swept/std::f32::consts::TAU).floor().min(5.) as u8);
                p.tactical_role=if e.warfare.active(self.tick) {e.warfare.role.label().into()} else {String::new()};
                if p.lots.is_empty() { p.label=spec(&p.kind)["label"].as_str().unwrap_or(&p.label).to_owned(); }
                p.flare_salvos=e.aircraft.flares;
                p.flare_cooldown=e.aircraft.next_flare.saturating_sub(self.tick) as f32*DT;
                p.aa_reload=e.aa_reload_until.saturating_sub(self.tick) as f32*DT;
                p.aa_elevated=e.aa_target.is_some() || e.aa_pose_until>self.tick;
                if transport_capacity(&p.kind)>0 { p.capacity=transport_capacity(&p.kind); p.occupants=self.entities.iter().filter(|u|u.transport.as_ref()==Some(&p.id)).count(); }
                if p.building {
                    p.capacity=geometry.window_capacity.get(&p.id).copied().unwrap_or(0);
                    p.occupants=self.entities.iter().filter(|u| u.view.owner==owner && u.view.garrison.as_ref().is_some_and(|g| g.building==p.id)).count();
                }
                p.aim=if e.medical_until>self.tick {e.view.aim} else {e.target.as_ref().and_then(|id|self.entities.iter().find(|t| &t.view.id==id).map(|t| aim_point(e,t,position(e))))};
                if p.category=="vehicle" && let Some(heading)=e.gun_heading && p.aim.is_some() {
                    p.aim=Some([p.x+heading.cos()*10.,p.z+heading.sin()*10.]);
                }
                p.activity=if p.kind=="helicopter" && e.aircraft.phase==FlightPhase::Hover && !p.training_target {if e.helicopter.evade_until>self.tick {"evading"} else if e.helicopter.burst_left>0 || e.last_shot>0 && self.tick.saturating_sub(e.last_shot)<6 {"shooting"} else if e.helicopter.target.is_some() {"attack-position"} else {"hovering"}} else if p.category=="aircraft" && !p.training_target {if p.kind=="helicopter" && e.aircraft.phase==FlightPhase::HeliTakeoff && !p.airborne {"starting-rotors"} else {e.aircraft.phase.label()}} else if p.downed {"wounded"} else if e.halted {"stopped"} else if e.aa_target.is_some() {"locking-aircraft"} else if e.aa_pose_until>self.tick {"launching-missile"} else if e.medical_until>self.tick {"treating"} else if p.kind=="engineer" && e.speed<0.1 && self.jobs.iter().any(|j|j.kind=="building" && j.owner==p.owner && distance(j.at,position(e))<6.) {"constructing"} else if e.last_shot>0 && self.tick.saturating_sub(e.last_shot)<6 {"shooting"}
                    else if p.garrison.as_ref().is_some_and(|g|g.exiting) {"leaving-building"}
                    else if p.garrison.as_ref().is_some_and(|g| g.entered && self.tick<g.ready_at) {"changing-window"}
                    else if p.garrison.as_ref().is_some_and(|g| g.entered) {"garrisoned"}
                    else if p.garrison.is_some() {"entering-building"}
                    else if e.orders.front().is_some_and(|o|o.kind=="board") {"boarding"}
                    else if !p.tactical_role.is_empty() {p.tactical_role.as_str()}
                    else if e.tactical_until>self.tick && e.cover_goal.is_none() && !e.path.is_empty() {"falling-back"}
                    else if e.cover_goal.is_some() && !e.path.is_empty() {"seeking-cover"} else if e.cover_goal.is_some() {"in-cover"} else if e.target.is_some() {"aiming"}
                    else if e.speed>1.8 {"running"} else if e.speed>0.1 {"walking"} else {"idle"}.into();
                if let Some(m)=&e.gunship {
                    p.aim=Some(m.target);
                    p.activity=match m.phase { GunshipPhase::Ingress=>"inbound", GunshipPhase::Orbit=>"orbiting", _=>"departing" }.into();
                    p.label=format!("AC-130 / {} / {} OF 5 ORBITS",p.activity.to_uppercase(),p.gunship_laps);
                }
                if !(p.garrison.as_ref().is_some_and(|g|g.entered || g.door_stage==1) || p.category=="aircraft" && e.aircraft.phase!=FlightPhase::Unassigned) { p.elevation=self.terrain.height(p.x,p.z); }p.bridge_end=e.bridge_end;p.cover=if p.garrison.as_ref().is_some_and(|g|g.entered) {2} else {self.cover_level(position(e),self.entity_threat_observed(e, &observed).or(e.threat.filter(|_|e.threat_until>self.tick)))};p.orders=if p.owner==owner {e.orders.iter().filter_map(|o|o.destination).take(12).collect()}else{vec![]};p}).collect(),
            props: self.props.clone(),
            events: if self.simulation { self.events.clone() } else { self.visible_events(owner, &observation) },
            objectives:self.entities.iter().filter(|e|e.view.kind=="objective").map(|e|json!({"owner":e.view.owner,"progress":e.capture_progress/SECURE_SECONDS,"radius":CAPTURE_RADIUS,"claimant":e.capture_owner,"locked":self.training.is_none() && self.tick<DEPLOYMENT_TICKS})).collect(),
            jobs:self.jobs.iter().enumerate().filter(|(_,j)|j.owner==owner).map(|(index,j)|json!({"id":j.id,"producer":j.producer,"queued":j.producer.is_some() && self.jobs[..index].iter().any(|prior|prior.producer==j.producer),"kind":j.kind,"type":j.unit_type,"x":j.at[0],"y":j.at[1],"progress":j.progress as f32/j.duration as f32})).collect(),
            projectiles:self.projectiles.iter().filter(|p|self.simulation || p.owner==owner || observation.sees(owner,p.to)).map(|p|{let t=(self.tick-p.born) as f32/(p.impact-p.born) as f32;json!({"x":p.from[0]+(p.to[0]-p.from[0])*t,"z":p.from[1]+(p.to[1]-p.from[1])*t})}).collect(),
        }
    }
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        let data = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }
    pub fn load_from(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("Save exceeds size limit.".into());
        }
        let mut game: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        game.upgrade_weapon_ranges();
        game.upgrade_house_layouts();
        game.validate()?;
        game.terrain.hydrate();
        game.repair_base_foundations();
        game.refresh_towns();
        game.ground_roster_cleanup();
        Ok(game)
    }
    pub fn validate(&self) -> Result<(), String> {
        let game = self;
        let valid_point = |p: Point| {
            p.iter().all(|v| v.is_finite() && *v >= 0.)
                && p[0] < game.terrain.cols as f32
                && p[1] < game.terrain.rows as f32
        };
        let mut ids = std::collections::HashSet::new();
        if game.version != 1
            || !(10..=crate::terrain::MAX_SIZE).contains(&game.terrain.cols)
            || !(10..=crate::terrain::MAX_SIZE).contains(&game.terrain.rows)
            || (!game.terrain.authored.is_empty()
                && game.terrain.authored.len() != (game.terrain.cols * game.terrain.rows) as usize)
            || !game.terrain.water_level.is_finite()
            || !game.terrain.authored_scale.is_finite()
            || game
                .terrain
                .authored
                .iter()
                .any(|h| !h.is_finite() || !(0. ..=1.).contains(h))
            || game.world.sectors.is_empty()
            || game.world.sectors.len() > 10000
            || game.sector.is_some_and(|i| i >= game.world.sectors.len())
            || game.entities.len() > 10000
            || game.props.len() > 2048
            || game.guided_missiles.len() > 10000
            || game.guided_missiles.iter().any(|m| {
                !(-1..=1).contains(&m.owner)
                    || !valid_point(m.from)
                    || m.impact <= m.born
                    || m.impact - m.born > 2000
                    || m.source.len() > 256
                    || m.target.len() > 256
            })
            || game.events.len() > 10000
            || game.props.iter().any(|p| {
                !valid_point(p.at)
                    || !p.hp.is_finite()
                    || !p.heading.is_finite()
                    || !["sandbags", "wall", "crates", "crater", "wreck"].contains(&p.kind.as_str())
                    || p.wreck.as_ref().is_some_and(|w| {
                        p.kind != "wreck"
                            || !spec(&w.piece.kind).is_object()
                            || !w.piece.model_scale.is_finite()
                            || !(0.01..=5.).contains(&w.piece.model_scale)
                            || !w.piece.heading.is_finite()
                            || w.piece.aim.is_some_and(|at| !valid_point(at))
                            || w.piece.bridge_end.is_some_and(|at| !valid_point(at))
                            || !w.piece.lots.is_empty()
                            || w.piece.town_blocks.len() > 32
                            || w.half_extents
                                .iter()
                                .any(|n| !n.is_finite() || !(0.01..=2000.).contains(n))
                            || w.crash
                                .as_ref()
                                .is_some_and(|c| w.piece.category != "aircraft" || !c.valid())
                    })
            })
            || game.entities.iter().any(|e| {
                !spec(&e.view.kind).is_object()
                    || if flag(spec(&e.view.kind), "airUnit") {
                        position(e)
                            .iter()
                            .any(|n| !n.is_finite() || n.abs() > 10000.)
                    } else {
                        !valid_point(position(e))
                    }
                    || !ids.insert(&e.view.id)
                    || !(-1..=1).contains(&e.view.owner)
                    || e.bridge_end.is_some_and(|end| !valid_point(end))
                    || !e.view.suppression.is_finite()
                    || !(0. ..=1.).contains(&e.view.suppression)
                    || e.view.velocity.iter().any(|n| !n.is_finite())
                    || e.view
                        .aim
                        .is_some_and(|at| at.iter().any(|v| !v.is_finite()))
                    || !e.view.model_scale.is_finite()
                    || !(0.01..=5.).contains(&e.view.model_scale)
                    || e.view.squad.is_some_and(|n| n >= 9)
                    || e.gun_heading.is_some_and(|n| !n.is_finite())
                    || e.warfare.goal.is_some_and(|p| !valid_point(p))
                    || e.helicopter.evade_goal.is_some_and(|p| !valid_point(p))
                    || !e.fuel.is_finite()
                    || !e.aircraft.round_credit.is_finite()
                    || e.aircraft.flares > 3
                    || !e.aircraft.world_y.is_finite()
                    || e.gunship.as_ref().is_some_and(|m| {
                        e.view.kind != "c130" || !m.valid(game.terrain.cols, game.terrain.rows)
                    })
                    || !e.aircraft.yaw_rate.is_finite()
                    || !e.aircraft.vertical_speed.is_finite()
                    || !e.aircraft.approach_height.is_finite()
                    || !e.aircraft.egress_height.is_finite()
                    || !e.aircraft.egress_velocity.is_finite()
                    || !(0. ..=1.).contains(&e.aircraft.rotor_power)
                    || !e.view.flight_pitch.is_finite()
                    || !e.view.flight_bank.is_finite()
                    || e.view
                        .altitude
                        .is_some_and(|h| !h.is_finite() || !(0. ..=2000.).contains(&h))
                    || e.aircraft.waypoints.len() > 32
                    || e.aircraft
                        .waypoints
                        .iter()
                        .flatten()
                        .any(|n| !n.is_finite() || n.abs() > 10000.)
                    || e.aircraft.mission.is_some_and(|p| !valid_point(p))
                    || e.aircraft.stand.is_some_and(|slot| {
                        slot >= crate::airfield_layout::stands(&e.view.kind).len()
                    })
                    || e.aircraft.stand.is_some_and(|slot| {
                        game.entities.iter().any(|o| {
                            o.view.id != e.view.id
                                && o.home == e.home
                                && crate::airfield_layout::shares_stands(&o.view.kind, &e.view.kind)
                                && o.aircraft.stand == Some(slot)
                        })
                    })
                    || e.view.garrison.as_ref().is_some_and(|g| {
                        e.view.category != "troop"
                            || g.door_stage > 2
                            || e.transport.is_some()
                            || self.garrison_slot(g).is_none()
                            || self.entities.iter().any(|other| {
                                other.view.id != e.view.id
                                    && other.view.garrison.as_ref().is_some_and(|o| {
                                        o.building == g.building
                                            && o.lot == g.lot
                                            && (o.window == g.window
                                                || other.view.owner != e.view.owner)
                                    })
                            })
                    })
                    || e.view.lots.len()
                        > if matches!(
                            e.view.town_plan,
                            crate::settlement_layout::TownPlan::Joined
                                | crate::settlement_layout::TownPlan::Rural
                        ) {
                            96
                        } else {
                            32
                        }
                    || e.view.lots.iter().any(|l| {
                        l.at.iter().any(|v| {
                            !v.is_finite()
                                || v.abs()
                                    > if matches!(
                                        e.view.town_plan,
                                        crate::settlement_layout::TownPlan::Joined
                                            | crate::settlement_layout::TownPlan::Rural
                                    ) {
                                        180.
                                    } else {
                                        64.
                                    }
                        }) || !l.yaw.is_finite()
                            || !l.ground.is_finite()
                            || !l.integrity.is_finite()
                            || !(0. ..=1.).contains(&l.integrity)
                            || !l.height.is_finite()
                            || !(0.1..=12.).contains(&l.height)
                            || !(0.1..=20.).contains(&l.width)
                            || !(0.1..=20.).contains(&l.depth)
                    })
            })
            || game.bases.len() > 2
            || game.bases.iter().any(|b| {
                !(0..=1).contains(&b.owner)
                    || !valid_point(b.origin)
                    || !b.yaw.is_finite()
                    || !b.scale.is_finite()
                    || !(0.01..=1.).contains(&b.scale)
            })
            || game.jobs.iter().any(|j| {
                j.owner < 0 || j.owner > 1 || j.duration == 0 || !spec(&j.unit_type).is_object()
            })
        {
            return Err("Unsupported or invalid native save.".into());
        }
        Ok(())
    }
}
include!("bases.rs");
include!("checkpoint.rs");
include!("specialists.rs");
include!("settlements.rs");
include!("tactics.rs");
include!("navigation.rs");
include!("garrison.rs");
include!("aircraft.rs");
include!("gunship.rs");
fn default_config() -> Value {
    serde_json::from_str::<Value>(include_str!("../config.json")).unwrap()["defaults"].clone()
}
impl NativeGame {
    fn upgrade_weapon_ranges(&mut self) {
        if self.config["weaponRangeRevision"].as_u64().unwrap_or(0) >= 1 {
            return;
        }
        for (key, old, new) in [
            ("shortRange", 32., 45.),
            ("mediumRange", 72., 100.),
            ("longRange", 144., 200.),
            ("xtralongRange", 210., 310.),
        ] {
            if self.config[key].is_null() || number(&self.config, key, old) == old {
                self.config[key] = json!(new);
            }
        }
        self.config["weaponRangeRevision"] = json!(1);
    }
}
fn read_point(v: &Value) -> Option<Point> {
    let x = v["x"].as_f64()? as f32;
    let z = v["y"].as_f64()? as f32;
    if x.is_finite() && z.is_finite() {
        Some([x, z])
    } else {
        None
    }
}
pub fn save_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from))
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|p| std::path::PathBuf::from(p).join(".local/share"))
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        })
        .join("RuleOfWar/native-operation.json")
}

#[cfg(test)]
mod tests {
    #[test]
    fn support_guns_keep_direct_targets_and_advance_past_nearer_enemies() {
        for owner in [0, 1] {
            let mut g = NativeGame::operation(crate::training::ID, 320, 320, default_config());
            g.entities.clear();
            g.bases.clear();
            g.props.clear();
            g.config["fogEnabled"] = json!(false);
            // Exercise an out-of-range order independently of the shipping balance.
            g.config["longRange"] = json!(144.);
            let gun = g.spawn("long", owner, [100., 200.]);
            let near = g.spawn("medium", 1 - owner, [130., 200.]);
            let chosen = g.spawn("medium", 1 - owner, [315., 200.]);
            for e in &mut g.entities[1..] {
                e.view.hp = 1000.;
                e.stance = "hold-fire".into();
            }
            g.command_as(
                owner,
                json!({"type":"command","commandType":"attack","unitIds":[gun],"targetId":chosen}),
            );
            for _ in 0..160 {
                g.tick += 1;
                g.movement();
                g.combat();
            }
            assert!(
                g.entities[0].view.x > 103.,
                "nearby enemy must not stop the ordered advance"
            );
            assert!(
                g.events.iter().all(|e| e.kind != "shot"),
                "out-of-range order must not silently shoot a nearer enemy"
            );
            g.entities[2].view.x = 200.;
            for _ in 0..180 {
                g.tick += 1;
                g.movement();
                g.combat();
            }
            let shots: Vec<_> = g
                .events
                .iter()
                .filter(|e| e.kind == "shot" && e.source == gun)
                .collect();
            assert!(!shots.is_empty());
            assert!(
                shots
                    .iter()
                    .all(|e| distance(e.from.unwrap(), [200., 200.]) < 30.)
            );
            assert_eq!(g.entities[0].target.as_deref(), Some(chosen.as_str()));
            assert_eq!(
                g.entities
                    .iter()
                    .find(|e| e.view.id == near)
                    .unwrap()
                    .view
                    .hp,
                1000.
            );
        }
    }
    #[test]
    fn direct_attack_survives_temporary_loss_of_sight_without_retargeting() {
        let mut g = NativeGame::operation(crate::training::ID, 320, 320, default_config());
        g.entities.clear();
        g.bases.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(false);
        let gun = g.spawn("long", 0, [100., 200.]);
        g.spawn("medium", 1, [130., 200.]);
        let chosen = g.spawn("medium", 1, [315., 200.]);
        for e in &mut g.entities[1..] {
            e.stance = "hold-fire".into();
        }
        g.command(
            json!({"type":"command","commandType":"attack","unitIds":[gun],"targetId":chosen}),
        );
        g.config["fogEnabled"] = json!(true);
        for _ in 0..120 {
            g.tick += 1;
            g.movement();
            g.combat();
        }
        assert_eq!(
            g.entities[0].orders.front().unwrap().target.as_deref(),
            Some(chosen.as_str())
        );
        assert!(g.events.iter().all(|e| e.kind != "shot"));
    }
    #[test]
    fn explicit_attacks_override_hold_fire_in_a_normal_battle() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.bases.clear();
        g.human_opponent = true;
        g.config["fogEnabled"] = json!(false);
        let rifle = g.spawn("short", 0, [30., 30.]);
        // Exercise the order override against a target rifles can damage.
        let target = g.spawn("short", 1, [38., 30.]);
        let dummy = g.entities.last_mut().unwrap();
        dummy.view.hp = 100.;
        dummy.view.max_hp = 100.;
        for (owner, id) in [(0, &rifle), (1, &target)] {
            g.command_as(
                owner,
                json!({"type":"command","commandType":"hold-fire","unitIds":[id]}),
            );
        }
        for _ in 0..40 {
            g.step();
        }
        assert!(g.projectiles.is_empty());
        g.command(
            json!({"type":"command","commandType":"attack","unitIds":[rifle],"targetId":target}),
        );
        for _ in 0..80 {
            g.step();
        }
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == target)
                .unwrap()
                .view
                .hp
                < 100.
        );
        g.command(json!({"type":"command","commandType":"hold-fire","unitIds":[rifle]}));
        g.command(json!({"type":"command","commandType":"attack-move","unitIds":[rifle],"destination":{"x":32.,"y":30.}}));
        assert_eq!(
            g.entities
                .iter()
                .find(|e| e.view.id == rifle)
                .unwrap()
                .stance,
            "attack"
        );
    }
    use super::*;
    #[test]
    fn ships_stay_in_water() {
        let mut g = NativeGame::new();
        let id = g.spawn("waterTransport", 0, [80., 50.]);
        assert_eq!(
            g.issue(
                0,
                std::slice::from_ref(&id),
                Order {
                    kind: "move".into(),
                    destination: Some([80., 55.]),
                    target: None
                },
                false
            ),
            1
        );
        for _ in 0..120 {
            g.tick += 1;
            g.movement();
        }
        let e = g.entities.iter().find(|e| e.view.id == id).unwrap();
        assert!(distance(position(e), [80., 55.]) < 0.2);
        assert_eq!(
            g.issue(
                0,
                &[id],
                Order {
                    kind: "move".into(),
                    destination: Some([70., 55.]),
                    target: None
                },
                false
            ),
            0
        );
    }
    #[test]
    fn engineer_deploys_nest_and_pays_catalog_cost() {
        let mut g = NativeGame::new();
        let before = g.supply[0];
        assert!(
            g.build("machineGunner", Some([30., 87.]))
                .contains("deployed")
        );
        assert_eq!(g.supply[0], before - 16.);
        assert!(g.entities.iter().any(|e| e.view.kind == "machineGunner"));
    }
    #[test]
    fn settlements_do_not_stack_passive_currency_income() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.spawn("village", 0, [40., 40.]);
        g.spawn("town", 0, [60., 60.]);
        let before = g.supply[0];
        for _ in 0..60 {
            g.economy_support();
        }
        assert_eq!(g.supply[0], before);
    }
    #[test]
    fn campaign_outcome_persists_in_save() {
        let mut g = NativeGame::new();
        g.start_sector(21).unwrap();
        g.terrain.brush(30., 30., 2, 0.1, false);
        let edited = g.terrain.cell(30., 30.).elevation;
        for e in &mut g.entities {
            if e.view.owner == 1 {
                e.view.hp = 0.;
            }
        }
        g.step();
        assert_eq!(g.world.sectors[21].owner, Some(0));
        let restored: NativeGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(restored.world.sectors[21].supply, 1);
        let mut replay = restored;
        replay.world.sectors[21].owner = Some(1);
        replay.start_sector(21).unwrap();
        assert_eq!(replay.terrain.cell(30., 30.).elevation, edited);
    }
    #[test]
    fn imports_actual_web_checkpoint() {
        let v: Value = serde_json::from_str(include_str!("../tests/web-checkpoint.json")).unwrap();
        let g = NativeGame::from_web_checkpoint(&v).unwrap();
        assert_eq!(g.tick, 7);
        assert_eq!(g.supply, [61., 47.]);
        assert_eq!(g.terrain.cols, 128);
        assert!(g.paused);
        let e = g
            .entities
            .iter()
            .find(|e| e.view.id == "web-infantry")
            .unwrap();
        assert_eq!(e.orders.len(), 2);
        assert_eq!(e.orders[1].kind, "attack-move");
        assert_eq!(e.orders[1].destination, Some([45., 35.]));
        assert_eq!(g.jobs[0].progress, 24);
        assert!((g.terrain.cell(17., 19.).elevation - 0.51).abs() < 0.0001);
    }
    #[test]
    fn large_maps_and_saves_keep_dimensions() {
        let mut g = NativeGame::operation("custom", 2000, 1800, default_config());
        assert!(g.path([1750., 900.], [1755., 900.], true).len() == 1);
        g.paused = true;
        let mut restored: NativeGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        restored.terrain.hydrate();
        assert_eq!(restored.terrain.rows, 1800);
        assert_eq!(restored.terrain.cols, 2000);
    }
    #[test]
    fn move_orders_keep_weapons_safe() {
        let mut g = NativeGame::new();
        g.entities.clear();
        let id = g.spawn("short", 0, [30., 30.]);
        g.spawn("short", 1, [35., 30.]);
        g.issue(
            0,
            &[id],
            Order {
                kind: "move".into(),
                destination: Some([40., 30.]),
                target: None,
            },
            false,
        );
        for _ in 0..40 {
            g.tick += 1;
            g.combat();
        }
        assert!(g.projectiles.iter().all(|p| p.owner != 0));
    }
    #[test]
    fn paths_cross_bridges_not_water() {
        let g = NativeGame::new();
        let p = g.path([30., 50.], [130., 50.], false);
        assert!(!p.is_empty());
        assert!(p.iter().all(|p| !water(p[0], p[1])));
        assert!(
            p.iter()
                .any(|p| p[0] == 80. && [38., 80., 122.].iter().any(|z| (p[1] - z).abs() < 4.))
        );
        assert!(g.path([30., 50.], [80., 50.], false).is_empty());
    }
    #[test]
    fn ownership_pause_and_victory_are_enforced() {
        let mut g = NativeGame::new();
        let enemy = g
            .entities
            .iter()
            .find(|e| e.view.owner == 1 && !e.view.building)
            .unwrap()
            .view
            .id
            .clone();
        assert_eq!(
            g.issue(
                0,
                &[enemy],
                Order {
                    kind: "move".into(),
                    destination: Some([30., 30.]),
                    target: None
                },
                false
            ),
            0
        );
        g.paused = true;
        g.step();
        assert_eq!(g.tick, 0);
        g.paused = false;
        g.winner = Some(0);
        g.step();
        assert_eq!(g.tick, 0);
    }
    #[test]
    fn recruitment_cost_completion_and_producer_loss() {
        let mut g = NativeGame::new();
        g.tick = 1;
        let id = g
            .entities
            .iter()
            .find(|e| e.view.owner == 0 && e.view.kind == "barracks")
            .unwrap()
            .view
            .id
            .clone();
        let count = g.entities.len();
        g.recruit("short", &id, 0);
        assert_eq!(g.supply[0], 72.);
        g.recruit("short", &id, 0);
        assert_eq!(g.supply[0], 64.);
        g.production();
        assert_eq!(
            g.jobs[1].progress, 0,
            "queued units must wait for the current recruit"
        );
        for _ in 0..400 {
            g.production();
        }
        assert_eq!(g.entities.len(), count + 2);
        g.recruit("short", &id, 0);
        g.entities.retain(|e| e.view.id != id);
        for _ in 0..400 {
            g.production();
        }
        assert_eq!(g.supply[0], 64.);
    }
    #[test]
    fn save_round_trip_preserves_deterministic_continuation() {
        let mut a = NativeGame::new();
        a.spawn("short", 0, [50., 50.]);
        a.spawn("short", 1, [60., 50.]);
        for _ in 0..10 {
            a.step();
        }
        // Retain an in-flight shell while exercising save continuation.
        a.projectiles.push(Projectile {
            owner: 0,
            from: [50., 50.],
            to: [60., 50.],
            born: a.tick,
            impact: a.tick + 20,
            damage: 1.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert!(!a.projectiles.is_empty());
        let mut b: NativeGame = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        for _ in 0..100 {
            a.step();
            b.step();
        }
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
    #[test]
    fn fog_hides_enemy_and_objectives_can_win() {
        let mut g = NativeGame::default();
        assert!(
            g.snapshot()
                .entities
                .iter()
                .all(|e| e.owner != 1 || e.building)
        );
        for e in &mut g.entities {
            if e.view.kind == "objective" {
                e.view.owner = 0;
            }
        }
        g.tick = DEPLOYMENT_TICKS;
        for _ in 0..360 {
            g.objectives();
        }
        assert_eq!(g.winner, Some(0));
    }
    #[test]
    fn combat_projectiles_damage_enemy_not_friend() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.spawn("short", 0, [30., 30.]);
        g.spawn("short", 1, [31., 30.]);
        g.projectiles.push(Projectile {
            owner: 0,
            from: [30., 30.],
            to: [31., 30.],
            born: 0,
            impact: 0,
            hit: true,
            damage: 1.,
            radius: 2.,
            weapon: String::new(),
            ground_hit: false,
        });
        g.combat();
        assert_eq!(g.entities[0].view.hp, 3.);
        assert_eq!(g.entities[1].view.hp, 2.);
    }
    #[test]
    fn queued_movement_arrives_and_stop_clears_it() {
        let mut g = NativeGame::new();
        let id = g.spawn("scout", 0, [40., 40.]);
        for destination in [[42., 40.], [42., 42.]] {
            assert_eq!(
                g.issue(
                    0,
                    std::slice::from_ref(&id),
                    Order {
                        kind: "move".into(),
                        destination: Some(destination),
                        target: None
                    },
                    true
                ),
                1
            );
        }
        for _ in 0..120 {
            g.tick += 1;
            g.movement();
        }
        let e = g.entities.iter().find(|e| e.view.id == id).unwrap();
        assert!(distance(position(e), [42., 42.]) < 0.15);
        assert!(e.orders.is_empty());
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "move".into(),
                destination: Some([50., 50.]),
                target: None,
            },
            false,
        );
        g.issue(
            0,
            std::slice::from_ref(&id),
            Order {
                kind: "stop".into(),
                destination: None,
                target: None,
            },
            false,
        );
        assert!(
            g.entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .path
                .is_empty()
        );
    }
    #[test]
    fn construction_checks_range_supply_and_completes() {
        let mut g = NativeGame::new();
        // A dedicated work site keeps the check independent of base dressing.
        g.entities.clear();
        g.spawn("commander", 0, [25., 67.]);
        let before = g.supply[0];
        g.build("mine", Some([120., 20.]));
        assert_eq!(g.supply[0], before);
        assert!(
            g.build("mine", Some([31., 67.]))
                .starts_with("Constructing")
        );
        assert_eq!(g.supply[0], before - 25.);
        for _ in 0..240 {
            g.production();
        }
        assert!(
            g.entities
                .iter()
                .any(|e| e.view.kind == "mine" && e.view.x == 31. && e.view.z == 67.)
        );
    }
    #[test]
    fn saves_are_replaced_atomically_and_invalid_versions_rejected() {
        let path =
            std::env::temp_dir().join(format!("rule-of-war-test-{}.json", std::process::id()));
        let mut g = NativeGame::new();
        g.save_to(&path).unwrap();
        g.tick = 80;
        g.save_to(&path).unwrap();
        assert_eq!(NativeGame::load_from(&path).unwrap().tick, 80);
        g.version = 999;
        g.save_to(&path).unwrap();
        assert!(NativeGame::load_from(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod routing_regression_tests {
    use super::*;
    #[test]
    fn deep_water_bridges_route_without_overflow_and_close_when_destroyed() {
        let mut game = NativeGame::new();
        for z in 0..160 {
            game.terrain.edits.insert(
                format!("80,{z}"),
                crate::terrain::Cell {
                    elevation: 0.0,
                    surface: 0.33,
                    moisture: 1.,
                    kind: 0,
                },
            );
        }
        assert!(game.path([75., 80.], [85., 80.], false).is_empty());
        game.spawn("bridge", 0, [75., 80.]);
        game.entities.last_mut().unwrap().bridge_end = Some([85., 80.]);
        let route = game.path([75., 80.], [85., 80.], false);
        assert!(!route.is_empty());
        assert!(route.iter().any(|p| p[0] == 80.));
        game.entities.last_mut().unwrap().view.hp = 0.;
        assert!(game.path([75., 80.], [85., 80.], false).is_empty());
    }
}

fn attack_point(e: &Entity, from: Point) -> Point {
    let local = building_local(e, from);
    e.view
        .lots
        .iter()
        .filter(|l| l.integrity > 0.)
        .min_by(|a, b| {
            distance(local, a.nearest(local)).total_cmp(&distance(local, b.nearest(local)))
        })
        .map(|l| building_world(e, l.nearest(local)))
        .unwrap_or_else(|| position(e))
}

fn building_local(e: &Entity, at: Point) -> Point {
    let (s, c) = e.view.heading.sin_cos();
    let scale = e.view.model_scale.max(0.1);
    let (x, z) = (at[0] - e.view.x, at[1] - e.view.z);
    [(c * x - s * z) / scale, (s * x + c * z) / scale]
}
fn building_world(e: &Entity, at: Point) -> Point {
    let (s, c) = e.view.heading.sin_cos();
    let scale = e.view.model_scale.max(0.1);
    [
        e.view.x + (c * at[0] + s * at[1]) * scale,
        e.view.z + (-s * at[0] + c * at[1]) * scale,
    ]
}
fn ordered_lot<'a>(
    target: &'a Entity,
    order: Option<&Order>,
) -> Option<&'a crate::settlement_layout::Lot> {
    let order =
        order.filter(|o| o.kind == "attack" && o.target.as_ref() == Some(&target.view.id))?;
    let anchor = building_local(target, order.destination?);
    // Include ruins in the lookup so a completed order cannot switch houses.
    target
        .view
        .lots
        .iter()
        .map(|lot| (lot, distance(anchor, lot.nearest(anchor))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(lot, _)| lot)
}
fn order_attack_point(target: &Entity, from: Point, order: Option<&Order>) -> Point {
    ordered_lot(target, order)
        .map(|lot| {
            let anchor = building_local(target, order.unwrap().destination.unwrap());
            building_world(target, lot.nearest(anchor))
        })
        .unwrap_or_else(|| attack_point(target, from))
}
fn aim_point(shooter: &Entity, target: &Entity, from: Point) -> Point {
    order_attack_point(target, from, shooter.orders.front())
}

#[cfg(test)]
mod building_target_tests {
    use super::*;
    fn town(g: &mut NativeGame) -> String {
        let id = g.spawn("village", -1, [160., 180.]);
        let e = g.entities.last_mut().unwrap();
        e.view.model_scale = 1.;
        e.view.lots = [-15., 15.]
            .map(|x| crate::settlement_layout::Lot {
                at: [x, 0.],
                width: 8.,
                depth: 8.,
                height: 3.2,
                yaw: 0.,
                style: 0,
                ground: 0.,
                integrity: 1.,
            })
            .to_vec();
        id
    }
    fn field() -> NativeGame {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.entities.clear();
        g.bases.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(false);
        g.human_opponent = true;
        g
    }
    #[test]
    fn direct_vehicle_orders_hit_the_clicked_house_after_save_load_and_stop_when_it_is_ruined() {
        for owner in [0, 1] {
            for kind in ["medium", "long", "xtralong"] {
                let mut g = field();
                let shooter = g.spawn(kind, owner, [130., 155.]);
                let target = town(&mut g);
                let chosen = [175., 180.];
                g.rng = 1;
                assert_eq!(
                    g.issue(
                        owner,
                        std::slice::from_ref(&shooter),
                        Order {
                            kind: "attack".into(),
                            target: Some(target),
                            destination: Some(chosen)
                        },
                        false
                    ),
                    1
                );
                g = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
                g.terrain.hydrate();
                for _ in 0..240 {
                    g.tick += 1;
                    g.movement();
                    g.combat();
                    if g.events
                        .iter()
                        .any(|e| e.kind == "shot" && e.source == shooter)
                    {
                        break;
                    }
                }
                let shot = g
                    .events
                    .iter()
                    .find(|e| e.kind == "shot" && e.source == shooter)
                    .unwrap_or_else(|| panic!("{kind} team {owner} did not shoot"));
                assert_eq!(
                    shot.from,
                    Some(chosen),
                    "{kind} team {owner} aimed at a different house"
                );
                for _ in 0..30 {
                    g.tick += 1;
                    g.combat();
                }
                assert_eq!(g.entities[1].view.lots[0].integrity, 1.);
                assert!(g.entities[1].view.lots[1].integrity < 1.);
                g.entities[1].view.lots[1].integrity = 0.;
                g.tick += 1;
                g.movement();
                g.combat();
                assert!(g.entities[0].orders.is_empty());
                assert_eq!(g.entities[1].view.lots[0].integrity, 1.);
            }
        }
    }
    #[test]
    fn another_house_in_the_same_town_blocks_the_selected_house() {
        let mut g = field();
        let shooter = g.spawn("medium", 0, [125., 180.]);
        let target = town(&mut g);
        g.issue(
            0,
            &[shooter],
            Order {
                kind: "attack".into(),
                target: Some(target),
                destination: Some([175., 180.]),
            },
            false,
        );
        assert!(!g.firing_line(&g.entities[0], &g.entities[1]));
        g.tick = 20;
        g.movement();
        assert!(
            !g.entities[0].path.is_empty(),
            "a blocked tank needs a firing-position route"
        );
        assert_eq!(
            aim_point(&g.entities[0], &g.entities[1], position(&g.entities[0])),
            [175., 180.]
        );
    }
}

#[cfg(test)]
mod core_loop_tests {
    use super::*;
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(true);
        g
    }
    #[test]
    fn vehicle_and_recon_sight_is_shared_by_entities_props_and_snapshots() {
        let mut g = field();
        spotting_tests::open_sight(&mut g);
        g.spawn("short", 0, [20., 30.]);
        g.spawn("short", 1, [75., 30.]);
        assert!(!g.visible(0, &g.entities[1]));
        assert!(!g.point_visible(0, [75., 30.]));
        g.spawn("medium", 0, [20., 30.]);
        assert!(g.visible(0, &g.entities[1]));
        assert!(g.point_visible(0, [75., 30.]));
        assert_eq!(g.snapshot_for(0).sights[1].radius, 72.);
        g.entities[2].view.downed = true;
        assert!(!g.visible(0, &g.entities[1]));
        g.spawn("scout", 0, [20., 30.]);
        assert!(g.point_visible(0, [55., 30.]));
        assert!(sight_range("helicopter") > sight_range("medium"));
    }
    #[test]
    fn larger_battle_and_training_maps_share_scaled_sight_without_longer_weapons() {
        for id in [crate::battlefield::ID, crate::training::ID] {
            let mut g = NativeGame::operation(id, 1024, 1024, default_config());
            spotting_tests::open_sight(&mut g);
            assert_eq!((g.terrain.cols, g.terrain.rows), (1024, 1024));
            g.config["fogEnabled"] = json!(true);
            g.entities.clear();
            g.props.clear();
            g.spawn("short", 0, [200., 300.]);
            let enemy = g.spawn("short", 1, [260., 300.]);
            assert_eq!(g.weapon_range("short"), 45.);
            for owner in [0, 1] {
                let snapshot = g.snapshot_for(owner);
                assert_eq!(snapshot.sights[0].radius, 64.);
                assert!(snapshot.entities.iter().all(|p| p.sight == 64.));
                assert_eq!(snapshot.entities.len(), 2);
            }
            g.entities[1].view.x = 265.;
            assert!(!g.snapshot_for(0).entities.iter().any(|p| p.id == enemy));
            g.entities[1].view.x = 390.;
            g.spawn("scout", 0, [200., 300.]);
            assert_eq!(g.unit_sight(&g.entities[2]), 224.);
            assert!(g.snapshot_for(0).entities.iter().any(|p| p.id == enemy));
            g.config["shortRange"] = json!(90.);
            assert_eq!(g.unit_sight(&g.entities[0]), 64.);
            // Existing operation dimensions also determine sight.
            g.terrain.cols = 768;
            g.terrain.rows = 768;
            assert_eq!(g.unit_sight(&g.entities[0]), 48.);
        }
    }
    #[test]
    fn infantry_holds_cover_through_a_lull_and_replans_when_cover_breaks() {
        let mut g = field();
        g.spawn("short", 0, [35., 30.]);
        g.spawn("short", 1, [15., 30.]);
        g.props.push(BattlefieldProp {
            scar: None,
            id: "wall".into(),
            kind: "sandbags".into(),
            at: [33., 30.],
            heading: std::f32::consts::FRAC_PI_2,
            wreck: None,
            hp: 5.,
        });
        g.entities[0].cover_goal = Some([35., 30.]);
        g.entities[0].threat = Some([15., 30.]);
        g.entities[0].orders.push_back(Order {
            kind: "attack-move".into(),
            destination: Some([65., 30.]),
            target: None,
        });
        g.tick = 100;
        g.tactical_step();
        g.movement();
        assert_eq!(g.entities[0].cover_goal, Some([35., 30.]));
        assert!(g.entities[0].path.is_empty());
        assert_eq!(g.entities[0].orders.len(), 1);
        g.props[0].hp = 0.;
        g.props.clear();
        g.tick = 101;
        g.tactical_step();
        assert!(g.entities[0].cover_goal.is_none());
        assert!(!g.entities[0].path.is_empty());
    }
    #[test]
    fn squads_seek_protected_positions_before_taking_damage_and_player_move_overrides() {
        let mut g = field();
        g.spawn("short", 0, [39., 36.]);
        g.spawn("short", 1, [15., 30.]);
        g.props.push(BattlefieldProp {
            scar: None,
            id: "wall".into(),
            kind: "wall".into(),
            at: [33., 30.],
            heading: std::f32::consts::FRAC_PI_2,
            wreck: None,
            hp: 5.,
        });
        g.tick = 4;
        g.tactical_step();
        let goal = g.entities[0].cover_goal.expect("proactive cover");
        assert!(g.cover_level(goal, Some([15., 30.])) > 0);
        let id = g.entities[0].view.id.clone();
        g.issue(
            0,
            &[id],
            Order {
                kind: "move".into(),
                destination: Some([45., 30.]),
                target: None,
            },
            false,
        );
        assert!(g.entities[0].cover_goal.is_none());
    }
    #[test]
    fn ai_rallies_before_advancing_and_leaves_guns_behind_the_assault() {
        let mut g = field();
        g.entities.clear();
        g.props.clear();
        g.bases = vec![BasePlan {
            owner: 1,
            origin: [140., 80.],
            yaw: 0.,
            scale: 1.,
            layout_version: 2,
        }];
        g.spawn("objective", -1, [60., 80.]);
        for z in [72., 80., 88.] {
            g.spawn("short", 1, [125., z]);
        }
        g.spawn("long", 1, [130., 95.]);
        g.ai();
        for e in g.entities.iter().skip(1) {
            assert!(e.orders.front().unwrap().destination.unwrap()[0] > 85.);
        }
        for e in g.entities.iter_mut().skip(1) {
            e.orders.clear();
            e.path.clear();
            e.view.x = 96.;
        }
        g.tick = DEPLOYMENT_TICKS;
        g.ai();
        for e in &g.entities[1..4] {
            assert!(e.orders.front().unwrap().destination.unwrap()[0] < 70.);
        }
        assert!(g.entities[4].orders.front().unwrap().destination.unwrap()[0] > 110.);
    }
    #[test]
    fn tank_rounds_obey_the_longer_reload_even_with_a_continuous_target() {
        let mut g = field();
        g.spawn("medium", 0, [30., 30.]);
        g.spawn("medium", 1, [40., 30.]);
        g.entities[1].stance = "hold-fire".into();
        g.entities[1].view.hp = 1000.;
        g.entities[1].view.max_hp = 1000.;
        for _ in 0..400 {
            g.tick += 1;
            g.combat();
        }
        let shots: Vec<_> = g
            .events
            .iter()
            .filter(|e| e.kind == "shot" && e.owner == 0)
            .map(|e| e.tick)
            .collect();
        assert!(shots.len() >= 3 && shots.len() <= 5);
        assert!(shots.windows(2).all(|s| s[1] - s[0] >= 90));
    }
    #[test]
    fn a_new_operation_uses_one_vehicle_sized_battlefield() {
        let g = NativeGame::default();
        assert_eq!(g.terrain.id, crate::battlefield::ID);
        assert_eq!(g.terrain.cols, crate::battlefield::COLS);
        assert_eq!(g.terrain.rows, crate::battlefield::ROWS);
        assert!(distance(g.bases[0].origin, g.bases[1].origin) > 2000.);
        assert!(distance(g.bases[0].origin, g.bases[1].origin) > 300.);
        assert!(
            g.entities
                .iter()
                .filter(|e| e.view.kind == "short" && e.view.owner == 0)
                .count()
                >= 3
        );
        for owner in [0, 1] {
            let carriers: Vec<_> = g
                .entities
                .iter()
                .filter(|e| e.view.kind == "troopCarrier" && e.view.owner == owner)
                .collect();
            assert_eq!(
                carriers.len(),
                1,
                "each faction starts with one troop carrier"
            );
            let carrier = carriers[0];
            assert!(g.passable_clearance(
                position(carrier),
                0,
                body_clearance("troopCarrier", carrier.view.model_scale)
            ));
            assert!(
                g.entities
                    .iter()
                    .filter(|e| !e.view.building && e.view.id != carrier.view.id)
                    .all(|e| distance(position(e), position(carrier)) >= 4.)
            );
            assert!(carrier.orders.is_empty() && carrier.transport.is_none());
        }
        assert_eq!(crate::menu::MAPS.len(), 1);
        assert_eq!(
            core_config(&json!({"fogEnabled":false,"shortRange":10}))["shortRange"],
            45
        );
    }
}

#[cfg(test)]
mod destination_cover_tests {
    use super::*;
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.human_opponent = true;
        g.config["fogEnabled"] = json!(false);
        g
    }
    fn wall(g: &mut NativeGame, at: Point) {
        g.props.push(BattlefieldProp {
            scar: None,
            id: format!("wall-{}", g.props.len()),
            kind: "sandbags".into(),
            at,
            heading: std::f32::consts::FRAC_PI_2,
            wreck: None,
            hp: 5.,
        });
    }
    #[test]
    fn a_move_near_cover_arrives_and_stays_without_needing_incoming_fire() {
        let mut g = field();
        let id = g.spawn("short", 0, [20., 30.]);
        wall(&mut g, [44., 30.]);
        g.issue(
            0,
            &[id],
            Order {
                kind: "move".into(),
                destination: Some([40., 30.]),
                target: None,
            },
            false,
        );
        let goal = g.entities[0].orders[0].destination.unwrap();
        assert!(distance(goal, [40., 30.]) < 8.);
        assert!(g.cover_level(goal, None) > 0);
        for tick in 1..=240 {
            g.tick = tick;
            g.tactical_step();
            g.movement();
        }
        assert!(distance(position(&g.entities[0]), goal) < 0.2);
        assert!(g.entities[0].orders.is_empty());
        assert_eq!(g.entities[0].cover_goal, Some(goal));
        let pos = position(&g.entities[0]);
        for tick in 241..=300 {
            g.tick = tick;
            g.tactical_step();
            g.movement();
        }
        assert_eq!(position(&g.entities[0]), pos);
    }
    #[test]
    fn threatened_units_prefer_destination_cover_over_nearby_start_cover() {
        let mut g = field();
        let id = g.spawn("short", 0, [30., 30.]);
        wall(&mut g, [31., 26.]);
        wall(&mut g, [47., 30.]);
        g.spawn("short", 1, [65., 30.]);
        g.issue(
            0,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([45., 30.]),
                target: None,
            },
            false,
        );
        g.tick = 4;
        g.tactical_step();
        let goal = g.entities[0]
            .cover_goal
            .expect("destination cover reserved");
        assert!(distance(goal, [45., 30.]) < 8.);
        assert!(distance(goal, [30., 30.]) > 10.);
    }
    #[test]
    fn two_individuals_reserve_different_slots_near_the_requested_point() {
        let mut g = field();
        let a = g.spawn("short", 0, [20., 30.]);
        let b = g.spawn("short", 0, [20., 34.]);
        wall(&mut g, [44., 30.]);
        assert_eq!(
            g.issue(
                0,
                &[a, b],
                Order {
                    kind: "move".into(),
                    destination: Some([40., 30.]),
                    target: None
                },
                false
            ),
            2
        );
        let a = g.entities[0].cover_goal.unwrap();
        let b = g.entities[1].cover_goal.unwrap();
        assert!(distance(a, b) >= 1.35);
        assert!(distance(a, [40., 30.]) < 8.);
        assert!(distance(b, [40., 30.]) < 8.);
    }
    #[test]
    fn an_injured_soldier_remains_one_person_and_save_import_does_not_expand_him() {
        let mut g = field();
        g.spawn("short", 0, [30., 30.]);
        damage(&mut g.entities[0].view, 1.);
        assert_eq!(g.entities[0].view.strength, 1);
        assert_eq!(g.entities[0].view.hp, 2.);
        g.entities[0].view.strength = 120;
        g.ground_roster_cleanup();
        assert_eq!(g.entities[0].view.strength, 1);
        heal(&mut g.entities[0].view, 1.);
        assert_eq!(g.entities[0].view.strength, 1);
    }
}

include!("training_simulation.rs");

include!("infantry_combat.rs");

include!("infantry_warfare.rs");

include!("transport.rs");

include!("helicopter_tactics.rs");

include!("infantry_engagement.rs");

include!("visibility.rs");
include!("spotting.rs");
include!("spotting_tests.rs");

include!("battle_benchmark.rs");
include!("order_regressions.rs");
include!("balance_regressions.rs");
include!("simulation_regressions.rs");
include!("visual_demo_sim.rs");
include!("gis_prototype_sim.rs");
