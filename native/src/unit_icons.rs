//! One deliberately drawn symbol set shared by world badges and the command UI.
use bevy::{
    asset::RenderAssetUsages,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::collections::{HashMap, HashSet};

const KINDS: [&str; 14] = [
    "short",
    "scout",
    "medic",
    "engineer",
    "medium",
    "long",
    "xtralong",
    "antiAir",
    "troopCarrier",
    "plane",
    "helicopter",
    "huey",
    "commander",
    "machineGunner",
];
// Command marks use the same rasterizer and palette as the unit silhouettes.
const COMMANDS: [&str; 14] = [
    "attack", "stop", "retreat", "hold", "garrison", "unload", "home", "close", "chevron", "pause",
    "save", "load", "build", "menu",
];
enum Mark {
    Polygon(Vec<Vec2>),
    Line(Vec2, Vec2, f32),
    Ring(f32, f32),
}
fn polygon(points: &[[f32; 2]]) -> Mark {
    Mark::Polygon(points.iter().copied().map(Vec2::from_array).collect())
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Mark {
    polygon(&[
        [x - w / 2., y - h / 2.],
        [x + w / 2., y - h / 2.],
        [x + w / 2., y + h / 2.],
        [x - w / 2., y + h / 2.],
    ])
}
fn line(a: [f32; 2], b: [f32; 2], width: f32) -> Mark {
    Mark::Line(a.into(), b.into(), width)
}
fn geometry(kind: &str) -> Vec<Mark> {
    match kind {
        "menu" => vec![
            rect(0., 0.5, 1.3, 0.13),
            rect(0., 0., 1.3, 0.13),
            rect(0., -0.5, 1.3, 0.13),
        ],
        "attack" => vec![
            line([-0.65, -0.65], [0.62, 0.62], 0.15),
            line([-0.12, 0.62], [0.62, 0.62], 0.15),
            line([0.62, -0.12], [0.62, 0.62], 0.15),
        ],
        "stop" => vec![rect(0., 0., 1.15, 1.15)],
        "retreat" => vec![
            line([0.65, 0.], [-0.65, 0.], 0.14),
            line([-0.65, 0.], [-0.12, 0.55], 0.14),
            line([-0.65, 0.], [-0.12, -0.55], 0.14),
        ],
        "hold" => vec![
            Mark::Ring(0.62, 0.12),
            line([-0.45, -0.45], [0.45, 0.45], 0.13),
        ],
        "home" | "garrison" => vec![
            polygon(&[[-0.78, 0.05], [0., 0.8], [0.78, 0.05]]),
            rect(-0.4, -0.28, 0.26, 0.82),
            rect(0.4, -0.28, 0.26, 0.82),
            rect(0., 0.02, 0.85, 0.25),
        ],
        "unload" => vec![
            line([-0.6, 0.72], [-0.6, -0.72], 0.12),
            line([-0.6, 0.72], [0., 0.72], 0.12),
            line([-0.6, -0.72], [0., -0.72], 0.12),
            line([-0.25, 0.], [0.7, 0.], 0.12),
            line([0.7, 0.], [0.3, 0.4], 0.12),
            line([0.7, 0.], [0.3, -0.4], 0.12),
        ],
        "close" => vec![
            line([-0.5, -0.5], [0.5, 0.5], 0.13),
            line([-0.5, 0.5], [0.5, -0.5], 0.13),
        ],
        "chevron" => vec![
            line([-0.55, -0.25], [0., 0.3], 0.12),
            line([0., 0.3], [0.55, -0.25], 0.12),
        ],
        "pause" => vec![rect(-0.3, 0., 0.24, 1.3), rect(0.3, 0., 0.24, 1.3)],
        "save" => vec![
            rect(-0.6, 0., 0.15, 1.35),
            rect(0.6, 0., 0.15, 1.35),
            rect(0., 0.6, 1.2, 0.15),
            rect(0., -0.6, 1.2, 0.15),
            rect(0., 0.35, 0.6, 0.3),
            rect(0., -0.3, 0.65, 0.35),
        ],
        "load" => vec![
            line([-0.6, -0.2], [-0.6, -0.65], 0.13),
            line([0.6, -0.2], [0.6, -0.65], 0.13),
            line([-0.6, -0.65], [0.6, -0.65], 0.13),
            line([0., 0.75], [0., -0.15], 0.13),
            line([0., -0.15], [-0.35, 0.2], 0.13),
            line([0., -0.15], [0.35, 0.2], 0.13),
        ],
        "build" => vec![rect(0., 0., 0.23, 1.4), rect(0., 0., 1.4, 0.23)],
        "medic" => vec![polygon(&[
            [-0.22, 0.78],
            [0.22, 0.78],
            [0.22, 0.22],
            [0.78, 0.22],
            [0.78, -0.22],
            [0.22, -0.22],
            [0.22, -0.78],
            [-0.22, -0.78],
            [-0.22, -0.22],
            [-0.78, -0.22],
            [-0.78, 0.22],
            [-0.22, 0.22],
        ])],
        "short" => [-0.49, 0., 0.49]
            .into_iter()
            .map(|x| {
                polygon(&[
                    [x - 0.15, -0.72],
                    [x + 0.15, -0.72],
                    [x + 0.15, 0.42],
                    [x + 0.09, 0.62],
                    [x, 0.79],
                    [x - 0.09, 0.62],
                    [x - 0.15, 0.42],
                ])
            })
            .collect(),
        "engineer" => vec![polygon(&[
            [-0.65, 0.80],
            [-0.77, 0.29],
            [-0.56, -0.12],
            [-0.23, -0.31],
            [-0.23, -0.79],
            [0.23, -0.79],
            [0.23, -0.31],
            [0.56, -0.12],
            [0.77, 0.29],
            [0.65, 0.80],
            [0.32, 0.46],
            [0.29, 0.08],
            [-0.29, 0.08],
            [-0.32, 0.46],
        ])],
        "scout" => vec![
            Mark::Ring(0.56, 0.09),
            line([0., -0.85], [0., -0.19], 0.075),
            line([0., 0.19], [0., 0.85], 0.075),
            line([-0.85, 0.], [-0.19, 0.], 0.075),
            line([0.19, 0.], [0.85, 0.], 0.075),
            Mark::Ring(0.035, 0.035),
        ],
        "plane" => vec![
            polygon(&[
                [0., 0.91],
                [0.12, 0.66],
                [0.14, -0.60],
                [0.08, -0.79],
                [-0.08, -0.79],
                [-0.14, -0.60],
                [-0.12, 0.66],
            ]),
            polygon(&[
                [-0.85, 0.13],
                [-0.13, 0.24],
                [0.13, 0.24],
                [0.85, 0.13],
                [0.85, -0.10],
                [0.12, -0.16],
                [-0.12, -0.16],
                [-0.85, -0.10],
            ]),
            rect(0., -0.65, 0.95, 0.16),
            rect(-0.24, -0.37, 0.19, 0.40),
            rect(0.24, -0.37, 0.19, 0.40),
            rect(-0.42, -0.64, 0.08, 0.35),
            rect(0.42, -0.64, 0.08, 0.35),
        ],
        "huey" => vec![
            rect(0., 0.21, 0.57, 0.86),
            polygon(&[[0., 0.78], [0.28, 0.53], [-0.28, 0.53]]),
            rect(0., -0.52, 0.10, 0.73),
            line([-0.85, 0.27], [0.85, 0.27], 0.10),
            line([0., -0.55], [0., 0.95], 0.085),
            rect(0., -0.75, 0.45, 0.07),
            rect(-0.38, 0.13, 0.055, 0.93),
            rect(0.38, 0.13, 0.055, 0.93),
        ],
        "helicopter" => vec![
            polygon(&[
                [0., 0.72],
                [0.18, 0.50],
                [0.22, -0.03],
                [0.10, -0.30],
                [0.06, -0.88],
                [-0.06, -0.88],
                [-0.10, -0.30],
                [-0.22, -0.03],
                [-0.18, 0.50],
            ]),
            line([-0.71, 0.72], [0.71, -0.42], 0.09),
            line([-0.71, -0.42], [0.71, 0.72], 0.09),
            rect(0., -0.70, 0.47, 0.075),
            rect(-0.30, 0.02, 0.055, 0.85),
            rect(0.30, 0.02, 0.055, 0.85),
        ],
        "machineGunner" => vec![
            rect(0., 0.24, 1.45, 0.16),
            rect(-0.3, 0.38, 0.48, 0.15),
            line([-0.1, 0.17], [-0.55, -0.65], 0.11),
            line([-0.1, 0.17], [0.47, -0.65], 0.11),
            line([-0.1, 0.17], [-0.15, -0.65], 0.10),
        ],
        "medium" | "antiAir" | "long" | "xtralong" | "troopCarrier" => {
            let mut marks = vec![
                rect(-0.58, -0.08, 0.21, 1.31),
                rect(0.58, -0.08, 0.21, 1.31),
                polygon(&[
                    [-0.40, -0.70],
                    [0.40, -0.70],
                    [0.44, 0.38],
                    [0.30, 0.58],
                    [-0.30, 0.58],
                    [-0.44, 0.38],
                ]),
            ];
            match kind {
                "troopCarrier" => marks.push(rect(0., 0.69, 0.48, 0.08)),
                "xtralong" => {
                    marks.push(rect(0., 0.68, 0.65, 0.15));
                    for x in [-0.24, 0., 0.24] {
                        marks.push(rect(x, 0.79, 0.10, 0.25));
                    }
                }
                "antiAir" => {
                    marks.push(rect(-0.16, 0.56, 0.09, 0.65));
                    marks.push(rect(0.16, 0.56, 0.09, 0.65));
                }
                "long" => marks.push(rect(0., 0.53, 0.075, 0.90)),
                _ => marks.push(rect(0., 0.56, 0.12, 0.71)),
            }
            marks
        }
        _ => {
            let points: Vec<_> = (0..10)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::PI / 5. + std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 0.81 } else { 0.35 };
                    [a.cos() * r, a.sin() * r]
                })
                .collect();
            vec![polygon(&points)]
        }
    }
}
fn covers(mark: &Mark, p: Vec2) -> bool {
    match mark {
        Mark::Ring(radius, width) => (p.length() - radius).abs() <= *width / 2.,
        Mark::Line(a, b, width) => {
            let d = *b - *a;
            let t = ((p - *a).dot(d) / d.length_squared().max(0.0001)).clamp(0., 1.);
            p.distance(*a + d * t) <= *width / 2.
        }
        Mark::Polygon(points) => {
            let mut inside = false;
            let mut a = *points.last().unwrap();
            for &b in points {
                if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                {
                    inside = !inside;
                }
                a = b;
            }
            inside
        }
    }
}
fn pixels(kind: &str) -> Vec<u8> {
    let marks = geometry(kind);
    let mut pixels = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let mut rgba = [0f32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let p = Vec2::new(
                        (x as f32 + (sx as f32 + 0.5) / 4.) / 32. - 1.,
                        1. - (y as f32 + (sy as f32 + 0.5) / 4.) / 32.,
                    );
                    let sample = if marks.iter().any(|m| covers(m, p)) {
                        [245., 245., 236., 255.]
                    } else if p.length() < 0.98 {
                        [8., 12., 13., 225.]
                    } else {
                        [0.; 4]
                    };
                    for i in 0..4 {
                        rgba[i] += sample[i] / 16.;
                    }
                }
            }
            pixels.extend(rgba.map(|v| v.round() as u8));
        }
    }
    pixels
}
#[derive(Resource)]
pub struct UnitIcons {
    images: HashMap<&'static str, Handle<Image>>,
    materials: HashMap<(&'static str, i32), Handle<StandardMaterial>>,
    quad: Handle<Mesh>,
}
fn key(kind: &str) -> &'static str {
    KINDS
        .into_iter()
        .find(|k| *k == kind)
        .unwrap_or(if kind == "machineGunner" {
            "long"
        } else {
            "commander"
        })
}
impl UnitIcons {
    pub fn image(&self, kind: &str) -> ImageNode {
        ImageNode::new(
            self.images
                .get(kind)
                .unwrap_or(&self.images[key(kind)])
                .clone(),
        )
    }
}
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut art = UnitIcons {
        images: HashMap::new(),
        materials: HashMap::new(),
        quad: meshes.add(Rectangle::new(2., 2.)),
    };
    for kind in KINDS.into_iter().chain(COMMANDS) {
        let image = images.add(Image::new(
            Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels(kind),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        for owner in [0, 1] {
            art.materials.insert(
                (kind, owner),
                materials.add(StandardMaterial {
                    base_color: if owner == 0 {
                        Color::srgb(0.18, 0.82, 1.)
                    } else {
                        Color::srgb(1., 0.27, 0.16)
                    },
                    base_color_texture: Some(image.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    ..default()
                }),
            );
        }
        art.images.insert(kind, image);
    }
    commands.insert_resource(art);
}
#[derive(Component)]
pub struct Badge(String);
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects the visibility resources, render assets and badge query independently"
)]
pub fn update(
    mut commands: Commands,
    controls: Res<crate::Controls>,
    battle: Res<crate::runtime::Battle>,
    front: Res<crate::frontend::Frontend>,
    menu: Res<crate::menu::MainMenu>,
    art: Res<UnitIcons>,
    camera: Single<(&GlobalTransform, &Projection), With<crate::RtsCamera>>,
    mut badges: Query<(
        Entity,
        &Badge,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let _timing = crate::performance::span("unit_icons.update");
    let visible: HashMap<_, _> = battle
        .snapshot
        .entities
        .iter()
        .filter(|p| {
            !controls.hud_hidden
                && !front.world
                && !menu.open
                && !p.building
                && (0..=1).contains(&p.owner)
                && p.hp > 0.
                && (!crate::visual_demo::enabled() || crate::visual_demo::command_mode())
        })
        .map(|p| (p.id.as_str(), p))
        .collect();
    let mut existing = HashSet::new();
    let transform = |p: &crate::runtime::Piece| {
        let pos = p.position();
        let distance = match camera.1 {
            Projection::Orthographic(p) => p.area.height() * 2.2,
            Projection::Perspective(p) => {
                camera.0.translation().distance(pos) * (p.fov * 0.5).tan()
                    / std::f32::consts::FRAC_PI_8.tan()
            }
            _ => camera.0.translation().distance(pos),
        };
        let size = (distance * 0.009).clamp(0.16, 20.);
        let height = match p.category.as_str() {
            "vehicle" => 2.5,
            "aircraft" => 1.45,
            _ => 2.05,
        } * p.model_scale;
        Transform::from_translation(pos + Vec3::Y * height + camera.0.rotation() * Vec3::Y * size)
            .with_rotation(camera.0.rotation())
            .with_scale(Vec3::splat(size))
    };
    for (entity, badge, mut t, mut material) in &mut badges {
        if let Some(p) = visible.get(badge.0.as_str()) {
            t.set_if_neq(transform(p));
            existing.insert(badge.0.clone());
            let target = &art.materials[&(key(&p.kind), p.owner)];
            if material.0 != *target {
                material.0 = target.clone();
            }
        } else {
            commands.entity(entity).despawn();
        }
    }
    for p in visible.values().filter(|p| !existing.contains(&p.id)) {
        commands.spawn((
            Badge(p.id.clone()),
            Mesh3d(art.quad.clone()),
            MeshMaterial3d(art.materials[&(key(&p.kind), p.owner)].clone()),
            transform(p),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_class_and_aircraft_has_distinct_legible_art_with_transparent_corners() {
        let icons: Vec<_> = KINDS.into_iter().map(pixels).collect();
        for (i, icon) in icons.iter().enumerate() {
            assert_eq!(icon[3], 0);
            assert!(
                icon.chunks_exact(4)
                    .filter(|p| p[0] > 200 && p[3] > 240)
                    .count()
                    > 120,
                "{}",
                KINDS[i]
            );
            for other in &icons[..i] {
                assert_ne!(icon, other);
            }
        }
        assert!(covers(&geometry("medic")[0], Vec2::ZERO));
        assert!(!covers(&geometry("engineer")[0], Vec2::new(0., 0.6)));
    }
    #[test]
    fn world_badges_follow_visibility_and_disappear_when_the_menu_opens() {
        let mut game = crate::simulation::NativeGame::new();
        game.entities.clear();
        game.config["fogEnabled"] = serde_json::json!(true);
        game.spawn("medic", 0, [20., 20.]);
        game.spawn("short", 1, [145., 145.]);
        let mut menu = crate::menu::MainMenu::default();
        menu.open = false;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::Controls>()
            .insert_resource(menu)
            .insert_resource(crate::runtime::Battle {
                snapshot: game.snapshot(),
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        let camera = app
            .world_mut()
            .spawn((
                crate::RtsCamera,
                GlobalTransform::from_translation(Vec3::splat(100.)),
                Projection::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world_mut().query::<&Badge>().iter(app.world()).count(),
            1
        );
        let badge = app
            .world_mut()
            .query_filtered::<Entity, With<Badge>>()
            .single(app.world())
            .unwrap();
        let material_tick = app
            .world()
            .entity(badge)
            .get_ref::<MeshMaterial3d<StandardMaterial>>()
            .unwrap()
            .last_changed();
        let transform_tick = app
            .world()
            .entity(badge)
            .get_ref::<Transform>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(badge)
                .get_ref::<MeshMaterial3d<StandardMaterial>>()
                .unwrap()
                .last_changed(),
            material_tick
        );
        assert_eq!(
            app.world()
                .entity(badge)
                .get_ref::<Transform>()
                .unwrap()
                .last_changed(),
            transform_tick
        );
        game.entities[1].view.x = 25.;
        game.entities[1].view.z = 20.;
        app.world_mut()
            .resource_mut::<crate::runtime::Battle>()
            .snapshot = game.snapshot();
        app.update();
        assert_eq!(
            app.world_mut().query::<&Badge>().iter(app.world()).count(),
            2
        );
        game.entities[1].view.x = 145.;
        game.entities[1].view.z = 145.;
        app.world_mut()
            .resource_mut::<crate::runtime::Battle>()
            .snapshot = game.snapshot();
        app.update();
        assert_eq!(
            app.world_mut().query::<&Badge>().iter(app.world()).count(),
            1
        );
        app.world_mut()
            .entity_mut(camera)
            .insert(Projection::Orthographic(OrthographicProjection {
                area: Rect::from_center_size(Vec2::ZERO, Vec2::new(100., 74.)),
                ..OrthographicProjection::default_3d()
            }));
        app.update();
        let size = app
            .world_mut()
            .query_filtered::<&Transform, With<Badge>>()
            .single(app.world())
            .unwrap()
            .scale;
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from_translation(Vec3::splat(1000.)));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<&Transform, With<Badge>>()
                .single(app.world())
                .unwrap()
                .scale,
            size,
            "orthographic badges must not grow with camera distance"
        );
        app.world_mut().resource_mut::<crate::menu::MainMenu>().open = true;
        app.update();
        assert_eq!(
            app.world_mut().query::<&Badge>().iter(app.world()).count(),
            0
        );
    }
}
