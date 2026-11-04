//! Small, reusable natural meshes. Built once at scene setup, never per frame.
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};

fn hash(mut n: u32) -> u32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^ (n >> 16)
}
fn unit(seed: u32, n: u32) -> f32 {
    (hash(seed ^ n.wrapping_mul(0x9e37_79b9)) & 0xffff) as f32 / 65535.
}

/// A flared foot, tapered upper stem and two small bends break the old cylinder
/// silhouette. Shared by trunks and their scaled forks, with longitudinal UVs.
pub fn trunk() -> Mesh {
    let rings = [
        (Vec3::new(0., -0.9, 0.), 0.17),
        (Vec3::new(0.01, -0.68, 0.005), 0.13),
        (Vec3::new(-0.014, -0.15, 0.014), 0.115),
        (Vec3::new(0.023, 0.35, 0.005), 0.084),
        (Vec3::new(0.055, 0.9, -0.018), 0.047),
    ];
    let mut positions = vec![];
    let mut normals = vec![];
    let mut uv = vec![];
    let mut colors = vec![];
    let mut indices = vec![];
    for (row, (center, radius)) in rings.iter().enumerate() {
        let t = row as f32 / (rings.len() - 1) as f32;
        for side in 0..=8 {
            let a = side as f32 * std::f32::consts::TAU / 8.;
            let ridge = 1. + 0.07 * (a * 3. + t * 1.4).sin();
            positions.push(
                (*center + Vec3::new(a.cos() * radius * ridge, 0., a.sin() * radius * ridge))
                    .to_array(),
            );
            normals.push(Vec3::new(a.cos(), 0.07, a.sin()).normalize().to_array());
            uv.push([side as f32 / 4., t * 3.]);
            let shade = (0.66 + t * 0.29) * (0.92 + 0.08 * (a * 3. + t).cos());
            colors.push([shade, shade * 0.98, shade * 0.94, 1.]);
        }
        if row > 0 {
            for side in 0..8 {
                let a = (row - 1) as u32 * 9 + side;
                let b = row as u32 * 9 + side;
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    for row in [0, rings.len() - 1] {
        let (center, radius) = rings[row];
        let start = positions.len() as u32;
        let normal = if row == 0 { -Vec3::Y } else { Vec3::Y };
        for side in 0..8 {
            let a = side as f32 * std::f32::consts::TAU / 8.;
            let t = row as f32 / (rings.len() - 1) as f32;
            let ridge = 1. + 0.07 * (a * 3. + t * 1.4).sin();
            positions.push(
                (center + Vec3::new(a.cos() * radius * ridge, 0., a.sin() * radius * ridge))
                    .to_array(),
            );
            normals.push(normal.to_array());
            uv.push([0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5]);
            colors.push([0.8, 0.78, 0.75, 1.]);
        }
        for side in 1..7 {
            if row == 0 {
                indices.extend([start, start + side, start + side + 1]);
            } else {
                indices.extend([start, start + side + 1, start + side]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    let _ = mesh.generate_tangents();
    mesh
}

/// A fractured boulder with broad rock faces and narrow worn edges. Its footprint
/// stays within the old 0.7 m radius, preserving the scatter/collision envelope.
pub fn rock(seed: u32) -> Mesh {
    let mut planes = vec![
        (Vec3::X, 0.53 + unit(seed, 1) * 0.12),
        (-Vec3::X, 0.50 + unit(seed, 2) * 0.13),
        (Vec3::Z, 0.49 + unit(seed, 3) * 0.12),
        (-Vec3::Z, 0.48 + unit(seed, 4) * 0.13),
        (-Vec3::Y, 0.39),
        (
            Vec3::new(0.18, 1., -0.14).normalize(),
            0.46 + unit(seed, 5) * 0.12,
        ),
    ];
    for i in 0..7 {
        let angle = i as f32 * std::f32::consts::TAU / 7. + unit(seed, 11 + i) * 0.28;
        let normal =
            Vec3::new(angle.cos(), 0.18 + unit(seed, 25 + i) * 0.72, angle.sin()).normalize();
        planes.push((normal, 0.55 + unit(seed, 41 + i) * 0.11));
    }
    // Intersections of three supporting planes produce a watertight convex hull.
    // Keeping the face polygons lets us bevel real edges rather than every
    // arbitrary triangulation diagonal of a subdivided sphere.
    let mut vertices: Vec<Vec3> = vec![];
    for a in 0..planes.len() {
        for b in a + 1..planes.len() {
            for c in b + 1..planes.len() {
                let (na, da) = planes[a];
                let (nb, db) = planes[b];
                let (nc, dc) = planes[c];
                let det = na.dot(nb.cross(nc));
                if det.abs() < 0.00001 {
                    continue;
                }
                let point = (nb.cross(nc) * da + nc.cross(na) * db + na.cross(nb) * dc) / det;
                if planes.iter().all(|(n, d)| n.dot(point) <= d + 0.0001)
                    && vertices
                        .iter()
                        .all(|v| v.distance_squared(point) > 0.000001)
                {
                    vertices.push(point);
                }
            }
        }
    }
    let radius = vertices.iter().map(|v| v.x.hypot(v.z)).fold(0., f32::max);
    let footprint = 0.7 / radius.max(0.7);
    let shape = |v: Vec3| Vec3::new(v.x * footprint, v.y, v.z * footprint);
    let mut positions = vec![];
    let mut normals = vec![];
    let mut colors = vec![];
    let mut uv = vec![];
    let mut indices = vec![];
    for (face, &(normal, distance)) in planes.iter().enumerate() {
        let mut polygon: Vec<_> = vertices
            .iter()
            .copied()
            .filter(|v| (normal.dot(*v) - distance).abs() < 0.0002)
            .collect();
        if polygon.len() < 3 {
            continue;
        }
        let center = polygon.iter().copied().sum::<Vec3>() / polygon.len() as f32;
        let u = normal
            .cross(if normal.y.abs() > 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            })
            .normalize();
        let v = normal.cross(u);
        polygon.sort_by(|a, b| {
            let angle = |p: Vec3| (p - center).dot(v).atan2((p - center).dot(u));
            angle(*a).total_cmp(&angle(*b))
        });
        let color = 0.88 + unit(seed, 71 + face as u32) * 0.17;
        let mut triangle = |a: Vec3, mut b: Vec3, mut c: Vec3, edge: bool| {
            if (b - a).cross(c - a).dot(normal) < 0. {
                std::mem::swap(&mut b, &mut c);
            }
            let points = [shape(a), shape(b), shape(c)];
            let n = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize_or(normal);
            let start = positions.len() as u32;
            for p in points {
                positions.push(p.to_array());
                normals.push(n.to_array());
                let root = ((p.y + 0.39) / 0.43).clamp(0., 1.);
                let earth = 0.64 + root * 0.36;
                let light = color * earth * if edge { 1.025 } else { 1. };
                colors.push([
                    light,
                    light * (0.96 + root * 0.04),
                    light * (0.88 + root * 0.10),
                    1.,
                ]);
                uv.push(crate::surfaces::coords(p.to_array(), n.to_array()));
            }
            indices.extend([start, start + 1, start + 2]);
        };
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let inner_a = a.lerp(center, 0.11);
            let inner_b = b.lerp(center, 0.11);
            triangle(center, inner_a, inner_b, false);
            // Shared outer edges sit just below the broad planes, creating a
            // narrow physical bevel that can catch sunlight at close range.
            triangle(inner_a, a * 0.975, b * 0.975, true);
            triangle(inner_a, b * 0.975, inner_b, true);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices))
}

/// Twelve shared vertices retain the old pebble's upload cost. A flattened,
/// asymmetric profile reads as broken stone rather than a tiny round ball.
pub fn pebble(seed: u32) -> Mesh {
    let mut mesh = Sphere::new(0.7).mesh().ico(0).unwrap();
    if let Some(VertexAttributeValues::Float32x3(vertices)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for (i, vertex) in vertices.iter_mut().enumerate() {
            let p = Vec3::from_array(*vertex);
            let shape = 0.83 + unit(seed, i as u32) * 0.30;
            *vertex = Vec3::new(
                p.x * shape,
                (p.y * 0.69 + p.x * 0.14).clamp(-0.28, 0.40),
                p.z * (1.02 - (shape - 0.83) * 0.4),
            )
            .to_array();
        }
    }
    mesh.compute_smooth_normals();
    crate::surfaces::uv(&mut mesh);
    let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!()
    };
    let colors: Vec<[f32; 4]> = vertices
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let root = ((p[1] + 0.28) / 0.50).clamp(0., 1.);
            let light = (0.63 + root * 0.31) * (0.92 + unit(seed, i as u32 + 21) * 0.12);
            [light, light * 0.98, light * (0.88 + root * 0.10), 1.]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_stems_taper_bend_and_keep_complete_bark_vertex_streams() {
        let mesh = trunk();
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        assert!(mesh.count_vertices() <= 70);
        assert_eq!(p.len(), n.len());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
        assert!(
            p.iter()
                .all(|p| Vec3::from_array(*p).is_finite() && (-0.9001..=0.9001).contains(&p[1]))
        );
        let base = p[0][0].hypot(p[0][2]);
        let tip = (p[36][0] - 0.055).hypot(p[36][2] + 0.018);
        assert!(base > tip * 3.);
        assert!(
            n.iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.).abs() < 0.0001)
        );
    }
    #[test]
    fn fractured_rocks_have_finite_unit_normals_bounded_footprints_and_seed_variation() {
        let a = rock(73);
        let b = rock(421);
        assert_ne!(
            a.attribute(Mesh::ATTRIBUTE_POSITION),
            b.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION),
            rock(73).attribute(Mesh::ATTRIBUTE_POSITION)
        );
        for mesh in [a, b, rock(0), rock(u32::MAX)] {
            assert!(mesh.indices().unwrap().len() / 3 <= 240);
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!()
            };
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!()
            };
            assert!(
                points
                    .iter()
                    .all(|p| Vec3::from_array(*p).is_finite() && p[0].hypot(p[2]) <= 0.7001)
            );
            assert!(
                normals
                    .iter()
                    .all(|n| (Vec3::from_array(*n).length() - 1.).abs() < 0.0001)
            );
            assert_eq!(points.len(), normals.len());
            assert!(points.len() > 90);
        }
        assert_eq!(pebble(31).count_vertices(), 12);
        assert_eq!(pebble(31).indices().unwrap().len(), 60);
    }
}
