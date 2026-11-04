// Included by models.rs. All dimensions are metres before the catalog scale.
impl Kit {
    fn military_building(&mut self, kind: &str) {
        use Finish::*;
        if kind == "airfield" {
            self.airfield();
            return;
        }
        if kind == "watchtower" {
            self.block(Concrete, [0., 0.12, 0.], [4., 0.24, 4.]);
            for x in [-1.3, 1.3] {
                for z in [-1.3, 1.3] {
                    self.block(Steel, [x, 2.7, z], [0.18, 5.4, 0.18]);
                }
            }
            for x in [-1.3,1.3] {
                for side in [-1.,1.] {
                    let a=Vec3::new(x,0.35,-side*1.3);
                    let b=Vec3::new(x,4.40,side*1.3);
                    let delta=b-a;
                    self.slab(Steel,((a+b)*0.5).to_array(),[0.075,delta.length(),0.075]);
                    self.rotate(Quat::from_rotation_arc(Vec3::Y,delta.normalize()));
                }
            }
            self.block(Steel, [0., 4.5, 0.], [3.5, 0.15, 3.5]);
            for side in [-1., 1.] {
                for (at, size, normal) in [
                    ([0., 5.05, side * 1.55], [3.2, 1., 0.1], Vec3::Z * side),
                    ([side * 1.55, 5.05, 0.], [0.1, 1., 3.2], Vec3::X * side),
                ] {
                    self.block(Cloth, at, size);
                    let part = self.0.last_mut().unwrap();
                    part.roof_lot = Some(usize::MAX);
                    part.wall_normal = Some(normal);
                }
            }
            self.windows(0., 5.6, 1.62, 3.);
            self.windows(0., 5.6, -1.62, 3.);
            self.add(Shape::Roof, Roof, [0., 5.85, 0.], [3.7, 0.45, 3.7]);
            self.0.last_mut().unwrap().roof_lot = Some(usize::MAX);
            for y in 0..15 {
                self.block(Steel, [0., y as f32 * 0.3 + 0.25, 1.5], [0.7, 0.045, 0.1]);
            }
            self.block(Steel, [-0.4, 2.3, 1.5], [0.07, 4.6, 0.07]);
            self.block(Steel, [0.4, 2.3, 1.5], [0.07, 4.6, 0.07]);
            return;
        }
        let (w, d, h) = match kind {
            "vehicleDepot" => (11.4, 10.4, 4.6),
            "fieldHospital" => (8.4, 7.4, 3.2),
            "mine" => (7.4, 6.4, 3.4),
            _ => (9.4, 6.4, 3.1),
        };
        self.block(Concrete, [0., 0.1, 0.], [w + 0.4, 0.2, d + 0.4]);
        self.facility_shell(kind, w, d, h, if kind == "mine" { Brick } else { Plaster });
        self.facility_roof(w, d, h);
        self.facility_details(w, d, h);
        // Buttresses, damp-proof plinth and a top cornice give facilities weight.
        for side in [-1., 1.] {
            let side_start = self.0.len();
            self.slab(
                Concrete,
                [side * (w * 0.5 - 0.04), 0.43, 0.],
                [0.26, 0.36, d],
            );
            self.slab(
                Concrete,
                [side * (w * 0.5 - 0.04), h - 0.10, 0.],
                [0.25, 0.18, d],
            );
            for part in &mut self.0[side_start..] {
                part.roof_lot = Some(usize::MAX);
                part.wall_normal = Some(Vec3::X * side);
            }
            for z in [-d * 0.42, 0., d * 0.42] {
                let start = self.0.len();
                self.slab(
                    Concrete,
                    [side * (w * 0.5 + 0.06), h * 0.5, z],
                    [0.22, h - 0.20, 0.24],
                );
                for part in &mut self.0[start..] {
                    part.roof_lot = Some(usize::MAX);
                    part.wall_normal = Some(Vec3::X * side);
                }
            }
        }
        let garage = kind == "vehicleDepot";
        for x in if garage {
            vec![-2.8, 2.8]
        } else {
            vec![-w * 0.3]
        } {
            let width = if garage { 4.2 } else { 1.1 };
            let height = if garage { 3.6 } else { 2.2 };
            let start = self.0.len();
            self.door_fixture(x, d / 2., width, height, garage);
            for part in &mut self.0[start..] {
                part.roof_lot = Some(usize::MAX);
                part.wall_normal = Some(Vec3::Z);
            }
            self.block(
                Concrete,
                [x, 0.08, d / 2. + 0.25],
                [width + 0.3, 0.16, 0.45],
            );
        }
        for z in [-d / 2. - 0.045, d / 2. + 0.045] {
            if !garage || z < 0. {
                self.windows(w * 0.16, 1.7, z, w * 0.65);
            }
            self.block(Concrete, [0., h - 0.2, z], [w, 0.12, 0.12]);
            let part = self.0.last_mut().unwrap();
            part.roof_lot = Some(usize::MAX);
            part.wall_normal = Some(Vec3::Z * z.signum());
        }
        for x in [-w / 2. - 0.045, w / 2. + 0.045] {
            let start = self.0.len();
            for z in [-d * 0.28, 0., d * 0.28] {
                self.block(Dark, [x, 1.7, z], [0.06, 0.9, 1.1]);
                self.block(Glass, [x * 1.001, 1.7, z], [0.07, 0.75, 0.95]);
            }
            for part in &mut self.0[start..] {
                part.roof_lot = Some(usize::MAX);
                part.wall_normal = Some(Vec3::X * x.signum());
            }
            self.cylinder(
                Steel,
                [x, h / 2., -d * 0.44],
                [0.075, h, 0.075],
                Quat::IDENTITY,
            );
        }
        self.block(Team, [0., h - 0.55, d / 2. + 0.12], [2.2, 0.4, 0.08]);
        let part = self.0.last_mut().unwrap();
        part.roof_lot = Some(usize::MAX);
        part.wall_normal = Some(Vec3::Z);
        self.block(Steel, [w * 0.3, h + 0.45, -d * 0.25], [1.2, 0.55, 0.8]);
        self.cylinder(
            Steel,
            [w * 0.35, h + 1., -d * 0.2],
            [0.12, 1.7, 0.12],
            Quat::IDENTITY,
        );
        if kind == "fieldHospital" {
            let start = self.0.len();
            let rise = (w * 0.17).clamp(0.65, 1.15);
            let slope = (rise / (w * 0.5 + 0.28)).atan();
            for (finish, size, dy) in [
                (White, [2.4, 0.025, 2.4], 0.),
                (Red, [0.38, 0.025, 1.8], 0.025),
                (Red, [1.8, 0.025, 0.38], 0.028),
            ] {
                self.slab(finish, [w * 0.25, h + rise * 0.5 + 0.09 + dy, 0.], size);
                self.rotate(Quat::from_rotation_z(-slope));
            }
            for part in &mut self.0[start..] {
                part.roof_lot = Some(usize::MAX);
            }
            let start = self.0.len();
            for (finish, size, z) in [
                (White, [0.9, 0.9, 0.035], d * 0.5 + 0.12),
                (Red, [0.18, 0.64, 0.04], d * 0.5 + 0.15),
                (Red, [0.64, 0.18, 0.04], d * 0.5 + 0.16),
            ] {
                self.slab(finish, [w * 0.30, h - 0.55, z], size);
            }
            for part in &mut self.0[start..] {
                part.roof_lot = Some(usize::MAX);
                part.wall_normal = Some(Vec3::Z);
            }
        } else if kind == "mine" {
            // Industrial extraction plant: braced headframe and ore bins.
            for x in [-2.1, 2.1] {
                self.block(Steel, [x, 3.8, -1.], [0.22, 7.6, 0.22]);
                self.block(Steel, [x, 4., -1.], [0.12, 4.8, 0.15]);
                self.rotate(Quat::from_rotation_z(x.signum() * 0.5));
            }
            self.block(Steel, [0., 7.4, -1.], [4.5, 0.3, 2.]);
            self.cylinder(
                Dark,
                [0., 7.6, -1.],
                [1.4, 0.22, 1.4],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
        }
    }
    fn airfield(&mut self) {
        use crate::airfield_layout as layout;
        use Finish::*;
        // A 20%-length military runway, with separate service and flight areas.
        self.block(
            Gravel,
            [
                layout::FOUNDATION_CENTER[0],
                0.045,
                layout::FOUNDATION_CENTER[1],
            ],
            [layout::FOUNDATION_SIZE[0], 0.09, layout::FOUNDATION_SIZE[1]],
        );
        self.block(
            Asphalt,
            [30., 0.135, 0.],
            [layout::RUNWAY_WIDTH, 0.035, layout::RUNWAY_LENGTH],
        );
        self.block(Concrete, [-12., 0.12, 4.], [40., 0.04, 180.]);
        self.block(Asphalt, [70., 0.135, 0.], [24., 0.035, 334.]);
        for z in [-162., 162.] {
            self.block(Asphalt, [50., 0.135, z], [54., 0.035, 8.]);
            self.block(
                Asphalt,
                [30., 0.135, z.signum() * 157.],
                [layout::RUNWAY_WIDTH, 0.035, 21.],
            );
        }
        for [x, z] in layout::PLANE_STANDS {
            self.block(White, [x, 0.18, z], [0.12, 0.02, 18.]);
            self.block(White, [x, 0.18, z + 9.], [18., 0.02, 0.12]);
            for side in [-1., 1.] {
                self.block(White, [x + side * 10., 0.18, z], [0.1, 0.02, 23.]);
                self.block(Concrete, [x + side * 1.8, 0.24, z - 3.], [0.6, 0.16, 0.22]);
            }
        }
        for [x, z] in layout::HELI_STANDS {
            self.block(Asphalt, [x, 0.135, z], [18., 0.035, 18.]);
            for side in [-1., 1.] {
                self.block(White, [x + side * 1.8, 0.18, z], [0.3, 0.02, 5.]);
            }
            self.block(White, [x, 0.18, z], [3.6, 0.02, 0.3]);
            for n in 0..32 {
                let angle = n as f32 * std::f32::consts::TAU / 32.;
                self.block(
                    White,
                    [x + angle.sin() * 7., 0.18, z + angle.cos() * 7.],
                    [1.2, 0.02, 0.14],
                );
                self.rotate(Quat::from_rotation_y(angle));
            }
        }
        for z in (-156..=156).step_by(6) {
            self.block(Team, [70., 0.17, z as f32], [0.13, 0.025, 2.4]);
        }
        for z in (-135..=135).step_by(10) {
            self.block(White, [30., 0.17, z as f32], [0.22, 0.02, 4.]);
        }
        for z in [-143., 143.] {
            for x in [-3., -2., -1., 1., 2., 3.] {
                self.block(White, [30. + x, 0.17, z], [0.5, 0.02, 7.]);
            }
        }
        for x in [
            30. - layout::RUNWAY_WIDTH * 0.47,
            30. + layout::RUNWAY_WIDTH * 0.47,
        ] {
            self.block(
                White,
                [x, 0.17, 0.],
                [0.1, 0.02, layout::RUNWAY_LENGTH - 2.],
            );
            for z in (-144..=144).step_by(12) {
                self.cylinder(
                    Steel,
                    [x, 0.20, z as f32],
                    [0.15, 0.10, 0.15],
                    Quat::IDENTITY,
                );
                self.sphere(Light, [x, 0.27, z as f32], [0.14, 0.08, 0.14]);
            }
        }
        let hangar_start = self.0.len();
        // Open-front steel hangar; the interior remains visible from the apron.
        for x in [-11.5, -2.5] {
            self.slab(Concrete, [x, 2.4, 8.], [0.25, 4.8, 12.]);
        }
        self.slab(Concrete, [-7., 2.4, 2.], [9., 4.8, 0.25]);
        let start = self.0.len();
        self.facility_roof(9., 12., 4.8);
        for part in &mut self.0[start..] {
            part.transform.translation += Vec3::new(-7., 0., 8.);
        }
        for z in [2., 5., 8., 11., 14.] {
            for side in [-1., 1.] {
                self.slab(Steel, [-7. + side * 4.45, 2.4, z], [0.13, 4.8, 0.16]);
            }
        }
        self.block(Steel, [-7., 4.5, 14.], [9., 0.55, 0.3]);
        self.block(Steel, [-11., 2., 14.], [0.6, 4., 0.3]);
        self.block(Steel, [-3., 2., 14.], [0.6, 4., 0.3]);
        self.block(Team, [-7., 4.5, 14.18], [2.6, 0.32, 0.025]);
        for part in &mut self.0[hangar_start..] {
            part.transform.translation = (part.transform.translation - Vec3::new(-7., 0., 8.))
                * layout::HANGAR_SCALE
                + Vec3::new(layout::HANGAR_AT[0], 0., layout::HANGAR_AT[1]);
            part.transform.scale *= layout::HANGAR_SCALE;
        }
        let office_assembly_start = self.0.len();
        let office_start = self.0.len();
        self.block(Plaster, [-10.4, 1.6, -9.], [0.2, 2.8, 8.]);
        self.block(Plaster, [-3.6, 1.6, -9.], [0.2, 2.8, 8.]);
        self.block(Plaster, [-7., 1.6, -12.9], [7., 2.8, 0.2]);
        self.block(Plaster, [-8.975, 1.6, -5.1], [3.05, 2.8, 0.2]);
        self.block(Plaster, [-5.025, 1.6, -5.1], [3.05, 2.8, 0.2]);
        self.block(Plaster, [-7., 2.7, -5.1], [0.9, 0.6, 0.2]);
        self.door_fixture(-7., -5., 0.9, 2.2, false);
        for part in &mut self.0[office_start..] {
            part.roof_lot = Some(usize::MAX);
            let at = part.transform.translation;
            part.wall_normal = Some(if at.x < -10. {
                -Vec3::X
            } else if at.x > -4. {
                Vec3::X
            } else if at.z < -12. {
                -Vec3::Z
            } else {
                Vec3::Z
            });
        }
        self.block(Steel, [-7., 3.08, -9.], [7.4, 0.16, 8.4]);
        self.0.last_mut().unwrap().roof_lot = Some(usize::MAX);
        self.block(Concrete, [-7., 4.7, -10.], [3.6, 3.2, 3.8]);
        self.block(Glass, [-7., 5.5, -10.], [3.8, 0.9, 4.]);
        self.block(Steel, [-7., 6.08, -10.], [4.1, 0.15, 4.3]);
        self.cylinder(Steel, [-7., 7.1, -10.], [0.07, 2., 0.07], Quat::IDENTITY);
        for x in [-9., -5.] {
            let start = self.0.len();
            self.windows(x, 1.8, -4.96, 2.7);
            for part in &mut self.0[start..] {
                part.wall_normal = Some(Vec3::Z);
            }
        }
        for z in [-18., -16.] {
            self.cylinder(
                Cloth,
                [-8., 0.8, z],
                [1.8, 4., 1.8],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
            for x in [-9.5, -6.5] {
                self.block(Concrete, [x, 0.28, z], [0.25, 0.55, 2.2]);
            }
        }
        self.cylinder(Steel, [-1., 3., -19.], [0.08, 6., 0.08], Quat::IDENTITY);
        for i in 0..5 {
            self.cylinder(
                if i % 2 == 0 { Red } else { White },
                [-0.75 + i as f32 * 0.28, 5.65 - i as f32 * 0.05, -19.],
                [0.5 - i as f32 * 0.06, 0.3, 0.5 - i as f32 * 0.06],
                Quat::from_rotation_z(1.4),
            );
        }
        for part in &mut self.0[office_assembly_start..] {
            part.transform.translation = (part.transform.translation - Vec3::new(-7., 0., -9.))
                * layout::OFFICE_SCALE
                + Vec3::new(layout::OFFICE_AT[0], 0., layout::OFFICE_AT[1]);
            part.transform.scale *= layout::OFFICE_SCALE;
        }
    }
}
