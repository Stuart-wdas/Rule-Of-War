//! Small household plots using the existing garden kit. All coordinates are
//! metres in the settlement frame; roads and authoritative door routes stay open.
use crate::{
    runtime::Piece,
    settlement_layout::{Lot, Road, TownPlan},
    terrain::Terrain,
};
use bevy::prelude::*;

pub struct Site<'a> {
    pub piece: &'a Piece,
    pub terrain: &'a Terrain,
    pub roads: Vec<Road>,
    pub entrances: Vec<Road>,
}
impl<'a> Site<'a> {
    pub fn new(piece: &'a Piece, terrain: &'a Terrain) -> Option<Self> {
        if terrain.id != crate::battlefield::ID
            || piece.lots.is_empty()
            || !matches!(piece.kind.as_str(), "town" | "village")
        {
            return None;
        }
        let roads = if piece.kind == "village" {
            vec![
                Road {
                    a: [0., -33.],
                    b: [0., 33.],
                    width: 3.2,
                },
                Road {
                    a: [-15., 0.],
                    b: [15., 0.],
                    width: 3.2,
                },
            ]
        } else {
            crate::settlement_layout::roads_for(&piece.kind, piece.town_plan)
        };
        let entrances = piece
            .lots
            .iter()
            .filter_map(|lot| {
                let door = Vec2::from_array(lot.door().0);
                let nearest = roads
                    .iter()
                    .map(|r| {
                        let a = Vec2::from_array(r.a);
                        let d = Vec2::from_array(r.b) - a;
                        a + d * ((door - a).dot(d) / d.length_squared().max(0.01)).clamp(0., 1.)
                    })
                    .min_by(|a, b| {
                        a.distance_squared(door)
                            .total_cmp(&b.distance_squared(door))
                    })?;
                Some(Road {
                    a: door.to_array(),
                    b: nearest.to_array(),
                    width: 2.8,
                })
            })
            .collect();
        Some(Self {
            piece,
            terrain,
            roads,
            entrances,
        })
    }
    pub fn world(&self, p: Vec2) -> Vec2 {
        let (s, c) = self.piece.heading.sin_cos();
        Vec2::new(self.piece.x, self.piece.z)
            + Vec2::new(c * p.x + s * p.y, -s * p.x + c * p.y) * self.piece.model_scale
    }
    pub fn height(&self, p: Vec2) -> f32 {
        let world = self.world(p);
        (crate::landscape::visual_height(self.terrain, world.x, world.y) - self.piece.elevation)
            / self.piece.model_scale.max(0.01)
    }
    pub fn clear(&self, p: Vec2, margin: f32) -> bool {
        self.clear_space(p, margin, true)
    }
    pub fn clear_ground(&self, p: Vec2, margin: f32) -> bool {
        self.clear_space(p, margin, false)
    }
    fn clear_space(&self, p: Vec2, margin: f32, entrances: bool) -> bool {
        let road_margin = margin + if entrances { 0.25 } else { 0. };
        if self
            .piece
            .lots
            .iter()
            .any(|l| l.contains(p.to_array(), margin))
            || self
                .roads
                .iter()
                .any(|r| r.distance(p.to_array()) < road_margin)
            || (entrances
                && self
                    .entrances
                    .iter()
                    .any(|r| r.distance(p.to_array()) < margin + 0.25))
            || self
                .piece
                .town_plan
                .plazas()
                .iter()
                .any(|a| p.distance(Vec2::from_array(*a)) < 5. + margin)
        {
            return false;
        }
        let world = self.world(p);
        if world.x < 1.
            || world.y < 1.
            || world.x > (self.terrain.cols - 2) as f32
            || world.y > (self.terrain.rows - 2) as f32
            || self.terrain.cell(world.x, world.y).water()
        {
            return false;
        }
        let chart =
            crate::rural_layout::map_point(world.x, world.y, self.terrain.cols, self.terrain.rows);
        if crate::rural_layout::surface_distance_for(chart, self.terrain.cols, self.terrain.rows)
            < road_margin
        {
            return false;
        }
        if self.piece.town_plan == TownPlan::Rural
            && crate::rural_layout::field_at_for(chart, self.terrain.cols, self.terrain.rows)
                .is_some_and(|f| f.crop && f.inset(chart) > 0.5)
        {
            return false;
        }
        true
    }
    pub fn clear_box(&self, center: Vec2, half: Vec2, yaw: f32, margin: f32) -> bool {
        let (s, c) = yaw.sin_cos();
        [-1., 0., 1.].into_iter().all(|z| {
            [-1., 0., 1.].into_iter().all(|x| {
                let p = Vec2::new(x * half.x, z * half.y);
                self.clear(
                    center + Vec2::new(c * p.x + s * p.y, -s * p.x + c * p.y),
                    margin,
                )
            })
        })
    }
}

pub struct Yard<'a> {
    pub lot: &'a Lot,
    pub style: usize,
}
impl<'a> Yard<'a> {
    pub fn all(piece: &'a Piece) -> impl Iterator<Item = Self> {
        // House plan already cycles in the low style bits. Use one palette
        // phase per settlement so adding that cycle cannot collapse four
        // household layouts into only the even-numbered variants.
        let phase = piece.lots.first().map_or(0, |l| l.style as usize / 4);
        piece
            .lots
            .iter()
            .enumerate()
            .filter(|(_, l)| l.integrity > 0.)
            .map(move |(i, lot)| Self {
                lot,
                style: (i + phase) % 4,
            })
    }
    pub fn point(&self, x: f32, z: f32) -> Vec2 {
        Vec2::from_array(self.lot.to_world([x, z]))
    }
    pub fn half_width(&self) -> f32 {
        self.lot.width * 0.42 + 2.2
    }
    pub fn rear(&self) -> f32 {
        -self.lot.depth * 0.42 - 3.6
    }
    pub fn front(&self) -> f32 {
        self.lot.local(self.lot.door().0)[1] + 2.2
    }
    pub fn side(&self) -> f32 {
        if self.style.is_multiple_of(2) {
            -1.
        } else {
            1.
        }
    }
}
