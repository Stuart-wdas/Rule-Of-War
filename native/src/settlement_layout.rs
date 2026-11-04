//! Native freestanding lots: shared geometry and authoritative collision footprints.
use serde::{Deserialize, Serialize};

pub const HOUSE_PLOT: f32 = 10.;
pub const HOUSE_HEIGHT: f32 = 3.2;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TownPlan {
    #[default]
    Square,
    H,
    L,
    E,
    Joined,
    Rural,
}
impl TownPlan {
    pub fn plaza(self) -> [f32; 2] {
        match self {
            Self::Rural => crate::rural_layout::world([-8., -18.]),
            Self::L | Self::Joined => [-22., 22.],
            Self::E => [-26., 0.],
            _ => [0., 0.],
        }
    }
    pub fn plazas(self) -> Vec<[f32; 2]> {
        if self == Self::Rural {
            crate::rural_layout::plazas()
        } else if self == Self::Joined {
            vec![[-84., 0.], [-22., 22.], [58., 0.]]
        } else {
            vec![self.plaza()]
        }
    }
    pub fn positions(self) -> Vec<[f32; 2]> {
        let mut points = vec![];
        match self {
            Self::Rural => {
                return crate::rural_layout::lots()
                    .into_iter()
                    .map(|l| l.at)
                    .collect();
            }
            Self::Joined => {
                for (plan, offset) in [(Self::H, -84.), (Self::L, 0.), (Self::E, 84.)] {
                    points.extend(plan.positions().into_iter().map(|p| [p[0] + offset, p[1]]));
                }
                // Infill ties the district edges together and completes the northern housing blocks.
                for at in [
                    [-84., -40.],
                    [-84., 40.],
                    [-35., 41.],
                    [47., 41.],
                    [5., -27.],
                    [19., -27.],
                    [33., -27.],
                    [5., -13.],
                    [19., -13.],
                    [33., -13.],
                ] {
                    points.push(at);
                }
            }
            Self::Square => {
                for z in [-27., -13., 13., 27.] {
                    for x in [-27., -13., 13., 27.] {
                        points.push([x, z]);
                    }
                }
            }
            Self::H => {
                for x in [-37., 37.] {
                    for z in [-27., -13., 13., 27.] {
                        points.push([x, z]);
                    }
                }
                for x in [-14., 0., 14.] {
                    for z in [-13., 13.] {
                        points.push([x, z]);
                    }
                }
            }
            Self::L => {
                for x in [-35., -9.] {
                    for z in [-28., -14., 0.] {
                        points.push([x, z]);
                    }
                }
                for x in [5., 19., 33.] {
                    for z in [9., 35.] {
                        points.push([x, z]);
                    }
                }
            }
            Self::E => {
                for z in [-27., -13., 13., 27.] {
                    points.push([-37., z]);
                }
                for z in [-39., -11., 39.] {
                    for x in [-13., 1., 15., 29.] {
                        points.push([x, z]);
                    }
                }
            }
        }
        points
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousePlan {
    Square,
    L,
    H,
    Straight,
}
#[derive(Clone, Copy, Debug)]
pub struct Wing {
    pub at: [f32; 2],
    pub size: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub normal: [f32; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lot {
    pub at: [f32; 2],
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub yaw: f32,
    pub style: u8,
    #[serde(default)]
    pub ground: f32,
    #[serde(default = "intact")]
    pub integrity: f32,
}
fn intact() -> f32 {
    1.
}
impl Lot {
    /// Top of the pitched roof at a local town coordinate, including eaves.
    /// Courtyards and collapsed lots have no roof contact.
    pub fn roof_height(&self, point: [f32; 2]) -> Option<f32> {
        if self.integrity <= 0.
            || (point[0] - self.at[0]).powi(2) + (point[1] - self.at[1]).powi(2)
                > (self.width * 0.5 + 0.24).powi(2) + (self.depth * 0.5 + 0.24).powi(2)
        {
            return None;
        }
        let p = self.local(point);
        self.wings()
            .iter()
            .filter_map(|wing| {
                let dx = p[0] - wing.at[0];
                let dz = p[1] - wing.at[1];
                if dx.abs() > wing.size[0] * 0.5 + 0.24 || dz.abs() > wing.size[1] * 0.5 + 0.24 {
                    return None;
                }
                let width = wing.size[0].min(wing.size[1]);
                let across = if wing.size[0] > wing.size[1] { dz } else { dx };
                let rise = (width * 0.38).clamp(0.80, 1.75);
                Some(
                    self.ground
                        + HOUSE_HEIGHT
                        + rise * (1. - across.abs() / (width * 0.5 + 0.22)).clamp(0., 1.)
                        + 0.12,
                )
            })
            .max_by(f32::total_cmp)
    }
    pub fn plan(&self) -> HousePlan {
        match self.style % 4 {
            0 => HousePlan::Square,
            1 => HousePlan::L,
            2 => HousePlan::H,
            _ => HousePlan::Straight,
        }
    }
    pub fn wings(&self) -> Vec<Wing> {
        let (w, d) = (self.width * 0.84, self.depth * 0.84);
        match self.plan() {
            HousePlan::Square => vec![Wing {
                at: [0., 0.],
                size: [w, d],
            }],
            HousePlan::Straight => vec![Wing {
                at: [0., 0.],
                size: [w, d / 3.],
            }],
            HousePlan::L => vec![
                Wing {
                    at: [-w / 3., 0.],
                    size: [w / 3., d],
                },
                Wing {
                    at: [w / 6., d / 3.],
                    size: [w * 2. / 3., d / 3.],
                },
            ],
            HousePlan::H => vec![
                Wing {
                    at: [-w / 3., 0.],
                    size: [w / 3., d],
                },
                Wing {
                    at: [w / 3., 0.],
                    size: [w / 3., d],
                },
                Wing {
                    at: [0., 0.],
                    size: [w / 3., d / 3.],
                },
            ],
        }
    }
    fn occupied(&self, x: i32, z: i32) -> bool {
        (0..3).contains(&x)
            && (0..3).contains(&z)
            && match self.plan() {
                HousePlan::Square => true,
                HousePlan::L => x == 0 || z == 2,
                HousePlan::H => x != 1 || z == 1,
                HousePlan::Straight => z == 1,
            }
    }
    /// Merge only exposed cell edges. Recesses have walls, internal wing joins do not.
    pub fn walls(&self) -> Vec<Wall> {
        let mut edges = std::collections::BTreeMap::<(i32, i32, i32), Vec<i32>>::new();
        for z in 0..3 {
            for x in 0..3 {
                if self.occupied(x, z) {
                    for (nx, nz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        if !self.occupied(x + nx, z + nz) {
                            let plane = if nx != 0 {
                                x + i32::from(nx > 0)
                            } else {
                                z + i32::from(nz > 0)
                            };
                            edges.entry((nx, nz, plane)).or_default().push(if nx != 0 {
                                z
                            } else {
                                x
                            });
                        }
                    }
                }
            }
        }
        let mut walls = vec![];
        for ((nx, nz, plane), mut along) in edges {
            along.sort_unstable();
            let mut start = along[0];
            let mut end = start + 1;
            for next in along.into_iter().skip(1).chain(std::iter::once(99)) {
                if next == end {
                    end += 1;
                    continue;
                }
                let point = |u: i32| {
                    if nx != 0 {
                        [
                            (plane as f32 / 3. - 0.5) * self.width * 0.84,
                            (u as f32 / 3. - 0.5) * self.depth * 0.84,
                        ]
                    } else {
                        [
                            (u as f32 / 3. - 0.5) * self.width * 0.84,
                            (plane as f32 / 3. - 0.5) * self.depth * 0.84,
                        ]
                    }
                };
                walls.push(Wall {
                    a: point(start),
                    b: point(end),
                    normal: [nx as f32, nz as f32],
                });
                start = next;
                end = next + 1;
            }
        }
        walls
    }
    pub fn to_world(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.at[0] + c * p[0] + s * p[1],
            self.at[1] - s * p[0] + c * p[1],
        ]
    }
    pub fn local(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.yaw.sin_cos();
        let (x, z) = (p[0] - self.at[0], p[1] - self.at[1]);
        [c * x - s * z, s * x + c * z]
    }
    /// The front doorway used by both the wall opening and the entry route.
    pub fn door(&self) -> ([f32; 2], [f32; 2]) {
        let (s, c) = self.yaw.sin_cos();
        let x = if self.plan() == HousePlan::H {
            -self.width * 0.84 / 3.
        } else {
            -self.width * 0.22
        };
        let z = self.depth
            * 0.84
            * if self.plan() == HousePlan::Straight {
                1. / 6.
            } else {
                0.5
            };
        (
            [self.at[0] + c * x + s * z, self.at[1] - s * x + c * z],
            [s, c],
        )
    }
    pub fn max_hp(&self) -> f32 {
        (self
            .wings()
            .iter()
            .map(|w| w.size[0] * w.size[1])
            .sum::<f32>()
            * HOUSE_HEIGHT
            * 0.16)
            .max(8.)
    }
    /// The same positions are used for rendered windows and reserved firing slots.
    pub fn windows(&self) -> Vec<([f32; 2], f32, [f32; 2])> {
        let mut result = vec![];
        let (s, c) = self.yaw.sin_cos();
        let flat = Self {
            at: [0., 0.],
            yaw: 0.,
            ..self.clone()
        };
        let (door, _) = flat.door();
        for wall in self.walls() {
            let side = wall.normal[0] != 0.;
            let axis = usize::from(side);
            let length = (wall.b[axis] - wall.a[axis]).abs();
            let count = ((length - 1.) / 1.8).floor().max(1.) as usize;
            for i in 0..count {
                let u = (i as f32 + 0.5) / count as f32;
                let mut at = [
                    wall.a[0] + (wall.b[0] - wall.a[0]) * u,
                    wall.a[1] + (wall.b[1] - wall.a[1]) * u,
                ];
                if wall.normal == [0., 1.]
                    && (at[1] - door[1]).abs() < 0.01
                    && (at[0] - door[0]).abs() < 1.0
                {
                    continue;
                }
                at[0] += wall.normal[0] * 0.18;
                at[1] += wall.normal[1] * 0.18;
                let n = [
                    c * wall.normal[0] + s * wall.normal[1],
                    -s * wall.normal[0] + c * wall.normal[1],
                ];
                result.push((self.to_world(at), 1.5, n));
            }
        }
        result
    }
    pub fn nearest(&self, point: [f32; 2]) -> [f32; 2] {
        let p = self.local(point);
        let nearest = self
            .wings()
            .into_iter()
            .map(|wing| {
                [
                    p[0].clamp(
                        wing.at[0] - wing.size[0] * 0.5,
                        wing.at[0] + wing.size[0] * 0.5,
                    ),
                    p[1].clamp(
                        wing.at[1] - wing.size[1] * 0.5,
                        wing.at[1] + wing.size[1] * 0.5,
                    ),
                ]
            })
            .min_by(|a, b| {
                ((a[0] - p[0]).powi(2) + (a[1] - p[1]).powi(2))
                    .total_cmp(&((b[0] - p[0]).powi(2) + (b[1] - p[1]).powi(2)))
            })
            .unwrap();
        self.to_world(nearest)
    }
    /// Exact wall crossing test, including very short paths past a rotated corner.
    pub fn intersects_segment(&self, from: [f32; 2], to: [f32; 2], margin: f32) -> bool {
        if self.integrity <= 0. {
            return false;
        }
        let bound = (self.width * 0.42 + margin).hypot(self.depth * 0.42 + margin);
        if from[0].max(to[0]) < self.at[0] - bound
            || from[0].min(to[0]) > self.at[0] + bound
            || from[1].max(to[1]) < self.at[1] - bound
            || from[1].min(to[1]) > self.at[1] + bound
        {
            return false;
        }
        let (from, to) = (self.local(from), self.local(to));
        self.wings().iter().any(|wing| {
            let (mut enter, mut exit) = (0_f32, 1_f32);
            for axis in 0..2 {
                let half = wing.size[axis] * 0.5 + margin;
                let delta = to[axis] - from[axis];
                let origin = from[axis] - wing.at[axis];
                if delta.abs() < 0.000001 {
                    if origin.abs() >= half {
                        return false;
                    }
                } else {
                    let a = (-half - origin) / delta;
                    let b = (half - origin) / delta;
                    enter = enter.max(a.min(b));
                    exit = exit.min(a.max(b));
                    if enter > exit {
                        return false;
                    }
                }
            }
            enter <= exit
        })
    }
    pub fn contains(&self, point: [f32; 2], margin: f32) -> bool {
        let squared_bound =
            (self.width * 0.42 + margin).powi(2) + (self.depth * 0.42 + margin).powi(2);
        if self.integrity <= 0.
            || (point[0] - self.at[0]).powi(2) + (point[1] - self.at[1]).powi(2) > squared_bound
        {
            return false;
        }
        let p = self.local(point);
        self.integrity > 0.
            && self.wings().iter().any(|wing| {
                (p[0] - wing.at[0]).abs() < wing.size[0] * 0.5 + margin
                    && (p[1] - wing.at[1]).abs() < wing.size[1] * 0.5 + margin
            })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Road {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub width: f32,
}
/// Interconnected streets, a civic square and perimeter access roads.
pub fn roads_for(kind: &str, plan: TownPlan) -> Vec<Road> {
    if kind == "town" && plan == TownPlan::Rural {
        return crate::rural_layout::roads();
    }
    if kind == "town" && plan == TownPlan::Joined {
        let mut roads = vec![];
        for (district, offset) in [(TownPlan::H, -84.), (TownPlan::L, 0.), (TownPlan::E, 84.)] {
            roads.extend(roads_for(kind, district).into_iter().map(|r| Road {
                a: [r.a[0] + offset, r.a[1]],
                b: [r.b[0] + offset, r.b[1]],
                width: r.width,
            }));
        }
        // Continuous distributor streets connect every district; side streets open onto them.
        for z in [-52., 52.] {
            roads.push(Road {
                a: [-126., z],
                b: [126., z],
                width: 7.,
            });
        }
        for x in [-110., -58., -22., 58.] {
            roads.push(Road {
                a: [x, -52.],
                b: [x, 52.],
                width: 6.,
            });
        }
        roads.push(Road {
            a: [126., -52.],
            b: [126., 52.],
            width: 6.,
        });
        roads.push(Road {
            a: [118., 0.],
            b: [126., 0.],
            width: 6.,
        });
        return roads;
    }
    if kind == "town" && plan != TownPlan::Square {
        let segments: Vec<([f32; 2], [f32; 2])> = match plan {
            TownPlan::H => vec![
                ([-26., -34.], [-26., 34.]),
                ([26., -34.], [26., 34.]),
                ([-26., 0.], [26., 0.]),
            ],
            TownPlan::L => vec![([-22., -34.], [-22., 22.]), ([-22., 22.], [37., 22.])],
            TownPlan::E => vec![
                ([-26., -32.], [-26., 32.]),
                ([-26., -28.], [34., -28.]),
                ([-26., 0.], [34., 0.]),
                ([-26., 28.], [34., 28.]),
            ],
            TownPlan::Square | TownPlan::Joined | TownPlan::Rural => unreachable!(),
        };
        return segments
            .into_iter()
            .map(|(a, b)| Road { a, b, width: 6. })
            .collect();
    }
    let extent = if kind == "town" { 39. } else { 33. };
    let mut roads = vec![
        Road {
            a: [-extent, 0.],
            b: [extent, 0.],
            width: 6.,
        },
        Road {
            a: [0., -extent],
            b: [0., extent],
            width: 6.,
        },
    ];
    for side in [-1., 1.] {
        roads.push(Road {
            a: [-extent, side * extent],
            b: [extent, side * extent],
            width: 4.5,
        });
        roads.push(Road {
            a: [side * extent, -extent],
            b: [side * extent, extent],
            width: 4.5,
        });
    }
    roads
}
impl Road {
    pub fn distance(self, point: [f32; 2]) -> f32 {
        let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
        let t = (((point[0] - self.a[0]) * d[0] + (point[1] - self.a[1]) * d[1])
            / (d[0] * d[0] + d[1] * d[1]))
            .clamp(0., 1.);
        ((point[0] - self.a[0] - d[0] * t).powi(2) + (point[1] - self.a[1] - d[1] * t).powi(2))
            .sqrt()
            - self.width * 0.5
    }
}
pub fn lots(kind: &str, seed: &str) -> Vec<Lot> {
    lots_for(kind, seed, TownPlan::Square)
}
pub fn lots_for(kind: &str, seed: &str, plan: TownPlan) -> Vec<Lot> {
    if kind == "town" && plan == TownPlan::Rural {
        return crate::rural_layout::lots();
    }
    let mut state = seed
        .bytes()
        .fold(2166136261u32, |a, b| (a ^ b as u32).wrapping_mul(16777619));
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state as f32 / u32::MAX as f32
    };
    let mut positions = vec![];
    if kind == "town" {
        positions = plan.positions();
    } else {
        for z in [-13., 13.] {
            for x in [-25., -12., 12., 25.] {
                positions.push([x, z]);
            }
        }
        positions.extend([[0., -25.], [0., 25.]]);
    }
    positions
        .into_iter()
        .enumerate()
        .map(|(i, at)| {
            // Entrances face a street or the square; setbacks vary within spaced plots.
            let at = [
                at[0] + (random() - 0.5) * 0.9,
                at[1] + (random() - 0.5) * 0.9,
            ];
            let road = roads_for(kind, plan)
                .into_iter()
                .map(|r| {
                    let d = [r.b[0] - r.a[0], r.b[1] - r.a[1]];
                    let t = (((at[0] - r.a[0]) * d[0] + (at[1] - r.a[1]) * d[1])
                        / (d[0] * d[0] + d[1] * d[1]))
                        .clamp(0., 1.);
                    [r.a[0] + d[0] * t, r.a[1] + d[1] * t]
                })
                .min_by(|a, b| {
                    ((a[0] - at[0]).powi(2) + (a[1] - at[1]).powi(2))
                        .total_cmp(&((b[0] - at[0]).powi(2) + (b[1] - at[1]).powi(2)))
                })
                .unwrap();
            let yaw = (road[0] - at[0]).atan2(road[1] - at[1]);
            Lot {
                at,
                width: HOUSE_PLOT,
                depth: HOUSE_PLOT,
                height: HOUSE_HEIGHT,
                yaw,
                style: (i as u8 % 4) + (random() * 4.).floor() as u8 * 4,
                ground: 0.,
                integrity: 1.,
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn h_l_and_e_towns_have_distinct_open_recesses_and_connected_streets() {
        for plan in [TownPlan::H, TownPlan::L, TownPlan::E] {
            let houses = lots_for("town", "shapes", plan);
            let roads = roads_for("town", plan);
            assert!(houses.iter().all(|lot| !lot.contains(plan.plaza(), 0.)));
            assert!(roads.iter().any(|r| r.distance(plan.plaza()) < 0.));
            for road in &roads {
                for step in 0..=50 {
                    let t = step as f32 / 50.;
                    let p = [
                        road.a[0] + (road.b[0] - road.a[0]) * t,
                        road.a[1] + (road.b[1] - road.a[1]) * t,
                    ];
                    assert!(
                        houses.iter().all(|lot| !lot.contains(p, 0.3)),
                        "house blocks {plan:?} street {p:?}"
                    );
                }
            }
            match plan {
                TownPlan::H => assert!(
                    !houses
                        .iter()
                        .any(|l| l.at[0].abs() < 20. && l.at[1].abs() > 20.)
                ),
                TownPlan::L => assert!(!houses.iter().any(|l| l.at[0] > 0. && l.at[1] < 0.)),
                TownPlan::E => assert!(
                    !houses
                        .iter()
                        .any(|l| l.at[0] > 0. && (15. ..28.).contains(&l.at[1]))
                ),
                TownPlan::Square | TownPlan::Joined | TownPlan::Rural => unreachable!(),
            }
        }
    }
    #[test]
    fn a_short_segment_cannot_cut_a_rotated_corner_between_clear_endpoints() {
        let mut lot = crate::training::practice_houses().remove(0);
        lot.at = [0., 0.];
        lot.yaw = 0.7;
        let (a, b) = ([6., 0.], [6., 1.]);
        assert!(!lot.contains(a, 0.25) && !lot.contains(b, 0.25));
        assert!(lot.intersects_segment(a, b, 0.25));
        assert!(!lot.intersects_segment([7., 0.], [7., 1.], 0.25));
        lot.integrity = 0.;
        assert!(!lot.intersects_segment(a, b, 0.25));
    }
    #[test]
    fn plans_share_one_plot_with_open_recesses_real_exterior_windows_and_doors() {
        let lots = crate::training::practice_houses();
        assert_eq!(lots.len(), 4);
        for lot in lots {
            assert_eq!([lot.width, lot.depth], [HOUSE_PLOT, HOUSE_PLOT]);
            assert_eq!(lot.height, HOUSE_HEIGHT);
            let (door, n) = lot.door();
            assert!(!lot.contains([door[0] + n[0] * 0.1, door[1] + n[1] * 0.1], 0.));
            assert!(lot.contains([door[0] - n[0] * 0.3, door[1] - n[1] * 0.3], 0.));
            for (p, y, n) in lot.windows() {
                assert_eq!(y, 1.5);
                assert!(!lot.contains(p, 0.));
                assert!(lot.contains([p[0] - n[0] * 0.3, p[1] - n[1] * 0.3], 0.));
            }
            let probe = lot.to_world([0., -3.]);
            assert_eq!(lot.contains(probe, 0.), lot.plan() == HousePlan::Square);
            let nearest = lot.nearest(probe);
            assert!(lot.contains(nearest, 0.01));
        }
    }
    #[test]
    fn lots_are_deterministic_separated_and_human_scaled() {
        for kind in ["town", "village"] {
            let a = lots(kind, "native-test");
            assert_eq!(a, lots(kind, "native-test"));
            assert!(a.len() >= 6);
            assert!(a.iter().all(|l| l.width == HOUSE_PLOT
                && l.depth == HOUSE_PLOT
                && l.height == HOUSE_HEIGHT));
            assert!(a.iter().any(|l| l.yaw.sin().abs() > 0.1));
            assert!(a.iter().all(|l| !l.contains([0., 0.], 0.3)));
        }
    }
    #[test]
    fn rotated_footprints_and_nearest_edge_agree() {
        let lot = Lot {
            at: [10., 10.],
            width: 6.,
            depth: 4.,
            height: 3.,
            yaw: 0.7,
            style: 0,
            ground: 0.,
            integrity: 1.,
        };
        assert!(lot.contains(lot.at, 0.));
        let edge = lot.nearest([22., 20.]);
        assert!(lot.contains(edge, 0.01));
        assert!(!lot.contains([22., 20.], 0.));
    }
}
