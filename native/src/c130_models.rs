// AC-130J silhouette: high straight wing, four six-blade turboprops, tall single
// fin, upswept cargo tail and two port-side weapons. Photo sources live in
// assets/aircraft/c130/references/; dimensions are in world metres.
fn c130_body() -> Mesh {
    let stations: [(f32, f32, f32, f32); 14] = [
        (-14.9, 0.05, 0.08, 1.3),
        (-13.8, 0.65, 0.65, 1.0),
        (-11.5, 1.25, 1.25, 0.65),
        (-8.5, 1.85, 1.9, 0.15),
        (-5.5, 2.05, 2.15, 0.),
        (-1., 2.05, 2.15, 0.),
        (4., 2.05, 2.15, 0.),
        (8., 1.95, 2.05, 0.),
        (10., 1.7, 1.85, -0.05),
        (11.4, 1.45, 1.55, -0.25),
        (12.6, 1.1, 1.12, -0.55),
        (13.5, 0.72, 0.75, -0.65),
        (14.2, 0.35, 0.38, -0.7),
        (14.9, 0.02, 0.02, -0.7),
    ];
    let mut positions = vec![];
    let mut indices = vec![];
    const SIDES: u32 = 24;
    for (z, width, height, y) in stations {
        for j in 0..SIDES {
            let a = j as f32 / SIDES as f32 * std::f32::consts::TAU;
            positions.push([width * a.cos(), y + height * a.sin(), z]);
        }
    }
    for ring in 0..stations.len() as u32 - 1 {
        for j in 0..SIDES {
            let a = ring * SIDES + j;
            let b = ring * SIDES + (j + 1) % SIDES;
            indices.extend([a, b, a + SIDES, b, b + SIDES, a + SIDES]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}
fn c130_surface(contour: &[[f32; 3]], offset: Vec3) -> Mesh {
    let mut p = vec![];
    let mut n = vec![];
    let mut indices = vec![];
    let center = contour.iter().copied().map(Vec3::from_array).sum::<Vec3>() / contour.len() as f32;
    let a: Vec<_> = contour
        .iter()
        .map(|v| Vec3::from_array(*v) - center - offset)
        .collect();
    let b: Vec<_> = contour
        .iter()
        .map(|v| Vec3::from_array(*v) - center + offset)
        .collect();
    polygon(&a, &mut p, &mut n, &mut indices);
    polygon(&b, &mut p, &mut n, &mut indices);
    for j in 0..a.len() {
        let k = (j + 1) % a.len();
        polygon(&[a[j], a[k], b[k], b[j]], &mut p, &mut n, &mut indices);
    }
    for vertex in &mut p {
        *vertex = (Vec3::from_array(*vertex) + center).to_array();
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
    .with_inserted_indices(Indices::U32(indices))
}
fn c130_meshes() -> Vec<Mesh> {
    vec![
        c130_body(),
        c130_surface(
            &[
                [-20., 2.15, -0.5],
                [-19.8, 2.15, 2.2],
                [-3., 2.15, 4.6],
                [3., 2.15, 4.6],
                [19.8, 2.15, 2.2],
                [20., 2.15, -0.5],
                [3., 2.15, -2.4],
                [-3., 2.15, -2.4],
            ],
            Vec3::Y * 0.16,
        ),
        c130_surface(
            &[
                [-8., 1.9, -13.4],
                [-7.8, 1.9, -11.6],
                [-1., 1.9, -8.8],
                [1., 1.9, -8.8],
                [7.8, 1.9, -11.6],
                [8., 1.9, -13.4],
            ],
            Vec3::Y * 0.1,
        ),
        c130_surface(
            &[
                [0., 1.6, -13.7],
                [0., 8.1, -13.4],
                [0., 8.2, -11.2],
                [0., 2.1, -6.9],
            ],
            Vec3::X * 0.14,
        ),
    ]
}
impl Kit {
    fn c130(&mut self) {
        use Finish::*;
        for shape in [
            Shape::C130Body,
            Shape::C130Wing,
            Shape::C130Tail,
            Shape::C130Fin,
        ] {
            self.add(shape, AircraftPaint, [0.; 3], [1.; 3]);
        }
        // Framed cockpit panes wrap the blunt transport nose.
        for side in [-1., 1.] {
            self.block(AircraftGlass, [side * 0.67, 1.13, 11.1], [1.1, 0.7, 0.12]);
            self.rotate(Quat::from_rotation_x(-0.35) * Quat::from_rotation_y(side * 0.38));
            self.block(AircraftGlass, [side * 1.55, 1.02, 9.75], [0.1, 0.65, 1.55]);
            self.rotate(Quat::from_rotation_z(-side * 0.35));
            self.sphere(AircraftPaint, [side * 1.85, -1.4, -1.4], [1.5, 1.5, 5.7]);
            // Flush wheel doors: the call-in always has its landing gear stowed.
            self.block(Dark, [side * 2.4, -1.7, -1.5], [0.025, 0.035, 3.6]);
            self.block(Red, [side * 19.8, 2.3, 0.4], [0.2, 0.16, 0.25]);
        }
        let along = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        for x in [-13., -6.3, 6.3, 13.] {
            self.cylinder(AircraftPaint, [x, 1.6, 3.35], [1.65, 4.6, 1.65], along);
            self.sphere(AircraftPaint, [x, 1.6, 5.5], [1.6, 1.6, 1.5]);
            self.block(Dark, [x, 0.91, 5.2], [0.75, 0.25, 0.8]);
            self.cylinder(Steel, [x, 1.6, 5.98], [0.5, 0.5, 0.5], along);
            let hub = [x, 1.6, 6.15];
            for blade in 0..6 {
                let a = blade as f32 * std::f32::consts::TAU / 6.;
                self.block(
                    Dark,
                    [x + a.sin() * 1.08, 1.6 + a.cos() * 1.08, 6.15],
                    [0.27, 2.05, 0.075],
                );
                self.rotate(Quat::from_rotation_z(-a));
                self.joint(32, hub);
                self.block(
                    White,
                    [x + a.sin() * 1.99, 1.6 + a.cos() * 1.99, 6.15],
                    [0.28, 0.12, 0.08],
                );
                self.rotate(Quat::from_rotation_z(-a));
                self.joint(32, hub);
            }
            self.sphere(AircraftPaint, [x, 1.6, 6.25], [0.7, 0.7, 0.9]);
            // Exhaust outlets behind each nacelle.
            self.cylinder(Dark, [x, 1.45, 0.98], [0.8, 0.18, 0.8], along);
        }
        self.sphere(Dark, [0.4, -1.8, 11.3], [1., 1., 1.]);
        self.sphere(AircraftGlass, [0.4, -2.15, 11.5], [0.65, 0.4, 0.6]);
        self.sphere(AircraftPaint, [0., 2.1, -1.5], [1.7, 0.9, 2.5]);
        self.air_rod(Steel, [0., 2.2, 4.9], [0., 3.5, 4.7], 0.08);
        self.air_rod(Steel, [0., 2.1, -5.2], [0., 3.1, -5.8], 0.08);
        // Socket order matches the 30 mm and 105 mm projectile events.
        for (z, length, bore) in [(6.3, 2.3, 0.15), (-5.6, 3.1, 0.34)] {
            self.sphere(Dark, [-1.92, -0.5, z], [0.55, 1.1, 1.4]);
            let start = [-1.9, -0.5, z];
            let end = [-1.9 - length, -0.5 - length * 0.27, z];
            self.air_rod(Steel, start, end, bore);
            self.socket(Vec3::Y * 0.5);
            self.air_rod(Dark, [end[0] + 0.11, end[1] + 0.03, z], end, bore * 1.25);
        }
        // Cargo-ramp seams and restrained high-contrast tail details.
        for side in [-1., 1.] {
            self.air_rod(
                Dark,
                [side * 1.1, -0.4, -10.8],
                [side * 0.5, 0.9, -13.5],
                0.04,
            );
            self.block(Dark, [side * 0.16, 6.8, -12.3], [0.025, 0.28, 1.25]);
            self.block(White, [side * 10., 2.33, 0.7], [1.4, 0.025, 0.24]);
        }
    }
}

#[cfg(test)]
mod c130_model_tests {
    use super::*;
    #[test]
    fn gunship_model_has_port_guns_four_propellers_and_valid_baked_geometry() {
        let piece = Piece {
            kind: "c130".into(),
            category: "aircraft".into(),
            ..default()
        };
        let parts = parts(&piece);
        let sockets = WeaponSockets::from_parts(&parts);
        assert_eq!(sockets.0.len(), 2);
        assert!(sockets.0[0].at.x < -4. && sockets.0[0].at.z > 5.);
        assert!(sockets.0[1].at.x < -4.5 && sockets.0[1].at.z < -5.);
        assert!(
            sockets
                .0
                .iter()
                .all(|s| s.forward.x < -0.9 && s.forward.y < 0.)
        );
        let propellers: std::collections::BTreeSet<_> = parts
            .iter()
            .filter(|p| p.joint == 32)
            .map(|p| p.pivot.x.to_bits())
            .collect();
        assert_eq!(propellers.len(), 4);
        let shapes = shape_meshes();
        let mut triangles = 0;
        for (_, _, _, mesh) in bake_animated(parts, &shapes) {
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions")
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("normals")
            };
            assert!(p.iter().all(|v| Vec3::from_array(*v).is_finite()));
            assert!(
                n.iter()
                    .all(|v| (Vec3::from_array(*v).length() - 1.).abs() < 0.001)
            );
            let i: Vec<_> = mesh.indices().unwrap().iter().collect();
            triangles += i.len() / 3;
            for t in i.chunks_exact(3) {
                let [a, b, c] = [p[t[0]], p[t[1]], p[t[2]]].map(Vec3::from_array);
                assert!((b - a).cross(c - a).length() > 0.0000001);
            }
        }
        assert!(triangles < 12000, "gunship has {triangles} triangles");
    }
}
