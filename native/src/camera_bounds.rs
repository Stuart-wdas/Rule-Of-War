//! Limit the camera's ground footprint, including perspective and wide screens.
use bevy::prelude::*;
pub fn footprint(zoom: f32, aspect: f32) -> [Vec2; 4] {
    let eye = crate::fixed_view::offset(zoom);
    let rotation = crate::fixed_view::rotation();
    let tangent = (crate::fixed_view::FOV * 0.5).tan();
    [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)].map(|(x, y)| {
        let ray = rotation * Vec3::new(x * aspect * tangent, y * tangent, -1.);
        let at = eye + ray * (-eye.y / ray.y);
        Vec2::new(at.x, at.z)
    })
}
pub fn limit(center: Vec3, zoom: f32, cols: i32, rows: i32, aspect: f32) -> (Vec3, f32) {
    let margin = crate::battlefield::SCENERY_MARGIN - 128.;
    let mut zoom = zoom.clamp(16., 1536.);
    let size = Vec2::new((cols - 1) as f32, (rows - 1) as f32);
    for _ in 0..40 {
        let corners = footprint(zoom, aspect.clamp(0.5, 5.));
        let low = corners
            .iter()
            .copied()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min);
        let high = corners
            .iter()
            .copied()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
        let minimum = (-Vec2::splat(margin) - low).max(Vec2::ZERO);
        let maximum = (size + Vec2::splat(margin) - high).min(size);
        if minimum.cmple(maximum).all() {
            let at = Vec2::new(center.x, center.z).clamp(minimum, maximum);
            return (Vec3::new(at.x, center.y, at.y), zoom);
        }
        zoom *= 0.9;
    }
    (Vec3::new(size.x * 0.5, center.y, size.y * 0.5), 16.)
}
pub fn apply(controls: &mut crate::Controls, t: &crate::terrain::Terrain, aspect: f32) {
    if t.id == crate::battlefield::ID || t.id == crate::gis_prototype::ID {
        let (center, zoom) = limit(controls.center, controls.zoom, t.cols, t.rows, aspect);
        controls.center = center;
        controls.zoom = zoom;
    } else {
        controls.center.x = controls.center.x.clamp(0., t.cols as f32);
        controls.center.z = controls.center.z.clamp(0., t.rows as f32);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_viewport_corner_stays_over_scenery_at_camera_boundaries() {
        for aspect in [4. / 3., 16. / 9., 21. / 9., 32. / 9.] {
            for zoom in [16., 170., 900., 1536., 9999.] {
                for at in [
                    Vec3::new(-10000., 20., -10000.),
                    Vec3::new(10000., 20., 10000.),
                    Vec3::new(-10000., 20., 10000.),
                    Vec3::new(10000., 20., -10000.),
                ] {
                    let (center, zoom) = limit(at, zoom, 2048, 1024, aspect);
                    assert!((0. ..=2047.).contains(&center.x) && (0. ..=1023.).contains(&center.z));
                    for offset in footprint(zoom, aspect) {
                        let p = Vec2::new(center.x, center.z) + offset;
                        assert!(
                            p.x >= -2048. && p.x <= 4095. && p.y >= -2048. && p.y <= 3071.,
                            "{p:?}"
                        );
                    }
                }
            }
        }
    }
}
