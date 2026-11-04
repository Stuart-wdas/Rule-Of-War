// Small physical relief maps and baked, metre-scale dirt. No per-frame texture work.

fn relief(name: &str, x: u32, y: u32) -> f32 {
    let (x, y) = (x % 256, y % 256);
    let fine = (noise(x, y) % 1000) as f32 / 1000.;
    let field = crate::natural_noise::value(x as f32 / 27., y as f32 / 27., 421);
    match name {
        "cloth" => {
            let weave = ((x as f32 * std::f32::consts::PI * 0.5).sin()
                * (y as f32 * std::f32::consts::PI * 0.5).cos())
                * 0.5;
            weave * 0.12 + fine * 0.025
        }
        "rubber" => fine * 0.025 + field * 0.07,
        "steel" | "painted_metal" | "aircraft_paint" => {
            let scratch = if noise(x / 2, y / 31).is_multiple_of(193) {
                -0.09
            } else {
                0.
            };
            fine * 0.008 + field * 0.025 + scratch
        }
        "wood" | "bark" => {
            let grain = (x as f32 * 0.39 + (y as f32 * 0.027).sin() * 3.).sin();
            grain * if name == "bark" { 0.16 } else { 0.045 } + fine * 0.025
        }
        "stone" => field * 0.22 + fine * 0.035,
        _ => 0.,
    }
}

/// Matching tangent normal and ORM maps for supplied colour-only materials.
/// Photograph-backed PBR maps always take precedence over this micro relief.
pub fn relief_maps(name: &str) -> Option<(Image, Image)> {
    if ![
        "cloth",
        "rubber",
        "steel",
        "painted_metal",
        "aircraft_paint",
        "wood",
        "bark",
        "stone",
    ]
    .contains(&name)
    {
        return None;
    }
    let mut normals = Vec::with_capacity(256 * 256 * 4);
    let mut orm = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let h = relief(name, x, y);
            let n = Vec3::new(
                relief(name, x.wrapping_add(255) % 256, y) - relief(name, x + 1, y),
                relief(name, x, y.wrapping_add(255) % 256) - relief(name, x, y + 1),
                1.,
            )
            .normalize();
            normals.extend(
                (n * 0.5 + Vec3::splat(0.5))
                    .to_array()
                    .map(|v| (v * 255.) as u8),
            );
            normals.push(255);
            let rough = match name {
                "steel" => 0.46 + h.abs() * 0.9,
                "painted_metal" | "aircraft_paint" => 0.64 + h.abs() * 0.8,
                "rubber" => 0.90,
                "cloth" => 0.96,
                _ => 0.85 + h.abs() * 0.3,
            };
            orm.extend([
                248,
                (rough.min(1.) * 255.) as u8,
                if name == "steel" { 215 } else { 0 },
                255,
            ]);
        }
    }
    let image = |pixels| {
        Image::new(
            Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        )
    };
    let (mut normal, mut orm) = (image(normals), image(orm));
    filtered_mips(&mut normal, true);
    filtered_mips(&mut orm, false);
    Some((normal, orm))
}

/// Bind-pose dirt follows the lower hull and undercarriage through animation.
pub fn weather_vehicle(mesh: &mut Mesh, finish: crate::models::Finish) {
    model_wear(mesh, finish, false);
}
pub fn weather_aircraft(mesh: &mut Mesh, finish: crate::models::Finish) {
    model_wear(mesh, finish, true);
}
fn model_wear(mesh: &mut Mesh, finish: crate::models::Finish, aircraft: bool) {
    use crate::models::Finish as F;
    if !matches!(
        finish,
        F::Team | F::Steel | F::Dark | F::Rubber | F::Cloth | F::AircraftPaint | F::A10Livery
    ) {
        return;
    }
    let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        return;
    };
    let colors: Vec<[f32; 4]> = vertices
        .iter()
        .zip(normals)
        .map(|(p, n)| {
            let patch = crate::natural_noise::value(p[0] * 4.1 + p[1], p[2] * 3.7, 927);
            let mud = if aircraft {0.} else {
                ((0.83 - p[1]) / 0.70).clamp(0., 0.72) * (0.65 + patch * 0.35)
            };
            let occluded = 1. - (-n[1]).max(0.) * 0.22;
            let dust = if aircraft {0.035} else { ((1.13-p[1])/0.90).clamp(0.,0.32)*(0.4+patch*0.6) };
            let stream = crate::natural_noise::value(p[0]*15.,p[2]*0.7,115);
            let grime = if aircraft {(1.-stream)*0.08} else {(1.-patch)*0.035};
            let tint = Vec3::splat((0.88 + patch * 0.12-grime) * occluded)
                .lerp(Vec3::new(0.53, 0.43, 0.29), mud)
                .lerp(Vec3::new(0.66,0.60,0.46),dust);
            [tint.x, tint.y, tint.z, 1.]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

#[derive(Clone, Copy)]
struct WeatherVertex {
    p: Vec3,
    n: Vec3,
    uv: Vec2,
    uv1: Vec2,
    tangent: Vec4,
}
impl WeatherVertex {
    fn middle(self, b: Self) -> Self {
        Self {
            p: (self.p + b.p) * 0.5,
            n: (self.n + b.n).normalize_or(Vec3::Y),
            uv: (self.uv + b.uv) * 0.5,
            uv1: (self.uv1 + b.uv1) * 0.5,
            tangent: (self.tangent.truncate() + b.tangent.truncate())
                .normalize_or(self.tangent.truncate())
                .extend(self.tangent.w),
        }
    }
}

/// A few extra vertices on broad facades let damp/streaks vary across a wall.
/// Keep each batch bounded, including very large base foundations.
pub fn weather_vertices(mesh: &mut Mesh) {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return;
    };
    let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return;
    };
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return;
    };
    let Some(VertexAttributeValues::Float32x4(tangents)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
    else {
        return;
    };
    let Some(indices) = mesh.indices() else {
        return;
    };
    let secondary_uv = match mesh.attribute(Mesh::ATTRIBUTE_UV_1) {
        Some(VertexAttributeValues::Float32x2(values)) => Some(values),
        _ => None,
    };
    let has_secondary_uv = secondary_uv.is_some();
    if indices.len() > 24000 {
        return;
    }
    let mut stack = Vec::new();
    for tri in indices.iter().collect::<Vec<_>>().chunks_exact(3) {
        stack.push((
            [tri[0], tri[1], tri[2]].map(|i| WeatherVertex {
                p: p[i].into(),
                n: n[i].into(),
                uv: uv[i].into(),
                uv1: secondary_uv.map(|values| Vec2::from(values[i])).unwrap_or(Vec2::ZERO),
                tangent: tangents[i].into(),
            }),
            0,
        ));
    }
    let (mut positions, mut normals, mut uvs, mut tangents) = (vec![], vec![], vec![], vec![]);
    let mut secondary_uvs = vec![];
    while let Some((mut tri, depth)) = stack.pop() {
        let edge = (0..3)
            .max_by(|a, b| {
                tri[*a]
                    .p
                    .distance_squared(tri[(*a + 1) % 3].p)
                    .total_cmp(&tri[*b].p.distance_squared(tri[(*b + 1) % 3].p))
            })
            .unwrap();
        if depth < 3 && tri[edge].p.distance_squared(tri[(edge + 1) % 3].p) > 2.4 * 2.4 {
            tri.rotate_left(edge);
            let m = tri[0].middle(tri[1]);
            stack.push(([tri[0], m, tri[2]], depth + 1));
            stack.push(([m, tri[1], tri[2]], depth + 1));
        } else {
            for v in tri {
                positions.push(v.p.to_array());
                normals.push(v.n.to_array());
                uvs.push(v.uv.to_array());
                if has_secondary_uv {
                    secondary_uvs.push(v.uv1.to_array());
                }
                tangents.push(v.tangent.to_array());
            }
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    // Calibrated house UVs use UV1 as their authored-chart marker. Leaving its
    // old vertex count here makes GPU upload truncate the subdivided roof/wall.
    if has_secondary_uv {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, secondary_uvs);
    }
    // Edge subdivision preserves the original UV charts. Interpolate their
    // already computed basis instead of running MikkTSpace over every new face
    // whenever a house takes damage.
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.remove_indices();
}

/// Folded cloth replaces three rectangular blocks on the sniper's shoulders.
pub fn mantle() -> Mesh {
    let (mut p, mut uv, mut indices) = (vec![], vec![], vec![]);
    const N: u32 = 8;
    for z in 0..=N {
        for x in 0..=N {
            let (u, v) = (x as f32 / N as f32, z as f32 / N as f32);
            let px = (u - 0.5) * 0.65;
            let pz = (v - 0.5) * 0.52 - 0.05;
            let hem = (u * 31. + v * 23.).sin() * 0.012;
            let y = 0.27 - (px / 0.34).abs().powi(3) * 0.27 - (pz / 0.34).abs().powi(2) * 0.16
                + (u * 38. + v * 4.).sin() * 0.013
                + hem;
            p.push([px, y, pz]);
            uv.push([u, v]);
            if x < N
                && z < N
                && !((N / 2 - 1..N / 2 + 1).contains(&x) && (N / 2 - 1..N / 2 + 2).contains(&z))
            {
                let a = z * (N + 1) + x;
                indices.extend([a, a + N + 1, a + 1, a + 1, a + N + 1, a + N + 2]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    let _ = mesh.generate_tangents();
    mesh
}

#[cfg(test)]
mod wear_tests {
    use super::*;
    #[test]
    fn shoulder_cloth_stays_within_the_existing_infantry_attachment_allowance() {
        let mesh = mantle();
        assert!(mesh.indices().unwrap().len() / 3 <= 128);
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        assert!(n.iter().all(|n| Vec3::from_array(*n).is_finite()));
    }
    #[test]
    fn wear_subdivision_preserves_bounds_normals_and_metre_scale_uvs() {
        let mut mesh = Mesh::from(Cuboid::new(12., 3., 0.22));
        uv(&mut mesh);
        let before = mesh.count_vertices();
        weather_architecture(&mut mesh, crate::models::Finish::Plaster);
        assert!(mesh.count_vertices() > before && mesh.count_vertices() <= before * 12);
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        let Some(VertexAttributeValues::Float32x2(t)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
            panic!()
        };
        for ((p, n), t) in p.iter().zip(n).zip(t) {
            assert!(p[0].abs() <= 6.001 && p[1].abs() <= 1.501 && p[2].abs() <= 0.111);
            assert!((Vec3::from_array(*n).length() - 1.).abs() < 0.001);
            assert!(Vec2::from_array(*t).distance(Vec2::from_array(coords(*p, *n))) < 0.001);
        }
    }
    #[test]
    fn physical_relief_is_linear_normalized_and_separates_fabric_from_metal() {
        for name in ["cloth", "rubber", "steel", "painted_metal", "bark", "stone"] {
            let (normal, orm) = relief_maps(name).unwrap();
            assert_eq!(normal.texture_descriptor.format, TextureFormat::Rgba8Unorm);
            assert_eq!(orm.texture_descriptor.format, TextureFormat::Rgba8Unorm);
            for p in normal.data.as_ref().unwrap()[..256 * 256 * 4]
                .chunks_exact(4)
                .step_by(97)
            {
                let n = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 127.5 - Vec3::ONE;
                assert!((n.length() - 1.).abs() < 0.02);
            }
            assert!(
                orm.data
                    .as_ref()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|p| p[1] > 100 && (p[2] > 0) == (name == "steel"))
            );
        }
        assert!(relief_maps("grass").is_none());
    }
}
