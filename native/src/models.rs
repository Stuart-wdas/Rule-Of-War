//! A coherent miniature military kit, built from reusable detailed meshes.
use crate::runtime::Piece;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

#[derive(Clone, Copy)]
pub enum Shape {
    Box,
    Sphere,
    Cylinder,
    Capsule,
    Roof,
    Hull,
    Airframe,
    Airfoil,
    Slab,
    A10Body,
    A10Canopy,
    A10Wing,
    A10Tail,
    A10Fin,
    JetNacelle,
    ApacheBody,
    ApacheCanopy,
    ApacheBoom,
    ApacheEngine,
    ApacheWing,
    ApacheFin,
    RotorBlade,
    ApacheRoof,
    AircraftDetailCylinder,
    AircraftDetailSphere,
    ArmourTurret,
    CarrierCabin,
    Tyre,
    RoofTile,
    ApacheSkin,
    ApacheMetal,
    ApacheDark,
    ApacheTyres,
    ApacheGlass,
    ApacheRotor,
    ApacheRotorMetal,
    ApacheTailRotor,
    ApacheTailRotorMetal,
    ApacheGun,
    ApacheGunMetal,
    ApacheRed,
    ApacheLight,
    HueyBody,
    HueyBoom,
    HueyGlass,
    HueyFin,
    HueyRotor,
    HueyTailRotor,
    HueyEngine,
    C130Body,
    C130Wing,
    C130Tail,
    C130Fin,
}
pub fn shape_meshes() -> Vec<Mesh> {
    let mut shapes = vec![
        bevelled_box(),
        Sphere::new(0.5).mesh().uv(20, 12),
        Cylinder::new(0.5, 1.).mesh().resolution(16).build(),
        Mesh::from(Capsule3d::new(0.5, 0.5)).scaled_by(Vec3::new(1., 2. / 3., 1.)),
        prism(false),
        prism(true),
        airframe_mesh(),
        airfoil_mesh(),
        Cuboid::new(1., 1., 1.).into(),
    ];
    for (index, mut mesh) in aircraft_geometry_meshes().into_iter().enumerate() {
        let (center, size) = air_bounds_index(index);
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            for v in vertices {
                *v = ((Vec3::from_array(*v) - center) / size).to_array();
            }
        }
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
        {
            for n in normals {
                *n = (Vec3::from_array(*n) * size).normalize().to_array();
            }
        }
        shapes.push(mesh);
    }
    // Small barrels, struts and lights need fewer segments than hero surfaces.
    shapes.extend([
        Cylinder::new(0.5, 1.).mesh().resolution(12).build(),
        Sphere::new(0.5).mesh().uv(10, 6),
    ]);
    shapes.extend([
        armour_shell(false),
        armour_shell(true),
        tyre_mesh(),
        tile_lip(),
    ]);
    shapes.extend(apache_authored_meshes());
    shapes.extend(huey_meshes());
    shapes.extend(c130_meshes());
    shapes
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finish {
    Team,
    Dark,
    Steel,
    Skin,
    Cloth,
    Roof,
    Concrete,
    Glass,
    Wood,
    Light,
    Brass,
    Red,
    White,
    Earth,
    Brick,
    Plaster,
    Charred,
    Scorched,
    Rust,
    Rubber,
    Asphalt,
    Gravel,
    AircraftPaint,
    Scorch,
    Ochre,
    Sage,
    BluePlaster,
    Rose,
    Ivory,
    Terracotta,
    SlateRoof,
    GreenRoof,
    AircraftGlass,
    A10Livery,
}
/// Static kits become one indexed mesh per material, rather than hundreds of entities.
pub fn bake(parts: &[Part], shapes: &[Mesh]) -> Vec<(Finish, Mesh)> {
    type Vertices = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>);
    let mut groups: std::collections::BTreeMap<Finish, Vertices> =
        std::collections::BTreeMap::new();
    for part in parts {
        let mesh = &shapes[part.shape as usize];
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            continue;
        };
        let group = groups.entry(part.finish).or_default();
        let offset = group.0.len() as u32;
        let affine = part.transform.compute_affine();
        let calibrated_uv = part.surface_frame.map(|frame| {
            positions
                .iter()
                .zip(normals)
                .map(|(p, n)| {
                    if matches!(
                        part.finish,
                        Finish::Roof | Finish::SlateRoof | Finish::GreenRoof | Finish::Terracotta
                    ) && matches!(part.shape, Shape::Slab | Shape::RoofTile)
                    {
                        // U follows the ridge; V measures real distance down the
                        // slope. House/wing yaw never rotates the photographed tiles.
                        [
                            p[2] * frame.scale.z * 0.65,
                            (p[0] + 0.5) * frame.scale.x * 0.65,
                        ]
                    } else {
                        crate::surfaces::coords(
                            frame.transform_point(Vec3::from_array(*p)).to_array(),
                            (frame.rotation * (Vec3::from_array(*n) / frame.scale))
                                .normalize_or_zero()
                                .to_array(),
                        )
                    }
                })
                .collect::<Vec<_>>()
        });
        let authored_uv = if let Some(uv) = &calibrated_uv {
            Some(uv)
        } else if part.finish == Finish::A10Livery || apache_shape(part.shape) {
            match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
                Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) => Some(uv),
                _ => None,
            }
        } else {
            None
        };
        if let Some(uv) = authored_uv {
            // An authored airframe can share a finish with procedural training
            // markers or wreck fittings. Backfill earlier projected vertices.
            if group.3.is_empty() && offset > 0 {
                let projected: Vec<_> = group
                    .0
                    .iter()
                    .zip(&group.1)
                    .map(|(p, n)| crate::surfaces::coords(*p, *n))
                    .collect();
                group.3.extend(projected);
            }
            group.3.extend_from_slice(uv);
        }
        group.0.extend(
            positions
                .iter()
                .map(|p| affine.transform_point3(Vec3::from_array(*p)).to_array()),
        );
        group.1.extend(normals.iter().map(|n| {
            (part.transform.rotation * (Vec3::from_array(*n) / part.transform.scale))
                .normalize_or_zero()
                .to_array()
        }));
        if authored_uv.is_none() && !group.3.is_empty() {
            let projected: Vec<_> = group.0[offset as usize..]
                .iter()
                .zip(&group.1[offset as usize..])
                .map(|(p, n)| crate::surfaces::coords(*p, *n))
                .collect();
            group.3.extend(projected);
        }
        if let Some(indices) = mesh.indices() {
            group.2.extend(indices.iter().map(|i| i as u32 + offset));
        } else {
            group
                .2
                .extend((0..positions.len() as u32).map(|i| i + offset));
        }
    }
    groups
        .into_iter()
        .map(|(finish, (positions, normals, indices, uv))| {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_indices(Indices::U32(indices));
            if !uv.is_empty() {
                assert_eq!(
                    uv.len(),
                    mesh.count_vertices(),
                    "authored aircraft UV export"
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv.clone());
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                let _ = mesh.generate_tangents();
            }
            (finish, mesh)
        })
        .map(|(finish, mut mesh)| {
            if finish != Finish::A10Livery {
                crate::surfaces::uv(&mut mesh);
            }
            (finish, mesh)
        })
        .collect()
}
/// Parts sharing a joint can be baked in bind pose and moved by one transform.
pub fn bake_animated(parts: Vec<Part>, shapes: &[Mesh]) -> Vec<(i8, Vec3, Finish, Mesh)> {
    let mut groups: std::collections::BTreeMap<(i8, [u32; 3]), Vec<Part>> =
        std::collections::BTreeMap::new();
    for part in parts {
        groups
            .entry((part.joint, part.pivot.to_array().map(f32::to_bits)))
            .or_default()
            .push(part);
    }
    let mut result = vec![];
    for ((role, pivot), parts) in groups {
        let pivot = Vec3::from_array(pivot.map(f32::from_bits));
        for (finish, mesh) in bake(&parts, shapes) {
            result.push((role, pivot, finish, mesh));
        }
    }
    result
}
/// A continuous lane sampled at its edges and centre, rather than floating patches.
fn practice_lane(p: &Piece, terrain: &crate::terrain::Terrain) -> Mesh {
    let extent = p
        .lots
        .iter()
        .map(|l| l.at[0].abs() + l.width * 0.5 + 2.)
        .fold(21., f32::max);
    let rows = (extent * 4.).ceil() as u32 + 1;
    const WIDTH: u32 = 7;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for row in 0..rows {
        let x = -extent + row as f32 * (extent * 2. / (rows - 1) as f32);
        let centre = (x * 0.12).sin() * 3.5;
        let taper = ((extent - x.abs()) / 1.5).clamp(0.12, 1.);
        let width = (1.45 + (x * 0.8).sin() * 0.12) * taper;
        for column in 0..WIDTH {
            let z = centre + (column as f32 / (WIDTH - 1) as f32 * 2. - 1.) * width;
            let y = terrain.height(p.x + x, p.z + z) - p.elevation + 0.045;
            positions.push([x, y, z]);
        }
        if row + 1 < rows {
            for column in 0..WIDTH - 1 {
                let a = row * WIDTH + column;
                indices.extend([a, a + 1, a + WIDTH, a + 1, a + WIDTH + 1, a + WIDTH]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    crate::surfaces::uv(&mut mesh);
    mesh
}
#[derive(Clone)]
pub struct Part {
    pub shape: Shape,
    pub finish: Finish,
    pub transform: Transform,
    pub joint: i8,
    pub pivot: Vec3,
    pub socket: Option<Vec3>,
    /// Separate roof batches per house; usize::MAX denotes a facility.
    pub roof_lot: Option<usize>,
    pub wall_normal: Option<Vec3>,
    /// Metre-based texture coordinates before a house is placed/rotated.
    pub surface_frame: Option<Transform>,
}
fn polygon(
    vertices: &[Vec3],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let mut vertices = vertices.to_vec();
    let center = vertices.iter().copied().sum::<Vec3>() / vertices.len() as f32;
    let mut normal = (vertices[1] - vertices[0])
        .cross(vertices[2] - vertices[0])
        .normalize_or_zero();
    if normal.dot(center) < 0. {
        vertices.reverse();
        normal = -normal;
    }
    let start = positions.len() as u32;
    positions.extend(vertices.iter().map(|p| p.to_array()));
    normals.extend(vertices.iter().map(|_| normal.to_array()));
    for i in 1..vertices.len() - 1 {
        indices.extend([start, start + i as u32, start + i as u32 + 1]);
    }
}
pub fn bevelled_box() -> Mesh {
    let mut positions = vec![];
    let mut normals = vec![];
    let mut indices = vec![];
    for axis in 0..3 {
        for sign in [-1., 1.] {
            let other = [(axis + 1) % 3, (axis + 2) % 3];
            let points = [[-1., -1.], [1., -1.], [1., 1.], [-1., 1.]].map(|s| {
                let mut p = Vec3::ZERO;
                p[axis] = 0.5 * sign;
                p[other[0]] = 0.47 * s[0];
                p[other[1]] = 0.47 * s[1];
                p
            });
            polygon(&points, &mut positions, &mut normals, &mut indices);
        }
    }
    for axis in 0..3 {
        for a in [-1., 1.] {
            for b in [-1., 1.] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let points = [
                    [-1., 0.5, 0.47],
                    [1., 0.5, 0.47],
                    [1., 0.47, 0.5],
                    [-1., 0.47, 0.5],
                ]
                .map(|s| {
                    let mut p = Vec3::ZERO;
                    p[axis] = s[0] * 0.47;
                    p[u] = s[1] * a;
                    p[v] = s[2] * b;
                    p
                });
                polygon(&points, &mut positions, &mut normals, &mut indices);
            }
        }
    }
    for x in [-1., 1.] {
        for y in [-1., 1.] {
            for z in [-1., 1.] {
                polygon(
                    &[
                        Vec3::new(x * 0.5, y * 0.47, z * 0.47),
                        Vec3::new(x * 0.47, y * 0.5, z * 0.47),
                        Vec3::new(x * 0.47, y * 0.47, z * 0.5),
                    ],
                    &mut positions,
                    &mut normals,
                    &mut indices,
                );
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}
pub fn prism(hull: bool) -> Mesh {
    let mut p = vec![];
    let mut n = vec![];
    let mut i = vec![];
    let rings: Vec<Vec<Vec3>> = if hull {
        // A broad bevelled glacis, never a single spear-shaped bow vertex.
        vec![
            vec![
                Vec3::new(-0.44, 0.25, -0.5),
                Vec3::new(0.44, 0.25, -0.5),
                Vec3::new(0.5, 0.25, -0.38),
                Vec3::new(0.5, 0.25, 0.18),
                Vec3::new(0.40, 0.25, 0.36),
                Vec3::new(-0.40, 0.25, 0.36),
                Vec3::new(-0.5, 0.25, 0.18),
                Vec3::new(-0.5, 0.25, -0.38),
            ],
            vec![
                Vec3::new(-0.4, -0.3, -0.45),
                Vec3::new(0.4, -0.3, -0.45),
                Vec3::new(0.44, -0.3, -0.34),
                Vec3::new(0.44, -0.3, 0.43),
                Vec3::new(0.37, -0.3, 0.53),
                Vec3::new(-0.37, -0.3, 0.53),
                Vec3::new(-0.44, -0.3, 0.43),
                Vec3::new(-0.44, -0.3, -0.34),
            ],
        ]
    } else {
        vec![
            vec![
                Vec3::new(-0.5, -0.1, -0.5),
                Vec3::new(0.5, -0.1, -0.5),
                Vec3::new(0., 0.5, -0.5),
            ],
            vec![
                Vec3::new(-0.5, -0.1, 0.5),
                Vec3::new(0.5, -0.1, 0.5),
                Vec3::new(0., 0.5, 0.5),
            ],
        ]
    };
    polygon(&rings[0], &mut p, &mut n, &mut i);
    polygon(&rings[1], &mut p, &mut n, &mut i);
    for edge in 0..rings[0].len() {
        let next = (edge + 1) % rings[0].len();
        polygon(
            &[
                rings[0][edge],
                rings[0][next],
                rings[1][next],
                rings[1][edge],
            ],
            &mut p,
            &mut n,
            &mut i,
        );
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
    .with_inserted_indices(Indices::U32(i))
}
#[derive(Default)]
struct Kit(Vec<Part>);
impl Kit {
    fn add(&mut self, shape: Shape, finish: Finish, pos: [f32; 3], scale: [f32; 3]) {
        self.0.push(Part {
            shape,
            finish,
            transform: Transform::from_translation(Vec3::from_array(pos))
                .with_scale(Vec3::from_array(scale)),
            joint: 0,
            pivot: Vec3::ZERO,
            socket: None,
            roof_lot: None,
            wall_normal: None,
            surface_frame: None,
        });
    }
    fn block(&mut self, m: Finish, p: [f32; 3], s: [f32; 3]) {
        self.add(Shape::Box, m, p, s);
    }
    fn sphere(&mut self, m: Finish, p: [f32; 3], s: [f32; 3]) {
        self.add(Shape::Sphere, m, p, s);
    }
    fn cylinder(&mut self, m: Finish, p: [f32; 3], s: [f32; 3], rotation: Quat) {
        self.add(Shape::Cylinder, m, p, s);
        self.0.last_mut().unwrap().transform.rotation = rotation;
    }
    fn rotate(&mut self, rotation: Quat) {
        self.0.last_mut().unwrap().transform.rotation = rotation;
    }
    fn slab(&mut self, finish: Finish, at: [f32; 3], size: [f32; 3]) {
        self.add(Shape::Slab, finish, at, size);
    }
    fn joint(&mut self, joint: i8, pivot: [f32; 3]) {
        let p = self.0.last_mut().unwrap();
        p.joint = joint;
        p.pivot = Vec3::from_array(pivot);
    }
    fn socket(&mut self, tip: Vec3) {
        self.0.last_mut().unwrap().socket = Some(tip);
    }
    fn infantry(&mut self, x: f32, z: f32, kind: &str) {
        let start = self.0.len();
        for side in [-1., 1.] {
            let sign = side as i8;
            self.add(
                Shape::Capsule,
                Finish::Dark,
                [x + side * 0.11, 0.085, z + 0.055],
                [0.17, 0.13, 0.29],
            );
            self.joint(sign * 12, [x + side * 0.13, 0.38, z]);
            self.add(
                Shape::Capsule,
                Finish::Cloth,
                [x + side * 0.13, 0.27, z],
                [0.15, 0.27, 0.17],
            );
            self.joint(sign * 11, [x + side * 0.13, 0.38, z]);
            self.add(
                Shape::Capsule,
                Finish::Cloth,
                [x + side * 0.13, 0.51, z],
                [0.17, 0.29, 0.19],
            );
            self.joint(sign, [x + side * 0.13, 0.65, z]);
            if kind == "medic" {
                self.add(
                    Shape::Capsule,
                    Finish::Cloth,
                    [x + side * 0.29, 0.89, z + 0.05],
                    [0.14, 0.36, 0.16],
                );
                self.rotate(Quat::from_rotation_z(side * 0.18));
                self.joint(sign * 2, [x + side * 0.24, 1.05, z]);
                self.sphere(
                    Finish::Skin,
                    [x + side * 0.32, 0.70, z + 0.10],
                    [0.14, 0.14, 0.16],
                );
                self.joint(sign * 2, [x + side * 0.24, 1.05, z]);
            } else {
                // Both bent arms and the rifle share one grip assembly throughout the pose.
                let shoulder = Vec3::new(x + side * 0.24, 1.04, z);
                let elbow = Vec3::new(x + if side > 0. { 0.32 } else { -0.15 }, 0.83, z + 0.18);
                let hand = Vec3::new(
                    x + 0.17,
                    if side > 0. { 0.89 } else { 0.94 },
                    z + if side > 0. { 0.22 } else { 0.50 },
                );
                for (from, to) in [(shoulder, elbow), (elbow, hand)] {
                    let delta = to - from;
                    self.add(
                        Shape::Capsule,
                        Finish::Cloth,
                        ((from + to) * 0.5).to_array(),
                        [0.115, delta.length() + 0.045, 0.115],
                    );
                    self.rotate(Quat::from_rotation_arc(Vec3::Y, delta.normalize()));
                    self.joint(4, [x, 0.65, z]);
                }
                self.sphere(Finish::Skin, hand.to_array(), [0.13, 0.12, 0.15]);
                self.joint(4, [x, 0.65, z]);
            }
            self.sphere(
                Finish::Team,
                [x + side * 0.25, 1.015, z],
                [0.16, 0.16, 0.18],
            );
            self.joint(
                if kind == "medic" { sign * 2 } else { 4 },
                if kind == "medic" {
                    [x + side * 0.24, 1.05, z]
                } else {
                    [x, 0.65, z]
                },
            );
        }
        self.add(
            Shape::Capsule,
            Finish::Cloth,
            [x, 0.87, z],
            [0.44, 0.62, 0.29],
        );
        self.add(
            Shape::Capsule,
            Finish::Dark,
            [x, 0.94, z - 0.16],
            [0.28, 0.34, 0.18],
        );
        self.add(
            Shape::Capsule,
            Finish::Team,
            [x, 0.9, z + 0.13],
            [0.35, 0.31, 0.12],
        );
        self.add(
            Shape::Capsule,
            Finish::Cloth,
            [x, 0.71, z],
            [0.36, 0.20, 0.26],
        );
        for dx in [-0.12, 0., 0.12] {
            self.add(
                Shape::Capsule,
                Finish::Cloth,
                [x + dx, 0.82, z + 0.185],
                [0.075, 0.12, 0.09],
            );
        }
        self.add(
            Shape::Capsule,
            Finish::Cloth,
            [x, 0.94, z - 0.195],
            [0.25, 0.30, 0.16],
        );
        self.cylinder(
            Finish::Cloth,
            [x, 1.11, z],
            [0.21, 0.10, 0.20],
            Quat::IDENTITY,
        );
        self.sphere(Finish::Skin, [x, 1.27, z], [0.30, 0.36, 0.30]);
        self.sphere(
            if kind == "commander" {
                Finish::Brass
            } else {
                Finish::Cloth
            },
            [x, 1.40, z],
            [0.34, 0.22, 0.35],
        );
        self.cylinder(
            Finish::Cloth,
            [x, 1.35, z],
            [0.35, 0.025, 0.36],
            Quat::IDENTITY,
        );
        if kind != "medic" {
            self.block(
                Finish::Dark,
                [x + 0.17, 0.96, z + 0.34],
                [0.105, 0.10, 0.57],
            );
            self.joint(4, [x, 0.65, z]);
            self.cylinder(
                Finish::Steel,
                [x + 0.17, 0.96, z + 0.70],
                [0.06, 0.40, 0.06],
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            );
            self.joint(4, [x, 0.65, z]);
            self.socket(Vec3::Y * 0.5);
            self.block(Finish::Dark, [x + 0.17, 0.84, z + 0.46], [0.08, 0.19, 0.11]);
            self.joint(4, [x, 0.65, z]);
            self.block(
                Finish::Steel,
                [x + 0.17, 1.03, z + 0.36],
                [0.05, 0.04, 0.30],
            );
            self.joint(4, [x, 0.65, z]);
        } else {
            self.block(Finish::White, [x, 1.01, z - 0.32], [0.22, 0.25, 0.04]);
            self.block(Finish::Red, [x, 1.01, z - 0.35], [0.06, 0.20, 0.03]);
            self.block(Finish::Red, [x, 1.01, z - 0.35], [0.17, 0.06, 0.035]);
            self.block(Finish::White, [x + 0.32, 0.54, z + 0.1], [0.25, 0.22, 0.22]);
            self.joint(2, [x + 0.24, 1.05, z]);
        }
        if kind == "engineer" {
            self.block(Finish::Brass, [x - 0.2, 0.65, z], [0.10, 0.30, 0.10]);
            self.joint(3, [x, 0.65, z]);
        }
        for part in &mut self.0[start..] {
            if part.joint == 0 {
                part.joint = 3;
                part.pivot = Vec3::new(x, 0.65, z);
            }
        }
    }
    fn house(&mut self, lot: &crate::settlement_layout::Lot) {
        let start = self.0.len();
        let wall = [
            Finish::Plaster,
            Finish::Brick,
            Finish::Ochre,
            Finish::Sage,
            Finish::BluePlaster,
            Finish::Rose,
            Finish::Ivory,
            Finish::Wood,
        ][(lot.style as usize + (lot.at[0].abs() as usize / 3) + (lot.at[1].abs() as usize / 5))
            % 8];
        if lot.integrity <= 0. {
            let local = crate::settlement_layout::Lot {
                at: [0., 0.],
                yaw: 0.,
                ..lot.clone()
            };
            let hash = lot.at[0].to_bits() ^ lot.at[1].to_bits().rotate_left(13) ^ lot.style as u32;
            for wing in local.wings() {
                self.slab(
                    Finish::Dark,
                    [wing.at[0], 0.035, wing.at[1]],
                    [wing.size[0] + 0.3, 0.07, wing.size[1] + 0.3],
                );
            }
            self.rubble_pile(&local.wings(), hash, wall, Finish::Roof);
            for (index, edge) in local.walls().iter().enumerate() {
                let n = crate::destruction::variation(hash, index + 241);
                let center = Vec2::from(edge.a).lerp(Vec2::from(edge.b), 0.5);
                let length = Vec2::from(edge.a).distance(Vec2::from(edge.b));
                let height = 0.35 + n * 0.85;
                self.slab(
                    wall,
                    [center.x, height * 0.5, center.y],
                    if edge.normal[0] != 0. {
                        [0.24, height, length * 0.65]
                    } else {
                        [length * 0.65, height, 0.24]
                    },
                );
            }
        } else {
            self.detailed_house(lot, wall);
            let source = self.0.drain(start..).collect();
            self.0.extend(crate::destruction::damaged_parts(
                source,
                lot.integrity,
                lot.at[0].to_bits() ^ lot.at[1].to_bits().rotate_left(13) ^ lot.style as u32,
            ));
        }
        let rotation = Quat::from_rotation_y(lot.yaw);
        for part in &mut self.0[start..] {
            part.surface_frame = Some(part.transform);
            if part.finish == Finish::Roof {
                part.finish = [
                    Finish::Terracotta,
                    Finish::SlateRoof,
                    Finish::GreenRoof,
                    Finish::Roof,
                ][(lot.style as usize + lot.at[0].abs() as usize) % 4];
            }
            part.transform.translation =
                rotation * part.transform.translation + Vec3::new(lot.at[0], lot.ground, lot.at[1]);
            part.transform.rotation = rotation * part.transform.rotation;
            part.wall_normal = part.wall_normal.map(|n| rotation * n);
        }
    }
    fn windows(&mut self, x: f32, y: f32, z: f32, w: f32) {
        let start = self.0.len();
        for dx in [-0.28, 0., 0.28] {
            self.block(Finish::Dark, [x + dx * w, y, z], [w * 0.19, 0.70, 0.07]);
            self.block(
                Finish::Glass,
                [x + dx * w, y, z + z.signum() * 0.012],
                [w * 0.16, 0.56, 0.065],
            );
            self.block(
                Finish::Concrete,
                [x + dx * w, y - 0.36, z + z.signum() * 0.06],
                [w * 0.21, 0.055, 0.19],
            );
            self.block(
                Finish::Steel,
                [x + dx * w, y, z + z.signum() * 0.04],
                [0.035, 0.59, 0.055],
            );
        }
        for part in &mut self.0[start..] {
            part.roof_lot = Some(usize::MAX);
            part.wall_normal = Some(Vec3::new(0., 0., z.signum()));
        }
    }
}
include!("building_models.rs");
include!("yard_models.rs");
include!("military_models.rs");
include!("vehicle_models.rs");
include!("aircraft_models.rs");
include!("aircraft_geometry.rs");
include!("model_refinement.rs");
include!("apache_authored.rs");
include!("huey_models.rs");
include!("c130_models.rs");
pub fn parts(p: &Piece) -> Vec<Part> {
    let mut k = Kit::default();
    if p.kind == "objective" {
        k.block(Finish::Concrete, [0., 0.1, 0.], [3.2, 0.2, 3.2]);
        k.block(Finish::Dark, [0., 0.35, 0.], [1.3, 0.45, 1.3]);
        k.cylinder(
            Finish::Steel,
            [0., 2.4, 0.],
            [0.12, 4.5, 0.12],
            Quat::IDENTITY,
        );
        k.block(Finish::Team, [0.9, 3.8, 0.], [1.8, 0.85, 0.035]);
        for x in [-1.2, 1.2] {
            k.block(Finish::Wood, [x, 0.3, 1.2], [0.7, 0.5, 0.4]);
        }
    } else if p.kind == "town" || p.kind == "village" {
        let fallback;
        let lots = if p.lots.is_empty() {
            fallback = crate::settlement_layout::lots(&p.kind, &p.id);
            &fallback
        } else {
            &p.lots
        };
        for (index, lot) in lots.iter().enumerate() {
            let start = k.0.len();
            k.house(lot);
            for part in &mut k.0[start..] {
                if part.roof_lot.is_some() {
                    part.roof_lot = Some(index);
                }
            }
        }
    } else if let Some(end) = p.bridge_end {
        let length = Vec2::new(end[0] - p.x, end[1] - p.z).length();
        k.block(Finish::Wood, [0., 0.25, 0.], [2.4, 0.5, length]);
        for x in [-1.1, 1.1] {
            k.block(Finish::Steel, [x, 0.95, 0.], [0.12, 0.10, length]);
            for i in 0..=(length / 4.) as i32 {
                k.block(
                    Finish::Steel,
                    [x, 0.6, -length * 0.5 + i as f32 * 4.],
                    [0.12, 0.7, 0.12],
                );
            }
        }
    } else if p.building {
        k.military_building(&p.kind);
        let hash = crate::destruction::seed(&p.id);
        for part in &mut k.0 {
            if part.finish == Finish::Plaster {
                part.finish = [
                    Finish::Ivory,
                    Finish::Sage,
                    Finish::Ochre,
                    Finish::BluePlaster,
                ][(hash as usize) % 4];
            } else if part.finish == Finish::Roof {
                part.finish = [Finish::SlateRoof, Finish::GreenRoof, Finish::Terracotta]
                    [(hash as usize / 4) % 3];
            }
        }
        if p.max_hp > 0. && p.hp > 0. {
            k.0 = crate::destruction::damaged_parts(k.0, p.hp / p.max_hp, hash);
        }
    } else if p.category == "naval" {
        let carrier = p.kind == "aircraftCarrier";
        let length = if carrier { 8.5 } else { 4.5 };
        let width = if carrier { 3.0 } else { 1.8 };
        k.add(
            Shape::Hull,
            Finish::Steel,
            [0., 0.25, 0.],
            [width, 1.0, length],
        );
        k.block(
            Finish::Cloth,
            [0., 0.60, -0.1],
            [width * 0.9, 0.16, length * 0.8],
        );
        k.block(
            Finish::Team,
            [width * 0.24, 1.2, -length * 0.2],
            [width * 0.35, 1.1, 1.6],
        );
        k.windows(width * 0.24, 1.3, -length * 0.2 + 0.83, width * 0.35);
        k.cylinder(
            Finish::Steel,
            [width * 0.24, 2.4, -length * 0.2],
            [0.07, 1.8, 0.07],
            Quat::IDENTITY,
        );
        k.sphere(
            Finish::White,
            [width * 0.24, 2.4, -length * 0.2],
            [0.45, 0.4, 0.45],
        );
        if carrier {
            for z in -3..4 {
                k.block(Finish::White, [-0.6, 0.70, z as f32], [0.10, 0.025, 0.65]);
            }
        } else {
            k.block(Finish::Wood, [0., 0.9, 0.85], [1.1, 0.5, 1.2]);
        }
    } else if p.category == "vehicle" {
        k.ground_vehicle(&p.kind);
    } else if p.category == "aircraft" {
        k.aircraft(p);
    } else {
        k.infantry(0., 0., &p.kind);
    }
    if p.training_target {
        let y = if p.category == "troop" { 1.75 } else { 1.9 };
        k.cylinder(
            Finish::Red,
            [0., y, 0.],
            [0.48, 0.05, 0.48],
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        k.cylinder(
            Finish::White,
            [0., y, 0.04],
            [0.31, 0.025, 0.31],
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        k.cylinder(
            Finish::Red,
            [0., y, 0.06],
            [0.15, 0.02, 0.15],
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
    }
    k.0
}
pub fn soldiers(_p: &Piece) -> usize {
    1
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponSocket {
    pub at: Vec3,
    pub forward: Vec3,
    pub pivot: Vec3,
    pub role: i8,
}
#[derive(Component, Default)]
pub struct WeaponSockets(pub Vec<WeaponSocket>);
impl WeaponSockets {
    pub fn from_parts(parts: &[Part]) -> Self {
        Self(
            parts
                .iter()
                .filter_map(|part| {
                    part.socket.map(|tip| WeaponSocket {
                        at: part.transform.transform_point(tip),
                        forward: (part.transform.rotation * tip).normalize_or(Vec3::Z),
                        pivot: part.pivot,
                        role: part.joint,
                    })
                })
                .collect(),
        )
    }
}
pub fn battlefield(kind: &str, destroyed: bool) -> Vec<Part> {
    let mut k = Kit::default();
    if kind == "crater" {
        // Persistent soil geometry is draped onto terrain in ground_impacts.
        return vec![];
    } else if destroyed && kind == "sandbags" {
        for i in 0..15 {
            let angle = i as f32 * 2.4;
            k.add(
                Shape::Capsule,
                Finish::Roof,
                [
                    angle.cos() * (0.6 + (i % 3) as f32 * 0.7),
                    0.12,
                    angle.sin() * 1.3,
                ],
                [0.27, 0.85, 0.48],
            );
            k.rotate(
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_y(angle),
            );
        }
    } else if destroyed && kind == "crates" {
        for i in 0..18 {
            let angle = i as f32 * 2.4;
            k.block(
                Finish::Wood,
                [
                    angle.cos() * 1.7,
                    0.08 + (i % 3) as f32 * 0.06,
                    angle.sin() * 1.2,
                ],
                [0.13, 0.10, 0.7 + (i % 4) as f32 * 0.15],
            );
            k.rotate(Quat::from_euler(EulerRot::XYZ, 0.12, angle, 0.18));
        }
    } else if destroyed || kind == "wreck" {
        for i in 0..7 {
            let angle = i as f32 * 2.4;
            k.block(
                if kind == "wreck" {
                    Finish::Dark
                } else {
                    Finish::Concrete
                },
                [
                    angle.cos() * 1.6,
                    0.15 + (i % 3) as f32 * 0.12,
                    angle.sin() * 1.2,
                ],
                [0.8 + (i % 2) as f32 * 0.4, 0.3, 0.5],
            );
            k.rotate(Quat::from_rotation_y(angle));
        }
    } else if kind == "sandbags" {
        for row in 0..3 {
            for column in 0..5 {
                k.add(
                    Shape::Capsule,
                    Finish::Roof,
                    [
                        -1.7 + column as f32 * 0.8 + (row % 2) as f32 * 0.15,
                        0.2 + row as f32 * 0.3,
                        0.,
                    ],
                    [0.34, 0.95, 0.55],
                );
                k.rotate(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
            }
        }
    } else if kind == "wall" {
        k.block(Finish::Concrete, [0., 0.65, 0.], [4.2, 1.3, 0.45]);
        for x in [-2., 0., 2.] {
            k.block(Finish::Concrete, [x, 0.75, 0.], [0.35, 1.5, 0.75]);
        }
    } else {
        for i in 0..3 {
            let x = i as f32 * 1.1 - 1.1;
            k.block(Finish::Wood, [x, 0.45, 0.], [1., 0.9, 1.]);
            for z in [-0.51, 0.51] {
                k.block(Finish::Steel, [x, 0.45, z], [0.08, 0.9, 0.025]);
            }
        }
        k.block(Finish::Wood, [0., 1.15, 0.], [0.9, 0.5, 0.9]);
    }
    k.0
}

pub fn house_parts(lot: &crate::settlement_layout::Lot) -> Vec<Part> {
    let mut kit = Kit::default();
    kit.house(lot);
    kit.0
}

pub fn rubble_parts(size: [f32; 2], hash: u32, wall: Finish, roof: Finish) -> Vec<Part> {
    let mut kit = Kit::default();
    kit.rubble_pile(
        &[crate::settlement_layout::Wing { at: [0., 0.], size }],
        hash,
        wall,
        roof,
    );
    kit.0
}

/// Base dressing stays separate from buildings, so a destroyed facility leaves its pad.
pub fn base_compound() -> Vec<Part> {
    use Finish::*;
    let mut k = Kit::default();
    // Resized service spine, access roads, lighting and the complete perimeter.
    k.block(Asphalt, [10., 0.035, 0.], [6., 0.07, 330.]);
    for (z, width) in [
        (-87., 25.),
        (-46., 31.),
        (-17., 25.),
        (14., 25.),
        (43., 25.),
    ] {
        // Branches meet the front doors and garage apron, rather than crossing the buildings.
        k.block(Gravel, [13. - width * 0.5, 0.04, z], [width, 0.08, 6.]);
    }
    for z in [-17., 65.] {
        k.block(Asphalt, [18., 0.05, z], [18., 0.10, 6.]);
    }
    for z in (-160..=160).step_by(8) {
        k.block(White, [10., 0.08, z as f32], [0.12, 0.02, 3.]);
    }
    for z in (-176..=176).step_by(8) {
        k.block(Concrete, [-19., 0.58, z as f32], [0.35, 1.16, 5.8]);
        k.block(Concrete, [132., 0.58, z as f32], [0.35, 1.16, 5.8]);
    }
    for z in [-178., 178.] {
        for x in (-14..=130).step_by(8) {
            if (2..=18).contains(&x) {
                continue;
            }
            k.block(Concrete, [x as f32, 0.58, z], [5.8, 1.16, 0.35]);
        }
    }
    for z in (-145..=145).step_by(29) {
        k.cylinder(Steel, [18., 4., z as f32], [0.1, 8., 0.1], Quat::IDENTITY);
        k.block(Light, [18.5, 7.8, z as f32], [0.55, 0.12, 0.4]);
    }
    for x in [-10., -7., -4.] {
        k.block(Wood, [x, 0.5, 80.], [2., 1., 1.5]);
        k.block(Steel, [x, 0.5, 80.], [0.1, 1.02, 1.52]);
    }
    k.cylinder(Steel, [12., 4., 80.], [0.12, 8., 0.12], Quat::IDENTITY);
    k.block(Team, [13., 7., 80.], [2., 1.2, 0.04]);
    k.0
}

pub fn settlement_ground(p: &Piece, terrain: &crate::terrain::Terrain) -> Vec<(Finish, Mesh)> {
    if p.label == "PRACTICE HOUSES" {
        return vec![(Finish::Gravel, practice_lane(p, terrain))];
    }
    if p.town_plan == crate::settlement_layout::TownPlan::Rural
        || (terrain.id == crate::battlefield::ID && ["town", "village"].contains(&p.kind.as_str()))
    {
        return crate::rural_surfaces::ground(p, terrain);
    }
    let extent = if p.kind == "town" { 44 } else { 38 };
    let rural = p.town_plan == crate::settlement_layout::TownPlan::Rural;
    let step = if rural { 0.5 } else { 1. };
    let (ex, ez) = if rural {
        (304, 200)
    } else if p.town_plan == crate::settlement_layout::TownPlan::Joined {
        (134, 60)
    } else {
        (extent, extent)
    };
    let width = ex * 2 + 1;
    let height = ez * 2 + 1;
    // Sample the existing road surface in the village's own orientation. A
    // finer grid avoids the sawtooth edges of diagonal metre-wide raster cells.
    let point = |x: f32, z: f32| {
        let at = [x * step, z * step];
        if rural {
            crate::rural_layout::world(at)
        } else {
            at
        }
    };
    let mut positions = vec![];
    for z in -ez..=ez {
        for x in -ex..=ex {
            let at = point(x as f32, z as f32);
            positions.push([
                at[0],
                crate::landscape::visual_height(terrain, p.x + at[0], p.z + at[1]) - p.elevation
                    + 0.045,
                at[1],
            ]);
        }
    }
    let roads = crate::settlement_layout::roads_for(&p.kind, p.town_plan);
    let driveways: Vec<_> = p
        .lots
        .iter()
        .map(|lot| {
            let door = lot.door().0;
            let end = roads
                .iter()
                .map(|r| {
                    let a = Vec2::from_array(r.a);
                    let d = Vec2::from_array(r.b) - a;
                    (a + d
                        * ((Vec2::from_array(door) - a).dot(d) / d.length_squared()).clamp(0., 1.))
                    .to_array()
                })
                .min_by(|a, b| {
                    Vec2::from_array(*a)
                        .distance_squared(Vec2::from_array(door))
                        .total_cmp(&Vec2::from_array(*b).distance_squared(Vec2::from_array(door)))
                })
                .unwrap();
            (door, end)
        })
        .collect();
    let plazas = p.town_plan.plazas();
    let mut groups: [Vec<u32>; 3] = std::array::from_fn(|_| vec![]);
    for z in 0..height - 1 {
        for x in 0..width - 1 {
            let q = point((x - ex) as f32 + 0.5, (z - ez) as f32 + 0.5);
            let road = roads
                .iter()
                .map(|r| r.distance(q))
                .fold(f32::INFINITY, f32::min);
            let material = if !rural
                && plazas
                    .iter()
                    .any(|plaza| (q[0] - plaza[0]).abs() < 7. && (q[1] - plaza[1]).abs() < 7.)
            {
                Some(1)
            } else if road <= 0. {
                Some(
                    if rural && crate::rural_layout::dirt_road(crate::rural_layout::local(q)) {
                        2
                    } else {
                        0
                    },
                )
            } else if rural
                && plazas
                    .iter()
                    .any(|p| (q[0] - p[0]).hypot(q[1] - p[1]) < 4.5)
            {
                Some(2)
            } else if road
                < 1.45 + (crate::natural_noise::value(q[0] * 0.7, q[1] * 0.7, 977) - 0.5) * 0.6
                || driveways.iter().any(|(a, b)| {
                    let d = Vec2::from_array(*b) - Vec2::from_array(*a);
                    let t = ((Vec2::from_array(q) - Vec2::from_array(*a)).dot(d)
                        / d.length_squared().max(0.001))
                    .clamp(0., 1.);
                    Vec2::from_array(q).distance(Vec2::from_array(*a) + d * t) < 0.75
                })
            {
                Some(2)
            } else {
                None
            };
            if let Some(material) = material {
                let a = (z * width + x) as u32;
                groups[material].extend([
                    a,
                    a + width as u32,
                    a + 1,
                    a + 1,
                    a + width as u32,
                    a + width as u32 + 1,
                ]);
            }
        }
    }
    groups
        .into_iter()
        .enumerate()
        .filter(|(_, g)| !g.is_empty())
        .map(|(index, indices)| {
            let mut remap = vec![u32::MAX; positions.len()];
            let mut used = vec![];
            let indices = indices
                .into_iter()
                .map(|old| {
                    let slot = &mut remap[old as usize];
                    if *slot == u32::MAX {
                        *slot = used.len() as u32;
                        used.push(positions[old as usize]);
                    }
                    *slot
                })
                .collect::<Vec<_>>();
            let colors: Vec<[f32; 4]> = used
                .iter()
                .map(|p| {
                    let q = Vec2::new(p[0], p[2]);
                    let patch = crate::natural_noise::value(q.x / 3.8, q.y / 3.8, 417);
                    let nearest = roads.iter().min_by(|a, b| {
                        a.distance(q.to_array())
                            .total_cmp(&b.distance(q.to_array()))
                    });
                    let mut tint = Vec3::splat(0.82 + patch * 0.18);
                    if let Some(road) = nearest {
                        let a = Vec2::from_array(road.a);
                        let d = (Vec2::from_array(road.b) - a).normalize_or_zero();
                        let across = (q - a).dot(Vec2::new(-d.y, d.x)).abs();
                        let tracks = (-((across - road.width * 0.24) / 0.42).powi(2)).exp();
                        let edge =
                            (1. - (-road.distance(q.to_array()) / 1.4).clamp(0., 1.)).powi(2);
                        if index == 0 {
                            let repairs = crate::natural_noise::value(q.x / 1.3, q.y / 6.7, 177);
                            tint =
                                Vec3::splat(0.75 + patch * 0.18 - tracks * 0.16 - repairs * 0.08)
                                    .lerp(Vec3::new(0.75, 0.66, 0.49), edge * 0.48);
                        } else {
                            tint *= Vec3::new(0.98, 0.96, 0.90);
                        }
                    }
                    [tint.x, tint.y, tint.z, 1.]
                })
                .collect();
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, used)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(indices));
            mesh.compute_smooth_normals();
            crate::surfaces::uv(&mut mesh);
            (
                [Finish::Asphalt, Finish::Concrete, Finish::Gravel][index],
                mesh,
            )
        })
        .collect()
}
#[cfg(test)]
pub fn lane(p: &Piece, terrain: &crate::terrain::Terrain) -> Mesh {
    settlement_ground(p, terrain).remove(0).1
}

#[cfg(test)]
mod tests {
    #[test]
    fn house_material_coordinates_follow_roofs_and_walls_when_a_lot_turns() {
        use super::*;
        use bevy::mesh::VertexAttributeValues;
        let shapes = shape_meshes();
        let mut lot = crate::settlement_layout::lots("village", "uv-calibration").remove(0);
        lot.at = [0., 0.];
        lot.yaw = 0.;
        lot.integrity = 1.;
        let straight = bake(&house_parts(&lot), &shapes);
        lot.yaw = 0.73;
        let turned = bake(&house_parts(&lot), &shapes);
        assert_eq!(straight.len(), turned.len());
        for ((finish, a), (other, b)) in straight.iter().zip(&turned) {
            assert_eq!(finish, other);
            let (
                Some(VertexAttributeValues::Float32x2(a)),
                Some(VertexAttributeValues::Float32x2(b)),
            ) = (
                a.attribute(Mesh::ATTRIBUTE_UV_0),
                b.attribute(Mesh::ATTRIBUTE_UV_0),
            )
            else {
                panic!("house surface UVs");
            };
            assert_eq!(
                a, b,
                "rotating a house must not rotate or compress its material pattern"
            );
        }
    }
    #[test]
    fn a10_paint_keeps_authored_uvs_through_shape_preparation_and_batching() {
        use super::*;
        use bevy::mesh::VertexAttributeValues;
        let mut shapes = shape_meshes();
        let shape = &mut shapes[Shape::A10Body as usize];
        let before = shape.attribute(Mesh::ATTRIBUTE_UV_0).unwrap().clone();
        crate::surfaces::uv(shape);
        assert_eq!(shape.attribute(Mesh::ATTRIBUTE_UV_0).unwrap(), &before);
        let p = Piece {
            kind: "plane".into(),
            category: "aircraft".into(),
            ..default()
        };
        let parts: Vec<_> = parts(&p)
            .into_iter()
            .filter(|p| matches!(p.shape, Shape::A10Body))
            .collect();
        let baked = bake(&parts, &shapes);
        assert_eq!(baked[0].0, Finish::A10Livery);
        let Some(VertexAttributeValues::Float32x2(uv)) = baked[0].1.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("Missing paint UVs")
        };
        assert_eq!(uv.len(), baked[0].1.count_vertices());
        assert_eq!(baked[0].1.attribute(Mesh::ATTRIBUTE_UV_0).unwrap(), &before);
        assert!(uv.iter().flatten().all(|v| (0. ..=1.).contains(v)));
    }
    #[test]
    fn tank_nose_has_a_broad_glacis_instead_of_a_single_point() {
        let mesh = prism(true);
        let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        let front: Vec<_> = vertices.iter().filter(|p| p[2] > 0.5).collect();
        assert!(front.iter().any(|p| p[0] < -0.35) && front.iter().any(|p| p[0] > 0.35));
        assert!(front.iter().all(|p| p[0].abs() > 0.35));
    }
    #[test]
    fn an_occupied_house_keeps_the_soldiers_full_body_for_the_cutaway_view() {
        use super::*;
        let p = Piece {
            kind: "short".into(),
            category: "troop".into(),
            garrison: Some(crate::simulation::Garrison {
                building: "house".into(),
                lot: Some(0),
                window: 0,
                entered: true,
                ready_at: 0,
                door_stage: 2,
                exiting: false,
            }),
            ..Default::default()
        };
        let parts = parts(&p);
        assert!(parts.iter().any(|p| p.joint.abs() == 11));
        assert!(parts.iter().any(|p| p.joint.abs() == 12));
        assert!(parts.iter().any(|p| p.transform.translation.y < 0.3));
    }
    #[test]
    fn aircraft_meshes_bake_with_valid_faces_and_fit_their_airfield_stands() {
        use super::*;
        let shapes = shape_meshes();
        for (kind, scale, half_width, half_length) in [
            ("plane", 2.7, 7., 7.),
            ("helicopter", 1.8, 5.1, 6.6),
            ("huey", 1.8, 7.4, 10.6),
        ] {
            let piece = Piece {
                kind: kind.into(),
                category: "aircraft".into(),
                model_scale: scale,
                ..default()
            };
            let batches = bake_animated(parts(&piece), &shapes);
            if kind == "plane" {
                let triangles: usize = batches
                    .iter()
                    .map(|(_, _, _, mesh)| mesh.indices().unwrap().len() / 3)
                    .sum();
                assert!(
                    triangles <= 14000,
                    "A-10 triangle budget exceeded: {triangles}"
                );
            }
            let mut min = Vec3::splat(f32::MAX);
            let mut max = Vec3::splat(f32::MIN);
            for (_, _, _, mesh) in batches {
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("missing positions");
                };
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) =
                    mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                else {
                    panic!("missing normals");
                };
                assert_eq!(p.len(), n.len());
                for (p, n) in p.iter().zip(n) {
                    let p = Vec3::from_array(*p) * scale;
                    assert!(p.is_finite());
                    assert!((Vec3::from_array(*n).length() - 1.).abs() < 0.001);
                    min = min.min(p);
                    max = max.max(p);
                }
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.chunks_exact(3) {
                    let a = Vec3::from_array(p[tri[0]]);
                    let b = Vec3::from_array(p[tri[1]]);
                    let c = Vec3::from_array(p[tri[2]]);
                    assert!(
                        (b - a).cross(c - a).length() > 0.0000001,
                        "degenerate aircraft face"
                    );
                }
            }
            assert!(
                min.x >= -half_width && max.x <= half_width,
                "{kind} width {min:?} {max:?}"
            );
            assert!(
                min.z >= -half_length && max.z <= half_length,
                "{kind} length {min:?} {max:?}"
            );
            assert!(min.y >= -0.001, "wheels below the stand surface");
        }
    }
    #[test]
    fn both_rifle_hands_stay_in_contact_through_aim_recoil_and_walking() {
        for kind in ["short", "scout", "commander", "engineer", "machineGunner"] {
            let p = crate::runtime::Piece {
                kind: kind.into(),
                category: "troop".into(),
                ..Default::default()
            };
            let parts = super::parts(&p);
            let hands: Vec<_> = parts
                .iter()
                .filter(|p| p.finish == super::Finish::Skin && p.joint == 4)
                .collect();
            assert_eq!(hands.len(), 2, "{kind}");
            let rifle = parts
                .iter()
                .find(|p| {
                    p.finish == super::Finish::Dark && p.joint == 4 && p.transform.scale.z > 0.5
                })
                .unwrap();
            for state in [
                crate::animation::Pose::default(),
                crate::animation::Pose {
                    speed: 3.,
                    aim: 0.8,
                    aiming: 1.,
                    shooting: 1.,
                    crouch: 0.8,
                    ..Default::default()
                },
            ] {
                for phase in [0., 1., 2., 3., 4., 5.] {
                    let gun = crate::animation::joint(
                        rifle.transform,
                        rifle.pivot,
                        rifle.joint,
                        &state,
                        phase,
                    );
                    for hand in &hands {
                        let grip = crate::animation::joint(
                            hand.transform,
                            hand.pivot,
                            hand.joint,
                            &state,
                            phase,
                        );
                        let local = gun
                            .compute_affine()
                            .inverse()
                            .transform_point3(grip.translation);
                        let separation = ((local.abs() - bevy::prelude::Vec3::splat(0.5))
                            .max(bevy::prelude::Vec3::ZERO)
                            * rifle.transform.scale)
                            .length();
                        assert!(
                            separation < 0.065,
                            "{kind} lost rifle contact: {separation}"
                        );
                    }
                }
            }
        }
    }
    use super::*;
    #[test]
    fn every_infantry_entity_is_one_man_even_when_damaged_or_imported() {
        for kind in [
            "short",
            "scout",
            "medic",
            "engineer",
            "commander",
            "machineGunner",
        ] {
            for (hp, downed) in [(3., false), (1., false), (0.01, true)] {
                let piece = Piece {
                    kind: kind.into(),
                    category: "troop".into(),
                    hp,
                    max_hp: 3.,
                    downed,
                    ..default()
                };
                assert_eq!(soldiers(&piece), 1);
                let kit = parts(&piece);
                // Exactly one face/head, rather than a replicated formation.
                assert_eq!(
                    kit.iter()
                        .filter(|p| p.finish == Finish::Skin && p.joint == 3)
                        .count(),
                    1
                );
                assert!(
                    kit.iter()
                        .filter(|p| p.finish == Finish::Cloth)
                        .any(|p| matches!(p.shape, Shape::Capsule))
                );
            }
        }
    }
    #[test]
    fn baking_joint_groups_preserves_animated_geometry_and_reduces_draws() {
        let shapes = shape_meshes();
        let vehicle = Piece {
            kind: "medium".into(),
            category: "vehicle".into(),
            ..default()
        };
        let parts = parts(&vehicle);
        let count = parts.len();
        let batches = bake_animated(parts, &shapes);
        assert!(batches.len() < count / 2);
        let base = Transform::from_xyz(0.3, 1.2, 2.)
            .with_rotation(Quat::from_rotation_z(0.2))
            .with_scale(Vec3::new(0.2, 2., 0.3));
        let pivot = Vec3::new(0., 0.8, 0.);
        let point = Vec3::new(0.1, 0.4, -0.2);
        let pose = crate::animation::Pose {
            speed: 2.,
            shooting: 0.6,
            treating: 0.3,
            aim: 0.4,
            ..default()
        };
        for role in [1, 11, -2, 3, 4, 20, 21, 22, 30] {
            let original =
                crate::animation::joint(base, pivot, role, &pose, 1.1).transform_point(point);
            let batched = crate::animation::joint(Transform::IDENTITY, pivot, role, &pose, 1.1)
                .transform_point(base.transform_point(point));
            assert!(original.distance(batched) < 0.00001, "role {role}");
        }
    }
    #[test]
    fn static_batches_preserve_transforms_normals_and_valid_indices() {
        let shape = bevelled_box();
        let count = shape.count_vertices();
        let parts = [-3., 3.].map(|x| Part {
            shape: Shape::Box,
            finish: Finish::Concrete,
            transform: Transform::from_xyz(x, 2., 1.).with_scale(Vec3::new(2., 3., 4.)),
            joint: 0,
            pivot: Vec3::ZERO,
            socket: None,
            roof_lot: None,
            wall_normal: None,
            surface_frame: None,
        });
        let batches = bake(&parts, &[shape]);
        assert_eq!(batches.len(), 1);
        let mesh = &batches[0].1;
        assert_eq!(mesh.count_vertices(), count * 2);
        assert!(
            mesh.indices()
                .unwrap()
                .iter()
                .all(|i| i < mesh.count_vertices())
        );
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        assert!(
            normals
                .iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.).abs() < 0.0001)
        );
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        assert!(positions.iter().all(|p| p[1] >= 0.5 && p[1] <= 3.5));
        assert!(positions.iter().any(|p| (p[0] - 4.).abs() < 0.001));
        assert!(positions.iter().any(|p| (p[0] + 4.).abs() < 0.001));
    }
    #[test]
    fn bevelled_geometry_has_outward_unit_normals() {
        let mesh = bevelled_box();
        let bevy::mesh::VertexAttributeValues::Float32x3(p) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        let bevy::mesh::VertexAttributeValues::Float32x3(n) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
        else {
            panic!()
        };
        assert!(
            p.iter()
                .zip(n)
                .all(|(p, n)| Vec3::from_array(*p).dot(Vec3::from_array(*n)) > 0.
                    && (Vec3::from_array(*n).length() - 1.).abs() < 0.00001)
        );
    }
    #[test]
    fn town_lane_follows_the_ground_with_upward_faces() {
        let terrain = crate::terrain::Terrain::new("forward-base", 128, 128);
        let p = Piece {
            x: 50.,
            z: 50.,
            elevation: terrain.height(50., 50.),
            ..default()
        };
        let mesh = lane(&p, &terrain);
        let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        for [x, y, z] in vertices {
            assert!(
                (y + p.elevation
                    - crate::landscape::visual_height(&terrain, p.x + x, p.z + z)
                    - 0.045)
                    .abs()
                    < 0.0001
            );
            assert!((y + p.elevation - terrain.height(p.x + x, p.z + z) - 0.045).abs() < 0.121);
        }
        let bevy::mesh::VertexAttributeValues::Float32x3(normals) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
        else {
            panic!()
        };
        assert!(
            normals
                .iter()
                .all(|n| Vec3::from_array(*n).is_finite() && n[1] > 0.)
        );
        assert!(mesh.indices().unwrap().iter().all(|i| i < vertices.len()));
    }
    #[test]
    fn every_catalog_type_has_valid_detailed_geometry() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../catalog.json")).unwrap();
        for (category, building) in [("units", false), ("buildings", true)] {
            for (kind, spec) in catalog[category].as_object().unwrap() {
                let p = Piece {
                    kind: kind.clone(),
                    category: spec["category"].as_str().unwrap_or("").into(),
                    building,
                    ..default()
                };
                let model = parts(&p);
                assert!(!model.is_empty(), "missing {kind}");
                assert!(model.iter().all(|p| p.transform.translation.is_finite()
                    && p.transform.scale.is_finite()
                    && p.transform.scale.cmpgt(Vec3::ZERO).all()));
            }
        }
    }
}
