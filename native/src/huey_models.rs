// UH-1Y Venom-inspired utility airframe. References: assets/aircraft/huey/README.md.
// Keep the saved huey unit ID and the existing transport/rotor animation joints.
fn venom_cabin(stations: &[[f32; 4]]) -> Mesh {
    // A rounded rectangular cross-section gives a usable cabin, flatter doors
    // and a continuous chined nose instead of an egg-shaped scaled primitive.
    const N: usize = 32;
    let rings: Vec<Vec<Vec3>> = stations
        .iter()
        .map(|&[z, w, bottom, top]| {
            (0..N)
                .map(|n| {
                    let a = n as f32 * std::f32::consts::TAU / N as f32;
                    let (s, c) = a.sin_cos();
                    Vec3::new(
                        c.signum() * c.abs().powf(0.64) * w,
                        (top + bottom) * 0.5
                            + s.signum() * s.abs().powf(0.72) * (top - bottom) * 0.5,
                        z,
                    )
                })
                .collect()
        })
        .collect();
    let mut mesh = AirMesh::default();
    for (i, ring) in rings.iter().enumerate() {
        for n in 0..N {
            let tangent = ring[(n + 1) % N] - ring[(n + N - 1) % N];
            let longitudinal =
                rings[(i + 1).min(rings.len() - 1)][n] - rings[i.saturating_sub(1)][n];
            mesh.positions.push(ring[n].to_array());
            mesh.normals
                .push(tangent.cross(longitudinal).normalize().to_array());
        }
    }
    for i in 0..rings.len() - 1 {
        for n in 0..N {
            let a = (i * N + n) as u32;
            let b = (i * N + (n + 1) % N) as u32;
            mesh.indices
                .extend([a, b, b + N as u32, a, b + N as u32, a + N as u32]);
        }
    }
    let mut rear = rings[0].clone();
    rear.reverse();
    mesh.face(&rear);
    mesh.face(rings.last().unwrap());
    mesh.finish()
}
fn huey_meshes() -> Vec<Mesh> {
    let body = venom_cabin(&[
        [-2.10, 0.24, 0.67, 1.30],
        [-1.82, 0.55, 0.47, 1.58],
        [-1.28, 0.76, 0.38, 1.76],
        [-0.65, 0.81, 0.37, 1.81],
        [0.30, 0.81, 0.38, 1.81],
        [0.85, 0.75, 0.40, 1.77],
        [1.31, 0.67, 0.40, 1.64],
        [1.70, 0.59, 0.43, 1.30],
        [2.08, 0.45, 0.51, 1.08],
        [2.35, 0.26, 0.60, 0.99],
        [2.42, 0.06, 0.71, 0.88],
    ]);
    let boom = air_loft(
        &[
            [-5.12, 0.075, 1.34, 1.52],
            [-4.65, 0.12, 1.20, 1.50],
            [-3.50, 0.20, 0.98, 1.43],
            [-2.30, 0.29, 0.71, 1.39],
            [-1.65, 0.38, 0.56, 1.43],
        ],
        20,
        true,
        false,
    );
    let mut glass = AirMesh::default();
    for side in [-1., 1.] {
        let mut pane = |points: &[[f32; 3]]| {
            let mut p: Vec<_> = points
                .iter()
                .map(|p| Vec3::new(p[0] * side, p[1], p[2]))
                .collect();
            if (p[1] - p[0]).cross(p[2] - p[0]).x * side < 0. {
                p.reverse();
            }
            glass.face(&p);
        };
        // Separate windscreen, pilot door, narrow crew window and two cargo panes.
        pane(&[
            [0.035, 1.70, 1.32],
            [0.52, 1.61, 1.30],
            [0.58, 1.10, 2.01],
            [0.035, 1.17, 2.10],
        ]);
        pane(&[
            [0.61, 1.62, 1.28],
            [0.744, 1.64, 0.75],
            [0.787, 0.99, 0.75],
            [0.67, 0.95, 1.77],
        ]);
        pane(&[
            [0.793, 1.60, 0.35],
            [0.814, 1.60, 0.62],
            [0.82, 1.08, 0.62],
            [0.82, 1.08, 0.35],
        ]);
        for z in [-1.01, -0.38] {
            pane(&[
                [0.832, 1.57, z],
                [0.832, 1.57, z + 0.47],
                [0.832, 1.13, z + 0.47],
                [0.832, 1.13, z],
            ]);
        }
        pane(&[
            [0.12, 0.86, 2.32],
            [0.43, 0.84, 2.15],
            [0.41, 0.63, 2.17],
            [0.12, 0.66, 2.37],
        ]);
        // Shallow green-blue eyebrow glazing above the pilots.
        pane(&[
            [0.08, 1.785, 1.03],
            [0.39, 1.76, 1.01],
            [0.45, 1.72, 1.27],
            [0.08, 1.755, 1.29],
        ]);
    }
    let fin = air_fin(
        &[
            [1.31, -4.84],
            [1.46, -5.18],
            [2.72, -5.26],
            [2.69, -5.02],
            [1.92, -4.61],
        ],
        0.095,
    );
    // One tapered composite blade is repeated around the articulated rotor head.
    let rotor = air_wing(&[
        [0.19, 0., 0.08, -0.09, 0.030],
        [0.50, 0.012, 0.15, -0.17, 0.037],
        [3.62, 0.008, 0.12, -0.20, 0.026],
        [3.94, -0.015, 0.055, -0.23, 0.020],
        [4.05, -0.025, -0.025, -0.21, 0.016],
    ]);
    let tail = air_fin(
        &[
            [0.16, -0.035],
            [0.65, -0.095],
            [0.78, -0.045],
            [0.74, 0.065],
            [0.20, 0.075],
        ],
        0.022,
    );
    let engine = venom_cabin(&[
        [-1.99, 0.13, 1.76, 2.02],
        [-1.76, 0.22, 1.68, 2.14],
        [-1.12, 0.29, 1.66, 2.21],
        [-0.42, 0.29, 1.67, 2.20],
        [0.12, 0.23, 1.72, 2.12],
        [0.34, 0.15, 1.80, 2.04],
    ]);
    vec![body, boom, glass.finish(), fin, rotor, tail, engine]
}
impl Kit {
    fn huey(&mut self) {
        use Finish::*;
        for (shape, finish) in [
            (Shape::HueyBody, AircraftPaint),
            (Shape::HueyBoom, AircraftPaint),
            (Shape::HueyGlass, AircraftGlass),
            (Shape::HueyFin, AircraftPaint),
        ] {
            self.add(shape, finish, [0.; 3], [1.; 3]);
        }
        let along = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        // Twin engine fairings and gearbox replace the single-engine doghouse.
        for side in [-1., 1.] {
            self.add(
                Shape::HueyEngine,
                AircraftPaint,
                [side * 0.46, 0., 0.],
                [1.; 3],
            );
            // Recessed side intakes and aft exhausts with a separate dark bore.
            self.slab(Steel, [side * 0.752, 1.97, -0.24], [0.035, 0.32, 0.29]);
            self.slab(Dark, [side * 0.776, 1.97, -0.24], [0.018, 0.25, 0.23]);
            for z in [-1.23, -1.11, -0.99, -0.87, -0.75] {
                self.slab(Dark, [side * 0.75, 1.98, z], [0.018, 0.17, 0.032]);
            }
            self.cylinder(Steel, [side * 0.46, 1.90, -1.99], [0.30, 0.21, 0.30], along);
            self.cylinder(
                Dark,
                [side * 0.46, 1.90, -2.105],
                [0.24, 0.015, 0.24],
                along,
            );
            // Flat sliding doors, narrow panel gaps, inset glazing and latches.
            self.slab(Sage, [side * 0.813, 1.075, -0.43], [0.028, 1.04, 1.51]);
            for z in [-1.20, 0.33] {
                self.air_rod(
                    Dark,
                    [side * 0.834, 0.59, z],
                    [side * 0.834, 1.65, z],
                    0.011,
                );
            }
            self.air_rod(
                Steel,
                [side * 0.80, 1.70, -1.25],
                [side * 0.80, 1.70, 0.35],
                0.019,
            );
            self.air_rod(
                Dark,
                [side * 0.836, 0.65, -1.18],
                [side * 0.836, 0.65, 0.31],
                0.009,
            );
            self.slab(Steel, [side * 0.843, 1.01, 0.20], [0.018, 0.035, 0.11]);
            for z in [-1.10, -0.84, -0.58, -0.32, -0.06, 0.19] {
                self.sphere(Steel, [side * 0.84, 0.71, z], [0.012, 0.012, 0.012]);
            }
            // Pilot door frames and windshield wipers follow the chined nose.
            self.air_rod(
                AircraftPaint,
                [side * 0.56, 1.65, 1.31],
                [side * 0.63, 1.09, 2.06],
                0.039,
            );
            self.air_rod(
                AircraftPaint,
                [side * 0.73, 1.66, 0.68],
                [side * 0.79, 0.96, 0.68],
                0.035,
            );
            self.air_rod(
                Dark,
                [side * 0.26, 1.18, 2.10],
                [side * 0.36, 1.48, 1.73],
                0.010,
            );
            self.slab(Dark, [side * 0.745, 0.87, 0.76], [0.026, 0.07, 0.12]);
            self.slab(Steel, [side * 0.76, 1.01, 0.81], [0.02, 0.022, 0.09]);
            self.slab(Dark, [side * 0.70, 0.72, 1.0], [0.025, 0.15, 0.16]);
            // Continuous tubular skids, curved toes and paired canted cross-tubes.
            self.air_rod(
                Dark,
                [side * 0.98, 0.115, -1.59],
                [side * 0.98, 0.115, 1.55],
                0.068,
            );
            self.air_rod(
                Dark,
                [side * 0.98, 0.115, 1.55],
                [side * 0.98, 0.18, 1.83],
                0.068,
            );
            self.air_rod(
                Dark,
                [side * 0.98, 0.18, 1.83],
                [side * 0.98, 0.30, 2.00],
                0.068,
            );
            for z in [-0.95, 0.88] {
                self.air_rod(
                    Steel,
                    [0., 0.40, z - 0.13],
                    [side * 0.69, 0.34, z - 0.09],
                    0.061,
                );
                self.air_rod(
                    Steel,
                    [side * 0.69, 0.34, z - 0.09],
                    [side * 0.98, 0.15, z],
                    0.061,
                );
                self.air_rod(
                    AircraftPaint,
                    [side * 0.62, 0.53, z - 0.18],
                    [side * 0.86, 0.28, z - 0.04],
                    0.082,
                );
            }
            self.slab(Dark, [side * 0.85, 0.47, -0.36], [0.17, 0.04, 1.29]);
            // Faction stripes, rear threat-warning receivers and flare-dispenser grille.
            self.slab(Team, [side * 0.311, 1.20, -2.53], [0.012, 0.085, 0.35]);
            self.sphere(
                AircraftPaint,
                [side * 0.67, 1.24, -1.66],
                [0.18, 0.15, 0.20],
            );
            self.sphere(Dark, [side * 0.75, 1.24, -1.66], [0.065, 0.07, 0.075]);
            self.slab(Dark, [side * 0.52, 0.77, -1.77], [0.06, 0.17, 0.28]);
            for z in [-1.86, -1.77, -1.68] {
                self.slab(Steel, [side * 0.557, 0.77, z], [0.012, 0.12, 0.016]);
            }
        }
        self.add(
            Shape::Airframe,
            AircraftPaint,
            [0., 2.10, -0.47],
            [0.80, 0.69, 1.06],
        );
        self.slab(Sage, [0., 1.84, 0.42], [0.80, 0.045, 0.32]);
        self.air_rod(AircraftPaint, [0., 1.73, 1.34], [0., 1.16, 2.14], 0.036);
        // FLIR turret under the nose, with a ring-mounted primary lens and two apertures.
        self.sphere(AircraftPaint, [0., 0.43, 2.08], [0.34, 0.33, 0.34]);
        self.cylinder(Steel, [0., 0.43, 2.246], [0.19, 0.018, 0.19], along);
        self.cylinder(
            AircraftGlass,
            [0., 0.43, 2.26],
            [0.145, 0.020, 0.145],
            along,
        );
        for x in [-0.095, 0.095] {
            self.cylinder(Dark, [x, 0.35, 2.23], [0.045, 0.012, 0.045], along);
        }
        // Wire cutters, roof antennas, belly antenna and swept horizontal stabilizer.
        self.air_rod(Steel, [0., 1.82, 0.92], [0., 2.08, 1.19], 0.019);
        self.air_rod(Dark, [0., 2.08, 1.19], [0., 1.88, 1.00], 0.018);
        self.air_rod(Dark, [0., 0.38, 1.54], [0., 0.23, 1.70], 0.017);
        self.air_rod(Dark, [0., 1.43, -3.45], [0., 1.83, -3.40], 0.015);
        self.slab(AircraftPaint, [0., 2.22, -1.25], [0.10, 0.26, 0.21]);
        self.slab(Dark, [0., 0.31, -0.77], [0.04, 0.20, 0.16]);
        self.add(
            Shape::Airfoil,
            AircraftPaint,
            [0., 1.25, -3.76],
            [1.64, 0.044, 0.48],
        );
        // Four composite main blades, pitch links, root grips and a compact rotor hub.
        let pivot = [0., 2.70, -0.40];
        self.cylinder(
            Steel,
            [0., 2.53, -0.40],
            [0.085, 0.35, 0.085],
            Quat::IDENTITY,
        );
        self.cylinder(Dark, [0., 2.40, -0.40], [0.34, 0.075, 0.34], Quat::IDENTITY);
        for n in 0..4 {
            let angle = n as f32 * std::f32::consts::FRAC_PI_2 + 0.28;
            let q = Quat::from_rotation_y(angle);
            self.add(Shape::HueyRotor, Dark, pivot, [1.; 3]);
            self.rotate(q);
            self.joint(30, pivot);
            let a = Vec3::from_array(pivot) + q * Vec3::new(0.12, -0.03, 0.);
            let b = Vec3::from_array(pivot) + q * Vec3::new(0.48, 0.006, 0.);
            self.air_rod(Steel, a.to_array(), b.to_array(), 0.075);
            self.joint(30, pivot);
            let c = Vec3::from_array(pivot) + q * Vec3::new(0.10, -0.19, 0.05);
            let d = Vec3::from_array(pivot) + q * Vec3::new(0.35, -0.015, 0.06);
            self.air_rod(Steel, c.to_array(), d.to_array(), 0.015);
            self.joint(30, pivot);
        }
        self.sphere(Dark, pivot, [0.30, 0.12, 0.30]);
        self.joint(30, pivot);
        // Four individually modelled tail blades share the existing tail rotor joint.
        let tail = [-0.22, 2.55, -5.10];
        self.air_rod(Steel, [0., tail[1], tail[2]], tail, 0.095);
        for n in 0..4 {
            self.add(Shape::HueyTailRotor, Dark, tail, [1.; 3]);
            self.rotate(Quat::from_rotation_x(
                n as f32 * std::f32::consts::FRAC_PI_2 + 0.23,
            ));
            self.joint(33, tail);
        }
        self.sphere(Steel, tail, [0.10, 0.14, 0.14]);
        self.joint(33, tail);
        self.sphere(Red, [-0.145, 2.66, -5.16], [0.055, 0.055, 0.055]);
        self.sphere(Light, [0., 2.39, -1.05], [0.05, 0.05, 0.05]);
        self.sphere(Red, [-0.83, 1.71, -0.20], [0.035, 0.035, 0.035]);
        self.sphere(Light, [0.83, 1.71, -0.20], [0.035, 0.035, 0.035]);
    }
}
