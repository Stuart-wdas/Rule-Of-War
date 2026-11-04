//! Source-model destruction shared by the simulation, wreck renderer and flying fragments.
use crate::{
    models::{self, Finish, Part},
    runtime::Piece,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Wreck {
    pub piece: Piece,
    pub born: u64,
    /// Local hull/structure footprint in world metres, excluding scattered fragments.
    pub half_extents: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash: Option<AircraftCrash>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AircraftCrash {
    pub world_y: f32,
    pub vertical_speed: f32,
    pub velocity: [f32; 2],
    pub pitch: f32,
    pub roll: f32,
    pub spin: f32,
}
impl AircraftCrash {
    pub fn valid(&self) -> bool {
        [
            self.world_y,
            self.vertical_speed,
            self.pitch,
            self.roll,
            self.spin,
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 10000.)
            && self
                .velocity
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 100.)
    }
    /// Advance the damaged airframe, retaining momentum and accelerating under gravity.
    pub fn advance(
        &mut self,
        at: &mut [f32; 2],
        heading: &mut f32,
        terrain: &crate::terrain::Terrain,
        dt: f32,
    ) -> bool {
        for (axis, position) in at.iter_mut().enumerate() {
            *position += self.velocity[axis] * dt;
            self.velocity[axis] *= (-0.18 * dt).exp();
        }
        at[0] = at[0].clamp(1., terrain.cols as f32 - 2.);
        at[1] = at[1].clamp(1., terrain.rows as f32 - 2.);
        self.world_y += self.vertical_speed * dt - 0.5 * 9.81 * dt * dt;
        self.vertical_speed -= 9.81 * dt;
        self.pitch = (self.pitch + dt * 0.65).min(1.25);
        self.roll += self.spin * dt;
        *heading += self.spin * 0.45 * dt;
        self.world_y <= terrain.height(at[0], at[1])
    }
}

pub fn heading(piece: &Piece) -> f32 {
    if piece.building {
        piece.heading
    } else {
        std::f32::consts::FRAC_PI_2 - piece.heading
    }
}

pub fn seed(id: &str) -> u32 {
    id.bytes().fold(2166136261u32, |hash, byte| {
        (hash ^ byte as u32).wrapping_mul(16777619)
    })
}

pub fn variation(seed: u32, index: usize) -> f32 {
    let mut n = seed.wrapping_add((index as u32).wrapping_mul(747796405));
    n = (n ^ (n >> 16)).wrapping_mul(2246822519);
    (n ^ (n >> 13)) as f32 / u32::MAX as f32
}

/// Eight stable damage steps keep chipped masonry consistent across saves and clients.
/// The shell stays standing until the simulation exhausts its structural health.
pub fn damage_stage(integrity: f32) -> u8 {
    if integrity >= 1. {
        0
    } else {
        ((1. - integrity.clamp(0., 1.)) * 8.).ceil() as u8
    }
}

pub fn damaged_parts(source: Vec<Part>, integrity: f32, hash: u32) -> Vec<Part> {
    let damage = damage_stage(integrity) as f32 / 8.;
    if damage == 0. {
        return source;
    }
    let mut result = Vec::new();
    let mut rubble = 0;
    for (i, part) in source.into_iter().enumerate() {
        // Preserve foundations; collision and window slots remain until collapse.
        let structural = part.wall_normal.is_some() || part.roof_lot.is_some();
        if !structural || part.transform.translation.y < 0.3 {
            result.push(part);
            continue;
        }
        let dimensions = part.transform.scale;
        let tiled = matches!(part.shape, models::Shape::Box | models::Shape::Slab);
        let counts = if tiled {
            dimensions
                .map(|v| (v / 1.1).ceil().clamp(1., 12.))
                .as_ivec3()
        } else {
            IVec3::ONE
        };
        for z in 0..counts.z {
            for y in 0..counts.y {
                for x in 0..counts.x {
                    let index = i * 4096 + (z * 144 + y * 12 + x) as usize;
                    let cluster = i * 4096 + (z / 2 * 144 + y / 2 * 12 + x / 2) as usize;
                    let n = variation(hash, cluster) * 0.65 + variation(hash, index + 301) * 0.35;
                    let mut chip = part.clone();
                    chip.transform.scale /= counts.as_vec3();
                    let offset = (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5))
                        / counts.as_vec3()
                        - Vec3::splat(0.5);
                    chip.transform.translation += part.transform.rotation * (offset * dimensions);
                    let roof = part.wall_normal.is_none();
                    let loss = damage * if roof { 0.65 } else { 0.46 };
                    if n < loss {
                        // Fallen pieces accumulate around the standing structure, rather than vanish.
                        if rubble < 96 && chip.transform.scale.max_element() > 0.3 {
                            chip.transform.translation += Vec3::new(
                                (variation(hash, index + 911) - 0.5) * 2.,
                                0.,
                                (variation(hash, index + 713) - 0.5) * 2.,
                            );
                            chip.transform.scale =
                                chip.transform.scale.min(Vec3::new(0.8, 0.25, 0.8));
                            chip.transform.translation.y = 0.18 + n * 0.6;
                            chip.transform.rotation =
                                Quat::from_euler(EulerRot::XYZ, n * 2., n * 15., 0.2);
                            chip.roof_lot = None;
                            chip.wall_normal = None;
                            chip.finish = if n < loss * 0.25 {
                                Finish::Charred
                            } else if !roof && n < loss * 0.45 {
                                Finish::Concrete
                            } else {
                                part.finish
                            };
                            result.push(chip);
                            rubble += 1;
                        }
                        continue;
                    }
                    if n < loss + damage * 0.22 {
                        chip.finish = Finish::Charred;
                        chip.transform.scale *= 0.88;
                        chip.transform.rotation *= Quat::from_rotation_z((n - 0.5) * damage * 0.18);
                    }
                    result.push(chip);
                }
            }
        }
    }
    result
}

/// Freeze the source's turret aim before altering any of its geometry.
pub fn source_parts(piece: &Piece) -> Vec<Part> {
    let mut source = piece.clone();
    source.training_target = false;
    let pose = crate::animation::Pose {
        speed: 0.,
        shooting: 0.,
        ..crate::animation::Pose::target(&source)
    };
    models::parts(&source)
        .into_iter()
        .map(|mut part| {
            part.transform =
                crate::animation::joint(part.transform, part.pivot, part.joint, &pose, 0.);
            part.socket = None;
            part
        })
        .collect()
}

impl Wreck {
    pub fn new(mut piece: Piece, born: u64) -> Self {
        let crash = (piece.category == "aircraft" && piece.airborne && piece.flight_height() > 0.5)
            .then(|| AircraftCrash {
                world_y: piece.position().y,
                vertical_speed: -1.5,
                velocity: piece.velocity,
                pitch: piece.flight_pitch,
                roll: piece.flight_bank,
                spin: if seed(&piece.id).is_multiple_of(2) {
                    1.1
                } else {
                    -1.1
                },
            });
        piece.training_target = false;
        piece.velocity = [0., 0.];
        piece.orders.clear();
        piece.activity.clear();
        let scale = piece.model_transform_scale();
        let mut half = Vec2::splat(0.35);
        for part in source_parts(&piece) {
            if piece.category == "vehicle" && (part.joint == 20 || part.joint == 21) {
                continue;
            }
            let t = part.transform;
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    for z in [-0.5, 0.5] {
                        let corner = t.transform_point(Vec3::new(x, y, z)) * scale;
                        half = half.max(Vec2::new(corner.x.abs(), corner.z.abs()));
                    }
                }
            }
        }
        Self {
            piece,
            born,
            half_extents: half.to_array(),
            crash,
        }
    }
    pub fn burning(&self, tick: u64) -> bool {
        !self.piece.building && tick.saturating_sub(self.born) < 360
    }
}

/// Keep the defining hull, tracks, wheels and weapon; crush roofs and break fittings.
/// Deterministic damage means saves and remote snapshots reconstruct the same wreck.
pub fn wreck_parts(wreck: &Wreck) -> Vec<Part> {
    let source = &wreck.piece;
    let hash = seed(&source.id);
    let mut result = Vec::new();
    for (i, mut part) in source_parts(source).into_iter().enumerate() {
        if matches!(part.finish, Finish::Glass | Finish::AircraftGlass) {
            continue;
        }
        let n = variation(hash, i);
        if source.building
            && matches!(part.finish, Finish::Brick | Finish::Plaster)
            && part.transform.scale.x > 3.
            && part.transform.scale.z > 3.
            && part.transform.scale.y > 0.8
        {
            // A solid intact building body becomes a hollow shell with unequal broken walls.
            let original = part.transform;
            for wall in 0..4 {
                let mut broken = part.clone();
                let height = original.scale.y * (0.2 + variation(hash, i + wall + 80) * 0.35);
                let along_x = wall < 2;
                let sign = if wall % 2 == 0 { -1. } else { 1. };
                let offset = if along_x {
                    Vec3::new(0., 0., sign * original.scale.z * 0.5)
                } else {
                    Vec3::new(sign * original.scale.x * 0.5, 0., 0.)
                };
                broken.transform.translation = original.translation + original.rotation * offset;
                broken.transform.translation.y = height * 0.5;
                broken.transform.scale = if along_x {
                    Vec3::new(original.scale.x, height, 0.28)
                } else {
                    Vec3::new(0.28, height, original.scale.z)
                };
                broken.finish = if wall == 0 {
                    Finish::Charred
                } else {
                    part.finish
                };
                result.push(broken);
            }
            continue;
        }
        if source.building
            && matches!(part.shape, models::Shape::Roof)
            && part.transform.scale.x > 3.
        {
            // Tear broad roofs into collapsed sheets instead of laying down an intact roof.
            for panel in 0..6 {
                let mut broken = part.clone();
                let angle = variation(hash, i + panel + 100) * 6.;
                broken.transform.scale *= Vec3::new(0.32, 0.35, 0.48);
                broken.transform.translation = Vec3::new(
                    part.transform.translation.x
                        + (panel % 3) as f32 * part.transform.scale.x * 0.3
                        - part.transform.scale.x * 0.3,
                    0.25 + (panel % 2) as f32 * 0.15,
                    part.transform.translation.z
                        + ((panel / 3) as f32 - 0.5) * part.transform.scale.z * 0.45,
                );
                broken.transform.rotation =
                    Quat::from_euler(EulerRot::XYZ, 0.12, angle * 0.15, 0.18);
                broken.finish = if panel % 2 == 0 {
                    Finish::Charred
                } else {
                    Finish::Rust
                };
                result.push(broken);
            }
            continue;
        }
        let t = &mut part.transform;
        if source.building {
            if t.translation.y > 2. || part.finish == Finish::Roof {
                // Roof sheets, beams and upper masonry collapse into their own footprint.
                t.translation.x += (n - 0.5) * 2.4;
                t.translation.z += (variation(hash, i + 41) - 0.5) * 2.4;
                t.scale.y = t.scale.y.min(0.38);
                t.translation.y = 0.15 + t.scale.y * 0.5 + n * 0.3;
                t.rotation =
                    Quat::from_euler(EulerRot::XYZ, (n - 0.5) * 0.5, n * 2., 0.12) * t.rotation;
            } else if t.scale.y > 0.8 {
                t.scale.y *= 0.45 + n * 0.28;
                t.translation.y = t.scale.y * 0.5;
            }
        } else {
            if matches!(part.joint, 20 | 21) {
                let q = Quat::from_euler(EulerRot::XYZ, 0.09, 0., -0.14);
                t.translation = part.pivot + q * (t.translation - part.pivot) - Vec3::Y * 0.12;
                t.rotation = q * t.rotation;
            }
            // Shed small armour fittings and track links; retain the recognisable chassis.
            let small = t.scale.max_element() < 0.6;
            if small && n > 0.78 {
                let angle = n * std::f32::consts::TAU * 7.;
                t.translation += Vec3::new(angle.cos() * 1.8, 0., angle.sin() * 2.1);
                t.rotation = Quat::from_euler(EulerRot::XYZ, n * 3., angle, n * 5.);
                t.translation.y = t.scale.length() * 0.25 + 0.015;
            } else {
                t.translation.y = (t.translation.y - 0.07).max(t.scale.y.min(0.12) * 0.5);
            }
            if source.category == "aircraft" {
                t.translation.y *= 0.6;
                t.scale.y *= 0.75;
                if t.scale.max_element() > 1.8 && t.scale.min_element() < 0.2 {
                    t.rotation = Quat::from_rotation_z((n - 0.5) * 0.85) * t.rotation;
                    t.scale.z *= 0.65;
                }
            }
        }
        part.finish = match part.finish {
            Finish::Steel | Finish::Brass => {
                if n > 0.6 {
                    Finish::Rust
                } else {
                    Finish::Charred
                }
            }
            Finish::Concrete | Finish::Brick | Finish::Plaster if source.building && n < 0.65 => {
                part.finish
            }
            Finish::Team | Finish::Cloth if n < 0.4 => Finish::Scorched,
            _ => Finish::Charred,
        };
        part.joint = 0;
        result.push(part);
    }
    if source.building
        && matches!(
            source.kind.as_str(),
            "barracks" | "mine" | "vehicleDepot" | "fieldHospital"
        )
    {
        // Facility debris uses its model footprint and is baked with the existing wreck.
        let scale = source.model_transform_scale();
        result.extend(models::rubble_parts(
            [
                (wreck.half_extents[0] * 2. / scale.x).clamp(1., 60.),
                (wreck.half_extents[1] * 2. / scale.z).clamp(1., 60.),
            ],
            hash,
            if source.kind == "mine" {
                Finish::Brick
            } else {
                Finish::Concrete
            },
            Finish::Rust,
        ));
    }
    result
}

/// Eject actual small components of the destroyed model, never generic cubes.
pub fn fragments(wreck: &Wreck) -> Vec<Part> {
    let hash = seed(&wreck.piece.id);
    fracture_parts(source_parts(&wreck.piece), hash, wreck.piece.building)
}

pub fn fracture_parts(source: Vec<Part>, hash: u32, masonry: bool) -> Vec<Part> {
    let mut expanded = vec![];
    for (i, part) in source.into_iter().enumerate() {
        if masonry
            && part.transform.scale.max_element() > 3.
            && matches!(
                part.finish,
                Finish::Brick | Finish::Plaster | Finish::Concrete | Finish::Roof | Finish::Wood
            )
        {
            for fragment in 0..4 {
                let mut broken = part.clone();
                let n = variation(hash, i + fragment + 101);
                broken.transform.translation += Vec3::new(
                    (n - 0.5) * part.transform.scale.x,
                    (n - 0.5) * part.transform.scale.y,
                    (variation(hash, i + fragment + 205) - 0.5) * part.transform.scale.z,
                );
                broken.transform.scale = part.transform.scale.min(Vec3::new(
                    0.45 + n * 0.45,
                    0.22 + n * 0.2,
                    0.5 + n * 0.35,
                ));
                expanded.push(broken);
            }
        } else {
            expanded.push(part);
        }
    }
    let mut parts: Vec<_> = expanded
        .into_iter()
        .enumerate()
        .filter(|(_, p)| {
            !matches!(p.finish, Finish::Glass | Finish::AircraftGlass)
                && p.transform.scale.max_element() < if masonry { 3. } else { 1.3 }
        })
        .collect();
    parts.sort_by(|(a, _), (b, _)| variation(hash, *a).total_cmp(&variation(hash, *b)));
    // New small fittings must not crowd every recognisable component out of
    // the fixed debris allowance. Keep one wheel/tube amongst the armour chips.
    if !masonry
        && let Some(index) = parts.iter().position(|(_, p)| {
            matches!(p.shape, models::Shape::Cylinder) && p.transform.scale.max_element() > 0.15
        })
    {
        parts.swap(0, index);
    }
    parts
        .into_iter()
        .take(14)
        .map(|(_, mut part)| {
            part.finish = match part.finish {
                Finish::Steel | Finish::Brass => Finish::Rust,
                Finish::Team | Finish::Cloth | Finish::AircraftPaint => Finish::Scorched,
                Finish::Light | Finish::Red | Finish::White => Finish::Charred,
                finish => finish,
            };
            part
        })
        .collect()
}

/// Full pressure at the strike, diminishing damage toward the blast boundary.
pub fn blast_falloff(distance: f32, radius: f32) -> f32 {
    if radius < 1. {
        return 1.;
    }
    let t = (distance / radius.max(0.01)).clamp(0., 1.);
    1. - 0.8 * t * t
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wreck_preserves_tank_hull_tracks_and_barrel_and_differs_from_transport() {
        let tank = Wreck::new(
            Piece {
                id: "tank".into(),
                kind: "medium".into(),
                category: "vehicle".into(),
                model_scale: 2.,
                ..default()
            },
            10,
        );
        let parts = wreck_parts(&tank);
        assert!(parts.iter().any(|p| matches!(p.shape, models::Shape::Hull)));
        let barrel = models::parts(&tank.piece)
            .into_iter()
            .filter(|p| p.joint == 21 && matches!(p.shape, models::Shape::Cylinder))
            .map(|p| p.transform.scale.y)
            .fold(0f32, f32::max);
        assert!(barrel > 1.5);
        assert!(
            parts
                .iter()
                .any(|p| matches!(p.shape, models::Shape::Cylinder)
                    && (p.transform.scale.y - barrel).abs() < 0.001)
        );
        assert!(parts.len() > 100);
        assert!(tank.half_extents[1] >= 3.);
        let truck = Wreck::new(
            Piece {
                kind: "troopCarrier".into(),
                ..tank.piece.clone()
            },
            10,
        );
        assert_ne!(parts.len(), wreck_parts(&truck).len());
        assert!(
            fragments(&tank)
                .iter()
                .any(|p| matches!(p.shape, models::Shape::Cylinder))
        );
    }
    #[test]
    fn damage_is_repeatable_and_blast_pressure_decreases() {
        let wreck = Wreck::new(
            Piece {
                id: "depot".into(),
                kind: "vehicleDepot".into(),
                building: true,
                ..default()
            },
            10,
        );
        let a = wreck_parts(&wreck);
        let b = wreck_parts(&wreck);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.transform, b.transform);
        }
        assert!(a.iter().all(|p| p.transform.translation.y < 2.5));
        assert_eq!(blast_falloff(0., 3.), 1.);
        assert!(blast_falloff(2.8, 3.) < blast_falloff(1., 3.));
        assert!(!wreck.burning(10));
    }
}
