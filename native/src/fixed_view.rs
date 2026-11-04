//! The battlefield lens and orientation also define the offline VFX camera.
use bevy::prelude::*;

pub const YAW: f32 = std::f32::consts::FRAC_PI_4;
pub const FOV: f32 = 25. * std::f32::consts::PI / 180.;
pub const RISE: f32 = 1.2;
pub fn normal() -> Vec3 {
    Vec3::new(YAW.sin(), RISE, YAW.cos()).normalize()
}
pub fn rotation() -> Quat {
    Transform::default().looking_to(-normal(), Vec3::Y).rotation
}
pub fn distance(vertical_span: f32) -> f32 {
    vertical_span / (2. * (FOV * 0.5).tan())
}
pub fn offset(zoom: f32) -> Vec3 {
    // Zoom always means vertical coverage, in both the art study and a match.
    normal() * distance(zoom)
}
pub fn projection() -> Projection {
    Projection::Perspective(PerspectiveProjection {
        fov: FOV,
        far: 8192.,
        ..default()
    })
}
pub fn transform(center: Vec3, zoom: f32) -> Transform {
    Transform::from_translation(center + offset(zoom)).with_rotation(rotation())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_lens_matches_the_blender_bake_through_pan_and_zoom() {
        let metadata: serde_json::Value = serde_json::from_str(include_str!(
            "../../assets/Realistic_Explosion_VFX/locked_view/mushroom/animation.json"
        ))
        .unwrap();
        let n = &metadata["camera_normal"];
        let blender = Vec3::new(
            n[0].as_f64().unwrap() as f32,
            n[2].as_f64().unwrap() as f32,
            n[1].as_f64().unwrap() as f32,
        );
        assert!(normal().distance(blender) < 0.00001);
        for zoom in [16., 68., 140., 1536.] {
            for center in [Vec3::ZERO, Vec3::new(100., 8., 500.)] {
                let camera =
                    Transform::from_translation(center + offset(zoom)).looking_at(center, Vec3::Y);
                assert!(camera.rotation.angle_between(rotation()) < 0.001);
                assert!((2. * offset(zoom).length() * (FOV * 0.5).tan() - zoom).abs() < 0.001);
            }
        }
    }
    #[test]
    fn startup_and_gameplay_share_the_demo_lens_without_legacy_zoom_scaling() {
        let Projection::Perspective(lens) = projection() else {
            panic!("semi-orthographic lens")
        };
        for center in [Vec3::ZERO, Vec3::new(50., 8., 500.)] {
            for span in [48., 68., 74., 140., 1536.] {
                let camera = transform(center, span);
                assert!(camera.rotation.angle_between(rotation()) < 0.001);
                assert!((camera.forward().as_vec3() + normal()).length() < 0.00001);
                let distance = camera.translation.distance(center);
                assert!((2. * distance * (lens.fov * 0.5).tan() - span).abs() < 0.002);
            }
        }
        assert!(lens.far >= 8192.);
    }
}
