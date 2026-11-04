mod airfield_layout;
mod animation;
mod art_export;
mod baked_blast;
mod base_scene;
mod battlefield;
mod camera_bounds;
mod camera_drag;
mod camera_stress;
mod cloud_shadows;
mod countryside_border;
mod cursor;
mod debrief;
mod destruction;
mod dust_disc;
mod editor;
mod effects;
mod feedback;
mod fixed_view;
mod flipbook;
mod fog;
mod frontend;
mod generation;
mod gis_prototype;
mod graphics;
mod grass_material;
mod ground_demo;
mod ground_detail;
mod ground_finish;
mod ground_impacts;
mod infantry;
mod landscape;
mod lighting;
mod loading;
mod material_export;
mod menu;
mod model_finish;
mod models;
mod natural_geometry;
mod natural_noise;
mod network;
mod performance;
mod projectiles;
mod render_limits;
mod render_profile;
mod runtime;
mod rural_layout;
mod rural_surfaces;
mod scene;
mod scenery;
mod settlement_layout;
mod shockwave;
mod simulation;
mod smoke;
mod sound;
mod strafe;
mod sun_shadows;
mod surfaces;
mod terrain;
mod theme;
mod tools;
mod training;
mod transmission;
mod ui;
mod unit_icons;
mod vehicle_atlas;
mod visual_demo;
mod volume_effects;
mod world;
mod yards;

use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use runtime::{Battle, CommandQueue};
use serde_json::json;
use std::collections::HashMap;

#[derive(Component)]
pub struct RtsCamera;

#[derive(Resource)]
pub struct Controls {
    pub hud_hidden: bool,
    pub command_drawer: bool,
    pub drawer_reveal: f32,
    pub drawer_height: f32,
    pub drawer_compact: bool,
    pub drawer_selection: Vec<String>,
    pub drawer_training: bool,
    pub selected: Vec<String>,
    pub capture_zone: Option<String>,
    pub attack_move: bool,
    pub move_only: bool,
    pub patrol: bool,
    pub build: Option<String>,
    pub bridge_start: Option<Vec3>,
    pub ground: Option<Vec3>,
    pub drag: Option<Vec2>,
    pub groups: HashMap<u8, Vec<String>>,
    pub training_tools: bool,
    pub training_shop: bool,
    pub training_purchase: Option<String>,
    center: Vec3,
    zoom: f32,
    yaw: f32,
    last_map: String,
    pan_velocity: Vec3,
    camera_drag: Option<camera_drag::Grab>,
    right_dragged: bool,
    roster_click: Option<(String, f64)>,
    base_return: Option<(Vec3, f32, f32)>,
    camera_revision: u64,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            hud_hidden: false,
            command_drawer: false,
            drawer_reveal: 0.,
            drawer_height: 148.,
            drawer_compact: false,
            drawer_selection: vec![],
            drawer_training: false,
            selected: vec![],
            capture_zone: None,
            attack_move: false,
            move_only: false,
            patrol: false,
            build: None,
            bridge_start: None,
            ground: None,
            drag: None,
            groups: HashMap::new(),
            training_tools: false,
            training_shop: true,
            training_purchase: None,
            center: Vec3::new(60., 0., 80.),
            zoom: 68.,
            yaw: fixed_view::YAW,
            last_map: String::new(),
            pan_velocity: Vec3::ZERO,
            camera_drag: None,
            right_dragged: false,
            roster_click: None,
            base_return: None,
            camera_revision: 0,
        }
    }
}
impl Controls {
    fn focus_at(&mut self, at: Vec3) {
        self.camera_revision = self.camera_revision.wrapping_add(1);
        self.center = at;
        self.pan_velocity = Vec3::ZERO;
        self.camera_drag = None;
        self.drag = None;
    }
    fn select_card(&mut self, id: &str, at: Vec3, now: f64) {
        let double = self
            .roster_click
            .as_ref()
            .is_some_and(|(last, time)| last == id && (0. ..=0.35).contains(&(now - time)));
        self.selected = vec![id.into()];
        self.roster_click = if double { None } else { Some((id.into(), now)) };
        if double {
            self.base_return = None;
            self.focus_at(at);
        }
    }
    fn toggle_base(&mut self, at: Vec3) {
        if let Some((center, zoom, yaw)) = self.base_return.take() {
            self.focus_at(center);
            self.zoom = zoom;
            self.yaw = yaw;
        } else {
            self.base_return = Some((self.center, self.zoom, self.yaw));
            self.focus_at(at);
            self.zoom = 170.;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--smoke-test")
        && args
            .iter()
            .any(|a| ["--host", "--join", "--world", "--lobby", "--settings"].contains(&a.as_str()))
    {
        eprintln!(
            "This build has one core battlefield. Launch normally and choose ENTER BATTLEFIELD."
        );
        std::process::exit(2);
    }
    if let Some(result) = tools::run(&args) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    let mut app = App::new();
    app.insert_resource(graphics::Graphics::load());
    #[cfg(feature = "nvidia-dlss")]
    app.insert_resource(bevy::anti_alias::dlss::DlssProjectId(
        bevy::asset::uuid::Uuid::from_u128(0x8a5eafab_047a_4f83_b2a3_8795f3e906ee),
    ));
    let option = |key: &str| -> Option<&str> {
        args.iter()
            .position(|arg| arg == key)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    if args.iter().any(|arg| arg == "--host") && args.iter().any(|arg| arg == "--join") {
        eprintln!("Choose --host or --join.");
        std::process::exit(2);
    }
    if args.iter().any(|arg| arg == "--import")
        && args.iter().any(|arg| arg == "--host" || arg == "--join")
    {
        eprintln!("Use --import for a solo operation, or --session to restore a multiplayer host.");
        std::process::exit(2);
    }
    for key in ["--host", "--join", "--session", "--import", "--smoke-test"] {
        if args.iter().any(|arg| arg == key)
            && option(key).is_none_or(|value| value.starts_with("--"))
        {
            eprintln!("{key} requires a value.");
            std::process::exit(2);
        }
    }
    if let Some(index) = args.iter().position(|arg| arg == "--smoke-test") {
        let path = args
            .get(index + 1)
            .expect("--smoke-test requires an output PNG path")
            .clone();
        app.insert_resource(smoke::Smoke::new(path))
            .insert_resource(bevy::winit::WinitSettings::continuous())
            .add_systems(PostStartup, smoke::setup_capture)
            .add_systems(Update, smoke::run);
    }
    camera_stress::register(&mut app);
    app.insert_resource(ClearColor(Color::srgb(0.055, 0.08, 0.10)))
        .init_resource::<fog::Fog>()
        .insert_resource(Battle::default())
        .init_resource::<Controls>()
        .init_resource::<feedback::Feedback>()
        .init_resource::<cursor::TacticalCursor>()
        .init_resource::<strafe::Overlay>()
        .add_plugins(
            DefaultPlugins
                .set(bevy::image::ImagePlugin::default_linear())
                .set(WindowPlugin {
                    primary_window: Some({
                        let mut window = Window {
                            title: if gis_prototype::enabled() {
                                "Rule of War / Hottot real-world prototype".into()
                            } else {
                                "Rule of War".into()
                            },
                            resolution: (1920, 1080).into(),
                            present_mode: if args
                                .iter()
                                .any(|a| a == "--uncapped" || a == "--smoke-test")
                            {
                                bevy::window::PresentMode::AutoNoVsync
                            } else {
                                default()
                            },
                            resize_constraints: WindowResizeConstraints {
                                min_width: 1100.,
                                min_height: 850.,
                                ..default()
                            },
                            ..default()
                        };
                        window.set_maximized(
                            !args
                                .iter()
                                .any(|a| a == "--smoke-test" || a == "--offscreen"),
                        );
                        window
                    }),
                    ..default()
                }),
        );
    grass_material::register(&mut app);
    model_finish::register(&mut app);
    ground_finish::register(&mut app);
    volume_effects::register(&mut app);
    graphics::register(&mut app);
    render_limits::register(&mut app);
    sun_shadows::register(&mut app);
    baked_blast::register(&mut app);
    shockwave::register(&mut app);
    transmission::register(&mut app);
    dust_disc::register(&mut app);
    visual_demo::register(&mut app);
    cloud_shadows::register(&mut app);
    infantry::register(&mut app);
    if args.iter().any(|a| a == "--profile") {
        app.init_resource::<performance::MainFrame>()
            .add_systems(First, performance::begin_frame)
            .add_systems(Last, performance::end_frame)
            .add_plugins((
                bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
                bevy::render::diagnostic::RenderDiagnosticsPlugin,
            ))
            .add_systems(Update, smoke::profile);
        render_profile::register(&mut app);
    }
    app.insert_resource(bevy::render::render_asset::RenderAssetBytesPerFrame::new(
        8 * 1024 * 1024,
    ));
    app.init_resource::<CommandQueue>()
        .init_resource::<menu::MainMenu>()
        .init_resource::<debrief::Debrief>()
        .init_resource::<loading::Loading>()
        .init_resource::<landscape::Ready>()
        .init_resource::<scenery::Stats>()
        .init_resource::<frontend::Frontend>()
        .init_resource::<editor::History>()
        .init_resource::<simulation::NativeGame>()
        .insert_resource(Time::<Fixed>::from_hz(20.))
        .add_systems(FixedUpdate, runtime::tick);
    if let Some(path) = option("--import") {
        app.world_mut().resource_mut::<loading::Loading>().begin(
            loading::Request::Import(std::path::PathBuf::from(path)),
            "Importing operation data".into(),
        );
    }
    let mut game = app
        .world_mut()
        .remove_resource::<simulation::NativeGame>()
        .unwrap();
    let session = if let Some(address) = option("--host") {
        network::Session::host(
            address,
            option("--session")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| simulation::save_path().with_file_name("native-host.json")),
            &mut game,
        )
    } else if let Some(address) = option("--join") {
        game.paused = true;
        network::Session::join(
            address,
            option("--session")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| simulation::save_path().with_file_name("native-guest.json")),
        )
    } else {
        Ok(network::Session::default())
    };
    let session = match session {
        Ok(session) => session,
        Err(e) => {
            eprintln!("Multiplayer startup failed: {e}");
            std::process::exit(2);
        }
    };
    println!("{}", session.status);
    app.insert_resource(game)
        .insert_resource(session)
        .add_systems(Last, network::save_on_exit);
    if args
        .iter()
        .any(|a| ["--host", "--join", "--import"].contains(&a.as_str()))
    {
        let mut menu = app.world_mut().resource_mut::<menu::MainMenu>();
        menu.open = false;
        menu.started = true;
    }
    app.add_systems(
        Startup,
        (
            theme::setup,
            unit_icons::setup,
            sound::setup,
            projectiles::setup,
            scene::setup,
            effects::setup,
            volume_effects::setup,
            ground_detail::setup,
            landscape::setup,
            ui::setup,
            cursor::setup,
            frontend::setup,
            menu::setup,
            debrief::setup,
        )
            .chain(),
    )
    .add_systems(PostStartup, ground_demo::setup.run_if(ground_demo::enabled))
    .add_systems(
        PostStartup,
        gis_prototype::setup.run_if(gis_prototype::enabled),
    )
    .add_systems(
        Update,
        (
            (
                (menu::input, debrief::input).chain(),
                frontend::input,
                loading::poll,
                runtime::receive,
                visual_demo::input,
                camera_control,
                visual_demo::camera,
                (landscape::stream, countryside_border::update).chain(),
                gis_prototype::roads,
                (
                    scenery::stream,
                    scenery::growth_shadows,
                    ground_detail::update,
                )
                    .chain(),
                fog::update,
                battlefield_input,
                ui::buttons,
                ui::rebuild_actions,
                ui::scroll_actions,
                ui::update,
                (ui::selected_frame, ui::layout).chain(),
                cursor::update,
                ui::minimap,
                ui::roster,
            )
                .chain(),
            (
                scene::sync_scene,
                scene::roof_visibility,
                scene::animate,
                scene::sync_props,
                base_scene::sync,
                scene::overlays,
                unit_icons::update,
                feedback::draw,
                strafe::draw,
                (effects::update, effects::ground_rings, baked_blast::update).chain(),
                volume_effects::update,
                sound::play,
                sound::engines,
                frontend::world_scene,
                frontend::update,
                menu::update,
                visual_demo::hud,
                debrief::update,
                theme::style_text,
                theme::skin,
            )
                .chain(),
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            infantry::prepare.before(scene::sync_scene),
            infantry::drive.after(scene::sync_scene),
        ),
    )
    .add_systems(
        PostUpdate,
        (
            infantry::aim,
            infantry::class_gear,
            infantry::cache_muzzles,
            projectiles::update,
        )
            .chain()
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    )
    .run();
}

fn camera_pan(keys: &ButtonInput<KeyCode>, yaw: f32, wasd: bool) -> Vec3 {
    let right = Vec3::new(yaw.cos(), 0., -yaw.sin());
    let forward = Vec3::new(-yaw.sin(), 0., -yaw.cos());
    let mut pan = Vec3::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) || (wasd && keys.pressed(KeyCode::KeyA)) {
        pan -= right;
    }
    if keys.pressed(KeyCode::ArrowRight) || (wasd && keys.pressed(KeyCode::KeyD)) {
        pan += right;
    }
    if keys.pressed(KeyCode::ArrowUp) || (wasd && keys.pressed(KeyCode::KeyW)) {
        pan += forward;
    }
    if keys.pressed(KeyCode::ArrowDown) || (wasd && keys.pressed(KeyCode::KeyS)) {
        pan -= forward;
    }
    pan.normalize_or_zero()
}
fn follow_camera(
    camera: &mut Transform,
    previous_center: Vec3,
    center: Vec3,
    yaw: f32,
    zoom: f32,
    dt: f32,
) {
    // Translate focus and eye together. Smooth distance only; orientation is pinned.
    camera.translation += center - previous_center;
    let _ = yaw; // Stored camera bookmarks from older saves cannot rotate the view.
    let distance = (camera.translation - center)
        .length()
        .lerp(fixed_view::offset(zoom).length(), 1. - (-dt * 14.).exp());
    camera.translation = center + fixed_view::normal() * distance;
    camera.rotation = fixed_view::rotation();
}
fn pan_displacement(pan: Vec3, speed: f32, dt: f32) -> Vec3 {
    // Wall-clock displacement stays the same at 15, 30 and 144 FPS. Camera
    // movement must not inherit the simulation's catch-up limits or inertia.
    pan * speed * dt.clamp(0., 0.25)
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
fn camera_control(
    front: Res<frontend::Frontend>,
    menu: Res<menu::MainMenu>,
    game: Res<simulation::NativeGame>,
    battle: Res<Battle>,
    session: Res<network::Session>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut controls: ResMut<Controls>,
    camera: Single<(&Camera, &mut Transform), With<RtsCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let _timing = performance::span("camera.control");
    if visual_demo::enabled() {
        motion.clear();
        wheel.clear();
        return;
    }
    let (projection, mut camera) = camera.into_inner();
    let camera_aspect = projection
        .logical_viewport_size()
        .map(|size| size.x / size.y.max(1.))
        .unwrap_or(window.width() / window.height());
    // The offscreen material comparison must not inherit desktop wheel events.
    // Interactive demos and matches retain all ordinary pan/zoom controls.
    if ground_demo::enabled() && std::env::args().any(|a| a == "--smoke-test") {
        motion.clear();
        wheel.clear();
        controls.center = Vec3::new(165., game.terrain.height(165., 180.), 180.);
        controls.zoom = if std::env::args().any(|a| a == "--ground-close") {
            42.
        } else {
            100.
        };
        *camera = fixed_view::transform(controls.center, controls.zoom);
        return;
    }
    let key = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
    if controls.last_map != key {
        if let Some(p) = battle
            .snapshot
            .entities
            .iter()
            .find(|e| e.owner == session.owner && !e.building)
        {
            controls.last_map = key.clone();
            controls.center = Vec3::new(p.x + if session.owner == 0 { 25. } else { -25. }, 0., p.z);
        }
        if session.owner == 0 {
            controls.last_map = key;
        }
        controls.selected.clear();
        controls.capture_zone = None;
        controls.groups.clear();
        controls.roster_click = None;
        controls.base_return = None;
        controls.build = None;
        controls.bridge_start = None;
        controls.drag = None;
        controls.camera_drag = None;
        controls.pan_velocity = Vec3::ZERO;
        controls.zoom = 68.;
        if game.training.is_some() {
            controls.center = Vec3::new(146., 0., 136.);
            controls.zoom = 140.;
            controls.yaw = fixed_view::YAW;
        }
        if game.simulation {
            controls.center = Vec3::new(
                game.terrain.cols as f32 * 0.5,
                0.,
                game.terrain.rows as f32 * 0.5,
            );
            controls.zoom = game.terrain.cols as f32 * 0.6;
        }
        if game.terrain.id == gis_prototype::ID {
            let overview = std::env::args().any(|a| a == "--gis-overview");
            let close = std::env::args().any(|a| a == "--gis-close");
            controls.center = Vec3::new(1024., game.terrain.height(1024., 420.), 420.);
            controls.zoom = if overview {
                1050.
            } else if close {
                150.
            } else {
                470.
            };
        }
    }
    let delta = motion.read().fold(Vec2::ZERO, |v, m| v + m.delta);
    if menu.open {
        controls.camera_drag = None;
        controls.pan_velocity = Vec3::ZERO;
        wheel.clear();
        let center = Vec3::new(
            game.terrain.cols as f32 * 0.25,
            game.terrain.height(
                game.terrain.cols as f32 * 0.25,
                game.terrain.rows as f32 * 0.5,
            ),
            game.terrain.rows as f32 * 0.5,
        );
        camera.translation = center + fixed_view::offset(95.);
        camera.rotation = fixed_view::rotation();
        return;
    }
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    if mouse.pressed(MouseButton::Middle) && front.world {
        controls.camera_drag = None;
        controls.yaw -= delta.x * 0.006;
    }

    if front.world {
        camera.translation = Vec3::new(controls.yaw.sin() * 110., 65., controls.yaw.cos() * 110.);
        camera.look_at(Vec3::ZERO, Vec3::Y);
        return;
    }
    controls.yaw = fixed_view::YAW;
    if front.settings || front.lobby || !window.focused {
        controls.camera_drag = None;
        controls.pan_velocity = Vec3::ZERO;
        wheel.clear();
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        controls.right_dragged = false;
    }
    if let Some(grab) = controls.camera_drag.as_ref()
        && !mouse.pressed(grab.button)
    {
        controls.camera_drag = None;
    }
    if let Some(cursor) = window.cursor_position() {
        if controls.camera_drag.is_none()
            && !ui::over_ui(cursor, window.size(), &controls)
            && (!front.editor || mouse.just_pressed(MouseButton::Middle))
        {
            let button = if mouse.just_pressed(MouseButton::Middle) && !alt {
                Some(MouseButton::Middle)
            } else if mouse.just_pressed(MouseButton::Right) {
                Some(MouseButton::Right)
            } else if mouse.just_pressed(MouseButton::Left) && alt {
                Some(MouseButton::Left)
            } else {
                None
            };
            if let Some(button) = button
                && let Ok(ray) =
                    projection.viewport_to_world(&GlobalTransform::from(*camera), cursor)
                && let Some(anchor) = landscape::intersect(&game.terrain, ray)
            {
                controls.camera_drag = Some(camera_drag::Grab {
                    button,
                    start: cursor,
                    anchor,
                    active: button != MouseButton::Right,
                });
                if button == MouseButton::Left {
                    controls.drag = None;
                }
            }
        }
        if let Some(grab) = controls.camera_drag.as_mut() {
            grab.active |= camera_drag::crossed_threshold(grab.start, cursor);
            let (active, button, anchor) = (grab.active, grab.button, grab.anchor);
            if active {
                if button == MouseButton::Right {
                    controls.right_dragged = true;
                }
                controls.pan_velocity = Vec3::ZERO;
                controls.drag = None;
                if let Ok(ray) =
                    projection.viewport_to_world(&GlobalTransform::from(*camera), cursor)
                    && let Some(shift) = camera_drag::displacement(ray, anchor)
                {
                    let previous = controls.center;
                    controls.center += shift;
                    camera_bounds::apply(&mut controls, &game.terrain, camera_aspect);
                    camera.translation += controls.center - previous;
                }
            }
        }
    }
    let dragging = controls.camera_drag.as_ref().is_some_and(|g| g.active);
    for event in wheel.read() {
        if dragging
            || window
                .cursor_position()
                .is_some_and(|p| ui::over_ui(p, window.size(), &controls))
        {
            continue;
        }
        controls.zoom = (controls.zoom * (1. - event.y * 0.07))
            .clamp(16., game.terrain.cols.max(game.terrain.rows) as f32 * 1.5);
    }
    let previous_center = controls.center;
    if keys.just_pressed(KeyCode::Home)
        && let Some(at) = battle
            .snapshot
            .bases
            .iter()
            .find(|b| b.owner == session.owner)
            .map(|b| b.origin)
            .or_else(|| {
                battle
                    .snapshot
                    .entities
                    .iter()
                    .find(|p| p.owner == session.owner && p.building && p.hp > 0.)
                    .map(|p| [p.x, p.z])
            })
    {
        controls.toggle_base(Vec3::new(at[0], game.terrain.height(at[0], at[1]), at[1]));
    }
    let pan = if dragging {
        Vec3::ZERO
    } else {
        camera_pan(
            &keys,
            controls.yaw,
            !(front.editor && mouse.pressed(MouseButton::Left)),
        )
    };
    let fast = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let speed = controls.zoom * 0.6 * menu.pan_speed * if fast { 2.5 } else { 1. };
    controls.pan_velocity = Vec3::ZERO;
    controls.center += pan_displacement(pan, speed, time.delta_secs());
    if !dragging
        && keys.just_pressed(KeyCode::KeyC)
        && let Some(p) = battle
            .snapshot
            .entities
            .iter()
            .find(|p| controls.selected.first() == Some(&p.id))
    {
        controls.focus_at(Vec3::new(p.x, 0., p.z));
    }
    camera_bounds::apply(&mut controls, &game.terrain, camera_aspect);
    if !dragging {
        controls.center.y = game.terrain.height(controls.center.x, controls.center.z);
    }
    if !dragging {
        follow_camera(
            &mut camera,
            previous_center,
            controls.center,
            controls.yaw,
            controls.zoom,
            time.delta_secs(),
        );
    } else {
        // Dragging translates the eye and terrain anchor together; it never orbits.
        camera.rotation = fixed_view::rotation();
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
fn battlefield_input(
    front: Res<frontend::Frontend>,
    menu: Res<menu::MainMenu>,
    loading: Res<loading::Loading>,
    ready: Res<landscape::Ready>,
    game: Res<simulation::NativeGame>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    battle: Res<Battle>,
    session: Res<network::Session>,
    queue: Option<Res<CommandQueue>>,
    mut controls: ResMut<Controls>,
    mut feedback: ResMut<feedback::Feedback>,
    time: Res<Time>,
) {
    if visual_demo::enabled() && !visual_demo::command_mode() {
        return;
    }
    let Some(queue) = queue else {
        return;
    };
    if menu.open || loading.busy() || !ready.ready || front.world || front.settings || front.lobby {
        controls.drag = None;
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    controls
        .selected
        .retain(|id| battle.snapshot.entities.iter().any(|p| &p.id == id));
    for (key, msg) in [
        (KeyCode::Space, "pause"),
        (KeyCode::F5, "save"),
        (KeyCode::F9, "load"),
        (KeyCode::F2, "new"),
    ] {
        if keys.just_pressed(key) {
            if session.owner != 0 {
                continue;
            }
            queue.send(json!({"type":msg}));
            if msg == "new" || msg == "load" {
                controls.selected.clear();
                controls.build = None;
                controls.groups.clear();
                controls.base_return = None;
                controls.roster_click = None;
            }
        }
    }
    if game.simulation || game.winner.is_some() {
        controls.drag = None;
        return;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        controls.move_only = false;
        controls.attack_move = true;
        controls.patrol = false;
        controls.build = None;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        controls.build = Some("callin:c130".into());
        controls.attack_move = false;
        controls.move_only = false;
        controls.patrol = false;
        controls.drag = None;
    }
    if keys.just_pressed(KeyCode::KeyT) && game.training.is_some() {
        controls.training_tools = !controls.training_tools || !controls.command_drawer;
        controls.command_drawer = controls.training_tools;
        controls.build = None;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        controls.move_only = true;
        controls.attack_move = false;
        controls.patrol = false;
        controls.build = None;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        controls.move_only = false;
        controls.patrol = true;
        controls.attack_move = false;
        controls.build = None;
    }
    for (key, kind) in [
        (KeyCode::KeyX, "stop"),
        (KeyCode::KeyE, "attack-stance"),
        (KeyCode::KeyF, "retreat"),
        (KeyCode::KeyR, "retreat"),
        (KeyCode::KeyH, "hold-fire"),
        (KeyCode::KeyL, "load"),
        (KeyCode::KeyU, "unload"),
        (KeyCode::KeyG, "garrison"),
    ] {
        if keys.just_pressed(key) {
            if kind == "attack-stance" {
                controls.move_only = false;
            }
            queue.order(&controls.selected, kind, None, None, false);
            if !controls.selected.is_empty() {
                feedback.acknowledge();
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        controls.selected = battle
            .snapshot
            .entities
            .iter()
            .filter(|p| {
                p.owner == session.owner
                    && !p.building
                    && !p.downed
                    && battle.catalog["units"][&p.kind]["damage"]
                        .as_f64()
                        .unwrap_or(0.)
                        > 0.
            })
            .map(|p| p.id.clone())
            .collect();
    }
    for (index, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
    .iter()
    .enumerate()
    {
        if keys.just_pressed(*key) {
            if ctrl {
                let selection = controls.selected.clone();
                controls.groups.insert(index as u8, selection);
                queue.send(json!({"type":"command","commandType":"assign-squad","unitIds":controls.selected,"squad":index}));
            } else {
                let squad: Vec<_> = battle
                    .snapshot
                    .entities
                    .iter()
                    .filter(|p| {
                        p.owner == session.owner && !p.building && p.squad == Some(index as u8)
                    })
                    .map(|p| p.id.clone())
                    .collect();
                if shift {
                    for id in squad {
                        if !controls.selected.contains(&id) {
                            controls.selected.push(id);
                        }
                    }
                } else {
                    controls.selected = squad;
                }
            }
        }
    }
    let Some(cursor) = window.cursor_position() else {
        controls.ground = None;
        if mouse.just_released(MouseButton::Left) {
            controls.drag = None;
        }
        return;
    };
    let (camera, transform) = *camera;
    let over_ui = ui::over_ui(cursor, window.size(), &controls);
    controls.ground = if over_ui {
        None
    } else {
        camera
            .viewport_to_world(transform, cursor)
            .ok()
            .and_then(|ray| landscape::intersect(&game.terrain, ray))
    };
    if front.editor
        || controls.camera_drag.as_ref().is_some_and(|g| g.active)
        || keys.pressed(KeyCode::AltLeft)
        || keys.pressed(KeyCode::AltRight)
    {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && !over_ui {
        if let (Some(kind), Some(point)) = (controls.build.clone(), controls.ground) {
            if kind == "callin:c130" {
                queue.send(json!({"type":"call-in","unitType":"c130","destination":{"x":point.x,"y":point.z}}));
                controls.build = None;
                controls.drag = None;
                feedback.order(point, true);
                return;
            }
            if let Some(kind) = kind.strip_prefix("range:") {
                let (action, item) = kind.split_once(':').unwrap_or(("", ""));
                queue.send(json!({"type":"training","action":action,"kind":item,"destination":{"x":point.x,"y":point.z}}));
                if !shift {
                    controls.build = None;
                }
                return;
            }
            if kind == "bridge" {
                if let Some(start) = controls.bridge_start.take() {
                    queue.send(json!({"type":"build","buildingType":"bridge","destination":{"x":start.x,"y":start.z},"end":{"x":point.x,"y":point.z}}));
                    controls.build = None;
                } else {
                    controls.bridge_start = Some(point);
                }
                return;
            }
            queue.send(
                json!({"type":"build","buildingType":kind,"destination":{"x":point.x,"y":point.z}}),
            );
            controls.build = None;
        } else {
            controls.drag = Some(cursor);
        }
    }
    if mouse.just_released(MouseButton::Left)
        && let Some(start) = controls.drag.take()
    {
        if start.distance(cursor) > 8. {
            if !shift {
                controls.selected.clear();
            }
            let min = start.min(cursor);
            let max = start.max(cursor);
            for p in &battle.snapshot.entities {
                if p.owner != session.owner || p.building {
                    continue;
                }
                if let Ok(screen) = camera.world_to_viewport(transform, p.position() + Vec3::Y)
                    && screen.cmpge(min).all()
                    && screen.cmple(max).all()
                    && !controls.selected.contains(&p.id)
                {
                    controls.selected.push(p.id.clone());
                }
            }
        } else if !over_ui {
            let picked = cursor::pick(&battle.snapshot, cursor, camera, transform);
            if let Some(flag) = picked.filter(|p| p.kind == "objective") {
                controls.capture_zone = Some(flag.id.clone());
                if !controls.attack_move && !controls.patrol {
                    return;
                }
            } else {
                controls.capture_zone = None;
            }
            if let Some(kind) = cursor::left_order(
                &battle.snapshot,
                &controls,
                &battle.catalog,
                session.owner,
                picked,
            ) && let Some(point) = controls.ground.or_else(|| picked.map(|p| p.position()))
            {
                let target = if kind == "attack" {
                    picked.map(|p| p.id.as_str())
                } else {
                    None
                };
                let point = if kind == "attack" {
                    picked
                        .map(|p| cursor::attack_position(p, cursor, camera, transform))
                        .unwrap_or(point)
                } else {
                    point
                };
                queue.order(&controls.selected, kind, Some(point), target, shift);
                feedback.order(point, kind != "patrol");
                if !shift {
                    controls.attack_move = false;
                    controls.move_only = false;
                    controls.patrol = false;
                }
                return;
            }
            if !shift {
                controls.selected.clear();
            }
            if let Some(p) = picked {
                controls.command_drawer = true;
                let double = feedback
                    .last_click
                    .as_ref()
                    .is_some_and(|(id, at)| id == &p.id && time.elapsed_secs() - at < 0.32);
                feedback.last_click = Some((p.id.clone(), time.elapsed_secs()));
                if double && p.owner == session.owner {
                    controls.selected = battle
                        .snapshot
                        .entities
                        .iter()
                        .filter(|e| {
                            e.owner == session.owner
                                && e.kind == p.kind
                                && camera.world_to_viewport(transform, e.position()).is_ok_and(
                                    |s| {
                                        s.x > 0.
                                            && s.y > 0.
                                            && s.x < window.width()
                                            && s.y < window.height()
                                            && !ui::over_ui(s, window.size(), &controls)
                                    },
                                )
                        })
                        .map(|e| e.id.clone())
                        .collect();
                    return;
                }
                if controls.selected.contains(&p.id) {
                    controls.selected.retain(|id| id != &p.id);
                } else {
                    controls.selected.push(p.id.clone());
                }
            }
        }
    }
    if mouse.just_released(MouseButton::Right) && !controls.right_dragged && !over_ui {
        if controls.build.take().is_some() {
            controls.bridge_start = None;
            return;
        }
        if let Some(point) = controls.ground {
            if let Some(flag) = cursor::pick(&battle.snapshot, cursor, camera, transform)
                .filter(|p| p.kind == "objective")
            {
                controls.capture_zone = Some(flag.id.clone());
            }
            if let Some(carrier) = cursor::pick(&battle.snapshot, cursor, camera, transform)
                .filter(|p| cursor::can_board(&battle.snapshot, &controls, session.owner, p))
            {
                queue.order(&controls.selected, "load", None, Some(&carrier.id), false);
                feedback.acknowledge();
                return;
            }
            let building = cursor::pick(&battle.snapshot, cursor, camera, transform)
                .filter(|p| cursor::can_garrison(&battle.snapshot, &controls, session.owner, p));
            if let Some(building) = building {
                queue.order(
                    &controls.selected,
                    "garrison",
                    Some(cursor::attack_position(building, cursor, camera, transform)),
                    Some(&building.id),
                    false,
                );
                feedback.acknowledge();
                return;
            }
            let picked = cursor::pick(&battle.snapshot, cursor, camera, transform).filter(|p| {
                cursor::hostile(p, session.owner)
                    && cursor::can_attack(
                        &battle.snapshot,
                        &controls,
                        &battle.catalog,
                        session.owner,
                    )
            });
            let target = picked.map(|p| p.id.as_str());
            let point = picked
                .map(|p| cursor::attack_position(p, cursor, camera, transform))
                .unwrap_or(point);
            let kind = if target.is_some() {
                "attack"
            } else {
                cursor::ground_order(&battle.snapshot, &controls, &battle.catalog, session.owner)
            };
            queue.order(&controls.selected, kind, Some(point), target, shift);
            if battle
                .snapshot
                .entities
                .iter()
                .any(|p| p.owner == session.owner && controls.selected.contains(&p.id))
            {
                feedback.order(point, matches!(kind, "attack" | "attack-move"));
            }
            if !shift {
                controls.move_only = false;
                controls.attack_move = false;
                controls.patrol = false;
            }
        }
    }
}
#[cfg(test)]
mod camera_tests {
    use super::*;
    #[test]
    fn card_focus_requires_two_clicks_on_the_same_unit_and_base_returns_to_the_saved_view() {
        let mut c = Controls::default();
        let start = c.center;
        c.select_card("a", Vec3::new(300., 2., 400.), 1.);
        assert_eq!(c.center, start);
        c.select_card("b", Vec3::new(500., 2., 400.), 1.1);
        assert_eq!(c.center, start);
        c.select_card("b", Vec3::new(501., 2., 401.), 1.3);
        assert_eq!(c.center, Vec3::new(501., 2., 401.));
        c.pan_velocity = Vec3::X * 100.;
        let previous = (c.center, c.zoom, c.yaw);
        c.toggle_base(Vec3::new(80., 0., 80.));
        assert_eq!(c.center, Vec3::new(80., 0., 80.));
        assert_eq!(c.pan_velocity, Vec3::ZERO);
        c.toggle_base(Vec3::new(80., 0., 80.));
        assert_eq!((c.center, c.zoom, c.yaw), previous);
        c.select_card("a", Vec3::ONE, 2.);
        c.select_card("a", Vec3::ONE, 3.);
        assert_eq!(c.center, previous.0);
    }
    #[test]
    fn keyboard_panning_keeps_camera_heading_and_pitch_when_it_starts_and_stops() {
        for yaw in [0f32, 0.8, 2.5] {
            let mut center = Vec3::new(100., 12., 100.);
            let offset = fixed_view::offset(80.);
            let mut camera =
                Transform::from_translation(center + offset).looking_at(center, Vec3::Y);
            let rotation = camera.rotation;
            for step in [
                Vec3::X,
                Vec3::NEG_Z,
                Vec3::new(2., 0.2, 3.),
                Vec3::ZERO,
                Vec3::ZERO,
            ] {
                let previous = center;
                center += step;
                follow_camera(&mut camera, previous, center, yaw, 80., 0.016);
                assert!(camera.rotation.angle_between(rotation) < 0.001);
                assert!((camera.translation - center).distance(offset) < 0.001);
            }
        }
    }
    #[test]
    fn keyboard_pan_has_equal_speed_at_low_and_high_frame_rates_and_stops_immediately() {
        for fps in [10, 15, 20, 30, 60, 144] {
            let displacement: Vec3 = (0..fps)
                .map(|_| pan_displacement(Vec3::X, 240., 1. / fps as f32))
                .sum();
            assert!(
                (displacement.x - 240.).abs() < 0.001,
                "{fps} FPS: {displacement:?}"
            );
            assert_eq!(
                pan_displacement(Vec3::ZERO, 240., 1. / fps as f32),
                Vec3::ZERO
            );
        }
    }
    #[test]
    fn wasd_pan_is_relative_to_orbit_and_diagonals_keep_speed() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyW);
        assert!((camera_pan(&keys, 0., true) + Vec3::Z).length() < 0.00001);
        assert!(
            (camera_pan(&keys, std::f32::consts::FRAC_PI_2, true) + Vec3::X).length() < 0.00001
        );
        keys.press(KeyCode::KeyD);
        assert!((camera_pan(&keys, 0., true).length() - 1.).abs() < 0.00001);
        assert_eq!(camera_pan(&keys, 0., false), Vec3::ZERO);
    }
}
