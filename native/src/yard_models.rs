// Reuses the same rails, posts, raised beds, timber and bench construction as
// the existing review street. Batch by material, with terrain-following bases.
pub fn yard_parts(piece: &Piece, terrain: &crate::terrain::Terrain) -> Vec<Part> {
    let Some(site) = crate::yards::Site::new(piece, terrain) else {
        return vec![];
    };
    let mut kit = Kit::default();
    for yard in crate::yards::Yard::all(piece) {
        if piece.town_plan != crate::settlement_layout::TownPlan::Rural {
            let w = yard.half_width();
            let back = yard.rear();
            let front = yard.front();
            for (a, b) in [
                ([-w, back], [w, back]),
                ([-w, back], [-w, front]),
                ([w, back], [w, front]),
            ] {
                kit.yard_fence(&site, yard.point(a[0], a[1]), yard.point(b[0], b[1]));
            }
            let door = yard.lot.local(yard.lot.door().0)[0];
            for (a, b) in [(-w, door - 1.65), (door + 1.65, w)] {
                if b > a {
                    kit.yard_fence(&site, yard.point(a, front), yard.point(b, front));
                }
            }
        }
        let side = yard.side();
        let back = -yard.lot.depth * 0.42 - 1.7;
        match yard.style {
            0 | 1 => {
                for z in [back, back - 1.25] {
                    kit.raised_bed(&site, &yard, [side * 3.3, z], [2.6, 0.75]);
                }
            }
            2 => {
                kit.raised_bed(&site, &yard, [side * 3.7, back], [2.0, 1.15]);
            }
            _ => {}
        }
        if yard.style == 1 || yard.style == 3 {
            let at = yard.point(side * 4.7, back);
            for row in 0..3 {
                for n in 0..3 - row {
                    kit.yard_add(
                        &site,
                        Shape::Cylinder,
                        Finish::Wood,
                        at + Vec2::from_angle(-yard.lot.yaw) * (n as f32 * 0.24 - 0.22),
                        [0.20, 1.0, 0.20],
                        0.12 + row as f32 * 0.19,
                        yard.lot.yaw,
                        true,
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    );
                }
            }
        } else {
            let at = yard.point(-side * 4.5, back);
            kit.yard_bench(&site, at, yard.lot.yaw);
        }
    }
    kit.0
}

impl Kit {
    fn yard_add(
        &mut self,
        site: &crate::yards::Site,
        shape: Shape,
        finish: Finish,
        at: Vec2,
        size: [f32; 3],
        height: f32,
        yaw: f32,
        check: bool,
        extra: Quat,
    ) {
        let extent = (extra * Vec3::new(size[0], 0., 0.)).abs()
            + (extra * Vec3::new(0., size[1], 0.)).abs()
            + (extra * Vec3::new(0., 0., size[2])).abs();
        if check && !site.clear_box(at, Vec2::new(extent.x, extent.z) * 0.5, yaw, 0.25) {
            return;
        }
        self.add(shape, finish, [at.x, site.height(at) + height, at.y], size);
        self.rotate(Quat::from_rotation_y(yaw) * extra);
    }
    fn yard_fence(&mut self, site: &crate::yards::Site, a: Vec2, b: Vec2) {
        let count = (a.distance(b) / 1.55).ceil() as usize;
        for i in 0..=count {
            let p = a.lerp(b, i as f32 / count.max(1) as f32);
            if site.clear(p, 0.2) {
                self.block(
                    Finish::Wood,
                    [p.x, site.height(p) + 0.47, p.y],
                    [0.10, 0.94, 0.10],
                );
            }
            if i == count {
                continue;
            }
            let end = a.lerp(b, (i + 1) as f32 / count as f32);
            let center = (p + end) * 0.5;
            if ![p, center, end].into_iter().all(|p| site.clear(p, 0.2)) {
                continue;
            }
            let delta = Vec3::new(end.x - p.x, site.height(end) - site.height(p), end.y - p.y);
            for h in [0.30, 0.70] {
                self.block(
                    Finish::Wood,
                    [center.x, site.height(center) + h, center.y],
                    [delta.length(), 0.075, 0.07],
                );
                self.rotate(Quat::from_rotation_arc(Vec3::X, delta.normalize()));
            }
        }
    }
    fn raised_bed(
        &mut self,
        site: &crate::yards::Site,
        yard: &crate::yards::Yard,
        at: [f32; 2],
        size: [f32; 2],
    ) {
        let center = yard.point(at[0], at[1]);
        if !site.clear_box(center, Vec2::from_array(size) * 0.5, yard.lot.yaw, 0.3) {
            return;
        }
        self.yard_add(
            site,
            Shape::Slab,
            Finish::Earth,
            center,
            [size[0], 0.16, size[1]],
            0.16,
            yard.lot.yaw,
            false,
            Quat::IDENTITY,
        );
        for side in [-1., 1.] {
            self.yard_add(
                site,
                Shape::Box,
                Finish::Wood,
                yard.point(at[0], at[1] + side * size[1] * 0.5),
                [size[0] + 0.1, 0.26, 0.085],
                0.17,
                yard.lot.yaw,
                false,
                Quat::IDENTITY,
            );
            self.yard_add(
                site,
                Shape::Box,
                Finish::Wood,
                yard.point(at[0] + side * size[0] * 0.5, at[1]),
                [0.085, 0.26, size[1]],
                0.17,
                yard.lot.yaw,
                false,
                Quat::IDENTITY,
            );
        }
        // Existing low, rounded garden foliage; keep the soil visible between rows.
        for row in [-1., 1.] {
            for plant in 0..6 {
                let p = yard.point(
                    at[0] - size[0] * 0.39 + plant as f32 * size[0] * 0.155,
                    at[1] + row * size[1] * 0.23,
                );
                self.yard_add(
                    site,
                    Shape::AircraftDetailSphere,
                    Finish::Cloth,
                    p,
                    [0.22, 0.18, 0.22],
                    0.30,
                    yard.lot.yaw,
                    false,
                    Quat::IDENTITY,
                );
            }
        }
    }
    fn yard_bench(&mut self, site: &crate::yards::Site, at: Vec2, yaw: f32) {
        if !site.clear_box(at, Vec2::new(0.85, 0.36), yaw, 0.35) {
            return;
        }
        self.yard_add(
            site,
            Shape::Box,
            Finish::Wood,
            at,
            [1.6, 0.10, 0.48],
            0.49,
            yaw,
            false,
            Quat::IDENTITY,
        );
        let (s, c) = yaw.sin_cos();
        let back = at + Vec2::new(s, c) * 0.22;
        self.yard_add(
            site,
            Shape::Box,
            Finish::Wood,
            back,
            [1.6, 0.43, 0.08],
            0.80,
            yaw,
            false,
            Quat::IDENTITY,
        );
        for side in [-1., 1.] {
            self.yard_add(
                site,
                Shape::Box,
                Finish::Steel,
                at + Vec2::new(c, -s) * side * 0.56,
                [0.08, 0.44, 0.36],
                0.23,
                yaw,
                false,
                Quat::IDENTITY,
            );
        }
    }
}

#[cfg(test)]
mod yard_tests {
    use super::*;
    #[test]
    fn household_details_leave_every_road_door_route_and_house_clear() {
        let game = crate::simulation::NativeGame::default();
        let mut count = 0;
        for entity in &game.entities {
            let p = &entity.view;
            let Some(site) = crate::yards::Site::new(p, &game.terrain) else {
                continue;
            };
            if p.kind == "town" && p.lots.len() >= 4 {
                let variants: std::collections::BTreeSet<_> =
                    crate::yards::Yard::all(p).map(|y| y.style).collect();
                assert_eq!(
                    variants.len(),
                    4,
                    "every town needs all four household layouts"
                );
            }
            let parts = yard_parts(p, &game.terrain);
            count += parts.len();
            for part in parts {
                let at = Vec2::new(part.transform.translation.x, part.transform.translation.z);
                assert!(
                    site.clear(at, 0.),
                    "household detail obstructs access at {at:?} in {}",
                    p.label
                );
                assert!(part.transform.translation.is_finite() && part.transform.scale.is_finite());
            }
        }
        assert!(
            count > 500 && count < 18000,
            "bounded household dressing: {count}"
        );
    }
}
