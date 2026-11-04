// Included in simulation.rs. Physical sight is independent of firing range and render LOD.
use bevy::prelude::{Vec2, Vec3};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct SpottingCache(Mutex<Option<(String, Arc<SightVegetation>)>>);
impl Clone for SpottingCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}
#[derive(Default)]
struct SightVegetation {
    cells: BTreeMap<(i32, i32), Vec<SightPlant>>,
}
#[derive(Clone, Copy)]
struct SightPlant {
    at: Point,
    radius: f32,
    bottom: f32,
    top: f32,
    density: f32,
}
impl SightVegetation {
    fn new(terrain: &crate::terrain::Terrain) -> Self {
        let mut result = Self::default();
        for p in crate::scenery::spotting_placements(terrain) {
            let plant = if p.tree {
                SightPlant {
                    at: [p.position.x, p.position.z],
                    radius: 1.7 * p.scale,
                    bottom: 0.6 * p.scale,
                    top: 6. * p.scale,
                    density: 0.32,
                }
            } else {
                SightPlant {
                    at: [p.position.x, p.position.z],
                    radius: if p.understory == 3 { 1.05 } else { 1.25 } * p.scale,
                    bottom: 0.,
                    top: if p.understory == 3 { 1.8 } else { 1.15 } * p.scale,
                    density: if p.understory == 3 { 1.1 } else { 0.8 },
                }
            };
            result
                .cells
                .entry((
                    (plant.at[0] / 8.).floor() as i32,
                    (plant.at[1] / 8.).floor() as i32,
                ))
                .or_default()
                .push(plant);
        }
        result
    }
    fn density(&self, game: &NativeGame, clearings: &[crate::scenery::Clearing], at: Vec3) -> f32 {
        let cell = ((at.x / 8.).floor() as i32, (at.z / 8.).floor() as i32);
        let mut density: f32 = 0.;
        for z in cell.1 - 1..=cell.1 + 1 {
            for x in cell.0 - 1..=cell.0 + 1 {
                if let Some(plants) = self.cells.get(&(x, z)) {
                    for plant in plants {
                        if (plant.at[0] - at.x).powi(2) + (plant.at[1] - at.z).powi(2)
                            > plant.radius.powi(2)
                        {
                            continue;
                        }
                        let ground = game.terrain.height(plant.at[0], plant.at[1]);
                        if at.y < ground + plant.bottom
                            || at.y > ground + plant.top
                            || game.terrain.cell(plant.at[0], plant.at[1]).water()
                        {
                            continue;
                        }
                        if clearings
                            .iter()
                            .any(|c| c.contains(Vec2::from_array(plant.at)))
                        {
                            continue;
                        }
                        density = density.max(plant.density);
                    }
                }
            }
        }
        density
    }
}

struct SightSolid {
    at: Point,
    half: Point,
    yaw: f32,
    bottom: f32,
    top: f32,
}
impl SightSolid {
    fn crosses(&self, from: Vec3, to: Vec3) -> bool {
        let (s, c) = self.yaw.sin_cos();
        let local = |p: Vec3| {
            Vec3::new(
                c * (p.x - self.at[0]) - s * (p.z - self.at[1]),
                p.y,
                s * (p.x - self.at[0]) + c * (p.z - self.at[1]),
            )
        };
        let a = local(from);
        let b = local(to);
        let delta = b - a;
        let lo = Vec3::new(-self.half[0], self.bottom, -self.half[1]);
        let hi = Vec3::new(self.half[0], self.top, self.half[1]);
        let (mut enter, mut exit) = (0.001_f32, 0.999_f32);
        for axis in 0..3 {
            if delta[axis].abs() < 0.000001 {
                if a[axis] <= lo[axis] || a[axis] >= hi[axis] {
                    return false;
                }
            } else {
                let t0 = (lo[axis] - a[axis]) / delta[axis];
                let t1 = (hi[axis] - a[axis]) / delta[axis];
                enter = enter.max(t0.min(t1));
                exit = exit.min(t0.max(t1));
                if enter > exit {
                    return false;
                }
            }
        }
        enter < exit
    }
}
struct SpottingWorld {
    vegetation: Arc<SightVegetation>,
    clearings: Vec<crate::scenery::Clearing>,
    solids: Vec<SightSolid>,
}
impl SpottingWorld {
    fn new(game: &NativeGame) -> Self {
        let key = crate::terrain::render_source_key(&game.terrain);
        let vegetation = {
            let mut cache = game.spotting.0.lock().unwrap_or_else(|e| e.into_inner());
            if cache.as_ref().is_none_or(|(old, _)| *old != key) {
                *cache = Some((key, Arc::new(SightVegetation::new(&game.terrain))));
            }
            cache.as_ref().unwrap().1.clone()
        };
        let mut solids = vec![];
        for building in game.entities.iter().filter(|e| {
            e.view.building
                && e.view.hp > 0.
                && !["objective", "bridge"].contains(&e.view.kind.as_str())
        }) {
            let scale = building.view.model_scale.max(0.1);
            if !building.view.lots.is_empty() {
                for lot in building.view.lots.iter().filter(|l| l.integrity > 0.) {
                    let (s, c) = lot.yaw.sin_cos();
                    for wing in lot.wings() {
                        let center = [
                            lot.at[0] + c * wing.at[0] + s * wing.at[1],
                            lot.at[1] - s * wing.at[0] + c * wing.at[1],
                        ];
                        solids.push(SightSolid {
                            at: building_world(building, center),
                            half: wing.size.map(|v| v * scale * 0.5),
                            yaw: building.view.heading + lot.yaw,
                            bottom: building.view.elevation + lot.ground * scale,
                            top: building.view.elevation
                                + lot.ground * scale
                                + (crate::settlement_layout::HOUSE_HEIGHT
                                    + (wing.size[0].min(wing.size[1]) * 0.38).clamp(0.8, 1.75))
                                    * scale,
                        });
                    }
                }
            } else if building.view.kind == "airfield" {
                for (index, (at, half)) in crate::airfield_layout::SOLIDS.into_iter().enumerate() {
                    solids.push(SightSolid {
                        at: building_world(building, at),
                        half: half.map(|v| v * scale),
                        yaw: building.view.heading,
                        bottom: building.view.elevation,
                        top: building.view.elevation + if index == 0 { 6.8 } else { 5.5 } * scale,
                    });
                }
            } else {
                let height = if building.view.kind == "watchtower" {
                    6.8
                } else {
                    4.8
                };
                solids.push(SightSolid {
                    at: position(building),
                    half: [
                        number(spec(&building.view.kind), "footprintWidth", 3.6) * scale * 0.5,
                        number(spec(&building.view.kind), "footprintDepth", 3.6) * scale * 0.5,
                    ],
                    yaw: building.view.heading,
                    bottom: building.view.elevation,
                    top: building.view.elevation + height * scale,
                });
            }
        }
        Self {
            vegetation,
            clearings: crate::scenery::spotting_clearings(game),
            solids,
        }
    }
    fn clear(&self, game: &NativeGame, from: Vec3, to: Vec3, concealment_limit: f32) -> bool {
        if self.solids.iter().any(|s| s.crosses(from, to)) || !game.terrain_sight_clear(from, to) {
            return false;
        }
        let distance = Vec2::new(to.x - from.x, to.z - from.z).length();
        let steps = (distance / 1.).ceil().max(1.) as usize;
        let stride = distance / steps as f32;
        let mut depth = 0.;
        for i in 0..steps {
            let at = from.lerp(to, (i as f32 + 0.5) / steps as f32);
            depth += self.vegetation.density(game, &self.clearings, at) * stride;
            if depth > concealment_limit {
                return false;
            }
        }
        true
    }
}
#[derive(Clone, Copy)]
struct SightSource {
    at: Point,
    height: f32,
    squared: f32,
    infantry: bool,
    facing: Option<Point>,
}
impl SightSource {
    fn new(game: &NativeGame, e: &Entity) -> Self {
        let slot = e
            .view
            .garrison
            .as_ref()
            .filter(|g| g.entered)
            .and_then(|g| game.garrison_slot(g));
        Self {
            at: slot.map_or(position(e), |s| s.0),
            height: slot.map_or_else(|| game.sight_height(e, true), |s| s.1),
            squared: game.unit_sight(e).powi(2),
            infantry: e.view.category == "troop",
            facing: slot.map(|s| s.2),
        }
    }
    fn covers(&self, point: Point) -> bool {
        let dx = point[0] - self.at[0];
        let dz = point[1] - self.at[1];
        dx * dx + dz * dz <= self.squared && self.facing.is_none_or(|n| dx * n[0] + dz * n[1] >= 0.)
    }
    fn point(&self) -> Vec3 {
        Vec3::new(self.at[0], self.height, self.at[1])
    }
}
impl NativeGame {
    fn sight_height(&self, e: &Entity, observer: bool) -> f32 {
        if let Some(slot) = e
            .view
            .garrison
            .as_ref()
            .filter(|g| g.entered)
            .and_then(|g| self.garrison_slot(g))
        {
            return slot.1;
        }
        let ground = self.terrain.height(e.view.x, e.view.z);
        if e.view.building {
            return e.view.elevation
                + if e.view.kind == "watchtower" { 7. } else { 5. } * e.view.model_scale;
        }
        if e.view.airborne {
            return ground
                + e.view
                    .altitude
                    .unwrap_or((e.aircraft.world_y - ground).max(1.))
                    .max(1.);
        }
        ground
            + if e.view.category == "troop" {
                (if observer { 1.55 } else { 1.1 }) * e.view.model_scale
            } else {
                1.15 * e.view.model_scale.max(1.)
            }
    }
    fn terrain_sight_clear(&self, from: Vec3, to: Vec3) -> bool {
        let length = Vec2::new(to.x - from.x, to.z - from.z).length();
        let steps = (length / 2.).ceil().max(1.) as usize;
        for i in 1..steps {
            let at = from.lerp(to, i as f32 / steps as f32);
            if self.terrain.height(at.x, at.z) + 0.12 > at.y {
                return false;
            }
        }
        true
    }
}
