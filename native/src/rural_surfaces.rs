//! Continuous versions of the existing asphalt and gravel road surfaces.
//! Road edges follow their authored paths, independently of the terrain grid.
use crate::{models::Finish, runtime::Piece, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::collections::HashMap;

fn rounded(points: &[[f32; 2]], radius: f32) -> Vec<Vec2> {
    let points: Vec<_> = points.iter().copied().map(Vec2::from_array).collect();
    let mut curve = vec![points[0]];
    for i in 1..points.len() - 1 {
        let p = points[i];
        let before = p - points[i - 1];
        let after = points[i + 1] - p;
        let r = radius.min(before.length() * 0.3).min(after.length() * 0.3);
        let a = p - before.normalize_or_zero() * r;
        let b = p + after.normalize_or_zero() * r;
        curve.push(a);
        for step in 1..=8 {
            let t = step as f32 / 8.;
            curve.push(a * (1. - t).powi(2) + p * (2. * t * (1. - t)) + b * t * t);
        }
    }
    curve.push(*points.last().unwrap());
    let mut result = vec![curve[0]];
    for segment in curve.windows(2) {
        let count = (segment[0].distance(segment[1]) / 1.25).ceil().max(1.) as usize;
        for i in 1..=count {
            result.push(segment[0].lerp(segment[1], i as f32 / count as f32));
        }
    }
    result
}
#[derive(Default)]
struct Surface {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
    indices: Vec<u32>,
    heights: HashMap<(i32, i32), f32>,
}
impl Surface {
    fn height(&mut self, t: &Terrain, p: Vec2) -> f32 {
        if p.x < 0. || p.y < 0. || p.x > (t.cols - 1) as f32 || p.y > (t.rows - 1) as f32 {
            return crate::battlefield::scenic_height(t, p.x, p.y);
        }
        let step = if t.cols.max(t.rows) > 1536 { 8 } else { 4 };
        let x = (p.x / step as f32).floor() as i32 * step;
        let z = (p.y / step as f32).floor() as i32 * step;
        let x1 = (x + step).min(t.cols - 1);
        let z1 = (z + step).min(t.rows - 1);
        let mut sample = |x, z| {
            *self
                .heights
                .entry((x, z))
                .or_insert_with(|| crate::landscape::visual_height(t, x as f32, z as f32))
        };
        let (a, b, c, d) = (sample(x, z), sample(x1, z), sample(x, z1), sample(x1, z1));
        let u = (p.x - x as f32) / (x1 - x).max(1) as f32;
        let v = (p.y - z as f32) / (z1 - z).max(1) as f32;
        let coarse = if u + v <= 1. {
            a + (b - a) * u + (c - a) * v
        } else {
            d + (c - d) * (1. - u) + (b - d) * (1. - v)
        };
        // Clear either terrain LOD, including the feather at detail-tile edges.
        coarse.max(crate::landscape::visual_height(t, p.x, p.y))
    }
    fn polygon(&mut self, mut polygon: Vec<Vec2>, p: &Piece, t: &Terrain, asphalt: bool) {
        // Clip the surface to the actual map; no floating strip beyond its edge.
        let margin = if t.id == crate::battlefield::ID || t.id == crate::gis_prototype::ID {
            crate::battlefield::SCENERY_MARGIN
        } else {
            0.
        };
        for (axis, edge, sign) in [
            (0, -margin, 1.),
            (0, (t.cols - 1) as f32 + margin, -1.),
            (1, -margin, 1.),
            (1, (t.rows - 1) as f32 + margin, -1.),
        ] {
            let input = std::mem::take(&mut polygon);
            if input.is_empty() {
                return;
            }
            let mut a = *input.last().unwrap();
            let mut da = (a[axis] - edge) * sign;
            for b in input {
                let db = (b[axis] - edge) * sign;
                if (da >= 0.) != (db >= 0.) {
                    polygon.push(a.lerp(b, da / (da - db)));
                }
                if db >= 0. {
                    polygon.push(b);
                }
                a = b;
                da = db;
            }
        }
        if polygon.len() < 3 {
            return;
        }
        let area = (0..polygon.len())
            .map(|i| polygon[i].perp_dot(polygon[(i + 1) % polygon.len()]))
            .sum::<f32>();
        if area.abs() < 0.00001 {
            return;
        }
        if area > 0. {
            polygon.reverse();
        }
        let start = self.positions.len() as u32;
        let (s, c) = p.heading.sin_cos();
        let scale = p.model_scale.max(0.01);
        for at in polygon {
            // Access lanes continue beneath existing base paving, rather than
            // stopping at an arbitrary rectangle several metres short of it.
            let edge = (138. - at.x.min(t.cols as f32 - at.x))
                .min(165. - (at.y - t.rows as f32 * 0.5).abs());
            let base = (edge / 4.).clamp(0., 1.);
            let h = self.height(t, at) + if asphalt { 0.085 } else { 0.065 - base * 0.04 };
            let local = (at - Vec2::new(p.x, p.z)) / scale;
            self.positions.push([
                c * local.x - s * local.y,
                (h - p.elevation) / scale,
                s * local.x + c * local.y,
            ]);
            let grain = crate::natural_noise::value(at.x / 3.8, at.y / 3.8, 417);
            let tint = if asphalt {
                Vec3::splat(0.76 + grain * 0.17)
            } else {
                Vec3::new(0.93, 0.86, 0.72) * (0.82 + grain * 0.14)
            };
            self.colors.push([tint.x, tint.y, tint.z, 1.]);
            // Match the metre-based UV convention of the material loader.
            // Applying /6 here magnified its calibrated texture scale by 3.9x.
            self.uv.push((at * 0.65).to_array());
        }
        for i in 1..self.positions.len() as u32 - start - 1 {
            self.indices.extend([start, start + i, start + i + 1]);
        }
    }
    fn disk(&mut self, at: Vec2, radius: f32, p: &Piece, t: &Terrain, asphalt: bool) {
        let ring = (0..24)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 24.;
                at + Vec2::new(a.cos(), a.sin()) * radius
            })
            .collect();
        self.polygon(ring, p, t, asphalt);
    }
    fn ribbon(&mut self, points: &[Vec2], width: f32, p: &Piece, t: &Terrain, asphalt: bool) {
        let sides = (width / 2.).ceil() as usize;
        for i in 0..points.len() - 1 {
            if points[i].distance_squared(points[i + 1]) < 0.000001 {
                continue;
            }
            let normal = |at: usize| {
                let d = points[(at + 1).min(points.len() - 1)] - points[at.saturating_sub(1)];
                let d = d.normalize_or_zero();
                Vec2::new(-d.y, d.x)
            };
            let (a, b) = (points[i], points[i + 1]);
            let (na, nb) = (normal(i), normal(i + 1));
            for side in 0..sides {
                let l = width * (0.5 - side as f32 / sides as f32);
                let r = width * (0.5 - (side + 1) as f32 / sides as f32);
                self.polygon(
                    vec![a + na * l, b + nb * l, b + nb * r, a + na * r],
                    p,
                    t,
                    asphalt,
                );
            }
        }
        self.disk(points[0], width * 0.5, p, t, asphalt);
        self.disk(*points.last().unwrap(), width * 0.5, p, t, asphalt);
    }
    fn mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_indices(Indices::U32(self.indices));
        mesh.compute_smooth_normals();
        mesh
    }
}
pub fn geographic_roads(t: &Terrain) -> Vec<(Finish, Mesh)> {
    let origin = Piece {
        model_scale: 1.,
        ..Default::default()
    };
    let mut asphalt = Surface::default();
    let mut gravel = Surface::default();
    for road in &crate::gis_prototype::map().roads {
        let points = rounded(&road.points, 0.);
        if road.paved {
            asphalt.ribbon(&points, road.width, &origin, t, true);
        } else {
            gravel.ribbon(&points, road.width, &origin, t, false);
        }
    }
    [(Finish::Asphalt, asphalt), (Finish::Gravel, gravel)]
        .into_iter()
        .filter(|(_, surface)| !surface.positions.is_empty())
        .map(|(finish, surface)| (finish, surface.mesh()))
        .collect()
}
pub fn ground(p: &Piece, t: &Terrain) -> Vec<(Finish, Mesh)> {
    let mut result = if p.town_plan == crate::settlement_layout::TownPlan::Rural {
        rural(p, t)
    } else {
        neighborhood(p, t)
    };
    result.extend(yard_ground(p, t));
    result
}
fn rural(p: &Piece, t: &Terrain) -> Vec<(Finish, Mesh)> {
    if p.town_plan != crate::settlement_layout::TownPlan::Rural {
        return neighborhood(p, t);
    }
    let (s, c) = p.heading.sin_cos();
    let position = |at: Vec2| {
        Vec2::new(p.x, p.z) + Vec2::new(c * at.x + s * at.y, -s * at.x + c * at.y) * p.model_scale
    };
    let chart = |at: Vec2| position(Vec2::from_array(crate::rural_layout::world(at.to_array())));
    let mut asphalt = Surface::default();
    let mut gravel = Surface::default();
    for (points, width) in crate::rural_layout::paved_paths_for(t.cols, t.rows) {
        let path: Vec<_> = rounded(&points, if width > 6. { 9. } else { 2.8 })
            .into_iter()
            .map(chart)
            .collect();
        asphalt.ribbon(&path, width * p.model_scale, p, t, true);
    }
    for (points, width) in crate::rural_layout::farm_paths_for(t.cols, t.rows) {
        let path: Vec<_> = rounded(&points, 3.).into_iter().map(chart).collect();
        gravel.ribbon(&path, width * p.model_scale, p, t, false);
    }
    for at in crate::rural_layout::plazas() {
        gravel.disk(position(Vec2::from_array(at)), 4.5, p, t, false);
    }
    let roads = crate::rural_layout::roads();
    for lot in &p.lots {
        let door = Vec2::from_array(lot.door().0);
        let nearest = roads
            .iter()
            .map(|r| {
                let a = Vec2::from_array(r.a);
                let d = Vec2::from_array(r.b) - a;
                a + d * ((door - a).dot(d) / d.length_squared()).clamp(0., 1.)
            })
            .min_by(|a, b| {
                a.distance_squared(door)
                    .total_cmp(&b.distance_squared(door))
            })
            .unwrap();
        let path: Vec<_> = rounded(&[door.to_array(), nearest.to_array()], 0.)
            .into_iter()
            .map(position)
            .collect();
        gravel.ribbon(&path, 1.5 * p.model_scale, p, t, false);
    }
    vec![
        (Finish::Asphalt, asphalt.mesh()),
        (Finish::Gravel, gravel.mesh()),
    ]
}

fn neighborhood(p: &Piece, t: &Terrain) -> Vec<(Finish, Mesh)> {
    let (s, c) = p.heading.sin_cos();
    let position = |at: Vec2| {
        Vec2::new(p.x, p.z) + Vec2::new(c * at.x + s * at.y, -s * at.x + c * at.y) * p.model_scale
    };
    let farm = p.kind == "village";
    let roads = if farm {
        vec![
            crate::settlement_layout::Road {
                a: [0., -33.],
                b: [0., 33.],
                width: 3.2,
            },
            crate::settlement_layout::Road {
                a: [-15., 0.],
                b: [15., 0.],
                width: 3.2,
            },
        ]
    } else {
        crate::settlement_layout::roads_for(&p.kind, p.town_plan)
    };
    let mut asphalt = Surface::default();
    let mut gravel = Surface::default();
    for road in &roads {
        let points: Vec<_> = rounded(&[road.a, road.b], 0.)
            .into_iter()
            .map(position)
            .collect();
        if farm {
            gravel.ribbon(&points, road.width, p, t, false);
        } else {
            asphalt.ribbon(&points, road.width, p, t, true);
        }
    }
    for plaza in p.town_plan.plazas() {
        gravel.disk(
            position(Vec2::from_array(plaza)),
            if farm { 8. } else { 5. },
            p,
            t,
            false,
        );
    }
    for lot in &p.lots {
        let door = Vec2::from_array(lot.door().0);
        let nearest = roads
            .iter()
            .map(|r| {
                let a = Vec2::from_array(r.a);
                let d = Vec2::from_array(r.b) - a;
                a + d * ((door - a).dot(d) / d.length_squared()).clamp(0., 1.)
            })
            .min_by(|a, b| {
                a.distance_squared(door)
                    .total_cmp(&b.distance_squared(door))
            })
            .unwrap();
        let points: Vec<_> = rounded(&[door.to_array(), nearest.to_array()], 0.)
            .into_iter()
            .map(position)
            .collect();
        gravel.ribbon(&points, 1.5, p, t, false);
    }
    let mut result = vec![];
    if !asphalt.positions.is_empty() {
        result.push((Finish::Asphalt, asphalt.mesh()));
    }
    if !gravel.positions.is_empty() {
        result.push((Finish::Gravel, gravel.mesh()));
    }
    result
}

fn yard_strip(
    surface: &mut Surface,
    site: &crate::yards::Site,
    a: Vec2,
    b: Vec2,
    width: f32,
    entrance: bool,
) {
    let d = b - a;
    let count = (d.length() / 0.8).ceil().max(1.) as usize;
    let n = Vec2::new(-d.y, d.x).normalize_or_zero() * width * 0.5;
    for i in 0..count {
        let a = a + d * (i as f32 / count as f32);
        let b = a + d / count as f32;
        let corners = [a - n, b - n, b + n, a + n];
        if !corners.iter().all(|p| {
            if entrance {
                site.clear_ground(*p, 0.02)
            } else {
                site.clear(*p, 0.02)
            }
        }) {
            continue;
        }
        surface.polygon(
            corners.into_iter().map(|p| site.world(p)).collect(),
            site.piece,
            site.terrain,
            false,
        );
    }
}
fn yard_ground(p: &Piece, t: &Terrain) -> Vec<(Finish, Mesh)> {
    let Some(site) = crate::yards::Site::new(p, t) else {
        return vec![];
    };
    let mut gravel = Surface::default();
    let mut earth = Surface::default();
    for yard in crate::yards::Yard::all(p) {
        for wall in yard.lot.walls() {
            let n = Vec2::from_array(wall.normal) * 0.39;
            let a = Vec2::from_array(wall.a) + n;
            let b = Vec2::from_array(wall.b) + n;
            yard_strip(
                &mut earth,
                &site,
                yard.point(a.x, a.y),
                yard.point(b.x, b.y),
                0.48,
                false,
            );
        }
        let door = yard.lot.local(yard.lot.door().0);
        let front = door[1] + 1.1;
        let side = yard.side() * (yard.lot.width * 0.42 + 1.15);
        let path = [
            yard.point(door[0], front),
            yard.point(side, front),
            yard.point(side, yard.rear() + 1.1),
        ];
        for line in path.windows(2) {
            yard_strip(&mut gravel, &site, line[0], line[1], 0.72, true);
        }
    }
    // Narrow gravel shoulders meet house access paths and stop at junctions.
    for road in &site.roads {
        let a = Vec2::from_array(road.a);
        let b = Vec2::from_array(road.b);
        let d = b - a;
        let n = Vec2::new(-d.y, d.x).normalize_or_zero();
        for side in [-1., 1.] {
            let offset = n * side * (road.width * 0.5 + 0.4);
            yard_strip(&mut gravel, &site, a + offset, b + offset, 0.65, true);
        }
    }
    [(Finish::Gravel, gravel), (Finish::Earth, earth)]
        .into_iter()
        .filter(|(_, s)| !s.positions.is_empty())
        .map(|(f, s)| (f, s.mesh()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worn_foundation_ground_follows_actual_rotated_and_offset_house_walls() {
        let game = crate::simulation::NativeGame::default();
        let mut vertices = 0;
        for entity in game.entities.iter().filter(|e| e.view.kind == "town") {
            let p = &entity.view;
            for (finish, mesh) in yard_ground(p, &game.terrain) {
                if finish != Finish::Earth {
                    continue;
                }
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("garden soil");
                };
                for point in points {
                    let at = [point[0], point[2]];
                    let nearest = p
                        .lots
                        .iter()
                        .map(|l| Vec2::from_array(l.nearest(at)).distance(Vec2::from_array(at)))
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        nearest < 0.8,
                        "soil strip belongs beside a wall: {at:?}, distance {nearest}"
                    );
                }
                vertices += points.len();
            }
        }
        assert!(
            vertices > 100,
            "houses must have actual worn foundation edges"
        );
    }
    #[test]
    fn road_meshes_and_architecture_share_one_metre_based_uv_convention() {
        let game = crate::simulation::NativeGame::default();
        let p = Piece {
            model_scale: 1.,
            ..default()
        };
        let mut surface = Surface::default();
        surface.polygon(
            vec![
                Vec2::new(20., 20.),
                Vec2::new(24., 20.),
                Vec2::new(24., 25.),
                Vec2::new(20., 25.),
            ],
            &p,
            &game.terrain,
            true,
        );
        for (position, uv) in surface.positions.iter().zip(surface.uv) {
            assert!((uv[0] - position[0] * 0.65).abs() < 0.0001);
            assert!((uv[1] - position[2] * 0.65).abs() < 0.0001);
        }
    }
    #[test]
    fn tar_reaches_both_map_edges_and_continuous_surfaces_stay_inside_the_map() {
        let game = crate::simulation::NativeGame::default();
        let town = &game
            .entities
            .iter()
            .find(|e| e.view.town_plan == crate::settlement_layout::TownPlan::Rural)
            .unwrap()
            .view;
        let roads = ground(town, &game.terrain);
        for field in game.entities.iter().filter(|e| e.view.kind == "airfield") {
            let (s, c) = field.view.heading.sin_cos();
            for segment in
                crate::rural_layout::arterial(game.terrain.cols, game.terrain.rows).windows(2)
            {
                for step in 0..=100 {
                    let f = step as f32 / 100.;
                    let x =
                        town.x + segment[0][0] + (segment[1][0] - segment[0][0]) * f - field.view.x;
                    let z =
                        town.z + segment[0][1] + (segment[1][1] - segment[0][1]) * f - field.view.z;
                    let local = Vec2::new(c * x - s * z, s * x + c * z) / field.view.model_scale;
                    assert!(
                        (local.x - 24.).abs() > 63. || local.y.abs() > 177.,
                        "arterial must skirt the entire apron"
                    );
                }
            }
        }
        for (kind, mesh) in &roads {
            let bevy::mesh::VertexAttributeValues::Float32x3(points) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!()
            };
            assert!(!points.is_empty() && mesh.indices().unwrap().len() > 0);
            for at in points {
                assert!(
                    at[0] + town.x >= -crate::battlefield::SCENERY_MARGIN - 0.001
                        && at[2] + town.z >= -crate::battlefield::SCENERY_MARGIN - 0.001
                        && at[0] + town.x
                            <= (game.terrain.cols - 1) as f32
                                + crate::battlefield::SCENERY_MARGIN
                                + 0.001
                        && at[2] + town.z
                            <= (game.terrain.rows - 1) as f32
                                + crate::battlefield::SCENERY_MARGIN
                                + 0.001
                );
            }
            if matches!(kind, Finish::Asphalt) {
                let edges = points
                    .iter()
                    .filter(|v| {
                        let x = v[0] + town.x;
                        let z = v[2] + town.z;
                        x < 0.01
                            || x > (game.terrain.cols - 1) as f32
                            || z < 0.01
                            || z > (game.terrain.rows - 1) as f32
                    })
                    .count();
                assert!(
                    edges >= 4,
                    "asphalt must continue all the way to both boundaries"
                );
                assert!(points.iter().any(|v| v[0] + town.x < 0.01));
                assert!(
                    points
                        .iter()
                        .any(|v| v[0] + town.x > (game.terrain.cols - 1) as f32)
                );
            }
        }
    }
}
