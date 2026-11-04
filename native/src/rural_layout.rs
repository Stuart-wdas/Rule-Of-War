//! Reference-led countryside composition. All distances are metres; the layout
//! places existing houses, shrubs, trees and ground surfaces without new assets.
use crate::settlement_layout::{HOUSE_HEIGHT, HOUSE_PLOT, Lot, Road};

pub const HALF_WIDTH: f32 = 142.;
pub const HALF_DEPTH: f32 = 90.;
pub const BOUND: i32 = 168;
const TURN: f32 = std::f32::consts::FRAC_PI_4;

/// Align the long road with the pinned camera's horizontal axis.
pub fn world(p: [f32; 2]) -> [f32; 2] {
    let (s, c) = TURN.sin_cos();
    [c * p[0] + s * p[1], -s * p[0] + c * p[1]]
}
pub fn local(p: [f32; 2]) -> [f32; 2] {
    let (s, c) = TURN.sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}
pub fn map_point(x: f32, z: f32, cols: i32, rows: i32) -> [f32; 2] {
    local([x - cols as f32 * 0.5, z - rows as f32 * 0.5])
}
pub fn coverage(p: [f32; 2]) -> f32 {
    ((HALF_WIDTH + 12. - p[0].abs()).min(HALF_DEPTH + 12. - p[1].abs()) / 12.).clamp(0., 1.)
}

const MAIN: &[[f32; 2]] = &[
    [-144., -12.],
    [-100., -2.],
    [-50., 7.],
    [0., 10.],
    [60., 16.],
    [91., 13.],
    [116., 2.],
    [144., -13.],
];
const LOOP: &[[f32; 2]] = &[
    [-30., 8.2],
    [-30., -10.],
    [-26., -18.],
    [21., -18.],
    [25., -11.],
    [25., 12.5],
];
const FARM_WEST: &[[f32; 2]] = &[
    [-85., 0.7],
    [-88., -14.],
    [-96., -26.],
    [-98., -49.],
    [-97., -66.],
];
const FARM_EAST: &[[f32; 2]] = &[
    [87., 13.4],
    [87., 30.],
    [96., 42.],
    [101., 57.],
    [101., 75.],
];
fn line(points: &[[f32; 2]], width: f32) -> Vec<Road> {
    points
        .windows(2)
        .map(|p| Road {
            a: p[0],
            b: p[1],
            width,
        })
        .collect()
}
pub fn chart_roads() -> &'static [Road] {
    static ROADS: std::sync::OnceLock<Vec<Road>> = std::sync::OnceLock::new();
    ROADS.get_or_init(|| {
        let mut roads = line(MAIN, 7.);
        roads.extend(line(LOOP, 4.5));
        roads.extend(line(&[[2., -18.], [2., -40.]], 4.));
        roads.extend(line(&[[-28., 8.3], [-34., 43.]], 4.));
        roads.extend(line(&[[13., 11.3], [11., 44.]], 4.));
        roads.extend(line(FARM_WEST, 3.2));
        roads.extend(line(FARM_EAST, 3.2));
        roads
    })
}
pub fn roads() -> Vec<Road> {
    chart_roads()
        .iter()
        .map(|r| Road {
            a: world(r.a),
            b: world(r.b),
            width: r.width,
        })
        .collect()
}
pub fn road_distance(p: [f32; 2]) -> f32 {
    chart_roads()
        .iter()
        .map(|r| r.distance(p))
        .fold(f32::INFINITY, f32::min)
}
pub fn main_road_distance(p: [f32; 2]) -> f32 {
    MAIN.windows(2)
        .map(|r| {
            Road {
                a: r[0],
                b: r[1],
                width: 0.,
            }
            .distance(p)
        })
        .chain([
            Road {
                a: MAIN[0],
                b: [-1024., -212.],
                width: 0.,
            }
            .distance(p),
            Road {
                a: MAIN[MAIN.len() - 1],
                b: [1024., -484.],
                width: 0.,
            }
            .distance(p),
        ])
        .fold(f32::INFINITY, f32::min)
}
/// Existing main-road alignment continued to both map boundaries.
pub fn main_path() -> Vec<[f32; 2]> {
    std::iter::once([-1024., -212.])
        .chain(MAIN.iter().copied())
        .chain(std::iter::once([1024., -484.]))
        .collect()
}
pub fn paved_paths() -> Vec<(Vec<[f32; 2]>, f32)> {
    vec![
        (main_path(), 7.),
        (LOOP.to_vec(), 4.5),
        (vec![[2., -18.], [2., -40.]], 4.),
        (vec![[-28., 8.3], [-34., 43.]], 4.),
        (vec![[13., 11.3], [11., 44.]], 4.),
    ]
}
pub fn farm_paths() -> Vec<(Vec<[f32; 2]>, f32)> {
    let mut paths = vec![(FARM_WEST.to_vec(), 3.2), (FARM_EAST.to_vec(), 3.2)];
    paths.extend(
        countryside()
            .tracks
            .iter()
            .map(|r| (vec![r.a, r.b], r.width)),
    );
    paths
}
pub fn surface_distance(p: [f32; 2]) -> f32 {
    (main_road_distance(p) - 3.5)
        .min(road_distance(p))
        .min(countryside_track_distance(p))
}
pub fn arterial(cols: i32, rows: i32) -> Vec<[f32; 2]> {
    if !crate::battlefield::expanded(cols, rows) {
        return main_path().into_iter().map(world).collect();
    }
    let reach = cols as f32 * 0.5 + crate::battlefield::SCENERY_MARGIN + 128.;
    let mut points = vec![
        [-reach, -220.],
        [-900., -220.],
        [-760., -180.],
        [-680., 65.],
        [-538., 65.],
        [-486., 65.],
        [-340., 90.],
    ];
    points.extend(MAIN.iter().copied().map(world));
    points.extend([
        [250., -78.],
        [340., -40.],
        [490., -38.],
        [549., -38.],
        [680., -12.],
        [760., 190.],
        [900., 220.],
        [reach, 220.],
    ]);
    points
}
fn connector_roads() -> [Road; 4] {
    [
        Road {
            a: [-270., -250.],
            b: [-270., -170.],
            width: 3.2,
        },
        Road {
            a: [-270., -170.],
            b: [-270., 91.],
            width: 3.2,
        },
        Road {
            a: [270., 250.],
            b: [300., 170.],
            width: 3.2,
        },
        Road {
            a: [300., 170.],
            b: [300., -57.],
            width: 3.2,
        },
    ]
}
pub fn paved_paths_for(cols: i32, rows: i32) -> Vec<(Vec<[f32; 2]>, f32)> {
    let mut paths = paved_paths();
    if crate::battlefield::expanded(cols, rows) {
        paths[0].0 = arterial(cols, rows).into_iter().map(local).collect();
    }
    paths
}
pub fn farm_paths_for(cols: i32, rows: i32) -> Vec<(Vec<[f32; 2]>, f32)> {
    if !crate::battlefield::expanded(cols, rows) {
        return farm_paths();
    }
    let mut paths = vec![(FARM_WEST.to_vec(), 3.2), (FARM_EAST.to_vec(), 3.2)];
    paths.extend(
        countryside_for(cols, rows)
            .tracks
            .iter()
            .map(|r| (vec![r.a, r.b], r.width)),
    );
    if crate::battlefield::expanded(cols, rows) {
        paths.extend(
            connector_roads()
                .iter()
                .map(|r| (vec![local(r.a), local(r.b)], r.width)),
        );
    }
    paths
}
pub fn surface_distance_for(p: [f32; 2], cols: i32, rows: i32) -> f32 {
    if !crate::battlefield::expanded(cols, rows) {
        return surface_distance(p);
    }
    let q = world(p);
    static PRIMARY: std::sync::OnceLock<Vec<[f32; 2]>> = std::sync::OnceLock::new();
    let primary = PRIMARY
        .get_or_init(|| arterial(crate::battlefield::COLS, crate::battlefield::ROWS))
        .windows(2)
        .map(|r| {
            Road {
                a: r[0],
                b: r[1],
                width: 7.,
            }
            .distance(q)
        })
        .fold(f32::INFINITY, f32::min);
    let country = countryside_for(cols, rows).track_distance(p);
    let farm = connector_roads()
        .iter()
        .map(|r| r.distance(q))
        .fold(f32::INFINITY, f32::min);
    primary.min(country).min(farm).min(road_distance(p))
}
pub fn site_plan(index: usize) -> crate::settlement_layout::TownPlan {
    use crate::settlement_layout::TownPlan;
    match index {
        0 => TownPlan::H,
        1 => TownPlan::L,
        _ => TownPlan::Square,
    }
}
pub fn site_lots(index: usize) -> Vec<Lot> {
    if index < 2 {
        return crate::settlement_layout::lots_for(
            "town",
            &format!("countryside-hamlet-{index}"),
            site_plan(index),
        );
    }
    [[-12_f32, -12.], [12., -12.], [12., 14.]]
        .into_iter()
        .enumerate()
        .map(|(i, at)| Lot {
            at,
            width: HOUSE_PLOT,
            depth: HOUSE_PLOT,
            height: HOUSE_HEIGHT,
            yaw: (-at[0]).atan2(-at[1]),
            style: [1, 0, 3][i] + ((index - 2) * 4) as u8,
            ground: 0.,
            integrity: 1.,
        })
        .collect()
}
pub fn plants_for(cols: i32, rows: i32) -> Vec<Plant> {
    if !crate::battlefield::expanded(cols, rows) {
        return plants();
    }
    let source = village_plants()
        .into_iter()
        .chain(countryside_plants_for(cols, rows));
    let sites = crate::battlefield::additional_sites(cols, rows);
    let center = [cols as f32 * 0.5, rows as f32 * 0.5];
    let valid = |at: [f32; 2], tree: bool| {
        at[0].abs() < cols as f32 * 0.5 - 6.
            && at[1].abs() < rows as f32 * 0.5 - 6.
            && surface_distance_for(local(at), cols, rows) > if tree { 4. } else { 1.3 }
            && sites.iter().all(|(site, farm)| {
                let h = if *farm { 39. } else { 56. };
                (at[0] + center[0] - site[0]).abs() > h || (at[1] + center[1] - site[1]).abs() > h
            })
    };
    let mut result = vec![];
    for plant in source {
        if valid(plant.at, plant.tree) {
            result.push(plant);
        }
    }
    for (index, (at, farm)) in sites.iter().enumerate() {
        let h = if *farm { 35_f32 } else { 52. };
        for side in 0..4 {
            let corners = [[-h, -h], [h, -h], [h, h], [-h, h]];
            let (a, b) = (corners[side], corners[(side + 1) % 4]);
            for step in 0..((h * 2. / 1.8) as usize) {
                let f = step as f32 / (h * 2. / 1.8);
                let p = [
                    at[0] - center[0] + a[0] + (b[0] - a[0]) * f,
                    at[1] - center[1] + a[1] + (b[1] - a[1]) * f,
                ];
                if surface_distance_for(local(p), cols, rows) > 2. {
                    result.push(Plant {
                        at: p,
                        tree: step % 17 == 6,
                        scale: 1.,
                    });
                }
            }
        }
        let lots = site_lots(index);
        for lot in &lots {
            let door = lot.door().1;
            let p = [
                at[0] - center[0] + lot.at[0] - door[0] * 8.,
                at[1] - center[1] + lot.at[1] - door[1] * 8.,
            ];
            let relative = [p[0] - (at[0] - center[0]), p[1] - (at[1] - center[1])];
            if lots.iter().all(|l| !l.contains(relative, 2.)) {
                result.push(Plant {
                    at: p,
                    tree: true,
                    scale: 1.3,
                });
            }
        }
    }
    result
}
/// Polygon coordinates and a bounded spatial index, shared with the pixel
/// shader. Keeping boundaries in world space removes terrain-grid stair steps.
pub fn field_paint(cols: i32, rows: i32) -> (Vec<Field>, Vec<Vec<usize>>) {
    let country = countryside_for(cols, rows);
    (
        FIELDS
            .iter()
            .copied()
            .chain(country.parcels.iter().map(|p| p.field))
            .collect(),
        country
            .bins
            .iter()
            .map(|bin| bin.iter().map(|i| i + FIELDS.len()).collect())
            .collect(),
    )
}
pub fn field_cell_size(cols: i32, rows: i32) -> f32 {
    countryside_for(cols, rows).cell
}
pub fn dirt_road(p: [f32; 2]) -> bool {
    chart_roads().iter().skip(15).any(|r| r.distance(p) < 0.9)
}
pub fn plazas() -> Vec<[f32; 2]> {
    // Retain three playable capture areas in the village and two farmyards.
    [[-98., -53.], [-8., -18.], [101., 63.]].map(world).to_vec()
}
pub fn lots() -> Vec<Lot> {
    // Twelve garden houses and two three-building farm groups, all existing
    // Square/L/H/straight house variants with their original dimensions.
    let sites = [
        ([-27., -29.], 1),
        ([-8., -29.], 0),
        ([12., -29.], 1),
        ([36., -16.], 0),
        ([-41., -3.], 1),
        ([-5., -3.], 2),
        ([36., 1.], 1),
        ([54., 3.], 0),
        ([-46., 26.], 1),
        ([-20., 28.], 0),
        ([1., 30.], 2),
        ([25., 34.], 0),
        ([-110., -53.], 1),
        ([-86., -65.], 0),
        ([-85., -43.], 3),
        ([88., 59.], 2),
        ([114., 52.], 0),
        ([116., 75.], 3),
    ];
    let roads = chart_roads();
    sites
        .into_iter()
        .enumerate()
        .map(|(i, (at, style))| {
            let closest = roads
                .iter()
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
            Lot {
                at: world(at),
                width: HOUSE_PLOT,
                depth: HOUSE_PLOT,
                height: HOUSE_HEIGHT,
                yaw: (closest[0] - at[0]).atan2(closest[1] - at[1]) + TURN,
                style: style + (i as u8 % 4) * 4,
                ground: 0.,
                integrity: 1.,
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
pub struct Field {
    pub corners: [[f32; 2]; 4],
    pub crop: bool,
    pub harvested: bool,
}
pub const FIELDS: &[Field] = &[
    Field {
        corners: [[-140., -83.], [-49., -83.], [-50., -41.], [-126., -28.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[-46., -88.], [45., -88.], [42., -46.], [-44., -51.]],
        crop: true,
        harvested: false,
    },
    Field {
        corners: [[49., -88.], [142., -88.], [132., -55.], [48., -42.]],
        crop: true,
        harvested: true,
    },
    Field {
        corners: [[-81., -33.], [-48., -40.], [-47., -1.], [-79., -6.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[50., -37.], [130., -51.], [141., -19.], [64., 6.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[-140., 0.], [-62., 16.], [-99., 42.], [-142., 55.]],
        crop: true,
        harvested: true,
    },
    Field {
        corners: [[-94., 46.], [-58., 20.], [-46., 88.], [-110., 88.]],
        crop: true,
        harvested: false,
    },
    Field {
        corners: [[-29., 47.], [23., 51.], [26., 88.], [-36., 88.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[32., 46.], [74., 29.], [81., 61.], [36., 72.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[35., 78.], [80., 68.], [134., 88.], [36., 90.]],
        crop: false,
        harvested: false,
    },
    Field {
        corners: [[95., 25.], [142., 2.], [142., 73.], [126., 81.]],
        crop: false,
        harvested: false,
    },
];
impl Field {
    pub fn inset(self, p: [f32; 2]) -> f32 {
        (0..4)
            .map(|i| {
                let (a, b) = (self.corners[i], self.corners[(i + 1) % 4]);
                let d = [b[0] - a[0], b[1] - a[1]];
                (d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / d[0].hypot(d[1])
            })
            .fold(f32::INFINITY, f32::min)
    }
}
pub fn field_at(p: [f32; 2]) -> Option<Field> {
    field_at_for(p, 1024, 1024)
}
pub fn field_at_for(p: [f32; 2], cols: i32, rows: i32) -> Option<Field> {
    if coverage(p) > 0. {
        if let Some(field) = FIELDS.iter().copied().find(|f| f.inset(p) > 0.) {
            return Some(field);
        }
    }
    countryside_for(cols, rows)
        .parcel(p)
        .map(|parcel| parcel.field)
}
/// Ground tint and dirt coverage reuse the supplied meadow and soil scans.
pub fn ground_for(p: [f32; 2], cols: i32, rows: i32) -> ([f32; 3], f32) {
    let field = if crate::battlefield::expanded(cols, rows) {
        field_at_for(p, cols, rows)
    } else {
        field_at(p)
    };
    let Some(field) = field else {
        return ([0.93, 1.04, 0.84], 0.035);
    };
    let edge = (field.inset(p) / 2.5).clamp(0., 1.);
    let rows = (p[0] * 2.8 + p[1] * 0.17).sin().abs().powi(12);
    let (tint, dirt) = if field.harvested {
        ([1.12, 0.94, 0.66], 0.80 + rows * 0.13)
    } else if field.crop {
        (
            [0.53 + rows * 0.07, 0.87 + rows * 0.05, 0.45],
            0.025 + rows * 0.10,
        )
    } else {
        ([0.91, 1.04, 0.72], 0.07)
    };
    (tint.map(|v| 1. + (v - 1.) * edge), dirt * edge)
}

pub struct Plant {
    pub at: [f32; 2],
    pub tree: bool,
    pub scale: f32,
}
/// Continuous hedges, garden boundaries and woodland fringes use the existing
/// foliage meshes at all detail levels. Gaps remain at every road and field gate.
pub fn plants() -> Vec<Plant> {
    let mut result = village_plants();
    result.extend(countryside_plants());
    result
}
fn village_plants() -> Vec<Plant> {
    let mut result = vec![];
    let roads = chart_roads();
    let houses = lots();
    let mut add = |p: [f32; 2], tree: bool, scale: f32| {
        let at = world(p);
        if roads
            .iter()
            .any(|r| r.distance(p) < if tree { 4. } else { 1.4 })
            || houses
                .iter()
                .any(|l| l.contains(at, if tree { 3. } else { 1.5 }))
            || plazas()
                .iter()
                .any(|q| (q[0] - at[0]).hypot(q[1] - at[1]) < 4.8)
        {
            return;
        }
        result.push(Plant { at, tree, scale });
    };
    for (f, field) in FIELDS.iter().enumerate() {
        for side in 0..4 {
            let (a, b) = (field.corners[side], field.corners[(side + 1) % 4]);
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            let count = (length / 1.8).ceil() as usize;
            for i in 0..count {
                // A six-metre farm gate on one edge of each field.
                let t = (i as f32 + 0.5) / count as f32;
                if side == f % 4 && (t - 0.60).abs() * length < 3. {
                    continue;
                }
                let n = crate::natural_noise::value(i as f32 * 0.71, (f * 7 + side) as f32, 347);
                let p = [
                    a[0] + (b[0] - a[0]) * t + (n - 0.5) * 0.8,
                    a[1] + (b[1] - a[1]) * t + (n - 0.5) * 0.7,
                ];
                add(p, false, 0.87 + n * 0.38);
                if i % 9 == 4 {
                    add(p, true, 1.05 + n * 0.48);
                }
            }
        }
    }
    // Garden backs and side boundaries, with wide open fronts facing the lanes.
    for (a, b) in [
        ([-50_f32, -39.], [43., -39.]),
        ([-51., -38.], [-51., 0.]),
        ([44., -37.], [44., -10.]),
        ([-52., 37.], [-40., 40.]),
        ([-26., 43.], [5., 46.]),
        ([17., 47.], [37., 47.]),
        ([-120., -74.], [-76., -74.]),
        ([-120., -73.], [-120., -35.]),
        ([-119., -33.], [-78., -33.]),
        ([80., 46.], [80., 78.]),
        ([81., 82.], [127., 82.]),
    ] {
        let count = ((b[0] - a[0]).hypot(b[1] - a[1]) / 1.7).ceil() as usize;
        for i in 0..count {
            let t = i as f32 / count as f32;
            add(
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                i % 13 == 7,
                0.95,
            );
        }
    }
    for lot in houses.iter().take(12) {
        let p = local(lot.at);
        add([p[0] - 4., p[1] - 8.], true, 1.18);
    }
    for z in (-90..=90).step_by(5) {
        for x in (-144..=144).step_by(5) {
            let n = crate::natural_noise::value(x as f32 * 0.51, z as f32 * 0.51, 971);
            let p = [x as f32 + (n - 0.5) * 3., z as f32 + (n - 0.5) * 2.];
            let woods = (p[0] < -115. && p[1] > 42.)
                || (p[0] < -126. && p[1] < -42.)
                || (p[0] > 127. && (p[1] < -50. || p[1] > 55.));
            if woods && n > 0.15 {
                add(p, true, 1.45 + n * 0.55);
            }
        }
    }
    result
}

struct Parcel {
    field: Field,
    woodland: bool,
}
struct Countryside {
    parcels: Vec<Parcel>,
    /// Shared chart-space bins keep terrain samples independent of plot count.
    cell: f32,
    bins: Vec<Vec<usize>>,
    edges: Vec<([f32; 2], [f32; 2])>,
    tracks: Vec<Road>,
    track_bins: Vec<Vec<usize>>,
}
const COUNTRY_GRID: usize = 16;
impl Countryside {
    fn index_bounds(bins: &mut [Vec<usize>], cell: f32, lo: [f32; 2], hi: [f32; 2], index: usize) {
        let origin = -(COUNTRY_GRID as f32 * 0.5) * cell;
        for bz in 0..COUNTRY_GRID {
            for bx in 0..COUNTRY_GRID {
                let min = [origin + bx as f32 * cell, origin + bz as f32 * cell];
                if hi[0] >= min[0]
                    && lo[0] <= min[0] + cell
                    && hi[1] >= min[1]
                    && lo[1] <= min[1] + cell
                {
                    bins[bz * COUNTRY_GRID + bx].push(index);
                }
            }
        }
    }
    fn bin(&self, p: [f32; 2]) -> Option<usize> {
        let x = (p[0] / self.cell + COUNTRY_GRID as f32 * 0.5).floor() as i32;
        let z = (p[1] / self.cell + COUNTRY_GRID as f32 * 0.5).floor() as i32;
        (x >= 0 && z >= 0 && x < COUNTRY_GRID as i32 && z < COUNTRY_GRID as i32)
            .then_some((z.max(0) as usize * COUNTRY_GRID) + x.max(0) as usize)
    }
    fn parcel(&self, p: [f32; 2]) -> Option<&Parcel> {
        self.bins[self.bin(p)?]
            .iter()
            .map(|i| &self.parcels[*i])
            .find(|parcel| parcel.field.inset(p) >= 0.)
    }
    fn track_distance(&self, p: [f32; 2]) -> f32 {
        let Some(bin) = self.bin(p) else {
            return f32::INFINITY;
        };
        self.track_bins[bin]
            .iter()
            .map(|i| self.tracks[*i].distance(p))
            .fold(f32::INFINITY, f32::min)
    }
}
fn countryside() -> &'static Countryside {
    countryside_for(1024, 1024)
}
fn countryside_for(cols: i32, rows: i32) -> &'static Countryside {
    static COUNTRY: std::sync::OnceLock<Countryside> = std::sync::OnceLock::new();
    static WIDE_COUNTRY: std::sync::OnceLock<Countryside> = std::sync::OnceLock::new();
    if crate::battlefield::expanded(cols, rows) {
        WIDE_COUNTRY.get_or_init(|| build_countryside(true))
    } else {
        COUNTRY.get_or_init(|| build_countryside(false))
    }
}
fn build_countryside(wide: bool) -> Countryside {
    // Add room outside the village in its own chart axes. Stretching world X
    // used to shear these boundaries away from the unchanged village hedges.
    let xs = if wide {
        [
            -1200., -940., -690., -435., -144., 144., 435., 690., 940., 1200.,
        ]
    } else {
        [
            -760., -600., -440., -290., -144., 144., 285., 425., 580., 760.,
        ]
    };
    let zs = if wide {
        [
            -1200., -990., -780., -555., -310., -92., 92., 310., 540., 760., 980., 1200.,
        ]
    } else {
        [
            -760., -600., -445., -305., -190., -92., 92., 200., 330., 475., 620., 760.,
        ]
    };
    let node = |x: usize, z: usize| {
        // Shared displaced corners form adjoining irregular quadrilaterals.
        // Continue the village's four boundary lines through neighbouring
        // fields. Jitter elsewhere retains the existing irregular farmland.
        let pinned = (x == 4 || x == 5) && (z == 5 || z == 6);
        let jitter = if pinned { 0. } else { 1. };
        [
            xs[x]
                + (crate::natural_noise::value(x as f32 * 2.3, z as f32 * 1.7, 812) - 0.5)
                    * 32.
                    * if wide && (x == 4 || x == 5) {
                        0.
                    } else {
                        jitter
                    },
            zs[z]
                + (crate::natural_noise::value(x as f32 * 1.9, z as f32 * 2.1, 139) - 0.5)
                    * 26.
                    * if wide && (z == 5 || z == 6) {
                        0.
                    } else {
                        jitter
                    },
        ]
    };
    let mut result = Countryside {
        parcels: vec![],
        cell: if wide { 192. } else { 96. },
        bins: vec![vec![]; COUNTRY_GRID * COUNTRY_GRID],
        edges: vec![],
        tracks: vec![],
        track_bins: vec![vec![]; COUNTRY_GRID * COUNTRY_GRID],
    };
    let mut edges = std::collections::BTreeSet::new();
    for z in 0..zs.len() - 1 {
        for x in 0..xs.len() - 1 {
            if x == 4 && z == 5 {
                continue;
            }
            let corners = [
                node(x, z),
                node(x + 1, z),
                node(x + 1, z + 1),
                node(x, z + 1),
            ];
            let center = [
                corners.iter().map(|p| p[0]).sum::<f32>() * 0.25,
                corners.iter().map(|p| p[1]).sum::<f32>() * 0.25,
            ];
            let at = world(center);
            let seed = x * 19 + z * 37;
            let woodland =
                seed % 17 == 3 || (at[0].abs() > 320. && at[1].abs() > 320. && seed % 3 == 1);
            let index = result.parcels.len();
            result.parcels.push(Parcel {
                field: Field {
                    corners,
                    crop: !woodland && seed % 5 < 3,
                    harvested: !woodland && seed % 5 == 0,
                },
                woodland,
            });
            let lo = [
                corners.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
                corners.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
            ];
            let hi = [
                corners
                    .iter()
                    .map(|p| p[0])
                    .fold(f32::NEG_INFINITY, f32::max),
                corners
                    .iter()
                    .map(|p| p[1])
                    .fold(f32::NEG_INFINITY, f32::max),
            ];
            Countryside::index_bounds(&mut result.bins, result.cell, lo, hi, index);
            for (a, b) in [
                ((x, z), (x + 1, z)),
                ((x + 1, z), (x + 1, z + 1)),
                ((x + 1, z + 1), (x, z + 1)),
                ((x, z + 1), (x, z)),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                if edges.insert(edge) {
                    result.edges.push((node(a.0, a.1), node(b.0, b.1)));
                }
            }
        }
    }
    // Through lanes meet the main road on both sides of the village. Cross
    // tracks follow field boundaries and lead toward both military bases.
    for x in [3, 6] {
        for z in 0..zs.len() - 1 {
            result.tracks.push(Road {
                a: node(x, z),
                b: node(x, z + 1),
                width: 3.4,
            });
        }
    }
    for z in [2, 4, 7, 9] {
        for x in 0..xs.len() - 1 {
            result.tracks.push(Road {
                a: node(x, z),
                b: node(x + 1, z),
                width: 3.0,
            });
        }
    }
    for (i, road) in result.tracks.iter().enumerate() {
        Countryside::index_bounds(
            &mut result.track_bins,
            result.cell,
            [road.a[0].min(road.b[0]) - 8., road.a[1].min(road.b[1]) - 8.],
            [road.a[0].max(road.b[0]) + 8., road.a[1].max(road.b[1]) + 8.],
            i,
        );
    }
    result
}
pub fn countryside_track_distance(p: [f32; 2]) -> f32 {
    countryside().track_distance(p)
}
fn country_plant_site(p: [f32; 2], tree: bool) -> bool {
    let at = world(p);
    // The approved settlement and both airfield/deployment corridors stay open.
    at[0].abs() < 506.
        && at[1].abs() < 506.
        && !(at[0].abs() > 374. && at[1].abs() < 162.)
        && coverage(p) <= 0.
        && main_road_distance(p) > if tree { 7. } else { 5. }
        && countryside_track_distance(p) > if tree { 3. } else { 1.1 }
}
fn countryside_plants() -> Vec<Plant> {
    countryside_plants_for(1024, 1024)
}
fn countryside_plants_for(cols: i32, rows: i32) -> Vec<Plant> {
    let country = countryside_for(cols, rows);
    let mut result = vec![];
    let mut add = |p: [f32; 2], tree: bool, scale: f32| {
        let at = world(p);
        let valid = if crate::battlefield::expanded(cols, rows) {
            at[0].abs() < cols as f32 * 0.5 - 6.
                && at[1].abs() < rows as f32 * 0.5 - 6.
                && !(at[0].abs() > cols as f32 * 0.5 - 138. && at[1].abs() < 162.)
                && coverage(p) <= 0.
                && surface_distance_for(p, cols, rows) > if tree { 4. } else { 1.3 }
        } else {
            country_plant_site(p, tree)
        };
        if valid {
            result.push(Plant { at, tree, scale });
        }
    };
    for (edge, (a, b)) in country.edges.iter().enumerate() {
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0].hypot(d[1]);
        let count = (length / 2.1).ceil() as usize;
        let middle = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let along_track = country.track_distance(middle) < 0.;
        for i in 0..count {
            let t = (i as f32 + 0.5) / count as f32;
            // Wide gates connect neighbouring fields, including across tracks.
            if (t - 0.59).abs() * length < 4. {
                continue;
            }
            let n = crate::natural_noise::value(i as f32 * 0.73, edge as f32 * 1.31, 243);
            for side in 0..if along_track { 2 } else { 1 } {
                let offset = if along_track {
                    if side == 0 { -3.7 } else { 3.7 }
                } else {
                    0.
                };
                let p = [
                    a[0] + d[0] * t - d[1] / length * offset + (n - 0.5) * 0.6,
                    a[1] + d[1] * t + d[0] / length * offset + (n - 0.5) * 0.6,
                ];
                add(p, false, 0.88 + n * 0.35);
                if i % 15 == 7 {
                    add(p, true, 1.1 + n * 0.55);
                }
            }
        }
    }
    for parcel in country.parcels.iter().filter(|p| p.woodland) {
        let center = [
            parcel.field.corners.iter().map(|p| p[0]).sum::<f32>() * 0.25,
            parcel.field.corners.iter().map(|p| p[1]).sum::<f32>() * 0.25,
        ];
        for z in -13..=13 {
            for x in -13..=13 {
                let n = crate::natural_noise::value(
                    (center[0] + x as f32 * 7.) * 0.41,
                    (center[1] + z as f32 * 7.) * 0.41,
                    971,
                );
                let p = [
                    center[0] + x as f32 * 7. + (n - 0.5) * 4.,
                    center[1] + z as f32 * 7. + (n - 0.5) * 3.,
                ];
                if n > 0.18 && parcel.field.inset(p) > 3. {
                    add(p, true, 1.35 + n * 0.65);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_fields_continue_parallel_to_the_village_boundaries() {
        let (cols, rows) = (crate::battlefield::COLS, crate::battlefield::ROWS);
        let country = countryside_for(cols, rows);
        for corner in [[-144., -92.], [144., -92.], [144., 92.], [-144., 92.]] {
            let adjoining: Vec<_> = country
                .edges
                .iter()
                .filter_map(|(a, b)| {
                    if *a == corner {
                        Some(*b)
                    } else if *b == corner {
                        Some(*a)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(adjoining.len(), 4, "shared field junction at {corner:?}");
            for end in adjoining {
                assert!(
                    (end[0] - corner[0]).abs() < 0.001 || (end[1] - corner[1]).abs() < 0.001,
                    "outer boundary diverges from village edge: {corner:?} to {end:?}"
                );
            }
        }
        let plants = plants_for(cols, rows);
        for p in [[-300., -92.], [300., -92.], [-300., 92.], [300., 92.]] {
            let at = world(p);
            let nearest = plants
                .iter()
                .filter(|p| !p.tree)
                .map(|p| (p.at[0] - at[0]).hypot(p.at[1] - at[1]))
                .fold(f32::INFINITY, f32::min);
            assert!(
                nearest < 2.4,
                "hedges must follow the painted boundary at {p:?}: {nearest}"
            );
        }
        let (fields, bins) = field_paint(cols, rows);
        assert!(fields.len() <= 112 && bins.iter().all(|b| b.len() <= 8));
        for z in (14..rows - 14).step_by(29) {
            for x in (14..cols - 14).step_by(29) {
                let p = map_point(x as f32, z as f32, cols, rows);
                if coverage(p) > 0. {
                    continue;
                }
                let field =
                    field_at_for(p, cols, rows).expect("expanded farmland has no field gaps");
                assert!(field.inset(p) >= 0.);
            }
        }
    }
    #[test]
    fn cultivated_fields_do_not_get_notched_by_the_town_grade_feather() {
        let p = [-150., -100.];
        assert!(coverage(p) > 0.);
        assert!(countryside().parcel(p).unwrap().field.harvested);
        assert!(
            field_at(p).unwrap().harvested,
            "grading the village must not cut a rectangle out of the adjoining field"
        );
    }
    #[test]
    fn countryside_covers_the_operation_and_keeps_track_and_base_corridors_open() {
        let country = countryside();
        let mut styles = [0; 3];
        let mut quarters = [0; 4];
        for parcel in &country.parcels {
            let p = [
                parcel.field.corners.iter().map(|p| p[0]).sum::<f32>() * 0.25,
                parcel.field.corners.iter().map(|p| p[1]).sum::<f32>() * 0.25,
            ];
            assert!(
                parcel.field.inset(p) > 0.,
                "outer fields must remain convex"
            );
        }
        for z in (14..1010).step_by(23) {
            for x in (14..1010).step_by(23) {
                let p = map_point(x as f32, z as f32, 1024, 1024);
                if coverage(p) > 0. {
                    continue;
                }
                let field = field_at(p).expect("outer terrain must belong to a planted parcel");
                styles[if field.harvested {
                    0
                } else if field.crop {
                    1
                } else {
                    2
                }] += 1;
            }
        }
        assert!(
            styles.iter().all(|n| *n > 150),
            "crop, stubble and pasture across the map: {styles:?}"
        );
        let plants = countryside_plants();
        assert!(
            plants.len() > 3500 && plants.len() < 24000,
            "bounded authored vegetation: {}",
            plants.len()
        );
        for plant in plants {
            let p = local(plant.at);
            assert!(country_plant_site(p, plant.tree));
            quarters[usize::from(plant.at[0] > 0.) + usize::from(plant.at[1] > 0.) * 2] += 1;
        }
        assert!(
            quarters.iter().all(|n| *n > 500),
            "hedgerows reach every quarter: {quarters:?}"
        );
        for side in [-290_f32, 285.] {
            let distance = country
                .tracks
                .iter()
                .filter(|r| (r.a[0] - side).abs() < 20. && (r.b[0] - side).abs() < 20.)
                .flat_map(|r| {
                    (0..=100).map(move |step| {
                        let t = step as f32 / 100.;
                        main_road_distance([
                            r.a[0] + (r.b[0] - r.a[0]) * t,
                            r.a[1] + (r.b[1] - r.a[1]) * t,
                        ])
                    })
                })
                .fold(f32::INFINITY, f32::min);
            assert!(
                distance < 2.,
                "both countryside lanes must meet the main road"
            );
        }
    }
    #[test]
    fn rural_streets_and_capture_yards_stay_clear_of_existing_house_footprints() {
        let houses = lots();
        assert_eq!(houses.len(), 18);
        for road in roads() {
            for step in 0..=100 {
                let t = step as f32 / 100.;
                let at = [
                    road.a[0] + (road.b[0] - road.a[0]) * t,
                    road.a[1] + (road.b[1] - road.a[1]) * t,
                ];
                assert!(
                    houses
                        .iter()
                        .all(|l| !l.contains(at, road.width * 0.5 + 0.6)),
                    "house beside road {at:?}"
                );
            }
        }
        for at in plazas() {
            assert!(houses.iter().all(|l| !l.contains(at, 4.)));
        }
        for field in FIELDS {
            let center = [
                field.corners.iter().map(|p| p[0]).sum::<f32>() * 0.25,
                field.corners.iter().map(|p| p[1]).sum::<f32>() * 0.25,
            ];
            assert!(field.inset(center) > 0., "field winding must be consistent");
        }
    }
}
