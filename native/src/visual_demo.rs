//! Opt-in art-direction study, sharing the game's renderer and damage simulation.
use crate::{
    Controls, RtsCamera,
    models::{self, Finish as F, Part, Shape},
    simulation::NativeGame,
};
use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub const ID: &str = "saint-orne-visual-study";
static COMMANDS: AtomicBool = AtomicBool::new(false);
static STYLED: AtomicBool = AtomicBool::new(true);
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::args().any(|a| a == "--visual-demo"))
}
pub fn command_mode() -> bool {
    COMMANDS.load(Ordering::Relaxed)
}
pub fn styled() -> bool {
    !enabled() || STYLED.load(Ordering::Relaxed)
}
fn option(key: &str) -> Option<String> {
    let a: Vec<_> = std::env::args().collect();
    a.iter()
        .position(|a| a == key)
        .and_then(|i| a.get(i + 1))
        .cloned()
}
#[derive(Resource)]
pub struct Study {
    pub elapsed: f32,
    pub age: f32,
    pub strikes: usize,
    pub effect: usize,
    pub orthographic: bool,
    pub styled: bool,
    pub autoplay: bool,
    pub controls: bool,
    pub compare_scales: bool,
    pub scale_choice: Option<usize>,
    zoom: usize,
    kick: f32,
    applied: Option<bool>,
    originals: Vec<(Handle<StandardMaterial>, StandardMaterial)>,
    graded: Vec<StandardMaterial>,
}
impl Default for Study {
    fn default() -> Self {
        let compare_scales = std::env::args().any(|a| a == "--demo-scales");
        let scale_choice = option("--demo-scale")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|s| (1..=3).contains(s));
        Self {
            elapsed: 0.,
            age: 0.,
            strikes: 0,
            effect: option("--demo-effect")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            orthographic: std::env::args().any(|a| a == "--demo-orthographic"),
            styled: !std::env::args().any(|a| a == "--demo-baseline"),
            autoplay: true,
            controls: std::env::args().any(|a| a == "--demo-orders"),
            compare_scales: compare_scales || scale_choice.is_some(),
            scale_choice,
            zoom: if std::env::args().any(|a| a == "--demo-close") {
                1
            } else if compare_scales || scale_choice.is_some() {
                4
            } else {
                2
            },
            kick: 0.,
            applied: None,
            originals: vec![],
            graded: vec![],
        }
    }
}
impl Study {
    fn preview_blasts(&self, game: &NativeGame) -> Vec<crate::volume_effects::Blast> {
        use crate::volume_effects::{Blast, Variant};
        let sizes = self.scale_choice.map_or_else(|| vec![1, 2, 3], |s| vec![s]);
        let vehicle_scale = game
            .entities
            .iter()
            .find(|e| e.view.kind == "medium" && e.view.owner == 1)
            .map(|e| e.view.model_scale);
        let radius = match self.effect {
            0 => 1.2,
            1 => 3.5,
            3 => 2.4,
            _ => vehicle_scale.map_or(3.5, |s| (s * 1.1).clamp(1., 4.)),
        };
        sizes
            .into_iter()
            .map(|size| {
                let offset = if self.scale_choice.is_some() {
                    0.
                } else {
                    (size as f32 - 2.) * 35.
                };
                let at = Vec3::new(96., 0., 96.)
                    + Vec3::new(
                        crate::fixed_view::YAW.cos(),
                        0.,
                        -crate::fixed_view::YAW.sin(),
                    ) * offset;
                Blast {
                    at: at.with_y(
                        game.terrain.height(at.x, at.z)
                            + 0.15
                            + if self.effect == 2 {
                                vehicle_scale.unwrap_or(0.)
                            } else {
                                0.
                            },
                    ),
                    radius,
                    variant: match self.effect {
                        0 => Variant::Shell,
                        1 => Variant::Debris,
                        3 => Variant::Rocket,
                        _ => Variant::Mushroom,
                    },
                    seed: 74129,
                    visual_scale: size as f32,
                }
            })
            .collect()
    }
    fn fire(
        &self,
        game: &mut NativeGame,
        blasts: &mut MessageWriter<crate::volume_effects::Blast>,
    ) {
        if self.compare_scales {
            for blast in self.preview_blasts(game) {
                blasts.write(blast);
            }
        } else {
            game.visual_demo_strike(self.effect);
        }
    }
}
#[derive(Component, Clone, Copy)]
pub(crate) enum Action {
    Impact,
    Artillery,
    Vehicle,
    Rocket,
    Reset,
    Look,
    Orders,
    Auto,
    CompareSizes,
    Small,
    Medium,
    Large,
}
#[derive(Component)]
pub(crate) struct Caption;
#[derive(Component)]
pub(crate) struct StudyChrome;
type PreviewEntities<'w, 's> = Query<
    'w,
    's,
    Entity,
    Or<(
        With<crate::baked_blast::Billow>,
        With<crate::volume_effects::BakedParticle>,
        With<crate::shockwave::Shell>,
        With<crate::effects::Particle>,
    )>,
>;
#[derive(Component)]
pub struct Dressing;
type Lighting<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut DirectionalLight,
        Option<&'static crate::lighting::Sun>,
        Option<&'static crate::lighting::Fill>,
    ),
>;
fn apply_lighting(lights: &mut Lighting<'_, '_>, ambient: &mut GlobalAmbientLight, styled: bool) {
    for (mut light, sun, fill) in lights {
        if sun.is_some() {
            *light = crate::lighting::sun();
            if !styled {
                light.illuminance = 10500.;
                light.color = Color::srgb(1., 0.97, 0.92);
            }
        } else if fill.is_some() {
            *light = crate::lighting::fill();
            if !styled {
                light.illuminance = 0.;
            }
        }
    }
    *ambient = crate::lighting::ambient();
    if !styled {
        ambient.brightness = 350.;
        ambient.color = Color::srgb(0.74, 0.83, 0.90);
    }
}
pub fn register(app: &mut App) {
    app.init_resource::<Study>()
        .add_systems(PostStartup, setup.run_if(enabled))
        .add_systems(PostStartup, apply_game_look.run_if(|| !enabled()));
}

fn apply_game_look(
    art: Res<crate::scene::Art>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut grass: ResMut<Assets<crate::grass_material::GrassMaterial>>,
    mut sunlight: Lighting,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    for (i, h) in art.materials.iter().enumerate() {
        if let Some(base) = materials.get(h).cloned() {
            let graded = grade(&base, i, &mut images);
            *materials.get_mut(h).unwrap() = graded;
        }
    }
    for (_, mat) in grass.iter_mut() {
        mat.extension.debug.y = 1.;
    }
    apply_lighting(&mut sunlight, &mut ambient, true);
}

fn grade(base: &StandardMaterial, finish: usize, images: &mut Assets<Image>) -> StandardMaterial {
    let mut m = base.clone();
    let colors = [
        (F::Plaster, [0.85, 0.81, 0.71]),
        (F::Ivory, [0.88, 0.86, 0.76]),
        (F::Ochre, [0.68, 0.57, 0.37]),
        (F::Sage, [0.53, 0.63, 0.49]),
        (F::BluePlaster, [0.48, 0.59, 0.63]),
        (F::Rose, [0.67, 0.47, 0.37]),
        (F::Brick, [0.60, 0.37, 0.26]),
        (F::Roof, [0.25, 0.29, 0.27]),
        (F::SlateRoof, [0.22, 0.29, 0.32]),
        (F::GreenRoof, [0.20, 0.33, 0.28]),
        (F::Terracotta, [0.57, 0.25, 0.17]),
        (F::Wood, [0.35, 0.30, 0.22]),
        (F::Concrete, [0.61, 0.60, 0.53]),
        (F::Asphalt, [0.26, 0.28, 0.27]),
        (F::Gravel, [0.59, 0.55, 0.44]),
    ];
    if let Some((_, c)) = colors.iter().find(|(f, _)| *f as usize + 2 == finish) {
        // Photographic PBR sets carry their own colour and roughness. Preserve
        // that information instead of the old greyscale/constant-roughness grade.
        let (name, tint) = match finish.saturating_sub(2) {
            i if i == F::Plaster as usize => ("plaster", [0.94, 0.92, 0.88]),
            i if i == F::Ivory as usize => ("plaster", [0.98, 0.96, 0.90]),
            i if i == F::Ochre as usize => ("plaster", [0.94, 0.85, 0.70]),
            i if i == F::Sage as usize => ("plaster", [0.77, 0.87, 0.78]),
            i if i == F::BluePlaster as usize => ("plaster", [0.78, 0.86, 0.90]),
            i if i == F::Rose as usize => ("plaster", [0.93, 0.80, 0.76]),
            i if i == F::Brick as usize => ("brick", [0.96, 0.94, 0.90]),
            i if i == F::Terracotta as usize => ("roof_terracotta", [0.86, 0.69, 0.56]),
            i if i == F::SlateRoof as usize => ("roof_slate", [0.39, 0.47, 0.53]),
            i if i == F::GreenRoof as usize => ("roof_slate", [0.38, 0.54, 0.41]),
            i if i == F::Roof as usize => ("roof_slate", [0.48, 0.50, 0.45]),
            i if i == F::Concrete as usize => ("concrete", [0.94, 0.93, 0.90]),
            i if i == F::Asphalt as usize => ("asphalt", [0.30, 0.32, 0.33]),
            i if i == F::Gravel as usize => ("gravel", [0.84, 0.82, 0.76]),
            _ => ("", [1.; 3]),
        };
        if crate::surfaces::pbr_repeat(name).is_some() {
            m.base_color = Color::srgb(tint[0], tint[1], tint[2]);
            m.perceptual_roughness = 1.;
            m.reflectance = 0.22;
            return m;
        }
        m.base_color = Color::srgb(c[0], c[1], c[2]);
        if let Some(source) = m
            .base_color_texture
            .as_ref()
            .and_then(|h| images.get(h))
            .cloned()
        {
            let mut image = source;
            let len = (image.width() * image.height() * 4) as usize;
            if let Some(pixels) = image.data.as_mut().filter(|p| p.len() >= len) {
                pixels.truncate(len);
                for p in pixels.chunks_exact_mut(4) {
                    let grey = p[0] as f32 * 0.22 + p[1] as f32 * 0.68 + p[2] as f32 * 0.10;
                    let value = (185. + (grey - 128.) * 0.4).clamp(0., 255.) as u8;
                    p[..3].fill(value);
                }
                image.texture_descriptor.mip_level_count = 1;
                crate::surfaces::filtered_mips(&mut image, false);
                m.base_color_texture = Some(images.add(image));
            }
        }
        m.perceptual_roughness = 0.94;
        m.reflectance = 0.18;
    }
    m
}

#[expect(clippy::too_many_arguments, reason = "One-time presentation setup")]
fn setup(
    mut commands: Commands,
    mut game: ResMut<NativeGame>,
    mut menu: ResMut<crate::menu::MainMenu>,
    mut controls: ResMut<Controls>,
    mut state: ResMut<Study>,
    art: Res<crate::scene::Art>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    nature: Res<crate::landscape::Nature>,
) {
    game.setup_visual_demo();
    COMMANDS.store(state.controls, Ordering::Relaxed);
    if state.controls {
        controls.selected = game
            .entities
            .iter()
            .filter(|e| e.view.owner == 0 && e.view.kind == "medium")
            .map(|e| e.view.id.clone())
            .collect();
    }
    menu.open = false;
    menu.started = true;
    controls.center = Vec3::new(96., game.terrain.height(96., 96.), 97.);
    controls.yaw = std::f32::consts::FRAC_PI_4;
    controls.zoom = 74.;
    controls.last_map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    for (i, h) in art.materials.iter().enumerate() {
        let original = materials.get(h).unwrap().clone();
        state.graded.push(grade(&original, i, &mut images));
        state.originals.push((h.clone(), original));
    }
    dressing(&mut commands, &game, &art, &nature, &mut meshes);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(24),
                right: px(24),
                top: px(20),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.065, 0.07, 0.94)),
            GlobalZIndex(30),
            StudyChrome,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("RULE OF WAR   /   SAINT-ORNE"),
                TextFont {
                    font_size: 24.,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.87, 0.72)),
            ));
            p.spawn((
                Text::new("SUMMER, LATE AFTERNOON  /  ART DIRECTION STUDY"),
                TextFont {
                    font_size: 12.,
                    ..default()
                },
                TextColor(crate::theme::MUTED),
            ));
            p.spawn((
                Text::new("Preparing scene..."),
                Caption,
                TextFont {
                    font_size: 13.,
                    ..default()
                },
                TextColor(crate::theme::PAPER),
            ));
        });
    commands.spawn((Node {position_type:PositionType::Absolute,left:px(24),right:px(24),bottom:px(22),
        flex_direction:FlexDirection::Column,row_gap:px(8),padding:UiRect::all(px(12)),..default()},
        BackgroundColor(Color::srgba(0.045,0.065,0.07,0.94)),GlobalZIndex(30),StudyChrome))
        .with_children(|p| {
            p.spawn(Node {flex_wrap:FlexWrap::Wrap,column_gap:px(7),row_gap:px(7),..default()}).with_children(|row| {
                for (label,action) in [("1  SHELL IMPACT",Action::Impact),("2  ARTILLERY",Action::Artillery),
                    ("3  VEHICLE",Action::Vehicle),("0  ROCKET",Action::Rocket),("R  RESET",Action::Reset),
                    ("B  LOOK A/B",Action::Look),("H  UNIT ORDERS",Action::Orders),("AUTO",Action::Auto),
                    ("V  COMPARE SIZES",Action::CompareSizes)] {
                    row.spawn((Button,action,Node{padding:UiRect::axes(px(12),px(10)),..default()},BackgroundColor(crate::theme::INK)))
                        .with_children(|b| { b.spawn((Text::new(label),TextFont{font_size:12.,..default()},TextColor(crate::theme::PAPER))); });
                }
            });
            p.spawn((Text::new("4 / 5 / 6: sizes   /   Enter: repeat   /   wheel: zoom   /   Space: pause   /   F6: quality"),
                TextFont{font_size:12.,..default()},TextColor(crate::theme::MUTED)));
        });
}

fn part(parts: &mut Vec<Part>, shape: Shape, finish: F, at: Vec3, size: Vec3) {
    parts.push(Part {
        shape,
        finish,
        transform: Transform::from_translation(at).with_scale(size),
        joint: 0,
        pivot: Vec3::ZERO,
        socket: None,
        roof_lot: None,
        wall_normal: None,
        surface_frame: None,
    });
}
fn dressing(
    commands: &mut Commands,
    game: &NativeGame,
    art: &crate::scene::Art,
    nature: &crate::landscape::Nature,
    meshes: &mut Assets<Mesh>,
) {
    let mut parts = vec![];
    let mut leaves = crate::scenery::Batch::default();
    let foliage = meshes.get(&nature.crown).unwrap().clone();
    let ground = game.terrain.height(96., 96.);
    let mut add = |s, f, x, y, z, w, h, d| {
        if matches!(f, F::Cloth) {
            leaves.add(
                &foliage,
                Transform::from_xyz(x, ground + y, z)
                    .with_scale(Vec3::new(w, h, d) * 0.52)
                    .with_rotation(Quat::from_rotation_y(x * 0.17 + z * 0.31)),
            );
            return;
        }
        part(
            &mut parts,
            s,
            f,
            Vec3::new(x, ground + y, z),
            Vec3::new(w, h, d),
        )
    };
    // One connected road, side lane, raised kerbs and narrow footways.
    add(Shape::Slab, F::Asphalt, 96., 0.035, 96., 88., 0.06, 7.8);
    add(Shape::Slab, F::Asphalt, 56., 0.035, 96., 7.8, 0.06, 86.);
    for side in [-1., 1.] {
        add(
            Shape::Slab,
            F::Concrete,
            99.,
            0.10,
            96. + side * 4.65,
            80.,
            0.16,
            1.45,
        );
        // Footway at the outer lane; the ordinary ground remains continuous below it.
        add(
            Shape::Slab,
            F::Gravel,
            56. + side * 4.65,
            0.07,
            96.,
            1.45,
            0.10,
            86.,
        );
    }
    for x in (60..138).step_by(5) {
        add(
            Shape::Slab,
            F::Concrete,
            x as f32,
            0.045,
            96.,
            1.8,
            0.012,
            0.08,
        );
    }
    for row in [-1., 1.] {
        for x in [74., 96., 118.] {
            let z = 96. + row * 17.;
            add(
                Shape::Slab,
                F::Gravel,
                x,
                0.065,
                96. + row * 10.2,
                1.45,
                0.07,
                10.,
            );
            // Garden boundaries stop short of the front path, leaving clear entrances.
            for side in [-1., 1.] {
                let edge = x + side * 9.8;
                for k in 0..10 {
                    add(
                        Shape::Box,
                        F::Wood,
                        edge,
                        0.55,
                        z - 9. + k as f32 * 1.8,
                        0.13,
                        1.05,
                        0.13,
                    );
                }
                for y in [0.35, 0.78] {
                    add(Shape::Box, F::Wood, edge, y, z - 1., 0.09, 0.09, 17.);
                }
                add(
                    Shape::Box,
                    F::Earth,
                    edge - side * 0.45,
                    0.09,
                    z,
                    0.7,
                    0.16,
                    16.,
                );
                for k in 0..7 {
                    add(
                        Shape::Sphere,
                        F::Cloth,
                        edge - side * 0.45,
                        0.62,
                        z - 6. + k as f32 * 1.9,
                        1.15,
                        0.95,
                        1.8,
                    );
                }
            }
            let back = z + row * 8.8;
            for k in 0..11 {
                add(
                    Shape::Box,
                    F::Wood,
                    x - 9. + k as f32 * 1.8,
                    0.55,
                    back,
                    0.13,
                    1.05,
                    0.13,
                );
            }
            for y in [0.35, 0.78] {
                add(Shape::Box, F::Wood, x, y, back, 18., 0.09, 0.09);
            }
            // Raised vegetable beds, wood piles and a small paved doorstep.
            for k in 0..3 {
                add(
                    Shape::Box,
                    F::Wood,
                    x - 5.3,
                    0.18,
                    back - row * (1.5 + k as f32 * 1.25),
                    3.,
                    0.25,
                    0.85,
                );
                add(
                    Shape::Box,
                    F::Earth,
                    x - 5.3,
                    0.32,
                    back - row * (1.5 + k as f32 * 1.25),
                    2.8,
                    0.05,
                    0.65,
                );
            }
            add(
                Shape::Slab,
                F::Concrete,
                x,
                0.10,
                z - row * 5.,
                2.4,
                0.16,
                1.7,
            );
            for k in 0..5 {
                add(
                    Shape::Cylinder,
                    F::Wood,
                    x + 5.8,
                    0.22 + k as f32 * 0.13,
                    z + row * 5.8,
                    0.3,
                    0.3,
                    1.1,
                );
            }
            // Garden trees form groups rather than filling every lawn.
            if x != 96. {
                let tx = x + 5.8;
                let tz = back - row * 2.;
                add(Shape::Cylinder, F::Charred, tx, 1.45, tz, 0.20, 2.9, 0.20);
                add(Shape::Sphere, F::Cloth, tx - 0.35, 3.1, tz, 2.6, 2.7, 2.4);
                add(
                    Shape::Sphere,
                    F::Cloth,
                    tx + 0.6,
                    3.6,
                    tz + 0.3,
                    2.2,
                    2.8,
                    2.2,
                );
            }
        }
    }
    // Telegraph poles, service wire and a roadside bench.
    for x in [64., 106., 138.] {
        add(Shape::Cylinder, F::Wood, x, 3.8, 89.8, 0.16, 7.6, 0.16);
        add(Shape::Box, F::Wood, x, 7.2, 89.8, 2., 0.15, 0.15);
    }
    for i in 0..37 {
        add(
            Shape::Box,
            F::Charred,
            64. + i as f32 * 2.,
            7.12 - (i as f32 * 0.18).sin().abs() * 0.4,
            89.8,
            2.05,
            0.028,
            0.028,
        );
    }
    add(Shape::Box, F::Wood, 89., 0.65, 103., 2.2, 0.14, 0.62);
    add(Shape::Box, F::Wood, 89., 1.0, 103.25, 2.2, 0.55, 0.10);
    let shapes: Vec<_> = art
        .meshes
        .iter()
        .map(|h| meshes.get(h).unwrap().clone())
        .collect();
    for z in [79., 113.] {
        for x in [74., 96., 118.] {
            let mut lawn = Plane3d::default()
                .mesh()
                .size(19.3, 25.)
                .subdivisions(12)
                .build();
            if let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                lawn.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                let weights: Vec<[f32; 2]> = p
                    .iter()
                    .map(|p| {
                        let edge = (9.65 - p[0].abs()).min(12.5 - p[2].abs());
                        [(1. - edge / 1.5).clamp(0., 1.), 0.]
                    })
                    .collect();
                let colors = vec![[0.79, 0.80, 0.77, 1.]; p.len()];
                lawn.insert_attribute(Mesh::ATTRIBUTE_UV_1, weights);
                lawn.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            }
            commands.spawn((
                Dressing,
                Mesh3d(meshes.add(lawn)),
                MeshMaterial3d(nature.grass.clone()),
                Transform::from_xyz(x, ground + 0.045, z),
            ));
        }
    }
    commands.spawn((
        Dressing,
        Mesh3d(meshes.add(leaves.mesh())),
        MeshMaterial3d(nature.leaves.clone()),
        Transform::default(),
    ));
    for (finish, mesh) in models::bake(&parts, &shapes) {
        commands.spawn((
            Dressing,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(art.materials[finish as usize + 2].clone()),
            Transform::default(),
        ));
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Demo input, presentation and real simulation"
)]
pub fn input(
    mut state: ResMut<Study>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    ready: Res<crate::landscape::Ready>,
    menu: Res<crate::menu::MainMenu>,
    mut game: ResMut<NativeGame>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grass: ResMut<Assets<crate::grass_material::GrassMaterial>>,
    mut sunlight: Lighting,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut blasts: MessageWriter<crate::volume_effects::Blast>,
    mut commands: Commands,
    preview_entities: PreviewEntities,
) {
    if !enabled() {
        return;
    }
    let mut actions: Vec<Action> = buttons
        .iter()
        .filter(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, a)| *a)
        .collect();
    for (key, action) in [
        (KeyCode::Digit1, Action::Impact),
        (KeyCode::Digit2, Action::Artillery),
        (KeyCode::Digit3, Action::Vehicle),
        (KeyCode::Digit0, Action::Rocket),
        (KeyCode::KeyR, Action::Reset),
        (KeyCode::KeyB, Action::Look),
        (KeyCode::KeyH, Action::Orders),
        (KeyCode::KeyV, Action::CompareSizes),
        (KeyCode::Digit4, Action::Small),
        (KeyCode::Digit5, Action::Medium),
        (KeyCode::Digit6, Action::Large),
    ] {
        if keys.just_pressed(key) {
            actions.push(action);
        }
    }
    if keys.just_pressed(KeyCode::Enter) {
        actions.push(
            [
                Action::Impact,
                Action::Artillery,
                Action::Vehicle,
                Action::Rocket,
            ][state.effect.min(3)],
        );
    }
    if keys.just_pressed(KeyCode::Space) && !state.controls {
        game.paused = !game.paused;
    }
    for action in actions {
        match action {
            Action::Reset => {
                game.setup_visual_demo();
                state.strikes = 0;
                state.elapsed = 0.;
                state.age = 0.;
            }
            Action::Look => state.styled = !state.styled,
            Action::Orders => {
                state.controls = !state.controls;
                COMMANDS.store(state.controls, Ordering::Relaxed);
            }
            Action::Auto => state.autoplay = !state.autoplay,
            _ => {
                if matches!(
                    action,
                    Action::CompareSizes | Action::Small | Action::Medium | Action::Large
                ) {
                    let was_comparing = state.compare_scales;
                    match action {
                        Action::CompareSizes => {
                            state.compare_scales =
                                !state.compare_scales || state.scale_choice.is_some();
                            state.scale_choice = None;
                        }
                        _ => {
                            state.compare_scales = true;
                            state.scale_choice = Some(match action {
                                Action::Small => 1,
                                Action::Medium => 2,
                                _ => 3,
                            });
                        }
                    }
                    state.zoom = if state.compare_scales { 4 } else { 2 };
                    if was_comparing != state.compare_scales {
                        game.setup_visual_demo();
                    }
                } else {
                    state.effect = match action {
                        Action::Impact => 0,
                        Action::Artillery => 1,
                        Action::Rocket => 3,
                        _ => 2,
                    };
                }
                for entity in &preview_entities {
                    commands.entity(entity).despawn();
                }
                state.fire(&mut game, &mut blasts);
                state.age = 0.;
                state.strikes += 1;
                state.kick = 1.;
            }
        }
    }
    if state.applied != Some(state.styled) {
        STYLED.store(state.styled, Ordering::Relaxed);
        for (i, (h, m)) in state.originals.iter().enumerate() {
            if let Some(target) = materials.get_mut(h) {
                *target = if state.styled {
                    state.graded[i].clone()
                } else {
                    m.clone()
                };
            }
        }
        for (_, m) in grass.iter_mut() {
            m.extension.debug.y = if state.styled { 1. } else { 0. };
        }
        apply_lighting(&mut sunlight, &mut ambient, state.styled);
        state.applied = Some(state.styled);
    }
    if !ready.ready || menu.open || game.paused {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    state.elapsed += dt;
    state.age += dt;
    state.kick = (state.kick - dt * 3.).max(0.);
    let testing =
        std::env::args().any(|a| a == "--smoke-test") && option("--demo-duration").is_none();
    if state.autoplay
        && ((state.strikes == 0 && state.elapsed > 3.) || (!testing && state.age > 8.))
    {
        if state.strikes > 0 {
            state.effect = if state.compare_scales {
                state.effect % 2 + 1
            } else {
                (state.effect + 1) % 4
            };
        }
        for entity in &preview_entities {
            commands.entity(entity).despawn();
        }
        state.fire(&mut game, &mut blasts);
        state.age = 0.;
        state.strikes += 1;
        state.kick = 1.;
    }
}

#[expect(clippy::too_many_arguments, reason = "Locked/inspection camera input")]
pub fn camera(
    state: Res<Study>,
    mut controls: ResMut<Controls>,
    game: Res<NativeGame>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut camera: Single<(&mut Projection, &mut Transform), With<RtsCamera>>,
    mut zoom: Local<usize>,
    mut comparison: Local<Option<(bool, Option<usize>)>>,
) {
    if !enabled() {
        return;
    }
    motion.clear();
    let mode = (state.compare_scales, state.scale_choice);
    if *comparison != Some(mode) {
        *comparison = Some(mode);
        *zoom = state.zoom;
        controls.center = Vec3::new(96., game.terrain.height(96., 96.), 97.);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        *zoom = state.zoom;
        controls.center = Vec3::new(96., game.terrain.height(96., 96.), 97.);
    }
    if *zoom == 0 {
        *zoom = state.zoom;
    }
    for w in wheel.read() {
        if w.y > 0. {
            *zoom = zoom.saturating_sub(1).max(1);
        } else {
            *zoom = (*zoom + 1).min(4);
        }
    }
    controls.zoom = [0., 48., 74., 94., 116.][*zoom];
    controls.yaw = crate::fixed_view::YAW;
    let pan = crate::camera_pan(&keys, controls.yaw, true) * time.delta_secs().min(0.1) * 28.;
    controls.center += pan;
    controls.center.x = controls.center.x.clamp(52., 140.);
    controls.center.z = controls.center.z.clamp(60., 134.);
    controls.center.y = game.terrain.height(controls.center.x, controls.center.z);
    let center = controls.center + Vec3::Y * if state.compare_scales { 8. } else { 1.5 };
    let offset = crate::fixed_view::normal();
    camera.1.translation = center
        + offset.normalize()
            * if state.orthographic {
                220.
            } else {
                crate::fixed_view::distance(controls.zoom)
            };
    camera.1.translation += Vec3::Y * (state.age * 48.).sin() * state.kick * 0.06;
    camera.1.rotation = crate::fixed_view::rotation();
    if state.orthographic {
        *camera.0 = Projection::Orthographic(OrthographicProjection {
            near: 0.1,
            far: 2000.,
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: controls.zoom,
            },
            ..OrthographicProjection::default_3d()
        });
    } else {
        *camera.0 = crate::fixed_view::projection();
    }
}

pub fn hud(
    controls: Res<Controls>,
    state: Res<Study>,
    game: Res<NativeGame>,
    quality: Res<crate::volume_effects::Quality>,
    menu: Res<crate::menu::MainMenu>,
    mut nodes: Query<(
        &mut Node,
        Option<&crate::menu::TacticalUi>,
        Option<&StudyChrome>,
    )>,
    mut text: Query<&mut Text, With<Caption>>,
) {
    if !enabled() {
        return;
    }
    for (mut node, tactical, chrome) in &mut nodes {
        if tactical.is_some() && !state.controls {
            node.display = Display::None;
        }
        if chrome.is_some() {
            node.display = if state.controls || menu.open || controls.hud_hidden {
                Display::None
            } else {
                Display::Flex
            };
        }
    }
    for mut text in &mut text {
        **text = format!(
            "{}  /  {}  /  {}  /  {}  /  {}\n{}",
            if state.orthographic {
                "LOCKED ORTHOGRAPHIC"
            } else {
                "LOCKED SEMI-ORTHOGRAPHIC"
            },
            if state.styled {
                "NEW LIGHT + MATERIALS"
            } else {
                "BASELINE LIGHT + MATERIALS"
            },
            [
                "SHELL IMPACT",
                "ARTILLERY BURST",
                "VEHICLE DESTRUCTION",
                "ROCKET IMPACT"
            ][state.effect.min(3)],
            quality.label(),
            if menu.open {
                "MENU"
            } else if game.paused {
                "PAUSED"
            } else if state.autoplay {
                "AUTO SEQUENCE"
            } else {
                "MANUAL REPLAY"
            },
            if state.compare_scales {
                match state.scale_choice {
                    Some(1) => "SMALL 1x / CURRENT SIZE",
                    Some(2) => "MEDIUM 2x / TWICE CURRENT SIZE",
                    Some(_) => "LARGE 3x / THREE TIMES CURRENT SIZE",
                    None => {
                        "LEFT: SMALL 1x (CURRENT)     /     CENTRE: MEDIUM 2x     /     RIGHT: LARGE 3x"
                    }
                }
            } else {
                "V: compare 1x / 2x / 3x   -   4 / 5 / 6: view each size"
            }
        );
    }
}

pub fn capture_due(s: &Study) -> bool {
    if let Some(duration) = option("--demo-duration").and_then(|s| s.parse::<f32>().ok()) {
        return s.elapsed >= duration;
    }
    let age = option("--demo-age")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.8);
    s.strikes > 0 && s.age >= age
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_comparison_matches_current_blast_radius_and_only_changes_presentation() {
        for effect in [1, 2, 3] {
            let mut game = NativeGame::new();
            game.setup_visual_demo();
            let state = Study {
                effect,
                compare_scales: true,
                scale_choice: None,
                ..default()
            };
            let blasts = state.preview_blasts(&game);
            assert_eq!(blasts.len(), 3);
            assert_eq!(
                blasts.iter().map(|b| b.visual_scale).collect::<Vec<_>>(),
                vec![1., 2., 3.]
            );
            assert!(
                blasts
                    .iter()
                    .all(|b| b.seed == blasts[0].seed && b.radius == blasts[0].radius)
            );
            let right = Vec3::new(
                crate::fixed_view::YAW.cos(),
                0.,
                -crate::fixed_view::YAW.sin(),
            );
            assert!(((blasts[1].at - blasts[0].at).dot(right) - 35.).abs() < 0.001);
            assert!(((blasts[2].at - blasts[1].at).dot(right) - 35.).abs() < 0.001);
            assert!(
                game.props.is_empty(),
                "preview layout must not damage the map"
            );
            game.visual_demo_strike(effect);
            let snapshot = game.snapshot();
            let event = snapshot
                .events
                .iter()
                .find(|e| matches!(e.kind.as_str(), "blast" | "destruction"))
                .unwrap();
            assert_eq!(
                blasts[0].radius, event.radius,
                "1x is the actual current effect radius"
            );
            let alone = Study {
                scale_choice: Some(3),
                ..state
            }
            .preview_blasts(&game);
            assert_eq!(alone.len(), 1);
            assert_eq!(alone[0].visual_scale, 3.);
        }
    }
}
