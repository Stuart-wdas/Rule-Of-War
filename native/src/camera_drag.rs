//! Grab-to-pan preserves a world-space anchor underneath the cursor.
use bevy::prelude::*;
pub struct Grab {
    pub button: MouseButton,
    pub start: Vec2,
    pub anchor: Vec3,
    pub active: bool,
}
pub fn displacement(ray: Ray3d, anchor: Vec3) -> Option<Vec3> {
    if ray.direction.y >= -0.01 {
        return None;
    }
    let distance = (anchor.y - ray.origin.y) / ray.direction.y;
    if !distance.is_finite() || distance < 0. {
        return None;
    }
    let at = ray.get_point(distance);
    Some(Vec3::new(anchor.x - at.x, 0., anchor.z - at.z))
}
pub fn crossed_threshold(start: Vec2, current: Vec2) -> bool {
    start.distance_squared(current) > 64.
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grabbing_tracks_the_anchor_at_rotated_and_zoomed_camera_angles() {
        let anchor = Vec3::new(32., 4., 64.);
        for yaw in [0f32, 0.8, 2.5] {
            for zoom in [20., 100., 500.] {
                let eye = anchor + Vec3::new(yaw.sin() * zoom, zoom, yaw.cos() * zoom);
                let current = anchor + Vec3::new(-3., 0., 5.);
                let ray = Ray3d::new(eye, Dir3::new(current - eye).unwrap());
                let shift = displacement(ray, anchor).unwrap();
                assert!(shift.distance(Vec3::new(3., 0., -5.)) < 0.0001);
                // Translating the camera by this amount puts the same anchor under the cursor.
                let moved = Ray3d::new(eye + shift, ray.direction);
                assert!(displacement(moved, anchor).unwrap().length() < 0.0001);
            }
        }
    }
    #[test]
    fn clicks_and_small_jitter_remain_orders_and_horizon_rays_do_not_jump() {
        assert!(!crossed_threshold(Vec2::ZERO, Vec2::new(3., 4.)));
        assert!(crossed_threshold(Vec2::ZERO, Vec2::new(9., 0.)));
        assert!(displacement(Ray3d::new(Vec3::Y, Dir3::X), Vec3::ZERO).is_none());
        assert!(displacement(Ray3d::new(Vec3::Y, Dir3::Y), Vec3::ZERO).is_none());
    }
}
