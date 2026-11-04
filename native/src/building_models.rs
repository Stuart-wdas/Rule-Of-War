// Included in models.rs. Wall segments contain real door and window openings.
impl Kit {
    /// Persistent, batched debris follows the floor plan, leaving courtyards open.
    fn rubble_pile(
        &mut self,
        wings: &[crate::settlement_layout::Wing],
        hash: u32,
        wall: Finish,
        roof: Finish,
    ) {
        for (wing_index, wing) in wings.iter().enumerate() {
            let wing_hash = hash.wrapping_add(wing_index as u32 * 971);
            // Low overlapping wedges give the debris weight beneath individual fragments.
            for i in 0..3 {
                let n = crate::destruction::variation(wing_hash, i + 900);
                self.add(
                    Shape::Roof,
                    Finish::Gravel,
                    [
                        wing.at[0] + (n - 0.5) * wing.size[0] * 0.25,
                        0.17,
                        wing.at[1] + (0.5 - n) * wing.size[1] * 0.25,
                    ],
                    [wing.size[0] * 0.72, 0.45 + n * 0.25, wing.size[1] * 0.72],
                );
                self.rotate(Quat::from_rotation_y((n - 0.5) * 0.3));
            }
            let count = (wing.size[0] * wing.size[1] * 1.5).ceil().clamp(24., 112.) as usize;
            for i in 0..count {
                let n = crate::destruction::variation(wing_hash, i * 4);
                let u = crate::destruction::variation(wing_hash, i * 4 + 1) * 2. - 1.;
                let v = crate::destruction::variation(wing_hash, i * 4 + 2) * 2. - 1.;
                let mound = (1. - u * u) * (1. - v * v);
                let (finish, size) = if i % 11 == 0 {
                    (
                        if i % 22 == 0 {
                            Finish::Charred
                        } else {
                            Finish::Wood
                        },
                        [0.12 + n * 0.08, 0.13, 1.1 + n * 1.2],
                    )
                } else if i % 5 == 0 {
                    (roof, [0.65 + n * 0.6, 0.10, 0.6 + n * 0.65])
                } else if i % 9 == 0 {
                    (
                        Finish::Concrete,
                        [0.8 + n * 0.75, 0.16 + n * 0.12, 0.65 + n * 0.55],
                    )
                } else {
                    (
                        if i % 7 == 0 { Finish::Concrete } else { wall },
                        [0.3 + n * 0.5, 0.16 + n * 0.22, 0.28 + n * 0.55],
                    )
                };
                self.add(
                    if i % 4 == 0 && i % 5 != 0 {
                        Shape::Roof
                    } else {
                        Shape::Box
                    },
                    finish,
                    [
                        wing.at[0] + u * wing.size[0] * 0.46,
                        0.13 + mound * (0.22 + n * 0.25) + size[1] * 0.5,
                        wing.at[1] + v * wing.size[1] * 0.46,
                    ],
                    size,
                );
                self.rotate(Quat::from_euler(
                    EulerRot::XYZ,
                    (n - 0.5) * 0.65,
                    n * std::f32::consts::TAU,
                    (u - v) * 0.2,
                ));
            }
        }
    }
    fn facade(
        &mut self,
        wall: Finish,
        span: f32,
        height: f32,
        plane: f32,
        side: bool,
        holes: &[[f32; 4]],
    ) {
        let mut xs = vec![-span * 0.5, span * 0.5];
        let mut ys = vec![0.2, height];
        for hole in holes {
            xs.extend([
                hole[0].clamp(-span * 0.5, span * 0.5),
                hole[1].clamp(-span * 0.5, span * 0.5),
            ]);
            ys.extend([hole[2].clamp(0.2, height), hole[3].clamp(0.2, height)]);
        }
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        for x in xs.windows(2) {
            for y in ys.windows(2) {
                let (u, v) = ((x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5);
                if holes
                    .iter()
                    .any(|h| u > h[0] && u < h[1] && v > h[2] && v < h[3])
                {
                    continue;
                }
                if x[1] - x[0] < 0.01 || y[1] - y[0] < 0.01 {
                    continue;
                }
                self.slab(
                    wall,
                    if side { [plane, v, u] } else { [u, v, plane] },
                    if side {
                        [0.22, y[1] - y[0], x[1] - x[0]]
                    } else {
                        [x[1] - x[0], y[1] - y[0], 0.22]
                    },
                );
                // Keep each outward-facing wall separate for the occupied-house cutaway.
                let part = self.0.last_mut().unwrap();
                part.roof_lot = Some(0);
                part.wall_normal = Some(if side {
                    Vec3::new(plane.signum(), 0., 0.)
                } else {
                    Vec3::new(0., 0., plane.signum())
                });
            }
        }
    }
    fn door_fixture(&mut self, x: f32, z: f32, width: f32, height: f32, garage: bool) {
        let finish = if garage { Finish::Steel } else { Finish::Wood };
        for side in [-1., 1.] {
            self.block(
                Finish::Concrete,
                [
                    x + side * (width * 0.5 + 0.08),
                    0.2 + height * 0.5,
                    z + 0.04,
                ],
                [0.16, height, 0.28],
            );
        }
        self.block(
            Finish::Concrete,
            [x, 0.2 + height + 0.07, z + 0.04],
            [width + 0.32, 0.14, 0.28],
        );
        self.block(
            Finish::Concrete,
            [x, 0.13, z + 0.3],
            [width + 0.4, 0.26, 0.85],
        );
        // Small wall fittings add believable human scale to otherwise bare walls.
        let service = x + width * 0.5 + 0.43;
        self.block(Finish::Dark, [service, 1.68, z + 0.16], [0.19, 0.29, 0.12]);
        self.block(Finish::Steel, [service, 1.68, z + 0.23], [0.13, 0.21, 0.03]);
        self.cylinder(
            Finish::Steel,
            [service, 1.04, z + 0.17],
            [0.022, 1.02, 0.022],
            Quat::IDENTITY,
        );
        self.block(
            Finish::Dark,
            [x - width * 0.5 - 0.27, 2.13, z + 0.22],
            [0.17, 0.22, 0.21],
        );
        self.block(
            Finish::Light,
            [x - width * 0.5 - 0.27, 2.11, z + 0.34],
            [0.10, 0.12, 0.015],
        );
        if garage {
            // Raised roller door, leaving the bay open all the way to its floor.
            self.cylinder(
                finish,
                [x, height + 0.42, z + 0.12],
                [0.32, width + 0.25, 0.32],
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        } else {
            let angle = -1.38f32;
            let leaf = width - 0.08;
            let at = Vec3::new(x - width * 0.5, 0.2 + height * 0.5, z)
                + Quat::from_rotation_y(angle) * Vec3::new(leaf * 0.5, 0., 0.);
            self.block(finish, at.to_array(), [leaf, height - 0.08, 0.065]);
            self.rotate(Quat::from_rotation_y(angle));
            for y in [0.65, 1.35] {
                let panel = at
                    + Vec3::new(0., y - (0.2 + height * 0.5), 0.)
                    + Quat::from_rotation_y(angle) * Vec3::new(0., 0., 0.04);
                self.block(Finish::Dark, panel.to_array(), [leaf * 0.7, 0.45, 0.025]);
                self.rotate(Quat::from_rotation_y(angle));
            }
            let handle = Vec3::new(x - width * 0.5, 1.12, z)
                + Quat::from_rotation_y(angle) * Vec3::new(leaf * 0.83, 0., 0.09);
            self.sphere(Finish::Brass, handle.to_array(), [0.085, 0.085, 0.085]);
        }
    }
    fn detailed_house(&mut self, lot: &crate::settlement_layout::Lot, wall: Finish) {
        let h = crate::settlement_layout::HOUSE_HEIGHT;
        let local = crate::settlement_layout::Lot {
            at: [0., 0.],
            yaw: 0.,
            ..lot.clone()
        };
        let windows = local.windows();
        let (door, _) = local.door();
        for wing in local.wings() {
            self.slab(
                Finish::Concrete,
                [wing.at[0], 0.09, wing.at[1]],
                [wing.size[0] + 0.24, 0.18, wing.size[1] + 0.24],
            );
            self.slab(
                Finish::Wood,
                [wing.at[0], 0.23, wing.at[1]],
                [wing.size[0] - 0.2, 0.08, wing.size[1] - 0.2],
            );
            self.house_roof(wing.size, wing.at, wing.size[0] > wing.size[1], wall);
        }
        let walls = local.walls();
        for edge in &walls {
            let side = edge.normal[0] != 0.;
            let axis = usize::from(side);
            let perpendicular = 1 - axis;
            let center = (edge.a[axis] + edge.b[axis]) * 0.5;
            let mut holes: Vec<_> = windows
                .iter()
                .filter(|(p, _, n)| {
                    *n == edge.normal
                        && (p[perpendicular] - edge.a[perpendicular] - n[perpendicular] * 0.18)
                            .abs()
                            < 0.01
                        && p[axis] >= edge.a[axis]
                        && p[axis] <= edge.b[axis]
                })
                .map(|(p, y, _)| {
                    [
                        p[axis] - center - 0.46,
                        p[axis] - center + 0.46,
                        *y - 0.4,
                        *y + 0.4,
                    ]
                })
                .collect();
            if edge.normal == [0., 1.]
                && (edge.a[1] - door[1]).abs() < 0.01
                && door[0] >= edge.a[0]
                && door[0] <= edge.b[0]
            {
                holes.push([door[0] - center - 0.53, door[0] - center + 0.53, 0.2, 2.25]);
            }
            let start = self.0.len();
            self.facade(
                wall,
                (edge.b[axis] - edge.a[axis]).abs(),
                h,
                edge.a[perpendicular] - edge.normal[perpendicular] * 0.11,
                side,
                &holes,
            );
            for part in &mut self.0[start..] {
                part.wall_normal = Some(Vec3::new(edge.normal[0], 0., edge.normal[1]));
                if side {
                    part.transform.translation.z += center;
                } else {
                    part.transform.translation.x += center;
                }
            }
            // Continuous stone plinth and eave band follow recesses and courtyards.
            for (y, thickness, finish) in [
                (0.40, 0.32, Finish::Concrete),
                (h - 0.12, 0.16, Finish::Ivory),
            ] {
                let start = self.0.len();
                let band_holes: Vec<_> = holes
                    .iter()
                    .copied()
                    .filter(|hole| hole[2] < y + thickness * 0.5 && hole[3] > y - thickness * 0.5)
                    .collect();
                let mut spans = vec![
                    -(edge.b[axis] - edge.a[axis]).abs() * 0.5,
                    (edge.b[axis] - edge.a[axis]).abs() * 0.5,
                ];
                for hole in &band_holes {
                    spans.extend([hole[0], hole[1]]);
                }
                spans.sort_by(f32::total_cmp);
                for span in spans.windows(2) {
                    let u = (span[0] + span[1]) * 0.5;
                    if span[1] - span[0] < 0.02
                        || band_holes.iter().any(|hole| u > hole[0] && u < hole[1])
                    {
                        continue;
                    }
                    let plane = edge.a[perpendicular] - edge.normal[perpendicular] * 0.09;
                    self.slab(
                        finish,
                        if side {
                            [plane, y, center + u]
                        } else {
                            [center + u, y, plane]
                        },
                        if side {
                            [0.27, thickness, span[1] - span[0]]
                        } else {
                            [span[1] - span[0], thickness, 0.27]
                        },
                    );
                }
                for part in &mut self.0[start..] {
                    part.roof_lot = Some(0);
                    part.wall_normal = Some(Vec3::new(edge.normal[0], 0., edge.normal[1]));
                }
            }
        }
        for (p, y, n) in &windows {
            let start = self.0.len();
            let side = n[0].abs() > 0.5;
            let x = p[0] - n[0] * 0.12;
            let z = p[1] - n[1] * 0.12;
            self.block(
                Finish::Glass,
                [x - n[0] * 0.08, *y, z - n[1] * 0.08],
                if side {
                    [0.035, 0.7, 0.82]
                } else {
                    [0.82, 0.7, 0.035]
                },
            );
            self.0.last_mut().unwrap().roof_lot = Some(0);
            for edge in [-1., 1.] {
                self.block(
                    Finish::Wood,
                    if side {
                        [x, *y, z + edge * 0.44]
                    } else {
                        [x + edge * 0.44, *y, z]
                    },
                    if side {
                        [0.12, 0.85, 0.08]
                    } else {
                        [0.08, 0.85, 0.12]
                    },
                );
                self.block(
                    Finish::Wood,
                    [x, *y + edge * 0.4, z],
                    if side {
                        [0.12, 0.08, 0.96]
                    } else {
                        [0.96, 0.08, 0.12]
                    },
                );
            }
            self.block(
                Finish::Wood,
                [x, *y, z],
                if side {
                    [0.09, 0.7, 0.035]
                } else {
                    [0.035, 0.7, 0.09]
                },
            );
            self.block(
                Finish::Concrete,
                [x + n[0] * 0.08, *y - 0.44, z + n[1] * 0.08],
                if side {
                    [0.28, 0.09, 1.05]
                } else {
                    [1.05, 0.09, 0.28]
                },
            );
            // A projecting lintel and recessed sill give each opening depth.
            self.slab(
                Finish::Concrete,
                [x + n[0] * 0.035, *y + 0.47, z + n[1] * 0.035],
                if side {
                    [0.18, 0.08, 1.06]
                } else {
                    [1.06, 0.08, 0.18]
                },
            );
            if lot.style.is_multiple_of(4) || lot.style % 4 == 3 {
                for offset in [-0.63, 0.63] {
                    let at = if side {
                        [x, *y, z + offset]
                    } else {
                        [x + offset, *y, z]
                    };
                    self.block(
                        Finish::Wood,
                        at,
                        if side {
                            [0.1, 0.8, 0.24]
                        } else {
                            [0.24, 0.8, 0.1]
                        },
                    );
                    for louver in 0..5 {
                        self.slab(
                            Finish::Wood,
                            [
                                at[0] + n[0] * 0.06,
                                *y - 0.28 + louver as f32 * 0.14,
                                at[2] + n[1] * 0.06,
                            ],
                            if side {
                                [0.045, 0.045, 0.21]
                            } else {
                                [0.21, 0.045, 0.045]
                            },
                        );
                        self.rotate(if side {
                            Quat::from_rotation_z(n[0] * 0.32)
                        } else {
                            Quat::from_rotation_x(-n[1] * 0.32)
                        });
                    }
                }
            }
            for part in &mut self.0[start..] {
                part.roof_lot = Some(0);
                part.wall_normal = Some(Vec3::new(n[0], 0., n[1]));
            }
        }
        let door_start = self.0.len();
        self.door_fixture(door[0], door[1], 1.06, 2.05, false);
        self.block(
            Finish::Roof,
            [door[0], 2.50, door[1] + 0.31],
            [1.7, 0.12, 0.70],
        );
        self.rotate(Quat::from_rotation_x(0.12));
        for part in &mut self.0[door_start..] {
            part.roof_lot = Some(0);
            part.wall_normal = Some(Vec3::Z);
        }
        let mut corners = std::collections::BTreeSet::new();
        for edge in &walls {
            for p in [edge.a, edge.b] {
                if corners.insert([p[0].to_bits(), p[1].to_bits()]) {
                    for course in 0..7 {
                        self.block(
                            Finish::Concrete,
                            [p[0], 0.4 + course as f32 * 0.42, p[1]],
                            [0.23, 0.18, 0.23],
                        );
                    }
                }
            }
        }
        let wing = local.wings()[0];
        let chimney = [
            wing.at[0] + wing.size[0] * 0.22,
            h + 0.60,
            wing.at[1] - wing.size[1] * 0.25,
        ];
        self.block(Finish::Brick, chimney, [0.48, 1.3, 0.55]);
        self.block(
            Finish::Concrete,
            [chimney[0], chimney[1] + 0.69, chimney[2]],
            [0.64, 0.12, 0.72],
        );
        self.block(
            Finish::Dark,
            [chimney[0], chimney[1] + 0.76, chimney[2]],
            [0.35, 0.04, 0.4],
        );
    }
    fn house_roof(&mut self, size: [f32; 2], center: [f32; 2], along_x: bool, wall: Finish) {
        let start = self.0.len();
        let (w, d) = if along_x {
            (size[1], size[0])
        } else {
            (size[0], size[1])
        };
        let h = crate::settlement_layout::HOUSE_HEIGHT;
        let rise = (w * 0.38).clamp(0.80, 1.75);
        self.add(
            Shape::Roof,
            wall,
            [0., h + rise / 6., 0.],
            [w, rise / 0.6, d],
        );
        let reach = w * 0.5 + 0.22;
        let slope = (rise / reach).atan();
        let length = reach.hypot(rise);
        for side in [-1., 1.] {
            self.slab(
                Finish::Roof,
                [side * reach * 0.5, h + rise * 0.5, 0.],
                [length, 0.105, d + 0.44],
            );
            self.rotate(Quat::from_rotation_z(-side * slope));
            // Individual pantile noses cast short contact shadows along the
            // eave; the scanned roof maps supply the finer roof field detail.
            let count = ((d + 0.44) / 0.30).ceil() as i32;
            for tile in 0..count {
                self.add(
                    Shape::RoofTile,
                    Finish::Roof,
                    [
                        side * (reach - 0.14),
                        h + rise * (0.14 / reach) + 0.067,
                        -d * 0.5 - 0.22 + (tile as f32 + 0.5) * (d + 0.44) / count as f32,
                    ],
                    [0.35, 0.045, (d + 0.44) / count as f32 * 0.98],
                );
                self.rotate(Quat::from_rotation_z(-side * slope));
            }
            self.block(
                Finish::Wood,
                [side * reach, h - 0.03, 0.],
                [0.14, 0.23, d + 0.445],
            );
            for end in [-1., 1.] {
                self.block(
                    Finish::Wood,
                    [side * reach * 0.5, h + rise * 0.5, end * (d * 0.5 + 0.21)],
                    [length + 0.12, 0.17, 0.11],
                );
                self.rotate(Quat::from_rotation_z(-side * slope));
            }
            // The supplied PBR maps carry the roof courses. Extra strips here
            // competed with their spacing and produced a second tile pattern.
            // Rafters remain visible through shell damage rather than a solid roof block.
            for i in 0..((d / 0.9).ceil() as i32) {
                self.slab(
                    Finish::Wood,
                    [
                        side * reach * 0.5,
                        h + rise * 0.5 - 0.14,
                        -d * 0.5 + 0.45 + i as f32 * 0.9,
                    ],
                    [length, 0.10, 0.09],
                );
                self.rotate(Quat::from_rotation_z(-side * slope));
            }
            self.cylinder(
                Finish::Steel,
                [side * (reach + 0.08), h - 0.12, 0.],
                [0.12, d + 0.445, 0.12],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            self.cylinder(
                Finish::Steel,
                [side * (reach + 0.08), h * 0.5, -d * 0.4],
                [0.08, h - 0.1, 0.08],
                Quat::IDENTITY,
            );
        }
        for cap in 0..((d + 0.7) / 0.42) as i32 {
            self.cylinder(
                Finish::Roof,
                [0., h + rise + 0.06, -d * 0.5 - 0.3 + cap as f32 * 0.42],
                [0.19, 0.4, 0.19],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
        }
        let rotation = if along_x {
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        } else {
            Quat::IDENTITY
        };
        for part in &mut self.0[start..] {
            if part.transform.translation.y >= h - 0.25 {
                part.roof_lot = Some(0);
            }
            part.transform.translation =
                rotation * part.transform.translation + Vec3::new(center[0], 0., center[1]);
            part.transform.rotation = rotation * part.transform.rotation;
        }
    }
    fn facility_shell(&mut self, kind: &str, w: f32, d: f32, h: f32, wall: Finish) {
        let start = self.0.len();
        let windows = crate::simulation::facility_windows(kind);
        for side in [-1., 1.] {
            let mut holes: Vec<_> = windows
                .iter()
                .filter(|s| s.2[1] == side)
                .map(|(p, y, _)| [p[0] - 0.55, p[0] + 0.55, *y - 0.45, *y + 0.45])
                .collect();
            if side > 0. {
                if kind == "vehicleDepot" {
                    for x in [-2.8, 2.8] {
                        holes.push([x - 2.1, x + 2.1, 0.2, 3.8]);
                    }
                } else if let Some(p) = crate::simulation::facility_door(kind) {
                    holes.push([p[0] - 0.55, p[0] + 0.55, 0.2, 2.4]);
                }
            }
            self.facade(wall, w, h, side * (d * 0.5 - 0.11), false, &holes);
            let holes: Vec<_> = windows
                .iter()
                .filter(|s| s.2[0] == side)
                .map(|(p, y, _)| [p[1] - 0.55, p[1] + 0.55, *y - 0.45, *y + 0.45])
                .collect();
            self.facade(wall, d, h, side * (w * 0.5 - 0.11), true, &holes);
        }
        self.slab(Finish::Concrete, [0., 0.22, 0.], [w - 0.4, 0.12, d - 0.4]);
        for part in &mut self.0[start..] {
            if part.roof_lot.is_some() {
                part.roof_lot = Some(usize::MAX);
            }
        }
    }
    fn facility_roof(&mut self, w: f32, d: f32, h: f32) {
        let start = self.0.len();
        let rise = (w * 0.17).clamp(0.65, 1.15);
        let reach = w * 0.5 + 0.28;
        let slope = (rise / reach).atan();
        let length = reach.hypot(rise);
        self.add(
            Shape::Roof,
            Finish::Plaster,
            [0., h + rise / 6., 0.],
            [w, rise / 0.6, d],
        );
        for side in [-1f32, 1.] {
            self.slab(
                Finish::Roof,
                [side * reach * 0.5, h + rise * 0.5, 0.],
                [length, 0.10, d + 0.56],
            );
            self.rotate(Quat::from_rotation_z(-side * slope));
            self.slab(
                Finish::Steel,
                [side * reach, h - 0.05, 0.],
                [0.12, 0.18, d + 0.56],
            );
            for i in 0..((d / 0.65).ceil() as i32) {
                let z = -d * 0.5 + i as f32 * 0.65;
                self.slab(
                    Finish::Roof,
                    [side * reach * 0.5, h + rise * 0.5 + 0.065, z],
                    [length, 0.025, 0.025],
                );
                self.rotate(Quat::from_rotation_z(-side * slope));
            }
            for z in [-d * 0.42, d * 0.42] {
                self.slab(
                    Finish::Wood,
                    [side * reach * 0.5, h + rise * 0.5 - 0.16, z],
                    [length, 0.14, 0.12],
                );
                self.rotate(Quat::from_rotation_z(-side * slope));
            }
        }
        self.cylinder(
            Finish::Steel,
            [0., h + rise + 0.04, 0.],
            [0.15, d + 0.60, 0.15],
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        for part in &mut self.0[start..] {
            part.roof_lot = Some(usize::MAX);
        }
    }
}
