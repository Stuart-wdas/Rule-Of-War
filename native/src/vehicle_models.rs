// Articulated military vehicles. Canonical dimensions retain gameplay footprints.
impl Kit {
    fn vehicle_wheel(&mut self, x: f32, z: f32, radius: f32, width: f32, height: f32) {
        use Finish::*;
        let side = x.signum();
        let pivot = [x, height, z];
        for (finish, diameter, depth, offset) in [
            (Rubber, radius * 2., width, 0.),
            (Steel, radius * 1.25, 0.035, side * (width * 0.5 + 0.01)),
            (Team, radius * 0.48, 0.045, side * (width * 0.5 + 0.03)),
        ] {
            self.cylinder(
                finish,
                [x + offset, pivot[1], z],
                [diameter, depth, diameter],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
            self.joint(22, pivot);
            if finish == Rubber && radius > 0.25 {
                self.0.last_mut().unwrap().shape = Shape::Tyre;
            }
        }
        for n in 0..6 {
            let angle = n as f32 * std::f32::consts::TAU / 6.;
            self.block(
                Finish::Dark,
                [
                    x + side * (width * 0.5 + 0.035),
                    pivot[1] + angle.sin() * radius * 0.4,
                    z + angle.cos() * radius * 0.4,
                ],
                [0.012, 0.025, 0.025],
            );
            self.joint(22, pivot);
        }
    }
    fn track_run(&mut self, side: f32) {
        use Finish::*;
        let x = side * 0.82;
        self.slab(Dark, [x, 0.36, 0.], [0.25, 0.38, 2.68]);
        // Closed stadium-shaped shoes wrap the sprockets and return rollers.
        let mut path = vec![];
        for i in 0..19 {
            path.push(Vec3::new(x, 0.60, -1.17 + i as f32 * 2.34 / 18.));
        }
        for i in 1..=8 {
            let a = i as f32 * std::f32::consts::PI / 8.;
            path.push(Vec3::new(x, 0.36 + 0.24 * a.cos(), 1.17 + 0.24 * a.sin()));
        }
        for i in 1..=18 {
            path.push(Vec3::new(x, 0.12, 1.17 - i as f32 * 2.34 / 18.));
        }
        for i in 1..=8 {
            let a = i as f32 * std::f32::consts::PI / 8.;
            path.push(Vec3::new(x, 0.36 - 0.24 * a.cos(), -1.17 - 0.24 * a.sin()));
        }
        for pair in path.windows(2) {
            let delta = pair[1] - pair[0];
            self.slab(
                Steel,
                ((pair[0] + pair[1]) * 0.5).to_array(),
                [0.34, 0.035, delta.length() * 0.92],
            );
            self.rotate(Quat::from_rotation_arc(Vec3::Z, delta.normalize()));
            self.slab(
                Rubber,
                ((pair[0] + pair[1]) * 0.5 + Vec3::new(side * 0.015, 0., 0.)).to_array(),
                [0.23, 0.045, delta.length() * 0.65],
            );
            self.rotate(Quat::from_rotation_arc(Vec3::Z, delta.normalize()));
        }
        for z in [-1.12, -0.67, -0.22, 0.23, 0.68, 1.13] {
            self.vehicle_wheel(x, z, 0.22, 0.30, 0.36);
        }
        for z in [-0.82, 0., 0.82] {
            self.cylinder(
                Rubber,
                [x, 0.53, z],
                [0.16, 0.28, 0.16],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        }
        for i in 0..5 {
            self.slab(
                Team,
                [side * 0.87, 0.75, -1.12 + i as f32 * 0.55],
                [0.10, 0.30, 0.50],
            );
        }
    }
    fn ground_vehicle(&mut self, kind: &str) {
        use Finish::*;
        let wheeled = matches!(kind, "troopCarrier" | "xtralong");
        self.add(Shape::Hull, Team, [0., 0.66, 0.], [1.52, 0.52, 2.85]);
        self.slab(Dark, [0., 0.41, 0.], [1.26, 0.16, 2.72]);
        self.vehicle_deck_details(wheeled);
        // Tow eyes, glacis seams and rubber mud flaps give the hull a built,
        // layered silhouette. Everything remains in the existing rigid batches.
        for side in [-1., 1.] {
            self.slab(Rubber, [side * 0.79, 0.37, -1.39], [0.33, 0.40, 0.045]);
            self.slab(Steel, [side * 0.56, 0.58, 1.46], [0.16, 0.07, 0.12]);
            self.slab(Dark, [side * 0.64, 0.85, 0.23], [0.025, 0.035, 1.56]);
            for z in [-1.03, -0.48, 0.07, 0.62] {
                self.slab(Team, [side * 0.91, 0.78, z], [0.10, 0.28, 0.48]);
                for dz in [-0.17, 0.17] {
                    self.sphere(Steel, [side * 0.967, 0.85, z + dz], [0.026, 0.026, 0.026]);
                    self.0.last_mut().unwrap().shape=Shape::AircraftDetailSphere;
                }
            }
        }
        for side in [-1., 1.] {
            if wheeled {
                for z in [-1.08, -0.35, 1.02] {
                    self.vehicle_wheel(side * 0.81, z, 0.29, 0.28, 0.33);
                }
                for z in [-0.72, 1.02] {
                    self.slab(
                        Team,
                        [side * 0.81, 0.76, z],
                        [0.37, 0.08, if z < 0. { 1.55 } else { 0.78 }],
                    );
                }
            } else {
                self.track_run(side);
            }
            self.block(Light, [side * 0.53, 0.76, 1.45], [0.13, 0.07, 0.035]);
            self.slab(Dark, [side * 0.53, 0.80, 1.46], [0.16, 0.025, 0.08]);
            self.block(Red, [side * 0.56, 0.65, -1.43], [0.065, 0.06, 0.035]);
            self.cylinder(
                Steel,
                [side * 0.51, 0.48, 1.46],
                [0.075, 0.16, 0.075],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        }
        if wheeled {
            self.armoured_cab();
        } else {
            self.slab(Team, [0., 0.86, 1.02], [1.36, 0.11, 0.70]);
            self.rotate(Quat::from_rotation_x(0.22));
            for x in [-0.52, 0.52] {
                self.slab(Steel, [x, 0.90, 0.95], [0.22, 0.035, 0.24]);
                self.cylinder(Team, [x, 0.94, 0.95], [0.12, 0.025, 0.12], Quat::IDENTITY);
            }
            for i in 0..8 {
                self.slab(
                    Dark,
                    [-0.39 + i as f32 * 0.11, 0.94, -1.04],
                    [0.055, 0.012, 0.46],
                );
            }
            // Spare track shoes, hinge pins and retaining straps break up the
            // broad hull sides without changing the vehicle's gameplay bounds.
            for side in [-1.,1.] {
                for z in [-0.58,-0.35,-0.12] {
                    self.slab(Steel,[side*0.686,0.98,z],[0.035,0.19,0.19]);
                    self.slab(Rubber,[side*0.710,0.98,z],[0.014,0.105,0.12]);
                    self.slab(Dark,[side*0.718,0.98,z],[0.012,0.035,0.18]);
                }
                for z in [-0.72,0.52] {
                    self.slab(Dark,[side*0.48,0.965,z],[0.22,0.012,0.018]);
                    self.slab(Steel,[side*0.48,0.987,z],[0.17,0.018,0.018]);
                }
            }
            for side in [-1., 1.] {
                self.block(Cloth, [side * 0.49, 0.93, -0.78], [0.30, 0.16, 0.38]);
                self.cylinder(
                    Steel,
                    [side * 0.59, 0.96, -1.12],
                    [0.09, 0.45, 0.09],
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
            }
            // Rear cargo basket with rolled equipment and tie-downs.
            self.slab(Dark, [0., 1.02, -1.28], [1.13, 0.06, 0.27]);
            for x in [-0.52, 0., 0.52] {
                self.slab(Steel, [x, 1.12, -1.40], [0.025, 0.25, 0.025]);
            }
            self.slab(Steel, [0., 1.24, -1.40], [1.10, 0.025, 0.025]);
            self.cylinder(Cloth, [0., 1.12, -1.27], [0.19, 0.78, 0.19], Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
            for x in [-0.24, 0.24] {
                self.slab(Dark, [x, 1.22, -1.27], [0.035, 0.025, 0.20]);
            }
        }
        match kind {
            "troopCarrier" => self.carrier_body(),
            "xtralong" => self.rocket_rack(),
            _ => self.combat_turret(kind),
        }
        self.cylinder(
            Dark,
            [0.56, 1.35, -1.10],
            [0.018, 1.2, 0.018],
            Quat::IDENTITY,
        );
        self.cylinder(
            Steel,
            [-0.52, 0.66, -1.46],
            [0.10, 0.18, 0.10],
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
    }
    fn armoured_cab(&mut self) {
        use Finish::*;
        self.add(Shape::CarrierCabin, Team, [0., 1.10, 0.93], [1.34, 0.60, 0.91]);
        for side in [-1., 1.] {
            self.slab(Glass, [side * 0.31, 1.17, 1.36], [0.49, 0.27, 0.035]);
            self.rotate(Quat::from_rotation_x(-0.24));
            self.slab(Steel, [side * 0.31, 1.315, 1.335], [0.56, 0.035, 0.05]);
            self.slab(Glass, [side * 0.68, 1.14, 0.93], [0.018, 0.25, 0.45]);
            self.slab(Dark, [side * 0.695, 0.99, 0.72], [0.028, 0.025, 0.13]);
            self.slab(Steel, [side * 0.74, 0.65, 0.67], [0.25, 0.045, 0.38]);
            self.slab(Dark, [side * 0.76, 1.13, 1.21], [0.09, 0.15, 0.035]);
        }
        self.slab(Team, [0., 1.40, 0.88], [1.35, 0.065, 0.75]);
        self.slab(Steel, [0., 0.52, 1.53], [1.48, 0.13, 0.10]);
        for i in 0..6 {
            self.slab(
                Dark,
                [-0.30 + i as f32 * 0.12, 0.85, 1.43],
                [0.055, 0.15, 0.025],
            );
        }
    }
    fn carrier_body(&mut self) {
        use Finish::*;
        self.add(Shape::CarrierCabin, Team, [0., 1.04, -0.47], [1.38, 0.78, 1.78]);
        self.slab(Team, [0., 1.42, -0.48], [1.27, 0.065, 1.64]);
        for z in [-0.95, -0.18] {
            self.slab(Dark, [0., 1.46, z], [0.56, 0.025, 0.48]);
            self.slab(Team, [0., 1.49, z], [0.49, 0.04, 0.41]);
        }
        for side in [-1., 1.] {
            for z in [-0.98, -0.49, 0.] {
                self.slab(Dark, [side * 0.701, 1.20, z], [0.025, 0.13, 0.24]);
                self.slab(Glass, [side * 0.717, 1.21, z], [0.012, 0.07, 0.17]);
            }
            self.slab(Team, [side * 0.70, 0.98, -0.47], [0.035, 0.16, 1.42]);
        }
        for x in [-0.32, 0.32] {
            self.slab(Team, [x, 1.05, -1.38], [0.58, 0.63, 0.045]);
            self.slab(Dark, [x * 0.35, 1.05, -1.415], [0.025, 0.17, 0.025]);
        }
        self.carrier_fittings();
    }
    fn rocket_rack(&mut self) {
        use Finish::*;
        self.slab(Steel, [0., 0.92, -0.68], [1.38, 0.14, 1.38]);
        let start = self.0.len();
        self.cylinder(Dark, [0., 1.06, -0.65], [0.95, 0.14, 0.95], Quat::IDENTITY);
        self.slab(Team, [0., 1.18, -0.63], [1.20, 0.25, 0.90]);
        for x in [-0.45, -0.15, 0.15, 0.45] {
            for y in [1.38, 1.69] {
                self.cylinder(
                    Team,
                    [x, y, -0.30],
                    [0.25, 1.85, 0.25],
                    Quat::from_rotation_x(1.15),
                );
                self.cylinder(
                    Steel,
                    [x, y + 0.365, 0.515],
                    [0.27, 0.07, 0.27],
                    Quat::from_rotation_x(1.15),
                );
                self.cylinder(
                    Dark,
                    [x, y + 0.385, 0.56],
                    [0.19, 0.012, 0.19],
                    Quat::from_rotation_x(1.15),
                );
                self.socket(Vec3::Y * 0.5);
            }
        }
        for part in &mut self.0[start..] {
            part.joint = 20;
            part.pivot = Vec3::new(0., 1.05, -0.65);
        }
    }
    fn combat_turret(&mut self, kind: &str) {
        use Finish::*;
        let pivot = Vec3::new(0., 1.06, -0.15);
        let start = self.0.len();
        self.cylinder(Dark, [0., 1.00, -0.15], [1.09, 0.09, 1.09], Quat::IDENTITY);
        self.add(Shape::ArmourTurret, Team, [0., 1.26, -0.16], [1.25, 0.50, 1.30]);
        self.turret_fittings(kind == "long");
        for side in [-1., 1.] {
            self.slab(Team, [side * 0.53, 1.28, 0.18], [0.20, 0.22, 0.63]);
            self.rotate(Quat::from_rotation_y(side * 0.21));
            for z in [-0.46, -0.16, 0.13] {
                self.slab(Team, [side * 0.65, 1.28, z], [0.085, 0.18, 0.24]);
            }
            for i in 0..3 {
                self.cylinder(
                    Steel,
                    [side * 0.63, 1.42, 0.25 - i as f32 * 0.11],
                    [0.065, 0.18, 0.065],
                    Quat::from_rotation_x(0.60),
                );
            }
        }
        self.slab(Dark, [0., 1.22, -0.91], [1.02, 0.25, 0.035]);
        self.slab(Cloth, [0., 1.35, -0.82], [0.73, 0.15, 0.20]);
        self.cylinder(
            Steel,
            [0.26, 1.53, -0.30],
            [0.36, 0.07, 0.36],
            Quat::IDENTITY,
        );
        self.cylinder(
            Team,
            [0.26, 1.58, -0.30],
            [0.30, 0.04, 0.30],
            Quat::IDENTITY,
        );
        self.slab(Dark, [-0.30, 1.52, 0.08], [0.19, 0.12, 0.19]);
        self.slab(Glass, [-0.30, 1.54, 0.183], [0.14, 0.065, 0.012]);
        if kind != "antiAir" {
            // Cupola-mounted machine gun, its cradle and a small ammunition box.
            self.slab(Dark, [0.27, 1.68, -0.30], [0.06, 0.20, 0.06]);
            self.slab(Steel, [0.27, 1.79, -0.10], [0.08, 0.08, 0.34]);
            self.cylinder(Dark, [0.27, 1.79, 0.23], [0.026, 0.38, 0.026], Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
            self.slab(Team, [0.40, 1.76, -0.18], [0.14, 0.13, 0.17]);
        }
        if kind == "antiAir" {
            self.cylinder(Steel, [0., 1.94, -0.46], [0.05, 0.67, 0.05], Quat::IDENTITY);
            self.sphere(Steel, [0., 2.24, -0.46], [0.80, 0.49, 0.13]);
            self.sphere(Dark, [0., 2.24, -0.38], [0.68, 0.38, 0.04]);
        }
        if kind == "long" {
            self.slab(Team, [0., 1.27, -0.73], [1.08, 0.32, 0.33]);
            self.cylinder(
                Dark,
                [-0.47, 1.44, -0.53],
                [0.08, 0.42, 0.08],
                Quat::from_rotation_z(0.5),
            );
        }
        for part in &mut self.0[start..] {
            part.joint = 20;
            part.pivot = pivot;
        }
        let start = self.0.len();
        let length = if kind == "long" {
            2.65
        } else if kind == "antiAir" {
            1.65
        } else {
            1.95
        };
        for x in if kind == "antiAir" {
            vec![-0.17, 0.17]
        } else {
            vec![0.]
        } {
            let y = 1.31;
            self.cylinder(
                Team,
                [x, y, 0.57],
                [0.29, 0.29, 0.29],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            self.cylinder(
                Steel,
                [x, y, 0.64 + length * 0.5],
                [0.085, length, 0.085],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            if kind != "antiAir" {
                self.cylinder(
                    Team,
                    [x, y, 0.90],
                    [0.135, 0.47, 0.135],
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
                self.cylinder(
                    Steel,
                    [x, y, 1.26],
                    [0.16, 0.24, 0.16],
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
            }
            self.cylinder(
                Steel,
                [x, y, 0.64 + length],
                [0.14, 0.11, 0.14],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            self.cylinder(
                Dark,
                [x, y, 0.705 + length],
                [0.085, 0.015, 0.085],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            self.socket(Vec3::Y * 0.5);
        }
        for part in &mut self.0[start..] {
            part.joint = 21;
            part.pivot = pivot;
        }
    }
}
