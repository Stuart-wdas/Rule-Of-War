//! Fit sun cascades around the ground actually in view, rather than empty air
//! between the narrow RTS lens and the battlefield.
use bevy::{
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder, SimulationLightSystems},
    prelude::*,
    transform::TransformSystems,
};

pub fn cascades(detail: crate::graphics::Detail, depths: [f32; 2]) -> CascadeShadowConfig {
    let near = (depths[0] - 48.).max(0.1);
    let far = (depths[1] + 48.).max(near + 16.);
    CascadeShadowConfigBuilder {
        num_cascades: match detail {
            crate::graphics::Detail::Low => 1,
            crate::graphics::Detail::Balanced => 2,
            crate::graphics::Detail::High => 3,
        },
        minimum_distance: near,
        maximum_distance: far,
        first_cascade_far_bound: near + (far - near) * 0.32,
        overlap_proportion: (0.12 * (far - near) / far).clamp(0.015, 0.1),
    }
    .build()
}

fn ground(t: &crate::terrain::Terrain, p: Vec3) -> f32 {
    if t.id == crate::battlefield::ID
        && (p.x < 0. || p.z < 0. || p.x > (t.cols - 1) as f32 || p.z > (t.rows - 1) as f32)
    {
        crate::battlefield::scenic_height(t, p.x, p.z)
    } else {
        t.height(p.x, p.z)
    }
}

fn ground_depths(
    t: &crate::terrain::Terrain,
    eye: Transform,
    fov: f32,
    aspect: f32,
) -> Option<[f32; 2]> {
    let tangent = (fov * 0.5).tan();
    let mut depths = [f32::INFINITY, 0_f32];
    for y in [-1., 0., 1.] {
        for x in [-1., 0., 1.] {
            // Keeping view Z at -1 makes the ray parameter a view-depth,
            // exactly the coordinate used by Bevy's cascade selection.
            let ray = eye.rotation * Vec3::new(x * aspect * tangent, y * tangent, -1.);
            if ray.y >= -0.01 {
                continue;
            }
            let mut depth = (ground(t, eye.translation) - eye.translation.y) / ray.y;
            for _ in 0..4 {
                let p = eye.translation + ray * depth;
                depth = (ground(t, p) - eye.translation.y) / ray.y;
            }
            if depth.is_finite() && depth > 0. {
                depths[0] = depths[0].min(depth);
                depths[1] = depths[1].max(depth);
            }
        }
    }
    depths[0].is_finite().then_some(depths)
}

fn update(
    game: Res<crate::simulation::NativeGame>,
    graphics: Res<crate::graphics::Graphics>,
    mut cameras: Query<
        (&Camera, &GlobalTransform, &Projection, &mut DistanceFog),
        With<crate::RtsCamera>,
    >,
    mut lights: Query<&mut CascadeShadowConfig, With<crate::lighting::Sun>>,
) {
    let Some((camera, transform, Projection::Perspective(lens), mut fog)) =
        cameras.iter_mut().next()
    else {
        return;
    };
    let aspect = camera
        .logical_viewport_size()
        .map(|s| s.x / s.y.max(1.))
        .unwrap_or(lens.aspect_ratio);
    let Some(depths) = ground_depths(
        &game.terrain,
        transform.compute_transform(),
        lens.fov,
        aspect,
    ) else {
        return;
    };
    let next = cascades(graphics.settings.shadows, depths);
    let next_fog = crate::lighting::atmosphere(depths);
    let same_falloff = matches!((&fog.falloff, &next_fog.falloff),
        (FogFalloff::Linear {start:a,end:b},FogFalloff::Linear {start:c,end:d}) if a==c && b==d);
    if fog.color != next_fog.color || !same_falloff {
        *fog = next_fog;
    }
    for mut config in &mut lights {
        if config.bounds != next.bounds
            || config.minimum_distance != next.minimum_distance
            || config.overlap_proportion != next.overlap_proportion
        {
            *config = next.clone();
        }
    }
}

pub fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        update
            .after(TransformSystems::Propagate)
            .before(SimulationLightSystems::UpdateDirectionalLightCascades),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sun_shadows_cover_ground_and_treetops_at_every_zoom_and_screen_shape() {
        let game = crate::simulation::NativeGame::default();
        for aspect in [4. / 3., 16. / 9., 32. / 9.] {
            for zoom in [16., 68., 170., 500., 1450., 1536.] {
                for [x, z] in [[0., 0.], [1024., 512.], [2047., 1023.]] {
                    let center = Vec3::new(x, game.terrain.height(x, z), z);
                    let eye = crate::fixed_view::transform(center, zoom);
                    let depths =
                        ground_depths(&game.terrain, eye, crate::fixed_view::FOV, aspect).unwrap();
                    for detail in [
                        crate::graphics::Detail::Low,
                        crate::graphics::Detail::Balanced,
                        crate::graphics::Detail::High,
                    ] {
                        let config = cascades(detail, depths);
                        let far = *config.bounds.last().unwrap();
                        assert!(config.minimum_distance <= (depths[0] - 30.).max(0.1) + 0.001);
                        assert!(far > depths[1] + 30.);
                        assert!(config.bounds.windows(2).all(|p| p[1] > p[0]));
                        if zoom >= 500. {
                            assert!(
                                far > crate::fixed_view::distance(zoom),
                                "ground fell beyond the sun shadows"
                            );
                            assert!(
                                config.minimum_distance > depths[0] * 0.85,
                                "do not spend shadow resolution on empty air"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn zoom_changes_shadow_coverage_without_a_graphics_setting_change() {
        let game = crate::simulation::NativeGame::default();
        let center = Vec3::new(1024., game.terrain.height(1024., 512.), 512.);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(game)
            .insert_resource(crate::graphics::Graphics::load())
            .add_systems(PostUpdate, update);
        let camera = app
            .world_mut()
            .spawn((
                crate::RtsCamera,
                Camera::default(),
                GlobalTransform::from(crate::fixed_view::transform(center, 68.)),
                crate::fixed_view::projection(),
                crate::lighting::atmosphere([80., 220.]),
            ))
            .id();
        let sun = app
            .world_mut()
            .spawn((crate::lighting::Sun, CascadeShadowConfig::default()))
            .id();
        app.update();
        let close = app
            .world()
            .get::<CascadeShadowConfig>(sun)
            .unwrap()
            .bounds
            .last()
            .copied()
            .unwrap();
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(crate::fixed_view::transform(
                center, 1450.,
            )));
        app.update();
        let far = app
            .world()
            .get::<CascadeShadowConfig>(sun)
            .unwrap()
            .bounds
            .last()
            .copied()
            .unwrap();
        assert!(far > close * 10. && far > 3000.);
    }
}
