// Closed, faceted armour shells and profiled tyres retain the canonical kit bounds.
#[derive(serde::Deserialize)]
struct RefinedKitMesh {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}
fn refined_mesh(name: &str) -> Mesh {
    static DATA: std::sync::OnceLock<std::collections::BTreeMap<String, RefinedKitMesh>> =
        std::sync::OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/art_refresh/refined_kit.json"))
            .expect("Blender-refined kit meshes")
    });
    let mesh = &data[name];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.vertices.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mesh.normals.clone())
    .with_inserted_indices(Indices::U32(mesh.indices.clone()))
}
fn armour_shell(cabin: bool) -> Mesh {
    refined_mesh(if cabin { "cabin" } else { "turret" })
}
fn tyre_mesh() -> Mesh {
    refined_mesh("tyre")
}
fn tile_lip() -> Mesh {
    let profile = [
        (-0.5, -0.15),
        (-0.3, 0.25),
        (0., 0.5),
        (0.3, 0.25),
        (0.5, -0.15),
    ];
    let (mut p, mut n, mut indices) = (vec![], vec![], vec![]);
    for pair in profile.windows(2) {
        let [(z0, y0), (z1, y1)] = [pair[0], pair[1]];
        for lower in [false, true] {
            let drop = if lower { 0.35 } else { 0. };
            polygon(
                &[
                    Vec3::new(-0.5, y0 - drop, z0),
                    Vec3::new(0.5, y0 - drop, z0),
                    Vec3::new(0.5, y1 - drop, z1),
                    Vec3::new(-0.5, y1 - drop, z1),
                ],
                &mut p,
                &mut n,
                &mut indices,
            );
        }
    }
    for x in [-0.5, 0.5] {
        for pair in profile.windows(2) {
            let [(z0, y0), (z1, y1)] = [pair[0], pair[1]];
            polygon(
                &[
                    Vec3::new(x, y0, z0),
                    Vec3::new(x, y1, z1),
                    Vec3::new(x, y1 - 0.35, z1),
                    Vec3::new(x, y0 - 0.35, z0),
                ],
                &mut p,
                &mut n,
                &mut indices,
            );
        }
    }
    for (z, y) in [profile[0], profile[4]] {
        polygon(
            &[
                Vec3::new(-0.5, y, z),
                Vec3::new(0.5, y, z),
                Vec3::new(0.5, y - 0.35, z),
                Vec3::new(-0.5, y - 0.35, z),
            ],
            &mut p,
            &mut n,
            &mut indices,
        );
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
    .with_inserted_indices(Indices::U32(indices))
}

impl Kit {
    fn vehicle_deck_details(&mut self, wheeled: bool) {
        use Finish::*;
        for side in [-1., 1.] {
            // A layered edge, drainage seam and grab handles catch the sun.
            self.slab(Team, [side * 0.72, 0.91, -0.66], [0.08, 0.07, 1.35]);
            for z in [-1.12, -0.82, -0.52] {
                self.slab(Dark, [side * 0.71, 0.953, z], [0.055, 0.012, 0.018]);
            }
            if !wheeled {
                for z in [-0.86, -0.44, 0.02, 0.48] {
                    self.slab(Team, [side * 0.73, 0.63, z], [0.08, 0.055, 0.35]);
                }
                // Rear external fuel containers and rolled canvas stay below the turret.
                self.block(Team, [side * 0.52, 0.83, -1.30], [0.22, 0.31, 0.18]);
                for y in [0.74, 0.91] {
                    self.slab(Dark, [side * 0.52, y, -1.398], [0.23, 0.022, 0.012]);
                }
            }
        }
    }
    fn turret_fittings(&mut self, support: bool) {
        use Finish::*;
        // Already inside the turret's articulation range.
        for side in [-1., 1.] {
            self.slab(Dark, [side * 0.49, 1.48, -0.35], [0.025, 0.018, 0.25]);
            for z in [-0.46, -0.24] {
                self.slab(Steel, [side * 0.49, 1.51, z], [0.024, 0.05, 0.024]);
            }
            for i in 0..4 {
                self.slab(
                    Team,
                    [side * 0.54, 1.25, -0.53 + i as f32 * 0.18],
                    [0.08, 0.18, 0.16],
                );
            }
        }
        if support {
            self.add(
                Shape::CarrierCabin,
                Team,
                [0., 1.29, -0.52],
                [1.04, 0.40, 0.69],
            );
            for x in [-0.39, 0., 0.39] {
                self.slab(Dark, [x, 1.48, -0.62], [0.19, 0.012, 0.28]);
            }
        } else {
            self.slab(Team, [-0.25, 1.50, -0.25], [0.28, 0.035, 0.33]);
        }
    }
    fn carrier_fittings(&mut self) {
        use Finish::*;
        // Recessed rear ramp, hinge barrels, spaced side armour and roof rails.
        self.slab(Dark, [0., 1.03, -1.405], [1.12, 0.65, 0.018]);
        self.add(
            Shape::CarrierCabin,
            Team,
            [0., 1.03, -1.425],
            [1.04, 0.61, 0.035],
        );
        for x in [-0.40, 0.40] {
            self.cylinder(
                Steel,
                [x, 0.76, -1.45],
                [0.055, 0.20, 0.055],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        }
        for side in [-1., 1.] {
            for z in [-0.98, -0.51, -0.04] {
                self.add(
                    Shape::ArmourTurret,
                    Team,
                    [side * 0.72, 1.01, z],
                    [0.09, 0.30, 0.42],
                );
            }
            self.slab(Steel, [side * 0.53, 1.49, -0.50], [0.025, 0.045, 1.28]);
            for z in [-1.08, 0.06] {
                self.slab(Team, [side * 0.53, 1.46, z], [0.055, 0.08, 0.055]);
            }
        }
    }
    fn aircraft_skin_details(&mut self, helicopter: bool) {
        use Finish::*;
        if helicopter {
            // Transmission fairing vents, wire cutters and cockpit rain gutters.
            for side in [-1., 1.] {
                for z in [-0.94, -0.84, -0.74, -0.64] {
                    self.slab(Dark, [side * 0.26, 1.37, z], [0.009, 0.075, 0.055]);
                }
                self.air_rod(
                    Steel,
                    [side * 0.19, 1.49, 0.34],
                    [side * 0.28, 1.64, 0.59],
                    0.018,
                );
                self.air_rod(
                    AircraftPaint,
                    [side * 0.29, 1.30, 0.14],
                    [side * 0.24, 1.34, 0.89],
                    0.014,
                );
                self.slab(
                    AircraftPaint,
                    [side * 0.34, 0.82, -0.25],
                    [0.025, 0.16, 0.26],
                );
            }
        } else {
            // Thin wing-root access panels and static dischargers, within the wing bounds.
            for side in [-1., 1.] {
                self.slab(
                    AircraftPaint,
                    [side * 0.57, 0.677, -0.18],
                    [0.16, 0.012, 0.32],
                );
                for z in [-0.24, -0.15, -0.06] {
                    self.slab(Dark, [side * 0.57, 0.686, z], [0.10, 0.005, 0.018]);
                }
            }
        }
    }
    fn facility_details(&mut self, w: f32, d: f32, h: f32) {
        use Finish::*;
        for side in [-1., 1.] {
            let start = self.0.len();
            let x = side * (w * 0.5 + 0.065);
            // Vent grille, rain hood, utility box and its external conduit.
            self.slab(Dark, [x, h * 0.82, -d * 0.26], [0.025, 0.70, 1.02]);
            for row in 0..7 {
                self.slab(
                    Steel,
                    [
                        x + side * 0.03,
                        h * 0.82 - 0.28 + row as f32 * 0.09,
                        -d * 0.26,
                    ],
                    [0.055, 0.032, 0.89],
                );
            }
            self.slab(
                Steel,
                [x + side * 0.07, h * 0.82 + 0.39, -d * 0.26],
                [0.21, 0.045, 1.12],
            );
            self.block(Steel, [x, 0.96, d * 0.30], [0.18, 0.43, 0.36]);
            self.cylinder(
                Steel,
                [x, 0.42, d * 0.30],
                [0.035, 0.64, 0.035],
                Quat::IDENTITY,
            );
            for part in &mut self.0[start..] {
                part.roof_lot = Some(usize::MAX);
                part.wall_normal = Some(Vec3::X * side);
            }
        }
    }
}

#[cfg(test)]
mod refinement_tests {
    use super::*;
    #[test]
    fn complete_military_kits_stay_inside_a_bounded_quality_budget() {
        let shapes = shape_meshes();
        for (kind, category) in [
            ("medium", "vehicle"),
            ("long", "vehicle"),
            ("troopCarrier", "vehicle"),
            ("xtralong", "vehicle"),
            ("antiAir", "vehicle"),
            ("plane", "aircraft"),
            ("helicopter", "aircraft"),
        ] {
            let p = Piece {
                kind: kind.into(),
                category: category.into(),
                ..default()
            };
            let triangles: usize = bake_animated(parts(&p), &shapes)
                .iter()
                .map(|(_, _, _, m)| m.indices().unwrap().len() / 3)
                .sum();
            println!("MODEL_TRIANGLES {kind} {triangles}");
            assert!(triangles > 0 && triangles <= 30000, "{kind}: {triangles}");
        }
    }
    #[test]
    fn refined_shells_tyres_and_tiles_have_finite_non_degenerate_faces() {
        use bevy::mesh::VertexAttributeValues as A;
        for mesh in [
            armour_shell(false),
            armour_shell(true),
            tyre_mesh(),
            tile_lip(),
        ] {
            let Some(A::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                panic!("positions")
            };
            let Some(A::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
                panic!("normals")
            };
            assert_eq!(p.len(), n.len());
            for (p, n) in p.iter().zip(n) {
                assert!(Vec3::from_array(*p).is_finite());
                assert!((Vec3::from_array(*n).length() - 1.).abs() < 0.001);
                assert!(Vec3::from_array(*p).abs().max_element() <= 0.51);
            }
            for tri in mesh
                .indices()
                .unwrap()
                .iter()
                .collect::<Vec<_>>()
                .chunks_exact(3)
            {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(p[i]));
                assert!((b - a).cross(c - a).length() > 0.000001);
            }
        }
    }
}
