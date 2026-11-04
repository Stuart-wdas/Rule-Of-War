//! Optional automated render check: --smoke-test <absolute screenshot path>.
use crate::{Controls, simulation::NativeGame};
use bevy::{
    app::AppExit,
    camera::RenderTarget,
    ecs::system::SystemParam,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

#[derive(Resource)]
pub struct CaptureTarget(Handle<Image>);

fn building_blast_at(game: &NativeGame) -> [f32; 2] {
    game.entities
        .iter()
        .filter(|e| e.view.kind == "town")
        .flat_map(|e| {
            e.view.lots.iter().map(move |lot| {
                let q = lot.to_world(lot.wings()[0].at);
                let at = e.view.position()
                    + Quat::from_rotation_y(e.view.heading)
                        * Vec3::new(q[0], 0., q[1])
                        * e.view.model_scale;
                [at.x, at.z]
            })
        })
        .min_by(|a, b| {
            let distance = |p: &[f32; 2]| (p[0] - 64.).powi(2) + (p[1] - 65.).powi(2);
            distance(a).total_cmp(&distance(b))
        })
        .unwrap_or([64., 65.])
}

/// An opt-in image target keeps render QA usable when the desktop window is minimized.
pub fn setup_capture(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    camera: Single<Entity, With<crate::RtsCamera>>,
) {
    if !std::env::args().any(|s| s == "--offscreen") {
        return;
    }
    let (width, height) = if std::env::args().any(|s| s == "--compact") {
        (1100, 850)
    } else {
        (1920, 1080)
    };
    let mut image = Image::new_target_texture(
        width,
        height,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    );
    image.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::COPY_SRC;
    let handle = images.add(image);
    commands.entity(*camera).insert((
        RenderTarget::Image(handle.clone().into()),
        IsDefaultUiCamera,
    ));
    commands.insert_resource(CaptureTarget(handle));
}

#[derive(SystemParam)]
pub struct RenderChecks<'w, 's> {
    stress: Option<Res<'w, crate::camera_stress::Study>>,
    scenery_stats: Option<Res<'w, crate::scenery::Stats>>,
    scene_art: Res<'w, crate::scene::Art>,
    blasts: MessageWriter<'w, crate::volume_effects::Blast>,
    result_state: Res<'w, crate::debrief::Debrief>,
    result_panel: Query<'w, 's, &'static Node, With<crate::debrief::ResultPanel>>,
    unit_icons: Query<'w, 's, (), With<crate::unit_icons::Badge>>,
    tactical_ui: Query<'w, 's, &'static Node, With<crate::menu::TacticalUi>>,
    main_menu: Query<'w, 's, &'static Node, With<crate::menu::MenuRoot>>,
    meshes: Res<'w, Assets<Mesh>>,
    terrain_images: Res<'w, Assets<Image>>,
    nature: Res<'w, crate::landscape::Nature>,
    grass_materials: Res<'w, Assets<crate::grass_material::GrassMaterial>>,
    concrete_materials: Res<'w, Assets<crate::ground_finish::GroundMaterial>>,
    concrete_nodes: Query<'w, 's, &'static MeshMaterial3d<crate::ground_finish::GroundMaterial>>,
    mesh_nodes: Query<'w, 's, &'static Mesh3d>,
    hierarchy: Query<'w, 's, &'static Children>,
    actors: Query<'w, 's, (Entity, &'static crate::scene::Actor)>,
    impact_scars: Query<'w, 's, &'static crate::ground_impacts::Scar>,
    actions: Query<'w, 's, &'static crate::ui::Action, With<Button>>,
    distance_fog: Query<'w, 's, &'static DistanceFog, With<crate::RtsCamera>>,
    projection: Query<'w, 's, &'static Projection, With<crate::RtsCamera>>,
    capture: Option<Res<'w, CaptureTarget>>,
    real_time: Res<'w, Time<Real>>,
    diagnostics: Option<Res<'w, bevy::diagnostic::DiagnosticsStore>>,
    cursor: Res<'w, crate::cursor::TacticalCursor>,
    strafe: Res<'w, crate::strafe::Overlay>,
    particles: Query<'w, 's, &'static ViewVisibility, With<crate::effects::Particle>>,
    volume_particles:
        Query<'w, 's, &'static ViewVisibility, With<crate::volume_effects::BakedParticle>>,
    baked_billows: Query<'w, 's, &'static ViewVisibility, With<crate::baked_blast::Billow>>,
    billow_styles: Query<'w, 's, &'static crate::baked_blast::Billow>,
    audio_voices: Query<'w, 's, (), With<crate::sound::OneShot>>,
    audio_engines: Query<'w, 's, (), With<crate::sound::Engine>>,
    audio_sinks: Query<'w, 's, (), With<AudioSink>>,
    billow_scales: Query<'w, 's, &'static Transform, With<crate::baked_blast::Billow>>,
    shell_scales: Query<'w, 's, &'static Transform, With<crate::shockwave::Shell>>,
    pressure_shells: Query<'w, 's, (), With<crate::shockwave::Shell>>,
    pressure_stats: Res<'w, crate::shockwave::Stats>,
    dirt_disc_stats: Res<'w, crate::dust_disc::Stats>,
    vfx_quality: Res<'w, crate::volume_effects::Quality>,
    graphics: Res<'w, crate::graphics::Graphics>,
    ground_wear: Res<'w, crate::ground_detail::GroundWear>,
    baked_art: Res<'w, crate::baked_blast::Art>,
    world_sun:
        Query<'w, 's, (&'static DirectionalLight, &'static Transform), With<crate::lighting::Sun>>,
    world_fill: Query<'w, 's, &'static DirectionalLight, With<crate::lighting::Fill>>,
    world_ambient: Res<'w, GlobalAmbientLight>,
    cloud_shadows: Res<'w, crate::cloud_shadows::CloudShadows>,
    world_probe: Query<'w, 's, &'static bevy::light::EnvironmentMapLight, With<crate::RtsCamera>>,
    world_cascades:
        Query<'w, 's, &'static bevy::light::CascadeShadowConfig, With<crate::lighting::Sun>>,
    vfx_showcase: Option<Res<'w, crate::volume_effects::Showcase>>,
    visual_demo: Res<'w, crate::visual_demo::Study>,
    trails: Query<'w, 's, &'static crate::projectiles::Transient>,
    flares: Query<'w, 's, (), With<crate::projectiles::Transient>>,
    flights: Query<'w, 's, (), With<crate::projectiles::Flight>>,
    scenery: Query<'w, 's, &'static crate::scenery::Scenery>,
    camera: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<crate::RtsCamera>>,
    infantry: Query<'w, 's, &'static ViewVisibility, With<crate::infantry::InfantrySkin>>,
    bones: Query<'w, 's, (Entity, &'static Transform), With<crate::infantry::InfantryBone>>,
}

#[derive(SystemParam)]
pub struct Pointer<'w, 's> {
    window: Single<'w, 's, (Entity, &'static mut Window)>,
    messages: MessageWriter<'w, bevy::input::mouse::MouseButtonInput>,
    keyboard: MessageWriter<'w, bevy::input::keyboard::KeyboardInput>,
}
#[derive(Resource)]
pub struct Smoke {
    pub path: String,
    frames: u32,
    mock_stats: crate::simulation::BattleBenchStats,
    mock_wave: u64,
    start_center: Option<Vec3>,
    max_frame_seconds: f32,
    limb_start: std::collections::HashMap<Entity, Vec3>,
    max_limb_motion: f32,
    bone_start: std::collections::HashMap<Entity, Quat>,
    max_bone_rotation: f32,
    peak_projectiles: usize,
    peak_effects: usize,
    peak_flares: usize,
    door_crossers: std::collections::BTreeSet<String>,
    cover_holders: usize,
    blasted: bool,
    blast_warmed: bool,
    captured_effects: usize,
    observed_baked_styles: std::collections::BTreeSet<(&'static str, usize)>,
    peak_audio_voices: usize,
    blast_started: f32,
    drag_started: f32,
    drag_stage: u8,
    drag_distance: f32,
    drag_start: Vec3,
    activities: std::collections::BTreeSet<String>,
    scenario_started: f32,
    camera_started: f32,
    captured: bool,
    captured_at: f32,
    clearance_violations: usize,
    vehicle_start: Option<Vec2>,
    vehicle_distance: f32,
    scenery_positions: std::collections::HashMap<IVec2, Vec3>,
    scenery_shifted: usize,
    scenery_duplicates: usize,
    scenery_peak_props: usize,
    scenery_peak_chunks: usize,
    scenery_coverage: bool,
    stopped_positions: std::collections::HashMap<String, Vec2>,
    stop_tick: u64,
    stop_checked: bool,
    resume_checked: bool,
    hud_check_start: Option<u32>,
    overview_started: Option<f32>,
    overview_builds: u32,
    overview_peak_distance: f32,
}
pub fn profile(diagnostics: Res<bevy::diagnostic::DiagnosticsStore>, smoke: Option<Res<Smoke>>) {
    if smoke.is_some_and(|s| s.frames.is_multiple_of(60)) {
        let mut samples: Vec<_> = diagnostics
            .iter()
            .filter_map(|d| d.smoothed().map(|v| (d.path().to_string(), v)))
            .filter(|(path, _)| path.contains("elapsed") || path == "frame_time" || path == "fps")
            .collect();
        samples.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (path, value) in samples.into_iter().take(12) {
            println!("Profile {path}: {value:.3}");
        }
    }
}
impl Smoke {
    pub fn new(path: String) -> Self {
        Self {
            path,
            frames: 0,
            overview_started: None,
            overview_builds: 0,
            overview_peak_distance: 0.,
            mock_stats: Default::default(),
            mock_wave: 0,
            start_center: None,
            max_frame_seconds: 0.,
            limb_start: default(),
            max_limb_motion: 0.,
            bone_start: default(),
            max_bone_rotation: 0.,
            peak_projectiles: 0,
            peak_effects: 0,
            peak_flares: 0,
            door_crossers: default(),
            cover_holders: 0,
            blasted: false,
            blast_warmed: false,
            captured_effects: 0,
            observed_baked_styles: default(),
            peak_audio_voices: 0,
            blast_started: 0.,
            drag_started: 0.,
            drag_stage: 0,
            drag_distance: 0.,
            drag_start: Vec3::ZERO,
            activities: default(),
            scenario_started: 0.,
            camera_started: 0.,
            captured: false,
            captured_at: 0.,
            clearance_violations: 0,
            vehicle_start: None,
            vehicle_distance: 0.,
            scenery_positions: default(),
            scenery_shifted: 0,
            scenery_duplicates: 0,
            scenery_peak_props: 0,
            scenery_peak_chunks: 0,
            scenery_coverage: false,
            stopped_positions: default(),
            stop_tick: 0,
            stop_checked: false,
            resume_checked: false,
            hud_check_start: None,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn run(
    mut smoke: ResMut<Smoke>,
    mut game: ResMut<NativeGame>,
    mut controls: ResMut<Controls>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    mut front: ResMut<crate::frontend::Frontend>,
    battle: Res<crate::runtime::Battle>,
    session: Res<crate::network::Session>,
    mut menu: ResMut<crate::menu::MainMenu>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    ready: Res<crate::landscape::Ready>,
    mut loading: ResMut<crate::loading::Loading>,
    time: Res<Time>,
    input: Pointer,
    limbs: Query<(Entity, &Transform), With<crate::scene::Limb>>,
    mut render: RenderChecks,
) {
    let (window_id, mut window) = input.window.into_inner();
    if render.capture.is_some() {
        window.focused = true;
    }
    let mut pointer = input.messages;
    let mut keyboard = input.keyboard;
    smoke.frames += 1;
    if crate::gis_prototype::enabled()
        && game.terrain.id == crate::gis_prototype::ID
        && ready.ready
        && smoke.scenario_started == 0.
    {
        smoke.scenario_started = time.elapsed_secs();
    }
    if std::env::args().any(|s| s == "--hide-ui")
        && smoke.frames > 60
        && (smoke.hud_check_start.is_some() || (ready.ready && !loading.busy() && !menu.open))
    {
        let frame = smoke.frames;
        let start = *smoke.hud_check_start.get_or_insert(frame);
        let elapsed = frame - start;
        let restore = std::env::args().any(|s| s == "--restore-ui");
        let menu_check = std::env::args().any(|s| s == "--hidden-ui-menu");
        let toggle = [0, 2].contains(&elapsed) || (restore && [50, 52].contains(&elapsed));
        let open_menu = menu_check && [80, 82].contains(&elapsed);
        if toggle || open_menu {
            keyboard.write(bevy::input::keyboard::KeyboardInput {
                key_code: if open_menu {
                    KeyCode::Escape
                } else {
                    KeyCode::F10
                },
                logical_key: if open_menu {
                    bevy::input::keyboard::Key::Escape
                } else {
                    bevy::input::keyboard::Key::F10
                },
                state: if [0, 50, 80].contains(&elapsed) {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
                text: None,
                repeat: false,
                window: window_id,
            });
        }
        if elapsed == 30 {
            assert!(controls.hud_hidden, "F10 did not hide the UI");
            assert!(
                render
                    .tactical_ui
                    .iter()
                    .all(|n| n.display == Display::None)
            );
            assert_eq!(render.unit_icons.iter().count(), 0);
        }
        if elapsed == 65 && restore {
            assert!(!controls.hud_hidden, "F10 did not restore the UI");
            assert!(
                render
                    .tactical_ui
                    .iter()
                    .any(|n| n.display != Display::None)
            );
            assert!(render.unit_icons.iter().count() > 0);
        }
        if elapsed == 95 && menu_check {
            assert!(
                menu.open,
                "Escape must still open the menu with the HUD hidden"
            );
            assert!(render.main_menu.iter().any(|n| n.display != Display::None));
        }
    }
    if smoke.frames == 1 && std::env::args().any(|s| s == "--compact") {
        window.resolution.set(1100., 850.);
    }
    if smoke.frames.is_multiple_of(60) {
        println!("Render check frame {}", smoke.frames);
    }
    if smoke.frames == 30 && !crate::visual_demo::enabled() && !crate::gis_prototype::enabled() {
        smoke.scenario_started = time.elapsed_secs();
        let args: Vec<String> = std::env::args().collect();
        menu.open = args.iter().any(|s| {
            s == "--menu" || s == "--operations" || s == "--guide" || s == "--graphics-menu"
        });
        menu.started = true;
        if args.iter().any(|s| s == "--blast-ground-check") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            let (x, z) = if args.iter().any(|s| s == "--blast-slope") {
                (310., 301.)
            } else {
                (170., 168.)
            };
            controls.center = Vec3::new(x, game.terrain.height(x, z), z);
            controls.zoom = 68.;
            controls.selected.clear();
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--impact-scar-check") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            let at = if args.iter().any(|s| s == "--scar-slope") {
                [292., 299.]
            } else {
                [193., 190.]
            };
            for (offset, weapon) in [
                ([-8., -3.], "medium"),
                ([0., -3.], "long"),
                ([9., -3.], "xtralong"),
                ([-4., 7.], "long"),
                ([-2.5, 7.8], "long"),
            ] {
                game.smoke_ground_impact([at[0] + offset[0], at[1] + offset[1]], weapon);
            }
            controls.center = Vec3::new(at[0], game.terrain.height(at[0], at[1]), at[1]);
            controls.zoom = 52.;
            controls.selected.clear();
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args
            .iter()
            .any(|s| s == "--victory-check" || s == "--defeat-check")
        {
            game.smoke_objective_victory(if args.iter().any(|s| s == "--defeat-check") {
                1
            } else {
                0
            });
            controls.center = Vec3::new(512., game.terrain.height(512., 512.), 512.);
            controls.zoom = 240.;
            controls.selected.clear();
        }
        if args.iter().any(|s| s == "--runway-check") {
            let id = game.smoke_runway_navigation();
            let unit = game.entities.iter().find(|e| e.view.id == id).unwrap();
            controls.center = Vec3::new(
                unit.view.x - 9.,
                game.terrain.height(unit.view.x, unit.view.z),
                unit.view.z - 3.,
            );
            controls.zoom = 80.;
            controls.yaw = 0.5;
            controls.selected = vec![id];
        }
        if args.iter().any(|s| s == "--mock-battle") {
            let case = mock_case();
            game.setup_mock_battle(&case);
            smoke.mock_wave = game.tick / 400;
            let (cx, cz) = (
                game.terrain.cols as f32 * 0.5,
                game.terrain.rows as f32 * 0.5,
            );
            controls.center = Vec3::new(cx, game.terrain.height(cx, cz), cz);
            controls.zoom = 240.;
            controls.yaw = 0.25;
            controls.selected.clear();
            if args.iter().any(|s| s == "--stop-check") {
                for e in game.entities.iter_mut().filter(|e| {
                    e.view.owner == 0
                        && !e.view.building
                        && !e.view.airborne
                        && e.view.category != "aircraft"
                }) {
                    e.view.hp = 10000.;
                    e.view.max_hp = 10000.;
                    controls.selected.push(e.view.id.clone());
                }
                assert!(!controls.selected.is_empty());
            }
        }
        if args.iter().any(|s| s == "--battle-profile") {
            *game = NativeGame::default();
            game.config["fogEnabled"] = serde_json::json!(true);
            for group in 0..4 {
                for owner in [0, 1] {
                    let x = 180. + group as f32 * 210.;
                    let mut ids = vec![];
                    for soldier in 0..6 {
                        let id = game.spawn(
                            "short",
                            owner,
                            [
                                x + if owner == 0 { -15. } else { 15. },
                                690. + soldier as f32 * 4.,
                            ],
                        );
                        let e = game.entities.last_mut().unwrap();
                        e.view.hp = 100.;
                        e.view.max_hp = 100.;
                        ids.push(id);
                    }
                    game.command_as(owner, serde_json::json!({"type":"command","commandType":"attack-move","unitIds":ids,"destination":{"x":x + if owner == 0 { 35. } else { -35. },"y":700.}}));
                }
            }
            controls.center = Vec3::new(390., game.terrain.height(390., 700.), 700.);
            controls.zoom = 180.;
            controls.yaw = 0.2;
            controls.selected.clear();
        }
        if args
            .iter()
            .any(|s| s == "--range-check" || s == "--town-entry")
        {
            controls.selected = game.smoke_infantry_range();
            controls.center = Vec3::new(174., game.terrain.height(174., 190.), 190.);
            controls.zoom = 145.;
            controls.yaw = 0.15;
        }
        if args.iter().any(|s| s == "--organic-map") {
            game.config["fogEnabled"] = serde_json::json!(false);
            game.human_opponent = true;
            let close = args.iter().any(|s| s == "--ground-close");
            let (x, z) = if close { (330., 650.) } else { (265., 720.) };
            if close {
                let id = game.spawn("short", 0, [x, z]);
                controls.selected = vec![id];
                game.spawn("medium", 0, [x + 9., z + 4.]);
            }
            controls.center = Vec3::new(x, game.terrain.height(x, z), z);
            controls.zoom = if close { 42. } else { 160. };
            controls.yaw = 0.35;
        }
        if args.iter().any(|s| s == "--joined-town") {
            *game = NativeGame::default();
            game.entities
                .retain(|e| ["town", "objective"].contains(&e.view.kind.as_str()));
            game.bases.clear();
            game.props.clear();
            game.config["fogEnabled"] = serde_json::json!(false);
            game.paused = true;
            let (cx, cz) = (
                game.terrain.cols as f32 * 0.5,
                game.terrain.rows as f32 * 0.5,
            );
            controls.center = Vec3::new(cx, game.terrain.height(cx, cz), cz);
            controls.zoom = 215.;
            controls.yaw = 0.12;
            controls.hud_hidden = args.iter().any(|s| s == "--hide-hud");
            if args.iter().any(|s| s == "--village-close") {
                let at = crate::rural_layout::world([0., -8.]);
                let (x, z) = (cx + at[0], cz + at[1]);
                controls.center = Vec3::new(x, game.terrain.height(x, z), z);
                controls.zoom = 80.;
            }
        }
        if args.iter().any(|s| s == "--environment-town") {
            game.config["fogEnabled"] = serde_json::json!(false);
            game.human_opponent = true;
            game.paused = true;
            controls.center = Vec3::new(497., game.terrain.height(497., 497.), 497.);
            controls.zoom = 78.;
            controls.yaw = 0.38;
            if args.iter().any(|s| s == "--foliage-close") {
                controls.center = Vec3::new(474., game.terrain.height(474., 543.) + 1.2, 543.);
                controls.zoom = 28.;
            }
        }
        if args.iter().any(|s| s == "--carrier-check") {
            let id = game.smoke_carrier(args.iter().any(|s| s == "--unloaded"));
            controls.center = Vec3::new(185., game.terrain.height(185., 180.), 180.);
            controls.zoom = 32.;
            controls.yaw = 0.5;
            controls.selected = vec![id];
        }
        if args.iter().any(|s| s == "--huey-check") {
            let id = game.smoke_huey(
                args.iter().any(|s| s == "--unloaded"),
                args.iter().any(|s| s == "--huey-flight"),
            );
            let p = game.entities.iter_mut().find(|e| e.view.id == id).unwrap();
            if args.iter().any(|s| s == "--model-enemy") {
                p.view.owner = 1;
            }
            let lift =
                p.view.flight_height() * crate::fixed_view::YAW.sin() / crate::fixed_view::RISE;
            controls.center = Vec3::new(p.view.x - lift, p.view.elevation, p.view.z - lift);
            controls.zoom = if args.iter().any(|s| s == "--huey-close") {
                22.
            } else {
                34.
            };
            controls.yaw = 0.5;
            controls.selected = vec![id];
        }
        if args
            .iter()
            .any(|s| s == "--heli-combat" || s == "--rocket-trails")
        {
            let rocket = args.iter().any(|s| s == "--rocket-trails");
            let id = if rocket {
                game.smoke_rocket_trails()
            } else {
                game.smoke_helicopter(args.iter().any(|s| s == "--heli-infantry"))
            };
            let unit = game
                .entities
                .iter()
                .find(|e| e.view.id == id)
                .unwrap()
                .view
                .clone();
            controls.center = if rocket {
                Vec3::new(165., game.terrain.height(165., 180.), 180.)
            } else {
                Vec3::new(
                    (unit.x + 190.) * 0.5,
                    game.terrain.height(190., 210.),
                    (unit.z + 210.) * 0.5,
                )
            };
            controls.zoom = if rocket { 150. } else { 135. };
            controls.yaw = 0.3;
            controls.selected = vec![id];
        }
        if args
            .iter()
            .any(|s| s == "--aircraft-check" || s == "--aircraft-parking")
        {
            *game = NativeGame::operation(
                crate::training::ID,
                320,
                320,
                crate::simulation::core_config(&game.config),
            );
            game.config["fogEnabled"] = serde_json::json!(false);
            game.entities
                .retain(|e| e.view.building && e.view.owner == 0);
            let mut planes = vec![];
            for kind in ["plane", "plane", "helicopter", "helicopter"] {
                game.command(serde_json::json!({"type":"training","action":"buy","kind":kind}));
                let id = game
                    .training
                    .as_ref()
                    .unwrap()
                    .last_purchase
                    .clone()
                    .unwrap();
                if kind == "plane" {
                    planes.push(id);
                }
            }
            let base = game
                .entities
                .iter()
                .find(|e| e.view.kind == "airfield")
                .unwrap()
                .view
                .clone();
            controls.center = Vec3::new(base.x + 5., base.elevation, base.z);
            controls.zoom = 410. * base.model_scale;
            controls.yaw = 0.25;
            controls.selected = vec![base.id.clone()];
            if args.iter().any(|s| s == "--aircraft-check") {
                let target = game.spawn("medium", 1, [190., 210.]);
                game.entities.last_mut().unwrap().view.hp = 1000.;
                game.command(serde_json::json!({"type":"command","commandType":"attack","unitIds":[planes[0]],"targetId":target}));
                for _ in 0..4500 {
                    game.step();
                    let approaching = args.iter().any(|s| s == "--attack-approach")
                        && game.entities.iter().any(|e| {
                            e.view.id == planes[0]
                                && e.view.flight_pitch < -0.08
                                && Vec2::new(e.view.x - 190., e.view.z - 210.).length() < 390.
                        });
                    if approaching || game.training.as_ref().unwrap().shots >= 4 {
                        break;
                    }
                }
                for _ in 0..8 {
                    game.step();
                }
                if args.iter().any(|s| s == "--pullup") {
                    for _ in 0..120 {
                        if game.training.as_ref().unwrap().shots >= 60 {
                            break;
                        }
                        game.step();
                    }
                    for _ in 0..40 {
                        game.step();
                    }
                }
                let plane = game
                    .entities
                    .iter()
                    .find(|e| e.view.id == planes[0])
                    .unwrap()
                    .view
                    .clone();
                controls.center = Vec3::new((plane.x + 190.) * 0.5, 0., (plane.z + 210.) * 0.5);
                controls.zoom = 220.;
                controls.yaw = -0.7;
                controls.selected = vec![planes[0].clone()];
                if args.iter().any(|s| s == "--flight-close") {
                    // Gameplay pans on the ground plane. Project the aircraft
                    // along the fixed viewing ray to centre it at altitude.
                    let normal = crate::fixed_view::normal();
                    let mut center = plane.position();
                    for _ in 0..2 {
                        let height = plane.position().y - game.terrain.height(center.x, center.z);
                        center = plane.position() - normal * (height / normal.y);
                    }
                    controls.center = center;
                    controls.zoom = 70.;
                }
                if args.iter().any(|s| s == "--cannon-contacts") {
                    controls.center = Vec3::new(190., game.terrain.height(190., 210.), 210.);
                    controls.zoom = 32.;
                    controls.yaw = 0.6;
                }
            }
            if args
                .iter()
                .any(|s| s == "--warthog-model" || s == "--apache-model")
            {
                let kind = if args.iter().any(|s| s == "--warthog-model") {
                    "plane"
                } else {
                    "helicopter"
                };
                let unit = &game
                    .entities
                    .iter()
                    .find(|e| e.view.kind == kind)
                    .unwrap()
                    .view;
                controls.center = Vec3::new(unit.x, unit.elevation, unit.z);
                controls.zoom = if kind == "plane" { 48. } else { 36. };
                if kind == "helicopter" && args.iter().any(|s| s == "--apache-close") {
                    controls.zoom = 20.;
                }
                controls.yaw = if kind == "plane" { 0.65 } else { -0.65 };
                controls.selected = vec![unit.id.clone()];
                if args.iter().any(|s| s == "--model-enemy") {
                    for entity in &mut game.entities {
                        entity.view.owner = 1;
                    }
                }
            }
            if args
                .iter()
                .any(|s| s == "--apron-close" || s == "--hardstanding-close")
            {
                let at = crate::airfield_layout::world(
                    &base,
                    if args.iter().any(|s| s == "--hardstanding-close") {
                        [47., 65.]
                    } else {
                        [-12., 4.]
                    },
                );
                controls.center = Vec3::new(at[0], base.elevation, at[1]);
                controls.zoom = 38.;
                let tank = game.spawn("medium", 0, [at[0] + 4., at[1] + 2.]);
                controls.selected = vec![tank];
            }
            controls.training_purchase = game.training.as_ref().unwrap().last_purchase.clone();
            controls.training_shop = false;
            controls.training_tools = false;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
            game.paused = true;
        }
        if args.iter().any(|s| s == "--town-entry") {
            controls.selected = game.smoke_town_entry(!args.iter().any(|s| s == "--town-routes"));
            let p = game
                .entities
                .iter()
                .find(|e| Some(&e.view.id) == controls.selected.first())
                .unwrap()
                .view
                .position();
            controls.center = p;
            controls.zoom = 100.;
            controls.yaw = 0.35;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--helicopter-crash") {
            *game = NativeGame::operation(
                crate::training::ID,
                384,
                384,
                crate::simulation::core_config(&game.config),
            );
            game.config["fogEnabled"] = serde_json::json!(false);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.spawn("helicopter", 1, [192., 192.]);
            let helicopter = game.entities.last_mut().unwrap();
            helicopter.view.id = "destruction-check-heli".into();
            helicopter.view.airborne = true;
            helicopter.view.altitude = Some(36.);
            helicopter.view.velocity = [6., 1.];
            helicopter.view.heading = 0.4;
            game.smoke_destroy_models();
            game.paused = false;
            for _ in 0..if args.iter().any(|s| s == "--crash-impact") {
                160
            } else {
                20
            } {
                game.step();
                if args.iter().any(|s| s == "--crash-impact")
                    && game
                        .props
                        .iter()
                        .all(|p| p.wreck.as_ref().is_none_or(|w| w.crash.is_none()))
                {
                    break;
                }
            }
            game.paused = true;
            let wreck = game.props.iter().find(|p| p.wreck.is_some()).unwrap();
            controls.center = Vec3::new(wreck.at[0], 0., wreck.at[1]);
            controls.zoom = if args.iter().any(|s| s == "--crash-impact") {
                45.
            } else {
                105.
            };
            controls.yaw = 0.4;
            controls.training_tools = false;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--garrison-check") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(true);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            let building = game.spawn("village", -1, [192., 192.]);
            game.entities[0].view.lots = vec![crate::settlement_layout::Lot {
                at: [0., 0.],
                width: 6.,
                depth: 4.,
                height: 3.2,
                yaw: 0.,
                style: 0,
                ground: 0.,
                integrity: 1.,
            }];
            game.refresh_towns();
            let a = game.spawn("short", 0, [186., 205.]);
            let b = game.spawn("short", 0, [185., 206.]);
            let target = game.spawn("short", 1, [192., 212.]);
            game.entities.last_mut().unwrap().view.hp = 1000.;
            game.entities.last_mut().unwrap().view.max_hp = 1000.;
            game.command_as(
                1,
                serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[target]}),
            );
            game.command(serde_json::json!({"type":"command","commandType":"garrison","unitIds":[a,b],"targetId":building,"destination":{"x":192.,"y":192.}}));
            controls.selected = vec![a, b];
            controls.center = Vec3::new(192., 0., 198.);
            controls.zoom = 28.;
            controls.yaw = 0.4;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args
            .iter()
            .any(|s| s == "--tank-model" || s == "--vehicle-model")
        {
            *game = NativeGame::operation(
                crate::training::ID,
                384,
                384,
                crate::simulation::core_config(&game.config),
            );
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.config["fogEnabled"] = serde_json::json!(false);
            let kind = args
                .iter()
                .position(|s| s == "--vehicle-model")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
                .unwrap_or("medium");
            game.spawn(
                kind,
                i32::from(args.iter().any(|s| s == "--model-enemy")),
                [192., 192.],
            );
            game.entities.last_mut().unwrap().view.heading = std::f32::consts::FRAC_PI_2;
            controls.center = Vec3::new(192., game.terrain.height(192., 192.) + 0.8, 192.);
            controls.zoom = 18.;
            controls.yaw = 0.5;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--operations") {
            menu.page = crate::menu::Page::Operations;
        }
        if args.iter().any(|s| s == "--guide") {
            menu.page = crate::menu::Page::Guide;
        }
        if args.iter().any(|s| s == "--graphics-menu") {
            menu.page = crate::menu::Page::Options;
        }
        if args.iter().any(|s| s == "--loading-check") {
            loading.begin(
                crate::loading::Request::Map {
                    id: "world-war".into(),
                    cols: 512,
                    rows: 512,
                    config: game.config.clone(),
                },
                "Preparing WORLD WAR · 512 × 512".into(),
            );
        }
        if args.iter().any(|s| s == "--scenery-check") {
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.terrain = crate::terrain::Terrain::new("scenery-check", 512, 512);
            game.paused = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            controls.center = Vec3::new(210., 0., 220.);
            controls.zoom = 120.;
            controls.yaw = 0.35;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--training-entry") {
            menu.open = true;
            commands.spawn((
                Interaction::Pressed,
                crate::menu::Choice::Training,
                BackgroundColor(Color::BLACK),
            ));
        }
        if args.iter().any(|s| s == "--training") && !args.iter().any(|s| s == "--training-entry") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            controls.training_tools = true;
            controls.training_shop = !args.iter().any(|s| s == "--training-tools");
            game.command(serde_json::json!({"type":"training","action":"buy","kind":"medium"}));
            let id = game
                .training
                .as_ref()
                .unwrap()
                .last_purchase
                .clone()
                .unwrap();
            game.command(serde_json::json!({"type":"training","action":"target","kind":"short","destination":{"x":116.,"y":176.}}));
            let target = game
                .entities
                .iter()
                .find(|e| {
                    e.view.training_target
                        && (e.view.x - 116.).abs() < 0.1
                        && (e.view.z - 176.).abs() < 0.1
                })
                .unwrap()
                .view
                .id
                .clone();
            game.command(serde_json::json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":target}));
            controls.training_purchase = Some(id.clone());
            controls.selected = vec![id];
            controls.center = Vec3::new(164., 0., 144.);
            controls.zoom = 160.;
            controls.yaw = 0.;
            if args.iter().any(|s| s == "--training-buildings") {
                if args.iter().any(|s| s == "--building-damage") {
                    for house in game
                        .entities
                        .iter_mut()
                        .filter(|e| e.view.label == "PRACTICE HOUSES")
                    {
                        for (i, lot) in house.view.lots.iter_mut().enumerate() {
                            lot.integrity = [1., 0.75, 0.35, 0.][i % 4];
                        }
                        house.view.hp = house
                            .view
                            .lots
                            .iter()
                            .map(|l| l.max_hp() * l.integrity)
                            .sum();
                    }
                    game.paused = true;
                }
                controls.center = Vec3::new(
                    crate::training::BUILDINGS[0],
                    0.,
                    crate::training::BUILDINGS[1],
                );
                controls.zoom = 100.;
                controls.yaw = 0.25;
                controls.training_tools = false;
                controls.selected = game
                    .entities
                    .iter()
                    .filter(|e| e.view.label == "PRACTICE HOUSES")
                    .map(|e| e.view.id.clone())
                    .collect();
            }
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--overview") {
            controls.center = Vec3::new(192., 0., 192.);
            controls.zoom = 300.;
            controls.yaw = 0.;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--countryside-map") {
            *game = NativeGame::default();
            game.paused = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            controls.selected.clear();
            let (cx, cz) = (
                game.terrain.cols as f32 * 0.5,
                game.terrain.rows as f32 * 0.5,
            );
            controls.center = Vec3::new(cx, game.terrain.height(cx, cz), cz);
            controls.zoom = 1450.;
            if args.iter().any(|s| s == "--field-alignment") {
                controls.zoom = 500.;
            }
            if args.iter().any(|s| s == "--countryside-close") {
                let at = crate::rural_layout::world([-245., -165.]);
                let (x, z) = (cx + at[0], cz + at[1]);
                controls.center = Vec3::new(x, game.terrain.height(x, z), z);
                controls.zoom = 210.;
            }
            if args.iter().any(|s| s == "--road-close") {
                let at = crate::rural_layout::world([-220., -29.3]);
                let (x, z) = (cx + at[0], cz + at[1]);
                controls.center = Vec3::new(x, game.terrain.height(x, z), z);
                controls.zoom = 100.;
            }
            if args
                .iter()
                .any(|s| s == "--west-hamlet" || s == "--farmstead" || s == "--yard-close")
            {
                let index = if args.iter().any(|s| s == "--farmstead") {
                    2
                } else {
                    0
                };
                let at = crate::battlefield::additional_sites(game.terrain.cols, game.terrain.rows)
                    [index]
                    .0;
                controls.center = Vec3::new(at[0], game.terrain.height(at[0], at[1]), at[1]);
                controls.zoom = 150.;
                if args.iter().any(|s| s == "--yard-close") {
                    if let Some(town) = game
                        .entities
                        .iter()
                        .filter(|e| e.view.kind == "town")
                        .min_by(|a, b| {
                            ((a.view.x - at[0]).powi(2) + (a.view.z - at[1]).powi(2)).total_cmp(
                                &((b.view.x - at[0]).powi(2) + (b.view.z - at[1]).powi(2)),
                            )
                        })
                    {
                        let lot = &town.view.lots[0];
                        let (x, z) = (town.view.x + lot.at[0], town.view.z + lot.at[1]);
                        controls.center = Vec3::new(x, game.terrain.height(x, z), z);
                        controls.zoom = 38.;
                    }
                }
            }
            if args.iter().any(|s| s == "--camera-boundary") {
                controls.center = Vec3::new(-9999., 0., -9999.);
                controls.zoom = 1536.;
            }
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--full-map-pan") {
            *game = NativeGame::default();
            game.paused = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            controls.center = Vec3::new(512., game.terrain.height(512., 512.), 512.);
            controls.zoom = 1075.;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--infantry-check") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            for (kind, at) in [
                ("short", [160., 168.]),
                ("short", [158., 173.]),
                ("engineer", [160., 179.]),
                ("commander", [164., 184.]),
                ("medic", [157., 183.]),
            ] {
                let id = game.spawn(kind, 0, at);
                if kind != "medic" {
                    game.command(serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[id]}));
                }
            }
            let first = game.entities[0].view.id.clone();
            let walker = game.entities[1].view.id.clone();
            let engineer = game.entities[2].view.id.clone();
            game.command(serde_json::json!({"type":"command","commandType":"move","unitIds":[walker],"destination":{"x":177.,"y":173.}}));
            game.props.push(crate::simulation::BattlefieldProp {
                scar: None,
                id: "pose-cover".into(),
                kind: "sandbags".into(),
                at: [161., 179.],
                heading: std::f32::consts::FRAC_PI_2,
                wreck: None,
                hp: 5.,
            });
            game.command(serde_json::json!({"type":"command","commandType":"move","unitIds":[engineer],"destination":{"x":160.,"y":179.}}));
            let target = game.spawn("medium", 1, [171., 168.]);
            let dummy = game.entities.last_mut().unwrap();
            dummy.view.training_target = true;
            dummy.view.hp = 100.;
            dummy.view.max_hp = 100.;
            game.command_as(
                1,
                serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[target]}),
            );
            controls.selected = vec![first];
            controls.training_tools = false;
            controls.center = Vec3::new(164., 0., 174.);
            controls.zoom = 23.;
            controls.yaw = 0.6;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
            if args.iter().any(|s| s == "--auto-attack-check") {
                game.entities.clear();
                game.props.clear();
                let id = game.spawn("short", 0, [160., 168.]);
                let target = game.spawn("short", 1, [182., 168.]);
                game.entities.last_mut().unwrap().view.hp = 1000.;
                game.entities.last_mut().unwrap().view.max_hp = 1000.;
                game.command_as(1, serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[target]}));
                game.spawn("watchtower", -1, [171., 168.]);
                // No attack or movement command is sent to the default-stance soldier.
                for _ in 0..400 {
                    game.step();
                }
                controls.selected = vec![id];
                controls.center = Vec3::new(171., game.terrain.height(171., 168.), 168.);
                controls.zoom = 55.;
                controls.yaw = 0.4;
            }
            if args.iter().any(|s| s == "--placement-cursor") {
                controls.build = Some("range:cover:wall".into());
            }
        }
        if args.iter().any(|s| s == "--weapons") {
            *game = NativeGame::default();
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            for (kind, at, target) in [
                ("short", [148., 148.], [176., 148.]),
                ("medium", [140., 172.], [190., 172.]),
                ("long", [130., 196.], [202., 196.]),
                ("xtralong", [140., 222.], [232., 222.]),
                ("antiAir", [142., 248.], [199., 248.]),
                ("plane", [184., 140.], [190., 140.]),
            ] {
                let id = game.spawn(kind, 0, at);
                let source = game.entities.last_mut().unwrap();
                source.view.airborne = kind == "plane";
                source.view.heading = 0.;
                source.view.hp = 500.;
                source.view.max_hp = 500.;
                let target_id = game.spawn("medic", 1, target);
                let dummy = game.entities.last_mut().unwrap();
                dummy.view.hp = 1000.;
                dummy.view.max_hp = 1000.;
                game.command(serde_json::json!({"type":"command","commandType":"attack","unitIds":[id],"targetId":target_id}));
            }
            game.paused = false;
            controls.center = Vec3::new(175., 0., 194.);
            controls.zoom = 102.;
            controls.yaw = 0.25;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--tactics") {
            *game = NativeGame::default();
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            for (kind, owner, at) in [
                ("barracks", 0, [35., 68.]),
                ("short", 0, [47., 64.]),
                ("short", 0, [47., 60.]),
                ("medic", 0, [47., 69.]),
                ("engineer", 0, [45., 75.]),
                ("medium", 0, [43., 65.]),
                ("barracks", 1, [91., 68.]),
                ("machineGunner", 1, [65., 64.]),
                ("short", 1, [65., 69.]),
                ("medium", 1, [76., 65.]),
            ] {
                game.spawn(kind, owner, [at[0] + 96., at[1] + 128.]);
                let unit = game.entities.last_mut().unwrap();
                if !unit.view.building {
                    unit.view.hp *= 6.;
                    unit.view.max_hp *= 6.;
                }
            }
            for (i, at) in [[52., 64.], [62., 70.], [54., 77.]].into_iter().enumerate() {
                game.props.push(crate::simulation::BattlefieldProp {
                    scar: None,
                    id: format!("training-{i}"),
                    kind: if i == 1 { "wall" } else { "sandbags" }.into(),
                    at: [at[0] + 96., at[1] + 128.],
                    heading: std::f32::consts::FRAC_PI_2,
                    wreck: None,
                    hp: 4.,
                });
            }
            let ids: Vec<_> = game
                .entities
                .iter()
                .filter(|e| e.view.owner == 0 && e.view.kind == "short")
                .map(|e| e.view.id.clone())
                .collect();
            game.command(serde_json::json!({"type":"command","commandType":"attack-move","unitIds":ids,"destination":{"x":150.,"y":192.}}));
            controls.selected = ids;
            controls.center = Vec3::new(155., 0., 195.);
            controls.zoom = 45.;
            controls.yaw = 0.18;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args
            .iter()
            .any(|s| s == "--actions" || s == "--towns" || s == "--vehicle-check")
        {
            let towns = args
                .iter()
                .any(|s| s == "--towns" || s == "--vehicle-check");
            *game = NativeGame::operation(
                "forward-base",
                if towns { 384 } else { 128 },
                if towns { 384 } else { 128 },
                game.config.clone(),
            );
            if towns {
                game.entities
                    .retain(|e| ["town", "objective"].contains(&e.view.kind.as_str()));
            } else {
                game.entities.clear();
            }
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            if towns {
                let requested = if args.iter().any(|s| s == "--town-h") {
                    Some(crate::settlement_layout::TownPlan::H)
                } else if args.iter().any(|s| s == "--town-l") {
                    Some(crate::settlement_layout::TownPlan::L)
                } else if args.iter().any(|s| s == "--town-e") {
                    Some(crate::settlement_layout::TownPlan::E)
                } else {
                    None
                };
                let town = game
                    .entities
                    .iter()
                    .filter(|e| {
                        e.view.kind == "town"
                            && requested.is_none_or(|plan| e.view.town_plan == plan)
                    })
                    .min_by(|a, b| (a.view.z - 192.).abs().total_cmp(&(b.view.z - 192.).abs()))
                    .unwrap();
                let (x, z) = (town.view.x, town.view.z);
                if args.iter().any(|s| s == "--vehicle-check") {
                    let spectator = game.spawn("short", 1, [350., 30.]);
                    game.command_as(1,serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[spectator]}));
                    let unit = game.spawn("medium", 0, [x - 53., z - 40.]);
                    smoke.vehicle_start = Some(Vec2::new(x - 53., z - 40.));
                    game.command(serde_json::json!({"type":"command","commandType":"move","unitIds":[unit],"destination":{"x":x+52.,"y":z+34.}}));
                    controls.selected = vec![unit];
                } else {
                    let plaza = town.view.town_plan.plaza();
                    game.spawn("short", 0, [x + plaza[0] + 5., z + plaza[1]]);
                    game.spawn("medium", 0, [x + plaza[0], z + plaza[1]]);
                }
                controls.center = Vec3::new(x, game.terrain.height(x, z), z + 18.);
                controls.zoom = 170.;
                controls.yaw = 0.25;
            } else {
                for (kind, owner, at) in [
                    ("barracks", 0, [31., 70.]),
                    ("short", 0, [44., 52.]),
                    ("short", 0, [41., 73.]),
                    ("medic", 0, [39., 72.]),
                    ("engineer", 0, [39., 82.]),
                    ("medium", 0, [40., 64.]),
                    ("long", 0, [39., 59.]),
                    ("xtralong", 0, [38., 68.]),
                    ("helicopter", 0, [36., 45.]),
                    ("machineGunner", 1, [63., 52.]),
                    ("medium", 1, [66., 64.]),
                    ("short", 1, [62., 70.]),
                ] {
                    game.spawn(kind, owner, at);
                }
                for e in &mut game.entities {
                    if e.view.owner == 1 {
                        e.view.hp = 100.;
                        e.view.max_hp = 100.;
                    }
                    if e.view.kind == "short" && e.view.owner == 0 && e.view.z > 70. {
                        e.view.downed = true;
                        e.view.hp = 0.01;
                        e.view.strength = 0;
                        e.downed_until = 400;
                    }
                    if e.view.kind == "helicopter" {
                        e.view.airborne = true;
                    }
                }
                for (kind, at) in [
                    ("short", [58., 52.]),
                    ("engineer", [56., 82.]),
                    ("helicopter", [46., 42.]),
                ] {
                    let ids: Vec<_> = game
                        .entities
                        .iter()
                        .filter(|e| e.view.kind == kind && e.view.owner == 0 && !e.view.downed)
                        .map(|e| e.view.id.clone())
                        .collect();
                    game.command(serde_json::json!({"type":"command","commandType":if kind=="short"{"attack-move"}else{"move"},"unitIds":ids,"destination":{"x":at[0],"y":at[1]}}));
                }
                game.props.push(crate::simulation::BattlefieldProp {
                    scar: None,
                    id: "action-cover".into(),
                    kind: "sandbags".into(),
                    at: [48., 52.],
                    heading: std::f32::consts::FRAC_PI_2,
                    wreck: None,
                    hp: 50.,
                });
                controls.selected = game
                    .entities
                    .iter()
                    .filter(|e| e.view.owner == 0 && e.view.kind == "medic")
                    .map(|e| e.view.id.clone())
                    .collect();
                controls.center = Vec3::new(52., 0., 65.);
                controls.zoom = 52.;
            }
            controls.yaw = 0.15;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--models") {
            let prototype = game
                .entities
                .iter()
                .find(|e| !e.view.building)
                .unwrap()
                .clone();
            let catalog: serde_json::Value =
                serde_json::from_str(include_str!("../catalog.json")).unwrap();
            game.entities.clear();
            game.bases.clear();
            game.config["fogEnabled"] = serde_json::json!(false);
            game.terrain = crate::terrain::Terrain::new("river-crossing", 128, 128);
            for (i, (kind, spec)) in catalog["units"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(_, s)| s["retired"] != true)
                .enumerate()
            {
                let mut entity = prototype.clone();
                entity.view.id = format!("showcase-{kind}");
                entity.view.kind = kind.clone();
                entity.view.category = spec["category"].as_str().unwrap_or("troop").into();
                entity.view.label = kind.clone();
                entity.view.owner = 0;
                entity.view.model_scale = spec["worldScale"].as_f64().unwrap_or(1.) as f32;
                entity.view.x = 30. + (i % 5) as f32 * 8.;
                entity.view.z = 48. + (i / 5) as f32 * 12.;
                entity.view.elevation = 0.;
                entity.view.airborne = spec["airUnit"] == true;
                game.entities.push(entity);
            }
            controls.center = Vec3::new(46., 0., 60.);
            controls.zoom = 48.;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--base" || s == "--airfield") {
            let plan = &game.bases[0];
            controls.center = Vec3::new(
                plan.origin[0] + 60. * plan.scale,
                0.,
                plan.origin[1] + 35. * plan.scale,
            );
            controls.zoom = 520. * plan.scale;
            controls.yaw = 0.1;
            if args.iter().any(|s| s == "--airfield") {
                let base = game
                    .entities
                    .iter()
                    .find(|e| e.view.kind == "airfield" && e.view.owner == 0)
                    .unwrap();
                controls.center = Vec3::new(
                    base.view.x,
                    game.terrain.height(base.view.x, base.view.z),
                    base.view.z,
                );
                controls.zoom = 410. * base.view.model_scale;
                controls.yaw = 0.25;
            }
            game.config["fogEnabled"] = serde_json::json!(false);
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--track-study") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            for (kind, at, to) in [
                ("medium", [180., 142.], [225., 153.]),
                ("long", [210., 154.], [248., 141.]),
                ("troopCarrier", [195., 188.], [220., 200.]),
            ] {
                let id = game.spawn(kind, 0, at);
                game.command(serde_json::json!({"type":"command","commandType":"move","unitIds":[id],"destination":{"x":to[0],"y":to[1]}}));
            }
            controls.selected.clear();
            controls.training_shop = false;
            controls.center = Vec3::new(215., game.terrain.height(215., 165.), 165.);
            controls.zoom = 85.;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--aa-check") {
            game.smoke_support_attack(args.iter().any(|s| s == "--missile-hit"));
            controls.center = Vec3::new(201., game.terrain.height(201., 168.) + 12., 168.);
            controls.zoom = 150.;
            controls.yaw = 0.1;
            controls.selected = vec![game.entities[0].view.id.clone()];
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--infantry-models") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(false);
            *game = NativeGame::operation(crate::training::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            for (i, kind) in ["short", "scout", "medic", "engineer", "commander", "short"]
                .iter()
                .enumerate()
            {
                let at = [160. + (i % 3) as f32 * 2.2, 168. + (i / 3) as f32 * 3.5];
                game.spawn(kind, if i == 5 { 1 } else { 0 }, at);
                let e = game.entities.last_mut().unwrap();
                e.view.heading = if i == 5 { -1.2 } else { 1.2 };
                let id = e.view.id.clone();
                game.smoke_infantry_pose(
                    &id,
                    kind == &"engineer" && args.iter().any(|s| s == "--infantry-launcher"),
                );
            }
            controls.center = Vec3::new(162.2, game.terrain.height(162.2, 170.7) + 0.65, 170.7);
            controls.zoom = 11.;
            controls.yaw = 0.35;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--explosions") {
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            game.winner = None;
            game.spawn("barracks", 1, [110., 110.]);
            game.spawn("town", -1, [68., 68.]);
            game.spawn("medium", 0, [48., 60.]);
            game.spawn("short", 0, [52., 53.]);
            if args.iter().any(|s| s == "--tank-impact") {
                game.spawn(
                    "medium",
                    if args.iter().any(|s| s == "--enemy-fire") {
                        0
                    } else {
                        1
                    },
                    [64., 65.],
                );
                for e in &mut game.entities {
                    if e.view.kind == "medium" && (e.view.x - 64.).abs() < 0.1 {
                        e.view.hp = 100.;
                        e.view.max_hp = 100.;
                    }
                }
                for owner in [0, 1] {
                    let ids: Vec<_> = game
                        .entities
                        .iter()
                        .filter(|e| e.view.owner == owner)
                        .map(|e| e.view.id.clone())
                        .collect();
                    game.command_as(owner,serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":ids}));
                }
            }
            controls.center = Vec3::new(64., 0., 65.);
            controls.zoom = 55.;
            if args.iter().any(|s| s == "--ejecta") {
                controls.center = Vec3::new(64., game.terrain.height(64., 65.) + 2., 65.);
                controls.zoom = 35.;
            }
            controls.yaw = 0.2;
            if args.iter().any(|s| s == "--building-blast") {
                let at = building_blast_at(&game);
                controls.center = Vec3::new(at[0], game.terrain.height(at[0], at[1]) + 3., at[1]);
                controls.zoom = 32.;
                controls.yaw = if args.iter().any(|s| s == "--blast-side") {
                    2.2
                } else {
                    0.2
                };
            }
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--destruction") {
            // A fresh simulation also clears old projectiles and production jobs.
            let config = crate::simulation::core_config(&game.config);
            *game = NativeGame::operation(crate::battlefield::ID, 384, 384, config);
            game.entities.clear();
            game.bases.clear();
            game.props.clear();
            game.human_opponent = true;
            game.config["fogEnabled"] = serde_json::json!(false);
            game.winner = None;
            game.spawn("barracks", 1, [110., 110.]);
            game.spawn("barracks", 0, [110., 125.]);
            game.spawn("medic", 0, [260., 260.]);
            for (i, (kind, at)) in [
                ("medium", [180., 185.]),
                ("troopCarrier", [192., 185.]),
                ("xtralong", [204., 185.]),
                ("antiAir", [216., 185.]),
                ("helicopter", [180., 204.]),
                ("vehicleDepot", [205., 208.]),
            ]
            .into_iter()
            .enumerate()
            {
                let id = game.spawn(kind, 1, at);
                let e = game.entities.iter_mut().find(|e| e.view.id == id).unwrap();
                e.view.id = format!("destruction-check-{i}");
                e.view.heading = std::f32::consts::FRAC_PI_2;
                e.view.aim = Some([at[0] + 8., at[1] + 8.]);
                let target_id = e.view.id.clone();
                game.command_as(1, serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[target_id]}));
            }
            controls.center = Vec3::new(198., 0., 195.);
            controls.zoom = 55.;
            controls.yaw = 0.15;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--production-check") {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(true);
            *game = NativeGame::operation(
                crate::training::ID,
                crate::training::SIZE,
                crate::training::SIZE,
                config,
            );
            game.entities
                .retain(|e| e.view.building && e.view.owner == 0);
            game.props.clear();
            for (kind, owner, at) in [
                ("short", 0, [182., 176.]),
                ("medic", 0, [188., 182.]),
                ("medium", 0, [198., 182.]),
                ("short", 1, [212., 176.]),
                ("engineer", 1, [218., 180.]),
                ("medium", 1, [214., 194.]),
            ] {
                let id = game.spawn(kind, owner, at);
                game.command_as(
                    owner,
                    serde_json::json!({"type":"command","commandType":"hold-fire","unitIds":[id]}),
                );
            }
            controls.training_tools = false;
            controls.training_shop = false;
            controls.center = Vec3::new(198., 0., 184.);
            controls.zoom = 84.;
            controls.yaw = 0.2;
            controls.selected = vec![
                game.entities
                    .iter()
                    .find(|e| e.view.kind == "vehicleDepot")
                    .unwrap()
                    .view
                    .id
                    .clone(),
            ];
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--visibility-check") {
            let training = args.iter().any(|s| s == "--training");
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = serde_json::json!(true);
            *game = NativeGame::operation(
                if training {
                    crate::training::ID
                } else {
                    crate::battlefield::ID
                },
                if training {
                    crate::training::SIZE
                } else {
                    crate::battlefield::COLS
                },
                if training {
                    crate::training::SIZE
                } else {
                    crate::battlefield::ROWS
                },
                config,
            );
            controls.training_tools = false;
            controls.training_shop = false;
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
        }
        if args.iter().any(|s| s == "--world") {
            front.world = true;
        }
        if args.iter().any(|s| s == "--editor") {
            front.editor = true;
            front.brush = 3;
        }
        if args.iter().any(|s| s == "--lobby") {
            front.lobby = true;
            front.address = "127.0.0.1:4174".into();
        }
        if args.iter().any(|s| s == "--settings") {
            front.settings = true;
            front.draft_config = Some(game.config.clone());
        }
        game.paused = !args.iter().any(|s| {
            s == "--blast-ground-check"
                || s == "--vfx-showcase"
                || s == "--visual-demo"
                || s == "--tactics"
                || s == "--actions"
                || s == "--vehicle-check"
                || (s == "--explosions" || s == "--destruction")
                || s == "--weapons"
                || s == "--training"
                || s == "--infantry-check"
                || s == "--garrison-check"
                || s == "--aircraft-check"
                || s == "--aircraft-parking"
                || s == "--aa-check"
                || s == "--carrier-check"
                || s == "--huey-check"
                || s == "--heli-combat"
                || s == "--rocket-trails"
                || s == "--joined-town"
                || s == "--countryside-map"
                || s == "--range-check"
                || s == "--town-entry"
                || s == "--runway-check"
                || s == "--battle-profile"
                || s == "--mock-battle"
                || s == "--organic-map"
        });
        if !args
            .iter()
            .any(|s| s == "--battle-profile" || s == "--mock-battle")
            && args.iter().any(|s| {
                s == "--aircraft-check"
                    || s == "--aircraft-parking"
                    || s == "--aa-check"
                    || s == "--carrier-check"
                    || s == "--heli-combat"
                    || s == "--rocket-trails"
                    || s == "--joined-town"
                    || s == "--countryside-map"
                    || s == "--range-check"
                    || s == "--town-entry"
                    || s == "--runway-check"
                    || s == "--battle-profile"
                    || s == "--mock-battle"
                    || s == "--organic-map"
            })
        {
            game.paused = true;
        }
        if args.iter().any(|s| {
            [
                "--carrier-check",
                "--heli-combat",
                "--rocket-trails",
                "--joined-town",
                "--countryside-map",
                "--range-check",
                "--runway-check",
                "--battle-profile",
                "--mock-battle",
                "--organic-map",
            ]
            .contains(&s.as_str())
        }) {
            controls.last_map = format!(
                "{}:{}:{}:{}",
                game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
            );
            controls.training_purchase =
                game.training.as_ref().and_then(|t| t.last_purchase.clone());
        }
        if !args.iter().any(|s| {
            s == "--tactics"
                || s == "--actions"
                || s == "--vehicle-check"
                || (s == "--explosions" || s == "--destruction")
                || s == "--weapons"
                || s == "--training"
                || s == "--infantry-check"
                || s == "--garrison-check"
                || s == "--aircraft-check"
                || s == "--aircraft-parking"
                || s == "--aa-check"
                || s == "--carrier-check"
                || s == "--heli-combat"
                || s == "--rocket-trails"
                || s == "--joined-town"
                || s == "--countryside-map"
                || s == "--range-check"
                || s == "--town-entry"
                || s == "--runway-check"
                || s == "--battle-profile"
                || s == "--mock-battle"
                || s == "--organic-map"
        }) {
            controls.selected = game
                .entities
                .iter()
                .filter(|e| e.view.owner == 0 && e.view.kind == "commander")
                .map(|e| e.view.id.clone())
                .collect();
        }
    }
    if smoke.frames > 30 && std::env::args().any(|s| s == "--aa-check") {
        controls.selected = game
            .entities
            .iter()
            .filter(|e| e.view.owner == 0 && e.view.kind == "engineer")
            .map(|e| e.view.id.clone())
            .collect();
    }
    if smoke.frames == 60 {
        crate::performance::reset();
        smoke.start_center = Some(controls.center);
        smoke.camera_started = time.elapsed_secs();
    }
    if std::env::args().any(|s| s == "--stop-check") {
        if smoke.frames == 80 {
            // World replacement at frame 30 can precede runtime.receive in the
            // same Update. Select after the authoritative snapshot has caught up.
            controls.selected = battle
                .snapshot
                .entities
                .iter()
                .filter(|p| {
                    p.owner == session.owner && !p.building && p.category != "aircraft" && !p.downed
                })
                .map(|p| p.id.clone())
                .collect();
            assert!(
                !controls.selected.is_empty(),
                "keyboard check needs live selected units"
            );
        }
        if [90, 92, 230, 232].contains(&smoke.frames) {
            let stop = smoke.frames < 200;
            keyboard.write(bevy::input::keyboard::KeyboardInput {
                key_code: if stop { KeyCode::KeyX } else { KeyCode::KeyE },
                logical_key: bevy::input::keyboard::Key::Character(
                    if stop { "x" } else { "e" }.into(),
                ),
                state: if smoke.frames == 90 || smoke.frames == 230 {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
                text: None,
                repeat: false,
                window: window_id,
            });
        }
        if smoke.frames == 100 {
            smoke.stop_tick = game.tick;
            for p in battle
                .snapshot
                .entities
                .iter()
                .filter(|p| controls.selected.contains(&p.id))
            {
                assert!(
                    !p.auto_attack && p.orders.is_empty() && p.aim.is_none(),
                    "X did not stop {}",
                    p.id
                );
                smoke
                    .stopped_positions
                    .insert(p.id.clone(), Vec2::new(p.x, p.z));
            }
            assert!(!smoke.stopped_positions.is_empty());
            smoke.stop_checked = true;
        }
        if (100..=220).contains(&smoke.frames) {
            for p in battle
                .snapshot
                .entities
                .iter()
                .filter(|p| smoke.stopped_positions.contains_key(&p.id))
            {
                assert!(
                    Vec2::new(p.x, p.z).distance(smoke.stopped_positions[&p.id]) < 0.01,
                    "{} moved after X",
                    p.id
                );
            }
            assert!(
                !battle
                    .snapshot
                    .events
                    .iter()
                    .any(|e| e.tick > smoke.stop_tick
                        && e.kind == "shot"
                        && smoke.stopped_positions.contains_key(&e.source)),
                "new shot after X"
            );
        }
        if smoke.frames == 245 {
            assert!(
                battle
                    .snapshot
                    .entities
                    .iter()
                    .filter(|p| controls.selected.contains(&p.id))
                    .all(|p| p.auto_attack),
                "E did not restore engagement"
            );
            smoke.resume_checked = true;
        }
    }
    if smoke.frames == 100 && std::env::args().any(|s| s == "--inspect-results") {
        commands.spawn((Interaction::Pressed, crate::debrief::ResultAction::Inspect));
    }
    if smoke.frames == 130 && std::env::args().any(|s| s == "--reopen-results") {
        commands.spawn((Interaction::Pressed, crate::debrief::ResultAction::Show));
    }
    if smoke.frames == 140 && std::env::args().any(|s| s == "--restart-results") {
        commands.spawn((Interaction::Pressed, crate::debrief::ResultAction::New));
    }
    if std::env::args().any(|s| s == "--production-check") {
        game.paused = true;
        if smoke.frames == 60 {
            commands.spawn((
                Interaction::Pressed,
                crate::ui::Action::Factory("vehicleDepot".into()),
                BackgroundColor(Color::BLACK),
            ));
        }
        if smoke.frames == 65 {
            for kind in ["medium", "medium", "long"] {
                commands.spawn((
                    Interaction::Pressed,
                    crate::ui::Action::Recruit(kind.into()),
                    BackgroundColor(Color::BLACK),
                ));
            }
        }
    }
    if smoke.frames >= 30 && std::env::args().any(|s| s == "--visibility-check") {
        game.paused = true;
        controls.center = Vec3::new(
            game.terrain.cols as f32 * 0.5,
            0.,
            game.terrain.rows as f32 * 0.56,
        );
        controls.zoom = game.terrain.cols as f32 * 1.05;
        controls.yaw = 0.;
    }
    if smoke.frames > 60 && ready.ready && std::env::args().any(|s| s == "--full-map-pan") {
        let count = ((game.terrain.cols + crate::scenery::CHUNK - 1) / crate::scenery::CHUNK)
            * ((game.terrain.rows + crate::scenery::CHUNK - 1) / crate::scenery::CHUNK);
        if smoke.overview_started.is_none() && render.scenery.iter().count() >= count as usize {
            smoke.overview_started = Some(time.elapsed_secs());
            smoke.overview_builds = ready.builds;
        }
        if let Some(start) = smoke.overview_started {
            let elapsed = time.elapsed_secs() - start;
            let directions = [KeyCode::KeyD, KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS];
            // Short presses across eight seconds exercise the real keyboard path,
            // crossing old terrain-detail boundaries without leaving the map.
            for key in directions {
                keys.release(key);
            }
            let phase = elapsed % 2.;
            if elapsed < 8. && phase < 0.4 {
                keys.press(directions[(elapsed / 2.).floor() as usize]);
            }
            window.focused = true;
            smoke.overview_peak_distance = smoke
                .overview_peak_distance
                .max(Vec2::new(controls.center.x - 512., controls.center.z - 512.).length());
            crate::performance::record(
                "frame.overview_pan",
                render.real_time.delta_secs_f64() * 1000.,
            );
        }
    }
    if smoke.frames > 30 && std::env::args().any(|s| s == "--training-buildings") {
        let [x, z] = crate::training::BUILDINGS;
        controls.center = Vec3::new(x, game.terrain.height(x, z), z);
        controls.zoom = 100.;
        controls.yaw = 0.25;
    }
    if smoke.frames > 30 && std::env::args().any(|s| s == "--garrison-check") {
        controls.center = Vec3::new(192., game.terrain.height(192., 192.) + 0.8, 192.);
        controls.zoom = 28.;
        controls.yaw = if std::env::args().any(|s| s == "--walls-back") {
            0.4 + std::f32::consts::PI
        } else {
            0.4
        };
    }
    if smoke.frames > 30 && std::env::args().any(|s| s == "--rotor-dust") {
        let startup = std::env::args().any(|s| s == "--rotor-startup");
        if let Some(id) = game.smoke_rotor_dust_frame(startup) {
            let p = &game.entities.iter().find(|e| e.view.id == id).unwrap().view;
            controls.center = Vec3::new(p.x, p.elevation + 1.5, p.z);
            controls.zoom = 48.;
            controls.selected = vec![id];
        }
    }
    if smoke.frames > 30 && std::env::args().any(|s| s == "--tank-model") {
        controls.center = Vec3::new(192., game.terrain.height(192., 192.) + 0.8, 192.);
        controls.zoom = 18.;
        controls.yaw = 0.5;
    }
    if smoke.frames > 30
        && std::env::args().any(|s| s == "--huey-check")
        && let Some(e) = game.entities.iter().find(|e| e.view.kind == "huey")
    {
        let p = &e.view;
        let lift = p.flight_height() * crate::fixed_view::YAW.sin() / crate::fixed_view::RISE;
        controls.center = Vec3::new(p.x - lift, p.elevation, p.z - lift);
        controls.zoom = if std::env::args().any(|s| s == "--huey-close") {
            22.
        } else {
            34.
        };
        controls.selected = vec![p.id.clone()];
    }
    if std::env::args().any(|s| s == "--towns")
        && smoke.frames >= 90
        && ready.ready
        && smoke.drag_stage < 2
    {
        let (camera, transform) = render.camera.single().unwrap();
        let flag = battle
            .snapshot
            .entities
            .iter()
            .filter(|p| p.kind == "objective")
            .min_by(|a, b| {
                (a.z - controls.center.z)
                    .abs()
                    .total_cmp(&(b.z - controls.center.z).abs())
            })
            .unwrap();
        if let Ok(at) = camera.world_to_viewport(transform, flag.pick_positions().next().unwrap()) {
            window.set_cursor_position(Some(at));
            window.focused = true;
            pointer.write(bevy::input::mouse::MouseButtonInput {
                button: MouseButton::Left,
                state: if smoke.drag_stage == 0 {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
                window: window_id,
            });
            smoke.drag_stage += 1;
        }
    }
    if std::env::args().any(|s| s == "--infantry-check") && smoke.frames >= 90 && ready.ready {
        let (camera, transform) = render.camera.single().unwrap();
        let at = if std::env::args().any(|s| s == "--attack-click") {
            battle
                .snapshot
                .entities
                .iter()
                .find(|p| p.training_target)
                .unwrap()
                .pick_positions()
                .next()
                .unwrap()
        } else {
            Vec3::new(165., game.terrain.height(165., 176.), 176.)
        };
        if let Ok(at) = camera.world_to_viewport(transform, at) {
            window.set_cursor_position(Some(at));
            window.focused = true;
        }
        if std::env::args().any(|s| s == "--attack-click") && smoke.drag_stage == 0 {
            smoke.drag_stage = 1;
            smoke.drag_started = time.elapsed_secs();
            pointer.write(bevy::input::mouse::MouseButtonInput {
                button: MouseButton::Left,
                state: bevy::input::ButtonState::Pressed,
                window: window_id,
            });
        } else if std::env::args().any(|s| s == "--attack-click")
            && smoke.drag_stage == 1
            && time.elapsed_secs() - smoke.drag_started >= 0.05
        {
            smoke.drag_stage = 2;
            pointer.write(bevy::input::mouse::MouseButtonInput {
                button: MouseButton::Left,
                state: bevy::input::ButtonState::Released,
                window: window_id,
            });
        }
    }
    if std::env::args().any(|s| {
        s == "--drag-check"
            || s == "--click-check"
            || s == "--middle-check"
            || s == "--alt-drag-check"
    }) && ready.ready
    {
        let click = std::env::args().any(|s| s == "--click-check");
        let alt_drag = std::env::args().any(|s| s == "--alt-drag-check");
        let button = if std::env::args().any(|s| s == "--middle-check") {
            MouseButton::Middle
        } else if alt_drag {
            MouseButton::Left
        } else {
            MouseButton::Right
        };
        let start = Vec2::new(window.width() * 0.45, window.height() * 0.40);
        if smoke.frames == 65 {
            window.set_cursor_position(Some(start));
            window.focused = true;
            smoke.drag_start = controls.center;
            smoke.drag_started = time.elapsed_secs();
            smoke.drag_stage = 1;
            if alt_drag {
                keys.press(KeyCode::AltLeft);
            }
            pointer.write(bevy::input::mouse::MouseButtonInput {
                button,
                state: bevy::input::ButtonState::Pressed,
                window: window_id,
            });
        } else if smoke.drag_stage == 1 {
            let t = ((time.elapsed_secs() - smoke.drag_started) / 0.75).clamp(0., 1.);
            window.set_cursor_position(Some(
                start
                    + if click {
                        Vec2::new(2., 1.)
                    } else {
                        Vec2::new(170. * t, 90. * t)
                    },
            ));
            window.focused = true;
            if t >= 1. {
                smoke.drag_stage = 2;
                if alt_drag {
                    keys.release(KeyCode::AltLeft);
                }
                pointer.write(bevy::input::mouse::MouseButtonInput {
                    button,
                    state: bevy::input::ButtonState::Released,
                    window: window_id,
                });
            }
        }
        if smoke.drag_stage > 0 {
            smoke.drag_distance = smoke
                .drag_distance
                .max(controls.center.distance(smoke.drag_start));
        }
    }
    if smoke.frames > 60 {
        smoke.peak_audio_voices = smoke
            .peak_audio_voices
            .max(render.audio_voices.iter().count());
        if std::env::args().any(|s| s == "--audio-stop-check")
            && time.elapsed_secs() - smoke.scenario_started >= 4.
        {
            game.paused = true;
        }
        for billow in &render.billow_styles {
            smoke
                .observed_baked_styles
                .insert((billow.family(), billow.style));
        }
        crate::performance::record("frame", render.real_time.delta_secs_f64() * 1000.);
        if crate::simulation::mock_battle_enabled() {
            if game.tick / 400 > smoke.mock_wave {
                smoke.mock_wave = game.tick / 400;
                game.battle_bench_wave(&mock_case(), false);
            }
            smoke.mock_stats.observe(&game);
        }
        if std::env::args().any(|s| s == "--vehicle-check") {
            if let (Some(start), Some(unit)) = (
                smoke.vehicle_start,
                game.entities.iter().find(|e| e.view.kind == "medium"),
            ) {
                smoke.vehicle_distance = smoke
                    .vehicle_distance
                    .max(start.distance(Vec2::new(unit.view.x, unit.view.z)));
            }
            smoke.clearance_violations = smoke
                .clearance_violations
                .max(game.vehicle_clearance_violations());
        }
        smoke.max_frame_seconds = smoke.max_frame_seconds.max(render.real_time.delta_secs());
        smoke.peak_flares = smoke.peak_flares.max(render.flares.iter().count());
        smoke.cover_holders = smoke.cover_holders.max(
            battle
                .snapshot
                .entities
                .iter()
                .filter(|p| p.activity == "in-cover")
                .count(),
        );
        smoke.peak_projectiles = smoke.peak_projectiles.max(render.flights.iter().count());
        smoke.peak_effects = smoke.peak_effects.max(
            render.particles.iter().count()
                + render.volume_particles.iter().count()
                + render.baked_billows.iter().count(),
        );
        for p in &battle.snapshot.entities {
            smoke.activities.insert(p.activity.clone());
            if p.garrison
                .as_ref()
                .is_some_and(|g| !g.entered && g.door_stage == 1)
            {
                smoke.door_crossers.insert(p.id.clone());
            }
        }
        for (entity, t) in &limbs {
            if let Some(start) = smoke.limb_start.get(&entity) {
                smoke.max_limb_motion = smoke.max_limb_motion.max(t.translation.distance(*start));
            } else {
                smoke.limb_start.insert(entity, t.translation);
            }
        }
        for (entity, t) in &render.bones {
            if let Some(start) = smoke.bone_start.get(&entity) {
                smoke.max_bone_rotation = smoke
                    .max_bone_rotation
                    .max(t.rotation.angle_between(*start));
            } else {
                smoke.bone_start.insert(entity, t.rotation);
            }
        }
    }
    if std::env::args().any(|s| s == "--scenery-check") && smoke.frames > 30 {
        let elapsed = time.elapsed_secs() - smoke.scenario_started;
        let (x, z, zoom) = if elapsed < 0.8 {
            (210., 220., 120.)
        } else if elapsed < 1.6 {
            (340., 220., 120.)
        } else if elapsed < 2.4 {
            (340., 360., 280.)
        } else if elapsed < 3.2 {
            (210., 220., 200.)
        } else {
            (210., 220., 120.)
        };
        controls.center.x = x;
        controls.center.z = z;
        controls.zoom = zoom;
        if elapsed > 0.5 {
            let mut cells = std::collections::HashSet::new();
            let mut coords = std::collections::HashSet::new();
            let mut duplicates = 0;
            for chunk in &render.scenery {
                if !coords.insert(chunk.coord) {
                    duplicates += 1;
                }
                for p in &chunk.placements {
                    if !cells.insert(p.cell) {
                        duplicates += 1;
                    }
                    if let Some(previous) = smoke.scenery_positions.insert(p.cell, p.position)
                        && previous.distance(p.position) > 0.001
                    {
                        smoke.scenery_shifted += 1;
                    }
                }
            }
            smoke.scenery_duplicates = smoke.scenery_duplicates.max(duplicates);
            smoke.scenery_peak_props = smoke.scenery_peak_props.max(cells.len());
            smoke.scenery_peak_chunks = smoke.scenery_peak_chunks.max(coords.len());
            let view = crate::scenery::view_bounds(&controls, render.camera.single().ok());
            let b = crate::scenery::chunk_bounds(&game.terrain, view, crate::scenery::CHUNK as f32);
            smoke.scenery_coverage =
                (b[2]..=b[3]).all(|z| (b[0]..=b[1]).all(|x| coords.contains(&IVec2::new(x, z))));
        }
    }
    if std::env::args().any(|s| s == "--camera-check")
        && ready.ready
        && smoke.frames >= 60
        && time.elapsed_secs() - smoke.camera_started < 0.75
    {
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
    }
    if smoke.frames > 60 && time.elapsed_secs() - smoke.camera_started >= 0.75 {
        keys.release(KeyCode::KeyW);
        keys.release(KeyCode::KeyD);
    }
    if std::env::args().any(|s| s == "--explosions" || s == "--destruction")
        && !smoke.blasted
        && smoke.frames > 60
        && time.elapsed_secs() - smoke.scenario_started
            >= if smoke.blast_warmed { 3.6 } else { 1.2 }
    {
        if std::env::args().any(|s| s == "--destruction") {
            if smoke.blast_warmed {
                game.smoke_destroy_models();
            } else {
                game.smoke_blast([198., 195.]);
            }
        } else if std::env::args().any(|s| s == "--tank-impact") {
            game.smoke_tank_hit(
                [64., 65.],
                smoke.blast_warmed && std::env::args().any(|s| s == "--tank-kill"),
                if std::env::args().any(|s| s == "--enemy-fire") {
                    1
                } else {
                    0
                },
            );
        } else if std::env::args().any(|s| s == "--building-blast") {
            let at = building_blast_at(&game);
            game.smoke_blast(at);
            // Keep the roof intact to isolate effect/geometry depth in this render check.
            for e in &mut game.entities {
                for lot in &mut e.view.lots {
                    lot.integrity = 1.;
                }
            }
        } else if std::env::args().any(|s| s == "--ejecta") {
            // A clear lane keeps the short ground burst visible instead of
            // hiding it inside the enlarged town's first house/off camera.
            game.smoke_blast([64., 65.]);
        } else {
            let at = game
                .entities
                .iter()
                .find(|e| e.view.kind == "town")
                .map(|e| {
                    [
                        e.view.x + e.view.lots[0].at[0],
                        e.view.z + e.view.lots[0].at[1],
                    ]
                })
                .unwrap();
            game.smoke_blast(at);
        }
        if smoke.blast_warmed {
            smoke.blasted = true;
            smoke.blast_started = time.elapsed_secs();
        } else {
            smoke.blast_warmed = true;
        }
    }
    if std::env::args().any(|s| {
        s == "--cannon-contacts"
            || s == "--aa-check"
            || s == "--heli-combat"
            || s == "--rocket-trails"
    }) && ready.ready
        && smoke.frames > 60
        && !smoke.blast_warmed
        && time.elapsed_secs() - smoke.scenario_started > 3.6
    {
        game.paused = false;
        smoke.blast_warmed = true;
        smoke.blast_started = time.elapsed_secs();
    }
    if ready.ready
        && smoke.frames > 60
        && !smoke.blasted
        && std::env::args().any(|s| s == "--blast-ground-check")
    {
        let args: Vec<_> = std::env::args().collect();
        let number = |name: &str, default: f32| {
            args.iter()
                .position(|s| s == name)
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(default)
        };
        render.blasts.write(crate::volume_effects::Blast {
            at: controls.center + Vec3::Y * 0.15,
            radius: 3.5,
            variant: match args
                .iter()
                .position(|s| s == "--blast-family")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
            {
                Some("shell") => crate::volume_effects::Variant::Shell,
                Some("rocket") => crate::volume_effects::Variant::Rocket,
                Some("vehicle") => crate::volume_effects::Variant::Mushroom,
                _ => crate::volume_effects::Variant::Debris,
            },
            seed: 74129,
            visual_scale: number("--blast-scale", 1.),
        });
        smoke.blasted = true;
        smoke.blast_started = time.elapsed_secs();
    }
    let graphics_cycle = std::env::args().any(|s| s == "--graphics-cycle");
    if graphics_cycle {
        use crate::graphics::Action;
        let action = match smoke.frames {
            120 => Some(Action::Upscaler),
            180 => Some(Action::Textures),
            240 => Some(Action::Preset),
            300 | 360 | 420 => Some(Action::Upscaler),
            480 => Some(Action::Preset),
            _ => None,
        };
        if let Some(action) = action {
            commands.spawn((action, Interaction::Pressed, BackgroundColor::default()));
        }
    }
    let capture_due = if let Some(study) = &render.stress {
        study.complete && ready.ready
    } else if crate::gis_prototype::enabled() {
        ready.ready
            && !loading.busy()
            && smoke.frames >= 240
            && game.terrain.id == crate::gis_prototype::ID
            && time.elapsed_secs() - smoke.scenario_started >= 10.
    } else if std::env::args().any(|s| s == "--hide-ui") {
        smoke
            .hud_check_start
            .is_some_and(|start| smoke.frames >= start + 110)
            && ready.ready
            && (!std::env::args().any(|s| s == "--countryside-map")
                || time.elapsed_secs() - smoke.scenario_started > 15.)
    } else if std::env::args().any(|s| s == "--joined-town") {
        ready.ready && smoke.frames >= 180 && time.elapsed_secs() - smoke.scenario_started >= 3.
    } else if graphics_cycle {
        ready.ready && smoke.frames >= 540
    } else if std::env::args().any(|s| s == "--blast-ground-check") {
        let args: Vec<_> = std::env::args().collect();
        let age = args
            .iter()
            .position(|s| s == "--blast-age")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.1);
        smoke.blasted && render.billow_styles.iter().any(|b| b.age >= age)
    } else if crate::visual_demo::enabled() {
        ready.ready && crate::visual_demo::capture_due(&render.visual_demo)
    } else if std::env::args().any(|s| s == "--restart-results") {
        smoke.frames >= 180 && ready.ready && !loading.busy() && game.winner.is_none()
    } else if let Some(showcase) = &render.vfx_showcase {
        let args: Vec<_> = std::env::args().collect();
        let age = args
            .iter()
            .position(|a| a == "--vfx-age")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.1);
        showcase.initialized && showcase.elapsed >= age
    } else if crate::simulation::mock_battle_enabled() {
        ready.ready && time.elapsed_secs() - smoke.scenario_started >= mock_duration() - 2.
    } else if std::env::args().any(|s| s == "--infantry-models") {
        ready.ready
            && render.infantry.iter().count() == 6
            && smoke.frames >= 180
            && time.elapsed_secs() - smoke.scenario_started >= 3.
    } else if std::env::args().any(|s| s == "--full-map-pan") {
        smoke
            .overview_started
            .is_some_and(|start| time.elapsed_secs() - start >= 9.)
    } else if std::env::args().any(|s| s == "--visibility-check") {
        ready.ready && smoke.frames >= 180 && time.elapsed_secs() - smoke.scenario_started >= 3.
    } else if std::env::args().any(|s| {
        s == "--cannon-contacts"
            || s == "--aa-check"
            || s == "--heli-combat"
            || s == "--rocket-trails"
    }) {
        smoke.blast_warmed
            && time.elapsed_secs() - smoke.blast_started
                >= if std::env::args().any(|s| s == "--missile-hit") {
                    1.25
                } else if std::env::args().any(|s| s == "--aa-check") {
                    0.6
                } else if std::env::args().any(|s| s == "--rocket-trails") {
                    1.0
                } else if std::env::args().any(|s| s == "--heli-combat") {
                    if std::env::args().any(|s| s == "--heli-evade") {
                        2.0
                    } else {
                        0.35
                    }
                } else {
                    0.25
                }
    } else if std::env::args().any(|s| s == "--explosions" || s == "--destruction") {
        smoke.blasted
            && time.elapsed_secs() - smoke.blast_started
                >= if std::env::args().any(|s| s == "--tank-impact") {
                    if std::env::args().any(|s| s == "--blast-early") {
                        0.08
                    } else {
                        0.3
                    }
                } else if std::env::args().any(|s| s == "--ejecta") {
                    0.75
                } else if std::env::args().any(|s| s == "--blast-early") {
                    0.4
                } else if std::env::args().any(|s| s == "--mushroom") {
                    3.
                } else if std::env::args().any(|s| s == "--aftermath") {
                    6.
                } else if std::env::args().any(|s| s == "--destruction") {
                    1.7
                } else {
                    0.12
                }
    } else if std::env::args().any(|s| {
        s == "--training"
            || s == "--garrison-check"
            || s == "--vehicle-check"
            || s == "--battle-profile"
            || s == "--mock-battle"
    }) {
        smoke.frames >= 180 && time.elapsed_secs() - smoke.scenario_started >= 8.
    } else {
        smoke.frames >= 180 && time.elapsed_secs() - smoke.scenario_started >= 3.
    };
    if !smoke.captured && capture_due {
        smoke.captured = true;
        smoke.captured_at = time.elapsed_secs();
        if std::env::args().any(|s| s == "--visibility-check") {
            smoke.blast_started = time.elapsed_secs();
        }
        if crate::visual_demo::enabled()
            || std::env::args().any(|s| {
                s == "--cannon-contacts"
                    || s == "--aa-check"
                    || s == "--heli-combat"
                    || s == "--rocket-trails"
            })
        {
            game.paused = true;
        }
        smoke.captured_effects = render.particles.iter().filter(|v| v.get()).count()
            + render.volume_particles.iter().filter(|v| v.get()).count()
            + render.baked_billows.iter().filter(|v| v.get()).count();
        commands
            .spawn(
                render
                    .capture
                    .as_ref()
                    .map_or_else(Screenshot::primary_window, |target| {
                        Screenshot::image(target.0.clone())
                    }),
            )
            .observe(save_to_disk(smoke.path.clone()));
    }
    if ready.ready && smoke.frames > 75 && std::env::args().any(|s| s == "--auto-attack-check") {
        controls.selected = battle
            .snapshot
            .entities
            .iter()
            .filter(|p| p.owner == session.owner && p.kind == "short" && p.hp > 0.)
            .map(|p| p.id.clone())
            .collect();
    }
    if ready.ready
        && smoke.frames > 75
        && std::env::args().any(|s| s == "--strafe-reticle")
        && let Ok((camera, transform)) = render.camera.single()
        && let Ok(at) = camera.world_to_viewport(
            transform,
            Vec3::new(190., game.terrain.height(190., 210.), 210.),
        )
    {
        window.set_cursor_position(Some(at));
    }
    if smoke.frames == 90
        && !crate::visual_demo::enabled()
        && !std::env::args().any(|s| {
            s == "--blast-ground-check"
                || s == "--tactics"
                || s == "--actions"
                || s == "--vehicle-check"
                || (s == "--explosions" || s == "--destruction")
                || s == "--weapons"
                || s == "--training"
                || s == "--infantry-check"
                || s == "--garrison-check"
                || s == "--aircraft-check"
                || s == "--aircraft-parking"
                || s == "--aa-check"
                || s == "--carrier-check"
                || s == "--heli-combat"
                || s == "--rocket-trails"
                || s == "--joined-town"
                || s == "--countryside-map"
                || s == "--range-check"
                || s == "--town-entry"
                || s == "--runway-check"
                || s == "--battle-profile"
                || s == "--mock-battle"
                || s == "--organic-map"
                || s == "--vfx-showcase"
        })
    {
        game.paused = true;
        controls.selected = battle
            .snapshot
            .entities
            .iter()
            .filter(|e| {
                e.owner == session.owner
                    && e.kind
                        == if std::env::args().any(|s| s == "--infantry-menu") {
                            "barracks"
                        } else if std::env::args().any(|s| s == "--airfield") {
                            "airfield"
                        } else {
                            "commander"
                        }
            })
            .map(|e| e.id.clone())
            .collect();
    }
    // Keep the automated host alive while the guest starts its renderer and captures.
    let exit_frame = if session.multiplayer() && session.owner == 0 {
        900
    } else {
        300
    };
    let minimum_seconds = if crate::simulation::mock_battle_enabled() {
        mock_duration()
    } else if std::env::args().any(|s| s == "--destruction") {
        if std::env::args().any(|s| s == "--aftermath") {
            11.
        } else {
            7.
        }
    } else if std::env::args().any(|s| {
        s == "--training"
            || s == "--garrison-check"
            || s == "--vehicle-check"
            || s == "--battle-profile"
            || s == "--mock-battle"
    }) {
        10.
    } else if session.multiplayer() && session.owner == 0 {
        12.
    } else {
        5.
    };
    if smoke.captured
        && time.elapsed_secs() - smoke.captured_at > 1.
        && std::path::Path::new(&smoke.path).exists()
        && smoke.frames >= exit_frame
        && time.elapsed_secs() - smoke.scenario_started >= minimum_seconds
        && (!std::env::args().any(|s| s == "--visibility-check")
            || time.elapsed_secs() - smoke.blast_started > 1.)
    {
        let mut report = serde_json::json!({"cursorMode":format!("{:?}",render.cursor.mode),"selectedFriendlyUnits":battle.snapshot.entities.iter().filter(|p|p.owner==session.owner && controls.selected.contains(&p.id)).count(),"trainingShots":game.training.as_ref().map(|t|t.shots),"trainingHits":game.training.as_ref().map(|t|t.hits),"trainingDamage":game.training.as_ref().map(|t|t.damage),"trainingToolsVisible":controls.training_tools,"peakWeaponFlares":smoke.peak_flares,"peakCoverHolders":smoke.cover_holders,"fogEnabled":battle.snapshot.fog_enabled,"sightSources":battle.snapshot.sights.len(),"capturedVisibleEffects":smoke.captured_effects,"dragCameraDistance":smoke.drag_distance,"rightDragConsumed":controls.right_dragged,"queuedOrdersAfterDrag":battle.snapshot.entities.iter().filter(|e|e.owner==session.owner).map(|e|e.orders.len()).sum::<usize>(),"peakEffects":smoke.peak_effects,"ruinedHouses":game.entities.iter().flat_map(|e| &e.view.lots).filter(|l|l.integrity<=0.).count(),"basePlans":game.bases.len(),"sceneryCoverageReady":smoke.scenery_coverage,"sceneryShiftedCells":smoke.scenery_shifted,"sceneryDuplicateCells":smoke.scenery_duplicates,"sceneryPeakProps":smoke.scenery_peak_props,"sceneryPeakChunks":smoke.scenery_peak_chunks,"peakVisibleProjectiles":smoke.peak_projectiles,"maxLimbMotion":smoke.max_limb_motion,"observedActions":smoke.activities,"freestandingBuildings":battle.snapshot.entities.iter().map(|p|p.lots.len()).sum::<usize>(),"owner":session.owner,"session":session.status,"ownedSquads":battle.snapshot.entities.iter().filter(|p|p.owner==session.owner && !p.building).count(),"terrainReady":ready.ready,"loadingBusy":loading.busy(),"terrainError":ready.error,"map":game.terrain.id,"cols":game.terrain.cols,"cameraDistance":smoke.start_center.map(|p| p.distance(controls.center)).unwrap_or(0.),"maxFrameMs":smoke.max_frame_seconds*1000.,"simulationTick":battle.snapshot.tick,"coverObjects":battle.snapshot.props.len(),"suppressedSquads":battle.snapshot.entities.iter().filter(|p|p.suppression>0.45).count(),"woundedSquads":battle.snapshot.entities.iter().filter(|p|p.downed).count()});
        if crate::simulation::mock_battle_enabled() {
            report["mockBattle"] = smoke.mock_stats.report(&game);
        }
        if let Some(d) = &render.diagnostics {
            report["gpuPassesMs"] = serde_json::json!(
                d.iter()
                    .filter(|v| v.path().to_string().contains("elapsed_gpu"))
                    .filter_map(|v| v.smoothed().map(|ms| (v.path().to_string(), ms)))
                    .collect::<std::collections::BTreeMap<_, _>>()
            );
        }
        report["winner"] = battle.snapshot.winner.clone();
        if let Some(study) = &render.stress {
            report["cameraStress"] = study.report();
        }
        if let Some(stats) = &render.scenery_stats {
            report["sceneryStreaming"] = stats.report();
        }
        report["renderPipeline"] = crate::render_profile::report();
        report["terrainMeshBuilds"] = serde_json::json!(ready.builds);
        report["terrainVertices"] = serde_json::json!(ready.vertices);
        report["terrainStreaming"] = serde_json::json!({"uploadedBytes":ready.upload_bytes,"peakUploadBytes":ready.peak_upload_bytes,
            "cachedDetailTiles":ready.cached_detail_tiles,"staleTiles":ready.stale_tiles,"pendingTiles":ready.pending_tiles});
        if std::env::args().any(|s| s == "--full-map-pan") {
            assert!(smoke.overview_started.is_some());
            assert!(smoke.overview_peak_distance > 40.);
            assert_eq!(
                ready.builds, smoke.overview_builds,
                "overview pan must not rebuild terrain"
            );
            report["overviewPanPeakDistance"] = serde_json::json!(smoke.overview_peak_distance);
            report["overviewPanTerrainRebuilds"] =
                serde_json::json!(ready.builds - smoke.overview_builds);
        }
        report["resultsVisible"] = serde_json::json!(
            render
                .result_panel
                .iter()
                .any(|n| n.display != Display::None)
        );
        report["resultsOpen"] = serde_json::json!(render.result_state.open);
        report["stopKeyPassed"] = serde_json::json!(smoke.stop_checked);
        report["resumeKeyPassed"] = serde_json::json!(smoke.resume_checked);
        if std::env::args().any(|s| s == "--stop-check") {
            assert!(smoke.stop_checked && smoke.resume_checked);
        }
        report["outcomeReason"] = battle.snapshot.outcome_reason.clone().into();
        report["infantryRanges"] = serde_json::json!(game.infantry_range_report());
        report["carriers"] = serde_json::json!(
            battle
                .snapshot
                .entities
                .iter()
                .filter(|p| !p.building && p.capacity > 0)
                .map(|p| serde_json::json!({"kind":p.kind,"passengers":p.occupants,"capacity":p.capacity}))
                .collect::<Vec<_>>()
        );
        report["rocketTrails"] =
            serde_json::json!(render.trails.iter().filter(|p| p.smoke).count());
        report["aircraft"] = serde_json::json!(battle.snapshot.entities.iter().filter(|p|p.category=="aircraft").map(|p|serde_json::json!({"kind":p.kind,"activity":p.activity,"altitude":p.flight_height(),"fuel":p.fuel_fraction,"pitch":p.flight_pitch,"bank":p.flight_bank,"speed":bevy::math::Vec2::from_array(p.velocity).length(),"ammunition":p.cannon_ammo,"rotorPower":p.rotor_power,"targetLocked":p.strafe_locked,"strafeTarget":p.strafe_target})).collect::<Vec<_>>());
        report["vehicleDistanceTravelled"] = serde_json::json!(smoke.vehicle_distance);
        if std::env::args().any(|s| s == "--missile-hit") {
            assert!(
                !battle
                    .snapshot
                    .entities
                    .iter()
                    .any(|p| p.kind == "plane" && p.hp > 0.)
            );
            assert!(game.props.iter().any(|p| {
                p.wreck
                    .as_ref()
                    .is_some_and(|w| w.piece.kind == "plane" && w.crash.is_some())
            }));
        }
        report["skinnedInfantry"] = serde_json::json!(render.infantry.iter().count());
        report["unitSpecificBadges"] = serde_json::json!(render.unit_icons.iter().count());
        report["hudHidden"] = serde_json::json!(controls.hud_hidden);
        report["vehicleDetailMaps"] = serde_json::json!(render.scene_art.vehicle_detail_maps);
        report["vehicleAtlasModels"] = serde_json::json!(render.scene_art.vehicle_atlases.count());
        report["vehicleAtlasBatches"] = serde_json::json!(crate::vehicle_atlas::matches());
        report["visibleTacticalUiRoots"] = serde_json::json!(
            render
                .tactical_ui
                .iter()
                .filter(|n| n.display != Display::None)
                .count()
        );
        report["mainMenuVisible"] =
            serde_json::json!(render.main_menu.iter().any(|n| n.display != Display::None));
        report["visibleSkinnedInfantry"] =
            serde_json::json!(render.infantry.iter().filter(|v| v.get()).count());
        report["maxSkeletalRotation"] = serde_json::json!(smoke.max_bone_rotation);
        if std::env::args().any(|s| s == "--infantry-models") {
            assert_eq!(
                render.infantry.iter().count(),
                6,
                "Infantry scenes failed to bind"
            );
            let mut budgets = serde_json::Map::new();
            for (root, actor) in &render.actors {
                let Some(p) = battle
                    .snapshot
                    .entities
                    .iter()
                    .find(|p| p.id == actor.0 && p.category == "troop")
                else {
                    continue;
                };
                let triangles: usize = render
                    .hierarchy
                    .iter_descendants(root)
                    .filter_map(|e| render.mesh_nodes.get(e).ok())
                    .filter_map(|m| render.meshes.get(&m.0))
                    .map(|m| m.indices().map_or(m.count_vertices(), |i| i.len()) / 3)
                    .sum();
                assert!(
                    triangles > 0 && triangles <= 15000,
                    "{} has {triangles} triangles",
                    p.kind
                );
                budgets.insert(p.label.clone(), serde_json::json!(triangles));
            }
            report["infantryTriangleBudgets"] = serde_json::Value::Object(budgets);
        }
        if std::env::args().any(|s| s == "--infantry-check") {
            assert!(
                render.infantry.iter().count()
                    >= if std::env::args().any(|s| s == "--auto-attack-check") {
                        2
                    } else {
                        4
                    },
                "Infantry GLB missing from live action scenario"
            );
            assert!(
                smoke.max_bone_rotation > 0.02,
                "Skeletal animations did not advance"
            );
        }
        report["friendlyPlannedWaypoints"] = serde_json::json!(
            battle
                .snapshot
                .entities
                .iter()
                .filter(|p| p.owner == session.owner)
                .map(|p| p.waypoints.len())
                .sum::<usize>()
        );
        report["visibleEnemyImpactEvents"] = serde_json::json!(
            battle
                .snapshot
                .events
                .iter()
                .filter(|e| e.owner != session.owner && e.kind == "impact")
                .count()
        );
        report["vehicleClearanceViolations"] = serde_json::json!(smoke.clearance_violations);
        report["timings"] = crate::performance::report();
        report["strafePreviewMarkers"] = serde_json::json!(render.strafe.previews);
        report["strafeMissionMarkers"] = serde_json::json!(render.strafe.missions);
        report["selectedAutoAttack"] = serde_json::json!(
            battle
                .snapshot
                .entities
                .iter()
                .filter(|p| controls.selected.contains(&p.id))
                .map(|p| p.auto_attack)
                .collect::<Vec<_>>()
        );
        report["cameraZoom"] = serde_json::json!(controls.zoom);
        if std::env::args().any(|s| s == "--production-check") {
            let cards: Vec<_> = render
                .actions
                .iter()
                .filter_map(|action| {
                    if let crate::ui::Action::Recruit(kind) = action {
                        Some(kind.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(cards.len(), 5);
            assert!(!cards.contains(&"short") && !cards.contains(&"plane"));
            assert_eq!(battle.snapshot.jobs.len(), 3);
            assert!(
                battle
                    .snapshot
                    .entities
                    .iter()
                    .any(|p| p.owner == 1 && !p.building),
                "visible opponents required for marker review"
            );
            report["productionCards"] = serde_json::json!(cards);
            report["productionQueue"] = serde_json::json!(battle.snapshot.jobs);
        }
        if std::env::args().any(|s| s == "--visibility-check") {
            assert!(
                render
                    .distance_fog
                    .iter()
                    .all(|f| f.color.alpha() <= 0.08001),
                "clear-air haze must never hide the zoomed-out map"
            );
            assert_eq!(game.terrain.cols, 1024);
            assert!(battle.snapshot.fog_enabled);
            let rifle = battle
                .snapshot
                .entities
                .iter()
                .find(|p| p.owner == session.owner && p.kind == "short")
                .unwrap();
            assert_eq!(rifle.sight, 64.);
            report["distanceFogCameras"] = serde_json::json!(render.distance_fog.iter().count());
            report["rifleSight"] = serde_json::json!(rifle.sight);
            let Projection::Perspective(projection) = render.projection.single().unwrap() else {
                panic!("RTS perspective camera missing")
            };
            assert!(projection.far > controls.zoom + game.terrain.rows as f32);
            report["cameraFar"] = serde_json::json!(projection.far);
        }
        report["vfxShowcaseEffect"] =
            serde_json::json!(render.vfx_showcase.as_ref().map(|s| s.label()));
        report["vfxQuality"] = serde_json::json!(render.vfx_quality.label());
        if crate::visual_demo::enabled() {
            report["visualDemo"] = serde_json::json!({"effect":render.visual_demo.effect,"strikes":render.visual_demo.strikes,
                "age":render.visual_demo.age,"styled":render.visual_demo.styled,"orthographic":render.visual_demo.orthographic,
                  "controls":render.visual_demo.controls,"compareScales":render.visual_demo.compare_scales,
                  "scaleChoice":render.visual_demo.scale_choice});
        }
        report["baked3dParticles"] = serde_json::json!(render.volume_particles.iter().count());
        report["concreteGroundBatches"] = serde_json::json!(render.concrete_nodes.iter().count());
        report["foundationGroundBatches"] = serde_json::json!(
            render
                .concrete_nodes
                .iter()
                .filter(|h| render.concrete_materials.get(&h.0).is_some_and(|m| m
                    .extension
                    .tuning
                    .y
                    > 0.5))
                .count()
        );
        report["concreteTexturePixels"] = serde_json::json!(
            render
                .concrete_materials
                .iter()
                .next()
                .and_then(|(_, m)| m.base.base_color_texture.as_ref())
                .and_then(|h| render.terrain_images.get(h))
                .map(|i| i.width())
        );
        report["photographicGroundDemo"] = serde_json::json!(crate::ground_demo::enabled());
        report["legacyTerrainBlendNoise"] =
            serde_json::json!(std::env::args().any(|a| a == "--legacy-terrain-noise"));
        if let Some(material) = render.grass_materials.get(&render.nature.grass) {
            report["groundTextureLayers"] = serde_json::json!(
                render
                    .terrain_images
                    .get(&material.extension.texture)
                    .map(|i| i.texture_descriptor.size.depth_or_array_layers)
            );
            report["groundPhysicalRepeats"] = serde_json::json!([
                material.extension.repeats.to_array().to_vec(),
                vec![material.extension.variation.x]
            ]);
        }
        report["photographicGroundMaterials"] = serde_json::json!(
            render
                .grass_materials
                .get(&render.nature.grass)
                .is_some_and(|m| m.extension.debug.z > 0.5)
        );
        report["cloudShadows"] = render.cloud_shadows.report();
        report["impactScars"] = serde_json::json!(
            game.props
                .iter()
                .filter(|p| p.kind == "crater")
                .map(|p| {
                    let mark = crate::ground_impacts::Mark::from_prop(p);
                    serde_json::json!({"at":p.at,"radius":mark.radius,
                    "excavated":mark.excavated,"seed":mark.seed})
                })
                .collect::<Vec<_>>()
        );
        report["impactScarMeshCount"] = serde_json::json!(render.impact_scars.iter().count());
        report["blenderLayeredVfx"] = serde_json::json!(crate::baked_blast::enabled());
        report["blenderHdrVfx"] = serde_json::json!(crate::baked_blast::hdr_enabled());
        report["blenderDaylightVfx"] = serde_json::json!(crate::baked_blast::daylight_enabled());
        report["blenderFramePixels"] = serde_json::json!(render.baked_art.frame_size());
        report["blenderRadianceScale"] = serde_json::json!(render.baked_art.radiance_scale());
        report["worldLighting"] = serde_json::json!({
            "atmosphereMaxOpacity":render.distance_fog.iter().next().map(|f|f.color.alpha()),
            "shadowMinimumDepth":render.world_cascades.iter().next().map(|p|p.minimum_distance),
            "shadowCascadeDepths":render.world_cascades.iter().next().map(|p|p.bounds.clone()),
            "environmentProbeIntensity":render.world_probe.iter().next().map(|p|p.intensity),
            "modelContactCacheMatches":crate::model_finish::matches(),
            "sunLux": render.world_sun.iter().next().map(|(l, _)| l.illuminance),
            "sunLinear": render.world_sun.iter().next().map(|(l, _)| l.color.to_linear().to_f32_array()),
            "sunRay": render.world_sun.iter().next().map(|(_, t)| t.forward().as_vec3().to_array()),
            "fillLux": render.world_fill.iter().next().map(|l| l.illuminance),
            "fillLinear": render.world_fill.iter().next().map(|l| l.color.to_linear().to_f32_array()),
            "ambientLinear": render.world_ambient.color.to_linear().to_f32_array(),
            "ambientBrightness": render.world_ambient.brightness,
        });
        report["blenderTextureMiB"] =
            serde_json::json!(render.baked_art.texture_bytes() as f64 / 1048576.);
        report["blenderBillowLayers"] = serde_json::json!(render.baked_billows.iter().count());
        report["blenderSimulatedHeat"] = serde_json::json!(render.baked_art.simulated_heat());
        report["blenderBillowAges"] = serde_json::json!(
            render
                .billow_styles
                .iter()
                .map(|b| b.age)
                .collect::<Vec<_>>()
        );
        report["blenderBillowStyles"] = serde_json::json!(
            render
                .billow_styles
                .iter()
                .map(|b| b.style)
                .collect::<Vec<_>>()
        );
        report["blenderBillowScales"] = serde_json::json!(
            render
                .billow_scales
                .iter()
                .map(|t| t.scale.x)
                .collect::<Vec<_>>()
        );
        report["pressureSphereScales"] = serde_json::json!(
            render
                .shell_scales
                .iter()
                .map(|t| t.scale.x)
                .collect::<Vec<_>>()
        );
        report["pressureCentersScreen"] = serde_json::json!(
            render
                .camera
                .single()
                .ok()
                .map(|(camera, transform)| {
                    render
                        .shell_scales
                        .iter()
                        .filter_map(|shell| {
                            camera
                                .world_to_viewport(transform, shell.translation)
                                .ok()
                                .map(|p| p.to_array())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        );
        report["pressureShells"] = serde_json::json!(render.pressure_shells.iter().count());
        report["pressureWavesSpawned"] = serde_json::json!(render.pressure_stats.spawned);
        report["pressureWavesPeak"] = serde_json::json!(render.pressure_stats.peak);
        report["dirtDiscsSpawned"] = serde_json::json!(render.dirt_disc_stats.spawned);
        report["dirtDiscsPeak"] = serde_json::json!(render.dirt_disc_stats.peak);
        report["dirtDiscMaximumRadius"] = serde_json::json!(render.dirt_disc_stats.maximum_radius);
        let camera_pinned = render
            .camera
            .single()
            .is_ok_and(|(_, t)| t.rotation().angle_between(crate::fixed_view::rotation()) < 0.001);
        report["cameraPinned"] = serde_json::json!(camera_pinned);
        report["graphics"] = serde_json::json!(render.graphics.settings);
        report["groundTrackStamps"] = serde_json::json!(render.ground_wear.stamp_count());
        report["groundTrackBounds"] = serde_json::json!(render.ground_wear.bounds());
        report["cameraCenter"] = serde_json::json!(controls.center.to_array());
        report["trackMaskPixels"] = serde_json::json!(
            render
                .terrain_images
                .get(&render.ground_wear.image)
                .and_then(|i| i.data.as_ref())
                .map(|d| d.iter().filter(|v| **v > 0).count())
        );
        report["dlssSupported"] = serde_json::json!(render.graphics.dlss_supported);
        report["terrainTexturePixels"] = serde_json::json!(
            render
                .grass_materials
                .get(&render.nature.grass)
                .and_then(|m| render.terrain_images.get(&m.extension.texture))
                .map(|i| i.width())
        );
        report["audioVoices"] = serde_json::json!(render.audio_voices.iter().count());
        report["audioEngines"] = serde_json::json!(render.audio_engines.iter().count());
        report["audioSinks"] = serde_json::json!(render.audio_sinks.iter().count());
        report["peakAudioVoices"] = serde_json::json!(smoke.peak_audio_voices);
        report["cameraFovDegrees"] =
            serde_json::json!(render.projection.single().ok().and_then(|p| {
                if let Projection::Perspective(p) = p {
                    Some(p.fov.to_degrees())
                } else {
                    None
                }
            }));
        if let Ok((_, transform)) = render.camera.single() {
            let eye = -transform.forward().as_vec3();
            report["cameraElevationDegrees"] = serde_json::json!(eye.y.asin().to_degrees());
            report["cameraHeadingDegrees"] = serde_json::json!(eye.x.atan2(eye.z).to_degrees());
        }
        report["observedBlenderStyles"] = serde_json::json!(smoke.observed_baked_styles);
        report["shrubClusters"] = serde_json::json!(
            render
                .scenery
                .iter()
                .flat_map(|c| &c.placements)
                .filter(|p| p.understory == 1)
                .count()
        );
        report["rockClusters"] = serde_json::json!(
            render
                .scenery
                .iter()
                .flat_map(|c| &c.placements)
                .filter(|p| !p.tree && p.understory == 0)
                .count()
        );
        report["groundcoverClusters"] = serde_json::json!(
            render
                .scenery
                .iter()
                .flat_map(|c| &c.placements)
                .filter(|p| p.understory == 2)
                .count()
        );
        report["townPlans"] = serde_json::json!(
            game.entities
                .iter()
                .filter(|e| e.view.kind == "town")
                .map(|e| e.view.town_plan)
                .collect::<Vec<_>>()
        );
        report["captureZoneShown"] = serde_json::json!(controls.capture_zone.is_some());
        report["captureRadius"] = serde_json::json!(crate::simulation::CAPTURE_RADIUS);
        report["fallingHelicopters"] = serde_json::json!(
            game.props
                .iter()
                .filter(|p| p.wreck.as_ref().is_some_and(|w| w.crash.is_some()))
                .count()
        );
        report["crashImpacts"] = serde_json::json!(
            battle
                .snapshot
                .events
                .iter()
                .filter(|e| e.kind == "crash-impact")
                .count()
        );
        report["sourceModelWrecks"] =
            serde_json::json!(game.props.iter().filter(|p| p.wreck.is_some()).count());
        report["garrisonedSoldiers"] = serde_json::json!(
            game.entities
                .iter()
                .filter(|e| e.view.garrison.as_ref().is_some_and(|g| g.entered))
                .count()
        );
        report["doorCrossedSoldiers"] = serde_json::json!(smoke.door_crossers.len());
        report["wreckKinds"] = serde_json::json!(
            game.props
                .iter()
                .filter_map(|p| p.wreck.as_ref().map(|w| w.piece.kind.clone()))
                .collect::<Vec<_>>()
        );
        let _ = std::fs::write(
            format!("{}.json", smoke.path),
            serde_json::to_vec_pretty(&report).unwrap(),
        );
        println!(
            "Polish check: {} effects, {} ruined houses, {} bases",
            smoke.peak_effects,
            game.entities
                .iter()
                .flat_map(|e| &e.view.lots)
                .filter(|l| l.integrity <= 0.)
                .count(),
            game.bases.len()
        );
        exit.write(AppExit::Success);
    }
}

fn mock_case() -> String {
    let args: Vec<_> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--mock-battle")
        .and_then(|i| args.get(i + 1))
        .filter(|s| !s.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "combined".into())
}
fn mock_duration() -> f32 {
    let args: Vec<_> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--battle-seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(90.)
        .clamp(15., 300.)
}
