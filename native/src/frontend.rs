//! Native operation selection, world campaign and terrain editing controls.
use crate::{Controls, RtsCamera, runtime::CommandQueue, simulation::NativeGame};
use bevy::{
    prelude::*,
    window::{FileDragAndDrop, PrimaryWindow},
};

#[derive(Resource, Default)]
pub struct Frontend {
    pub world: bool,
    pub editor: bool,
    pub settings: bool,
    pub lobby: bool,
    pub address: String,
    pub draft_config: Option<serde_json::Value>,
    pub selected_sector: usize,
    pub brush: i32,
    pub config_row: usize,
    pub was_paused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_frontend_does_not_dirty_environment_visibility() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Frontend>()
            .init_resource::<crate::network::Session>()
            .insert_resource(NativeGame::new())
            .add_systems(Update, update);
        app.world_mut().spawn((MenuText, Text::default()));
        let ground = app
            .world_mut()
            .spawn((crate::landscape::Environment, Visibility::Inherited))
            .id();
        let actor = app
            .world_mut()
            .spawn((
                crate::scene::Actor("test-building".into()),
                Visibility::Inherited,
            ))
            .id();
        app.update();
        let ticks = [ground, actor].map(|id| {
            app.world()
                .entity(id)
                .get_ref::<Visibility>()
                .unwrap()
                .last_changed()
        });
        app.update();
        assert_eq!(
            [ground, actor].map(|id| app
                .world()
                .entity(id)
                .get_ref::<Visibility>()
                .unwrap()
                .last_changed()),
            ticks
        );
        app.world_mut().resource_mut::<Frontend>().world = true;
        app.update();
        for id in [ground, actor] {
            assert_eq!(
                *app.world().get::<Visibility>(id).unwrap(),
                Visibility::Hidden
            );
        }
        app.world_mut().resource_mut::<Frontend>().world = false;
        app.update();
        for id in [ground, actor] {
            assert_eq!(
                *app.world().get::<Visibility>(id).unwrap(),
                Visibility::Inherited
            );
        }
    }
    #[test]
    fn panels_restore_pause_and_discard_unapplied_settings() {
        let mut game = NativeGame::new();
        let mut front = Frontend::default();
        let config = game.config.clone();
        toggle_panel(&mut front, &mut game, 2);
        assert!(game.paused && front.settings);
        front.draft_config.as_mut().unwrap()["shortRange"] = serde_json::json!(3);
        toggle_panel(&mut front, &mut game, 1);
        assert!(game.paused && front.editor && front.draft_config.is_none());
        assert_eq!(game.config, config);
        toggle_panel(&mut front, &mut game, 1);
        assert!(!game.paused);
        game.paused = true;
        toggle_panel(&mut front, &mut game, 3);
        toggle_panel(&mut front, &mut game, 3);
        assert!(game.paused);
    }
}
#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct SectorMarker;
#[derive(Component)]
pub struct Globe;
#[derive(Component)]
pub struct MenuText;
#[derive(Component)]
pub enum MenuAction {
    Editor,
    Host,
    Join,
    LobbyBack,
    Home,
}

pub fn setup(
    mut commands: Commands,
    session: Res<crate::network::Session>,
    icons: Res<crate::unit_icons::UnitIcons>,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(76),
                bottom: px(12),
                height: px(48),
                flex_direction: FlexDirection::Row,
                column_gap: px(5),
                ..default()
            },
            Panel,
            crate::menu::TacticalUi,
        ))
        .with_children(|p| {
            for (name, symbol, action) in [
                ("Menu [Escape]", "menu", MenuAction::Home),
                ("Terrain editor [F8]", "engineer", MenuAction::Editor),
            ] {
                if session.owner != 0 {
                    break;
                }
                p.spawn((
                    Button,
                    action,
                    crate::ui::Tooltip(name.into()),
                    Node {
                        width: px(38),
                        height: percent(100),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::bottom(px(1)),
                        ..default()
                    },
                    BorderColor::all(Color::srgb(0.23, 0.26, 0.25)),
                    BackgroundColor(Color::srgb(0.08, 0.095, 0.09)),
                ))
                .with_children(|p| {
                    p.spawn((
                        icons.image(symbol),
                        Node {
                            width: px(22),
                            height: px(22),
                            ..default()
                        },
                    ));
                });
            }
        });
    commands.spawn((
        MenuText,
        crate::menu::TacticalUi,
        Text::new(""),
        TextFont {
            font_size: 14.,
            ..default()
        },
        TextColor(crate::scene::GOLD),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(80),
            max_width: px(760),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.04, 0.05, 0.85)),
    ));
}
fn toggle_panel(front: &mut Frontend, game: &mut NativeGame, panel: u8) {
    let active = front.world || front.editor || front.settings || front.lobby;
    let close = match panel {
        0 => front.world,
        1 => front.editor,
        2 => front.settings,
        _ => front.lobby,
    };
    if !active {
        front.was_paused = game.paused;
    }
    front.world = !close && panel == 0;
    front.editor = !close && panel == 1;
    front.settings = !close && panel == 2;
    front.lobby = !close && panel == 3;
    if front.settings {
        front.draft_config = Some(game.config.clone());
    } else {
        front.draft_config = None;
    }
    if front.lobby && front.address.is_empty() {
        front.address = "127.0.0.1:4174".into();
    }
    game.paused = if close { front.was_paused } else { true };
    front.brush = front.brush.max(2);
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn input(
    mut front: ResMut<Frontend>,
    mut game: ResMut<NativeGame>,
    mut controls: ResMut<Controls>,
    queue: Res<CommandQueue>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    mut buttons: Query<(&Interaction, &MenuAction, &mut BackgroundColor), Changed<Interaction>>,
    mut files: MessageReader<FileDragAndDrop>,
    mut session: ResMut<crate::network::Session>,
    mut history: ResMut<crate::editor::History>,
    mut typed: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut menu: ResMut<crate::menu::MainMenu>,
    mut loading: ResMut<crate::loading::Loading>,
) {
    if menu.open || loading.busy() {
        files.clear();
        typed.clear();
        return;
    }
    if session.owner != 0 {
        files.clear();
        return;
    }
    for event in files.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            loading.begin(
                crate::loading::Request::Import(path_buf.clone()),
                "Importing operation data".into(),
            );
            return;
        }
    }
    let mut connect = None;
    for (interaction, action, mut color) in &mut buttons {
        *color = BackgroundColor(if *interaction == Interaction::Hovered {
            Color::srgb(0.12, 0.28, 0.3)
        } else {
            Color::srgb(0.07, 0.13, 0.16)
        });
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::Editor => {
                toggle_panel(&mut front, &mut game, 1);
            }

            MenuAction::Host => connect = Some(true),
            MenuAction::Join => connect = Some(false),
            MenuAction::LobbyBack => {
                front.lobby = false;
                game.paused = front.was_paused;
                menu.show(&mut game, session.owner);
                return;
            }
            MenuAction::Home => {
                menu.show(&mut game, session.owner);
                return;
            }
        }
    }

    if keys.just_pressed(KeyCode::F8) {
        toggle_panel(&mut front, &mut game, 1);
    }

    if front.lobby {
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        if ctrl && keys.just_pressed(KeyCode::KeyA) {
            front.address.clear();
        }
        for event in typed.read() {
            if event.state != bevy::input::ButtonState::Pressed {
                continue;
            }
            if event.key_code == KeyCode::Backspace {
                front.address.pop();
            } else if !ctrl && let Some(text) = &event.text {
                for c in text
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit() || ['.', ':', '[', ']'].contains(c))
                {
                    if front.address.len() < 64 {
                        front.address.push(c);
                    }
                }
            }
        }
        let host = connect == Some(true) || keys.just_pressed(KeyCode::KeyH);
        if connect.is_some() || host || keys.just_pressed(KeyCode::Enter) {
            if let Err(e) = session.persist(&game) {
                game.notice = e;
                return;
            }
            game.paused = front.was_paused;
            let path = crate::simulation::save_path().with_file_name(if host {
                "native-host.json"
            } else {
                "native-guest.json"
            });
            let next = if host {
                crate::network::Session::host(&front.address, path, &mut game)
            } else {
                crate::network::Session::join(&front.address, path)
            };
            match next {
                Ok(next) => {
                    *session = next;
                    front.lobby = false;
                    controls.last_map.clear();
                    *history = default();
                }
                Err(e) => {
                    game.paused = true;
                    game.notice = format!("Multiplayer: {e}");
                }
            }
        }
    } else {
        typed.clear();
    }
    if front.world {
        for (key, command) in [(KeyCode::F5, "save"), (KeyCode::F9, "load")] {
            if keys.just_pressed(key) {
                queue.send(serde_json::json!({"type":command}));
            }
        }
        if let Some(cursor) = window.cursor_position()
            && mouse.just_pressed(MouseButton::Left)
            && !crate::ui::over_ui(cursor, window.size(), &controls)
        {
            let (cam, transform) = *camera;
            if let Some((index, _)) = game
                .world
                .sectors
                .iter()
                .enumerate()
                .filter(|(i, _)| sector_position(*i, &game.world).dot(transform.translation()) > 0.)
                .filter_map(|(i, _)| {
                    cam.world_to_viewport(transform, sector_position(i, &game.world))
                        .ok()
                        .map(|p| (i, p.distance(cursor)))
                })
                .filter(|(_, d)| *d < 24.)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                front.selected_sector = index;
            }
        }
        if keys.just_pressed(KeyCode::Enter) {
            let i = front.selected_sector;
            if game.world.can_contest(i, 0) {
                loading.begin(
                    crate::loading::Request::Sector(Box::new(game.clone()), i),
                    "Preparing campaign deployment".into(),
                );
            } else {
                game.notice = "Choose a hostile sector adjacent to friendly territory.".into();
            }
        }
    }
    if !front.editor || mouse.just_released(MouseButton::Left) {
        history.finish(&game.terrain);
    }
    if front.editor {
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        if ctrl && keys.just_pressed(KeyCode::KeyZ) {
            history.undo(&mut game.terrain);
        }
        if ctrl && keys.just_pressed(KeyCode::KeyY) {
            history.redo(&mut game.terrain);
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            front.brush = (front.brush - 1).max(1);
        }
        if keys.just_pressed(KeyCode::BracketRight) {
            front.brush = (front.brush + 1).min(24);
        }
        if let Some(p) = controls.ground
            && mouse.pressed(MouseButton::Left)
            && window
                .cursor_position()
                .is_some_and(|p| !crate::ui::over_ui(p, window.size(), &controls))
        {
            let lower = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            history.paint(
                &mut game.terrain,
                p.x,
                p.z,
                front.brush,
                if lower { -0.015 } else { 0.015 },
                keys.pressed(KeyCode::KeyW),
            );
        }
        if keys.just_pressed(KeyCode::F4) {
            game.notice = match game.export_map() {
                Ok(path) => format!("Map exported to {}", path.display()),
                Err(e) => format!("Export failed: {e}"),
            };
        }
    }
    if front.settings {
        let defaults: serde_json::Value =
            serde_json::from_str(include_str!("../config.json")).unwrap();
        let limits = defaults["limits"].as_object().unwrap();
        let fields: Vec<_> = limits.keys().cloned().collect();
        if keys.just_pressed(KeyCode::ArrowDown) {
            front.config_row = (front.config_row + 1) % fields.len();
        }
        if keys.just_pressed(KeyCode::ArrowUp) {
            front.config_row = (front.config_row + fields.len() - 1) % fields.len();
        }
        let key = &fields[front.config_row % fields.len()];
        let increment = if keys.pressed(KeyCode::ShiftLeft) {
            10
        } else {
            1
        };
        let delta = if keys.just_pressed(KeyCode::ArrowRight) {
            increment
        } else if keys.just_pressed(KeyCode::ArrowLeft) {
            -increment
        } else {
            0
        };
        if delta != 0 {
            let draft = front.draft_config.as_mut().unwrap();
            let current = draft[key].as_i64().unwrap_or(0);
            let lo = limits[key][0].as_i64().unwrap();
            let hi = limits[key][1].as_i64().unwrap();
            draft[key] = serde_json::json!((current + delta).clamp(lo, hi));
        }
        if keys.just_pressed(KeyCode::KeyF) {
            let draft = front.draft_config.as_mut().unwrap();
            let fog = draft["fogEnabled"].as_bool().unwrap_or(true);
            draft["fogEnabled"] = serde_json::json!(!fog);
        }
        if keys.just_pressed(KeyCode::Enter) {
            let config = front.draft_config.take().unwrap();
            let cols = config["cols"].as_i64().unwrap_or(160) as i32;
            let rows = config["rows"].as_i64().unwrap_or(160) as i32;
            loading.begin(
                crate::loading::Request::Map {
                    id: "custom".into(),
                    cols,
                    rows,
                    config,
                },
                "Generating custom terrain".into(),
            );
            front.draft_config = Some(game.config.clone());
        }
    }
}

pub fn sector_position(index: usize, world: &crate::world::World) -> Vec3 {
    let longitude =
        (world.sectors[index].x as f32 - 1.) * std::f32::consts::TAU / world.width.max(1) as f32;
    let latitude = 0.6 - world.sectors[index].y as f32 * 1.2 / (world.height - 1).max(1) as f32;
    Vec3::new(
        longitude.sin() * latitude.cos(),
        latitude.sin(),
        longitude.cos() * latitude.cos(),
    ) * 31.
}
pub fn world_scene(
    mut commands: Commands,
    front: Res<Frontend>,
    game: Res<NativeGame>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<(Entity, Option<&SectorMarker>), With<Globe>>,
    mut last: Local<String>,
) {
    let key = format!(
        "{}:{}:{}",
        front.world, game.world.turn, front.selected_sector
    );
    if *last == key {
        return;
    }
    *last = key;
    for (e, _) in &existing {
        commands.entity(e).despawn();
    }
    if !front.world {
        return;
    }
    let sphere = meshes.add(Sphere::new(2.1).mesh().uv(16, 12));
    commands.spawn((
        Globe,
        Mesh3d(meshes.add(Sphere::new(29.).mesh().uv(64, 32))),
        MeshMaterial3d(materials.add(Color::srgb(0.035, 0.12, 0.18))),
        Transform::default(),
    ));
    for (i, s) in game.world.sectors.iter().enumerate() {
        let color = if i == front.selected_sector {
            crate::scene::GOLD
        } else {
            match s.owner {
                Some(0) => crate::scene::BLUE,
                Some(1) => crate::scene::RED,
                _ => Color::srgb(0.32, 0.4, 0.36),
            }
        };
        commands.spawn((
            Globe,
            SectorMarker,
            Mesh3d(sphere.clone()),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(sector_position(i, &game.world)),
        ));
    }
}
type GroundVisibility<'w, 's> = Query<
    'w,
    's,
    &'static mut Visibility,
    (
        With<crate::landscape::Environment>,
        Without<crate::scene::Actor>,
        Without<crate::fog::FogSurface>,
    ),
>;

pub fn update(
    front: Res<Frontend>,
    game: Res<NativeGame>,
    session: Res<crate::network::Session>,
    mut panels: Query<&mut Node, With<Panel>>,
    mut text: Single<&mut Text, With<MenuText>>,
    mut grounds: GroundVisibility,
    mut actors: Query<
        &mut Visibility,
        (
            With<crate::scene::Actor>,
            Without<crate::landscape::Environment>,
        ),
    >,
) {
    let _timing = crate::performance::span("frontend.update");
    for mut node in &mut panels {
        let display = if session.owner == 0 {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    let visible = if front.world {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut v in &mut grounds {
        v.set_if_neq(visible);
    }
    for mut v in &mut actors {
        v.set_if_neq(visible);
    }
    let next_text = if front.world {
        let s = &game.world.sectors[front.selected_sector.min(game.world.sectors.len() - 1)];
        format!(
            "WORLD CAMPAIGN | Turn {}\n{} | {} | Owner: {} | Supply: {}\nClick a sector. Enter: deploy. F7: return.\n{}",
            game.world.turn,
            s.name,
            s.biome,
            s.owner.map(|o| o.to_string()).unwrap_or("Neutral".into()),
            s.supply,
            game.notice
        )
    } else if front.editor {
        format!(
            "TERRAIN EDITOR | {} x {} | Brush {}\nDrag: raise | Shift+drag: lower | W+drag: water | [ / ]: brush size\nCtrl+Z / Ctrl+Y: undo / redo | F4: export map | F8: return",
            game.terrain.cols, game.terrain.rows, front.brush
        )
    } else if front.lobby {
        format!(
            "NATIVE MULTIPLAYER\nAddress: {}\nType IP:port | Backspace: erase | Ctrl+A: clear\nH: host/resume | Enter: join/reconnect | F6: return\nHost on 0.0.0.0:4174 for LAN play; guest enters the host's LAN IP.\n{}",
            front.address, game.notice
        )
    } else if front.settings {
        let defaults: serde_json::Value =
            serde_json::from_str(include_str!("../config.json")).unwrap();
        let fields: Vec<_> = defaults["limits"].as_object().unwrap().keys().collect();
        let key = fields[front.config_row % fields.len()];
        format!(
            "DEVELOPER SETTINGS\n{}: {}\nUp/Down: field | Left/Right: value | Shift: x10\nF: fog ({}) | Enter: apply and restart | F10: close",
            key,
            front.draft_config.as_ref().unwrap()[key],
            front.draft_config.as_ref().unwrap()["fogEnabled"]
        )
    } else {
        String::new()
    };
    if ***text != next_text {
        ***text = next_text;
    }
}
