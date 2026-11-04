//! Native title screen, operation selection, controls and presentation options.
use crate::{
    frontend::Frontend,
    loading::{Loading, Request},
    simulation::NativeGame,
    theme,
};
use bevy::{
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

pub const MAPS: [(&str, i32, &str, &str); 1] = [(
    crate::battlefield::ID,
    crate::battlefield::COLS,
    "FORWARD BASE",
    "A long countryside battlefield. Three towns, outlying farms, and five command posts.",
)];
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Page {
    #[default]
    Home,
    Operations,
    Options,
    Guide,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_menu_visibility_keeps_hud_root_layout_stable() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MainMenu {
                open: false,
                started: true,
                ..default()
            })
            .insert_resource(crate::landscape::Ready {
                ready: true,
                ..default()
            })
            .insert_resource(crate::graphics::Graphics::load())
            .insert_resource(NativeGame::new())
            .init_resource::<Frontend>()
            .init_resource::<Loading>()
            .init_resource::<crate::network::Session>()
            .init_resource::<crate::Controls>()
            .init_resource::<Assets<Image>>()
            .add_systems(Update, update);
        let root = app.world_mut().spawn((MenuRoot, Node::default())).id();
        let hud = app.world_mut().spawn((TacticalUi, Node::default())).id();
        let player = app
            .world_mut()
            .spawn((TacticalUi, crate::ui::PlayerControls, Node::default()))
            .id();
        let spectator = app
            .world_mut()
            .spawn((TacticalUi, crate::ui::SimulationControls, Node::default()))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Node>(player).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            app.world().get::<Node>(spectator).unwrap().display,
            Display::None
        );
        let ticks = [root, hud].map(|id| {
            app.world()
                .entity(id)
                .get_ref::<Node>()
                .unwrap()
                .last_changed()
        });
        app.update();
        assert_eq!(
            [root, hud].map(|id| app
                .world()
                .entity(id)
                .get_ref::<Node>()
                .unwrap()
                .last_changed()),
            ticks
        );
        app.world_mut().resource_mut::<NativeGame>().simulation = true;
        app.update();
        assert_eq!(
            app.world().get::<Node>(player).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Node>(spectator).unwrap().display,
            Display::Flex
        );
        app.world_mut().resource_mut::<crate::Controls>().hud_hidden = true;
        app.update();
        assert_eq!(app.world().get::<Node>(hud).unwrap().display, Display::None);
        assert_eq!(
            app.world()
                .entity(root)
                .get_ref::<Node>()
                .unwrap()
                .last_changed(),
            ticks[0]
        );
    }
    #[test]
    fn escape_menu_restores_pause_and_does_not_discard_an_operation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppExit>()
            .insert_resource(MainMenu {
                open: false,
                started: true,
                ..default()
            })
            .insert_resource(NativeGame::new())
            .init_resource::<Frontend>()
            .init_resource::<Loading>()
            .init_resource::<crate::network::Session>()
            .init_resource::<crate::Controls>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(crate::landscape::Ready {
                ready: true,
                error: None,
                ..default()
            })
            .add_systems(Update, input);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut()
            .resource_mut::<crate::Controls>()
            .command_drawer = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<crate::Controls>().command_drawer);
        assert!(!app.world().resource::<MainMenu>().open);
        assert!(!app.world().resource::<NativeGame>().paused);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Escape);
            keys.clear();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F10);
        app.update();
        assert!(app.world().resource::<crate::Controls>().hud_hidden);
        assert!(!app.world().resource::<NativeGame>().paused);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::F10);
            keys.clear();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<MainMenu>().open);
        assert!(app.world().resource::<crate::Controls>().hud_hidden);
        assert!(app.world().resource::<NativeGame>().paused);
        let entities = app.world().resource::<NativeGame>().entities.len();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Escape);
            keys.clear();
            keys.press(KeyCode::Escape);
        }
        app.update();
        assert!(!app.world().resource::<MainMenu>().open);
        assert!(!app.world().resource::<NativeGame>().paused);
        assert_eq!(
            app.world().resource::<NativeGame>().entities.len(),
            entities
        );
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.press(KeyCode::F10);
        }
        app.update();
        assert!(!app.world().resource::<crate::Controls>().hud_hidden);
        assert!(!app.world().resource::<NativeGame>().paused);
    }
}
#[derive(Resource)]
pub struct MainMenu {
    pub open: bool,
    pub started: bool,
    pub page: Page,
    pub selected: usize,
    pub pan_speed: f32,
    pub sound: bool,
    was_paused: bool,
}
impl Default for MainMenu {
    fn default() -> Self {
        Self {
            open: true,
            started: false,
            page: Page::Home,
            selected: 0,
            pan_speed: 1.,
            sound: true,
            was_paused: false,
        }
    }
}
impl MainMenu {
    pub fn show(&mut self, game: &mut NativeGame, owner: i32) {
        self.was_paused = game.paused;
        if owner == 0 {
            game.paused = true;
        }
        self.open = true;
        self.page = Page::Home;
    }
}
#[derive(Component)]
pub struct MenuRoot;
#[derive(Component)]
pub struct TacticalUi;
#[derive(Component)]
pub struct LoadingLabel;
#[derive(Component, Clone, Copy)]
pub enum Choice {
    New,
    Simulation,
    SimulationContinue,
    Training,
    TrainingContinue,
    Map(usize),
    Deploy,
    Continue,
    Resume,
    Options,
    Guide,
    Back,
    Quit,
    Speed,
    Fullscreen,
    Fog,
    Sound,
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        MenuRoot,
        GlobalZIndex(100),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.04, 0.05, 0.48)),
    ));
}

fn resume(menu: &mut MainMenu, game: &mut NativeGame, owner: i32) {
    menu.open = false;
    if owner == 0 {
        game.paused = menu.was_paused;
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn input(
    mut menu: ResMut<MainMenu>,
    mut game: ResMut<NativeGame>,
    mut front: ResMut<Frontend>,
    mut loading: ResMut<Loading>,
    ready: Res<crate::landscape::Ready>,
    session: Res<crate::network::Session>,
    keys: Res<ButtonInput<KeyCode>>,
    mut controls: ResMut<crate::Controls>,
    mut buttons: Query<(&Interaction, &Choice, &mut BackgroundColor), Changed<Interaction>>,
    mut exit: MessageWriter<AppExit>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::F11) {
        toggle_fullscreen(&mut window);
    }
    if loading.busy() {
        return;
    }
    if keys.just_pressed(KeyCode::F10)
        && !menu.open
        && ready.ready
        && !front.world
        && !front.editor
        && !front.settings
        && !front.lobby
    {
        controls.hud_hidden = !controls.hud_hidden;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if menu.open {
            if menu.page != Page::Home {
                menu.page = Page::Home;
            } else if menu.started && ready.ready {
                resume(&mut menu, &mut game, session.owner);
            }
        } else if front.world || front.editor || front.settings || front.lobby {
            front.world = false;
            front.editor = false;
            front.settings = false;
            front.lobby = false;
            front.draft_config = None;
            if session.owner == 0 {
                game.paused = front.was_paused;
            }
        } else {
            if controls.build.is_some()
                || controls.move_only
                || controls.attack_move
                || controls.patrol
                || controls.drag.is_some()
            {
                controls.build = None;
                controls.bridge_start = None;
                controls.attack_move = false;
                controls.move_only = false;
                controls.patrol = false;
                controls.drag = None;
                return;
            }
            if controls.command_drawer && !controls.hud_hidden {
                controls.command_drawer = false;
                return;
            }
            menu.was_paused = game.paused;
            if session.owner == 0 {
                game.paused = true;
            }
            menu.open = true;
            menu.page = Page::Home;
            controls.drag = None;
        }
        return;
    }
    if !menu.open {
        return;
    }
    let mut chosen = None;
    for (interaction, choice, mut color) in &mut buttons {
        *color = BackgroundColor(if *interaction == Interaction::Hovered {
            Color::srgb(0.18, 0.24, 0.25)
        } else {
            Color::srgba(0.095, 0.11, 0.10, 0.97)
        });
        if *interaction == Interaction::Pressed {
            chosen = Some(*choice);
        }
    }
    if keys.just_pressed(KeyCode::Enter) && !session.multiplayer() {
        chosen = Some(if menu.page == Page::Operations {
            Choice::Deploy
        } else if menu.started {
            Choice::Resume
        } else {
            Choice::New
        });
    }
    match chosen {
        Some(Choice::Simulation) if !session.multiplayer() => loading.begin(
            Request::Simulation(crate::simulation::core_config(&game.config)),
            "Preparing AI vs AI simulation".into(),
        ),
        Some(Choice::SimulationContinue) if !session.multiplayer() => loading.begin(
            Request::Restore(
                crate::simulation::save_path().with_file_name("native-simulation.json"),
            ),
            "Restoring AI vs AI simulation".into(),
        ),
        Some(Choice::Training) if !session.multiplayer() => {
            let mut config = crate::simulation::core_config(&game.config);
            config["fogEnabled"] = false.into();
            loading.begin(
                Request::Map {
                    id: crate::training::ID.into(),
                    cols: crate::training::SIZE,
                    rows: crate::training::SIZE,
                    config,
                },
                "Preparing training ground".into(),
            );
        }
        Some(Choice::TrainingContinue) if !session.multiplayer() => loading.begin(
            Request::Restore(crate::simulation::save_path().with_file_name("native-training.json")),
            "Restoring training ground".into(),
        ),
        Some(Choice::Map(index)) => menu.selected = index.min(MAPS.len() - 1),
        Some(Choice::New | Choice::Deploy) if !session.multiplayer() => {
            let (id, size, name, _) = MAPS[menu.selected];
            let id = if crate::gis_prototype::enabled() {
                crate::gis_prototype::ID
            } else {
                id
            };
            loading.begin(
                Request::Map {
                    id: id.into(),
                    cols: size,
                    rows: crate::battlefield::ROWS,
                    config: crate::simulation::core_config(&game.config),
                },
                format!("Preparing {name} · {size} × {}", crate::battlefield::ROWS),
            );
        }
        Some(Choice::Continue) if !session.multiplayer() => loading.begin(
            Request::Restore(if crate::gis_prototype::enabled() {
                crate::simulation::save_path().with_file_name(crate::gis_prototype::SAVE)
            } else {
                crate::simulation::save_path()
            }),
            "Restoring your saved operation".into(),
        ),
        Some(Choice::Resume) if menu.started && ready.ready => {
            resume(&mut menu, &mut game, session.owner)
        }
        Some(Choice::Options) => menu.page = Page::Options,
        Some(Choice::Guide) => menu.page = Page::Guide,
        Some(Choice::Back) => {
            menu.page = Page::Home;
            loading.error = None;
        }
        Some(Choice::Quit) => {
            let _ = session.persist(&game);
            exit.write(AppExit::Success);
        }
        Some(Choice::Speed) => {
            menu.pan_speed = if menu.pan_speed >= 2. {
                0.5
            } else {
                menu.pan_speed + 0.5
            }
        }
        Some(Choice::Fullscreen) => toggle_fullscreen(&mut window),
        Some(Choice::Sound) => menu.sound = !menu.sound,
        Some(Choice::Fog) if session.owner == 0 => {
            let fog = game.config["fogEnabled"].as_bool().unwrap_or(true);
            game.config["fogEnabled"] = serde_json::json!(!fog);
        }
        _ => {}
    }
}
fn toggle_fullscreen(window: &mut Window) {
    window.mode = if window.mode == WindowMode::Windowed {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
}
fn label(parent: &mut ChildSpawnerCommands, value: &str, size: f32, color: Color) {
    parent.spawn((
        Text::new(value),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    ));
}
fn button(parent: &mut ChildSpawnerCommands, value: &str, choice: Choice, accent: bool) {
    parent
        .spawn((
            Button,
            choice,
            Node {
                width: percent(100),
                min_height: px(42),
                padding: UiRect::axes(px(18), px(9)),
                border: UiRect {
                    left: px(if accent { 3. } else { 1. }),
                    bottom: px(1),
                    ..default()
                },
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(if accent {
                theme::ACCENT
            } else {
                Color::srgb(0.2, 0.29, 0.3)
            }),
            BackgroundColor(Color::srgba(0.095, 0.11, 0.10, 0.97)),
        ))
        .with_children(|p| {
            label(
                p,
                value,
                18.,
                if accent { theme::ACCENT } else { theme::PAPER },
            )
        });
}
fn lobby_button(
    parent: &mut ChildSpawnerCommands,
    value: &str,
    action: crate::frontend::MenuAction,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: percent(100),
                min_height: px(50),
                padding: UiRect::axes(px(18), px(12)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.11, 0.19, 0.21)),
        ))
        .with_children(|p| label(p, value, 23., theme::PAPER));
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn update(
    mut commands: Commands,
    menu: Res<MainMenu>,
    controls: Res<crate::Controls>,
    graphics: Res<crate::graphics::Graphics>,
    loading: Res<Loading>,
    ready: Res<crate::landscape::Ready>,
    game: Res<NativeGame>,
    front: Res<Frontend>,
    session: Res<crate::network::Session>,
    mut images: ResMut<Assets<Image>>,
    mut root: Single<(Entity, &mut Node), With<MenuRoot>>,
    mut hud: Query<
        (
            &mut Node,
            Option<&crate::ui::PlayerControls>,
            Option<&crate::ui::SimulationControls>,
        ),
        (With<TacticalUi>, Without<MenuRoot>),
    >,
    mut captions: Query<&mut Text, With<LoadingLabel>>,
    mut last: Local<String>,
) {
    let preparing = loading.busy() || (!menu.open && !ready.ready && !front.world);
    let show = menu.open || preparing || front.lobby;
    let menu_display = if show { Display::Flex } else { Display::None };
    if root.1.display != menu_display {
        root.1.display = menu_display;
    }
    for (mut node, player, simulation) in &mut hud {
        let display = if show
            || controls.hud_hidden
            || player.is_some() && game.simulation
            || simulation.is_some() && !game.simulation
        {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    for mut caption in &mut captions {
        let next = format!(
            "{}{}\n{} · {:.1}s",
            if loading.busy() {
                "PREPARING OPERATION"
            } else {
                "BUILDING BATTLEFIELD"
            },
            ".".repeat((loading.seconds() * 2.) as usize % 4),
            loading.label,
            loading.seconds()
        );
        if **caption != next {
            **caption = next;
        }
    }
    let save_exists = if crate::gis_prototype::enabled() {
        crate::simulation::save_path()
            .with_file_name(crate::gis_prototype::SAVE)
            .exists()
    } else {
        crate::simulation::save_path().exists()
    };
    let key = format!(
        "{show}:{preparing}:{:?}:{}:{}:{}:{}:{}:{:?}:{:?}:{}:{}:{}:{}:{:?}:{}",
        menu.page,
        menu.selected,
        menu.started,
        menu.pan_speed,
        menu.sound,
        session.multiplayer(),
        loading.error,
        ready.error,
        game.config["fogEnabled"],
        front.lobby,
        front.address,
        if front.lobby {
            game.notice.as_str()
        } else {
            ""
        },
        graphics.settings,
        graphics.dlss_supported
    );
    if *last == key {
        return;
    }
    *last = key;
    commands.entity(root.0).despawn_children();
    if !show {
        return;
    }
    commands.entity(root.0).with_children(|outer| {
        if front.lobby && !preparing {
            outer.spawn((Node {width:percent(100),height:percent(100),padding:UiRect::all(px(64)),column_gap:px(64),align_items:AlignItems::Center,..default()},theme::Panel,BackgroundColor(Color::srgba(0.028,0.037,0.033,0.97))))
            .with_children(|row| {
                row.spawn(Node {width:percent(48),flex_direction:FlexDirection::Column,row_gap:px(20),..default()}).with_children(|p| {
                    label(p,"OPERATIONS DIVISION  /  NETWORK",17.,theme::ACCENT);
                    label(p,"TWO COMMANDS.\nONE BATTLEFIELD.",64.,theme::PAPER);
                    label(p,"Host as Field Command, or join as the Iron Front.\nYour operation is saved and can be resumed.",22.,theme::MUTED);
                    label(p,"LAN hosting: use 0.0.0.0:4174.\nTo join, enter the host computer's LAN IP and port.",19.,theme::MUTED);
                    lobby_button(p,"BACK TO MAIN MENU",crate::frontend::MenuAction::LobbyBack);
                });
                row.spawn((Node {width:percent(46),padding:UiRect::all(px(32)),flex_direction:FlexDirection::Column,row_gap:px(18),border:UiRect::top(px(2)),..default()},BorderColor::all(theme::ACCENT),BackgroundColor(Color::srgb(0.07,0.12,0.14))))
                .with_children(|p| {
                    label(p,"CONNECTION ADDRESS",23.,theme::ACCENT);
                    p.spawn((Node {width:percent(100),min_height:px(64),padding:UiRect::all(px(16)),..default()},BackgroundColor(theme::INK))).with_children(|p| label(p,if front.address.is_empty() {"Type an IP address and port…"} else {&front.address},28.,theme::PAPER));
                    label(p,"Type to edit  ·  Ctrl+A clears  ·  Backspace erases",17.,theme::MUTED);
                    lobby_button(p,"HOST / RESUME  ·  H",crate::frontend::MenuAction::Host);
                    lobby_button(p,"JOIN / RECONNECT  ·  ENTER",crate::frontend::MenuAction::Join);
                    label(p,&game.notice,18.,theme::MUTED);
                });
            });
            return;
        }
        if preparing {
            outer.spawn((Node { width:percent(100),height:percent(100),align_items:AlignItems::Center,justify_content:JustifyContent::Center,..default() },BackgroundColor(Color::srgba(0.025,0.04,0.05,0.9))))
            .with_children(|p| { p.spawn((Node { width:px(580),padding:UiRect::all(px(36)),flex_direction:FlexDirection::Column,row_gap:px(18),border:UiRect::top(px(2)),..default() },theme::Panel,BorderColor::all(theme::ACCENT),BackgroundColor(theme::INK)))
                .with_children(|p| { label(p,"RULE OF WAR",46.,theme::PAPER); p.spawn((Text::new("Preparing your operation…"),LoadingLabel,TextFont {font_size:22.,..default()},TextColor(theme::ACCENT))); label(p,"Terrain and deployment are prepared in the background.",17.,theme::MUTED); }); });
            return;
        }
        outer.spawn((Node { width:percent(44),height:percent(100),min_width:px(350),padding:UiRect::axes(px(32),px(32)),flex_direction:FlexDirection::Column,row_gap:px(10),..default() },theme::Panel,BackgroundColor(Color::srgba(0.028,0.037,0.033,0.97))))
        .with_children(|p| {
            label(p,"FIELD COMMAND  /  STRATEGY",16.,theme::ACCENT);
            label(p,match menu.page { Page::Home=>"RULE OF WAR",Page::Operations=>"CHOOSE YOUR\nTHEATRE",Page::Options=>"FIELD\nPREFERENCES",Page::Guide=>"COMMANDER'S\nFIELD MANUAL" },if menu.page==Page::Home {60.} else {48.},theme::PAPER);
            label(p,match menu.page { Page::Home=>"COMMAND / RECRUIT / SECURE",Page::Operations=>"Lead Field Command against the Iron Front.\nChoose the scale of your next operation.",Page::Options=>"Make the battlefield your own.",Page::Guide=>"Plan the advance. Protect the supply line.\nHold the command network." },20.,theme::MUTED);
            p.spawn(Node { height:px(12),..default() });
            match menu.page {
                Page::Home=>{
                    if menu.started && ready.ready { button(p,"RESUME OPERATION",Choice::Resume,true); }
                    if !session.multiplayer() {
                        if crate::gis_prototype::enabled() {
                            button(p,"RESTART HOTTOT PROTOTYPE",Choice::New,!menu.started);
                        } else {
                        button(p,"NEW BATTLE / 2048 x 1024",Choice::New,!menu.started);
                        button(p,"SIMULATION / AI vs AI",Choice::Simulation,false);
                        if crate::simulation::save_path().with_file_name("native-simulation.json").exists() {button(p,"CONTINUE SIMULATION",Choice::SimulationContinue,false);}
                        button(p,"TRAINING / PRACTICE & BUILD",Choice::Training,false);
                        if crate::simulation::save_path().with_file_name("native-training.json").exists() {button(p,"CONTINUE TRAINING",Choice::TrainingContinue,false);}
                        }
                        if save_exists { button(p,"CONTINUE SAVED OPERATION",Choice::Continue,false); }
                    }
                    button(p,"SETTINGS",Choice::Options,false);
                    button(p,"CONTROLS & GUIDE",Choice::Guide,false);
                    button(p,"QUIT TO DESKTOP",Choice::Quit,false);
                }
                Page::Operations=>{
                    let (_,size,name,_)=MAPS[menu.selected];
                    label(p,&format!("{name}  /  {size} × 1024"),26.,theme::ACCENT);
                    button(p,"DEPLOY FIELD COMMAND",Choice::Deploy,true);
                    button(p,"BACK",Choice::Back,false);
                }
                Page::Options=>{
                    button(p,&format!("CAMERA SPEED  /  {:.1}×",menu.pan_speed),Choice::Speed,true);
                    button(p,"TOGGLE FULLSCREEN  /  F11",Choice::Fullscreen,false);
                    button(p,&format!("BATTLEFIELD AUDIO  /  {}",if menu.sound {"ON"}else{"OFF"}),Choice::Sound,false);
                    if session.owner==0 { button(p,&format!("FOG OF WAR  /  {}",if game.config["fogEnabled"]==false {"OFF"} else {"ON"}),Choice::Fog,false); }
                    label(p,"Hold Shift while panning for a faster advance.\nPreferences apply immediately.",18.,theme::MUTED);
                    button(p,"BACK",Choice::Back,false);
                }
                Page::Guide=>{
                    label(p,"WASD / arrows     Pan the camera\nShift                 Pan faster\nMouse wheel          Zoom\nCamera angle          Locked\nAlt + left drag       Grab map\nMiddle / right drag    Grab map\nC                     Focus selection\nLeft click / drag      Select units\nRight click            Move, fire or enter building\nG / U                 Enter nearby / leave building\nQ / M / P             Attack-move / move / patrol\nX / R                 Stop / retreat to base\nZ / double-click      Select forces / same type\nCtrl + 1–9 / 1–9      Store / recall groups\nF5 / F9               Save / load\nEscape                Menu / return\nF10                   Hide / show UI\nF11                   Fullscreen",17.,theme::PAPER);
                    button(p,"BACK",Choice::Back,false);
                }
            }
            if let Some(error)=loading.error.as_ref().or(ready.error.as_ref()) { label(p,error,17.,Color::srgb(0.95,0.58,0.4)); }
        });
        outer.spawn((Node { position_type:PositionType::Absolute,right:px(32),top:px(60),width:percent(43),padding:UiRect::all(px(24)),border:UiRect::top(px(2)),flex_direction:FlexDirection::Column,row_gap:px(14),..default() },theme::Panel,BorderColor::all(theme::ACCENT),BackgroundColor(Color::srgba(0.035,0.049,0.043,0.94))))
        .with_children(|p| {
            if menu.page==Page::Operations {
                for (i,(_,size,name,description)) in MAPS.iter().enumerate() {
                    p.spawn((Button,Choice::Map(i),Node { width:percent(100),padding:UiRect::all(px(18)),flex_direction:FlexDirection::Column,row_gap:px(6),border:UiRect::left(px(3)),..default() },BorderColor::all(if menu.selected==i {theme::ACCENT} else {theme::MUTED}),BackgroundColor(Color::srgba(0.04,0.085,0.10,0.94))))
                    .with_children(|p| { label(p,name,30.,theme::PAPER); label(p,&format!("{size} × 1024  /  SINGLE PLAYER"),17.,theme::ACCENT); label(p,description,18.,theme::MUTED); });
                }
            } else if menu.page==Page::Options {
                crate::graphics::panel(p, &graphics);
            } else if menu.page==Page::Guide {
                label(p,"PRESERVE YOUR FORCE",18.,theme::ACCENT);
                label(p,"Cover. Flank.\nBring them home.",44.,theme::PAPER);
                label(p,"FOUR INFANTRY CLASSES\nAssault: standard rifle fighter.\nSniper: 110m rifle / 4.5s reload.\nMedic: heal and rescue wounded.\nSupport: rifle, AA launcher and field works.\n\nMISSILES & COUNTERMEASURES\nSupport locks aircraft within 160m, then reloads for 12s. Aircraft evade 40% of missiles, or 80% with flares. A missile that hits always destroys the aircraft. Aircraft carry 3 salvos; 12s cooldown. Airfield service replenishes them.\n\nUse walls and sandbags for cover. Sustained fire suppresses soldiers. R retreats to base. A medic has 20s to rescue casualties.\n\nOnly your own building interiors are revealed. Blue unit symbols mark Field Command; red symbols mark Iron Front.",18.,theme::PAPER);
            } else {
                label(p,"OPERATIONS DIVISION  /  INTELLIGENCE",16.,theme::ACCENT);
                label(p,"BUILD YOUR FORCE.\nCONTROL THE FIELD.",38.,theme::PAPER);
                let mut pixels=Vec::with_capacity(128*80*4);
                for z in 0..80 {for x in 0..128 {
                    let c=game.terrain.cell(x as f32/127.*game.terrain.cols as f32,z as f32/79.*game.terrain.rows as f32);
                    let shade=(c.elevation*50.) as u8;
                    let grid=if x%16==0||z%16==0 {10}else{0};
                    pixels.extend(if c.water() {[31+grid,56+grid,59+grid,255]}else{[46+shade+grid,61+shade+grid,43+shade+grid,255]});
                }}
                let image=Image::new(bevy::render::render_resource::Extent3d {width:128,height:80,depth_or_array_layers:1},bevy::render::render_resource::TextureDimension::D2,pixels,bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,bevy::asset::RenderAssetUsages::default());
                p.spawn((ImageNode::new(images.add(image)),Node {width:percent(100),height:px(190),border:UiRect::all(px(1)),..default()},BorderColor::all(Color::srgb(0.25,0.30,0.25)))).with_children(|map| {
                    for base in &game.bases {
                        let color=if base.owner==0 {crate::scene::BLUE}else{crate::scene::RED};
                        map.spawn((Node {position_type:PositionType::Absolute,left:percent(base.origin[0]/game.terrain.cols as f32*100.),top:percent(base.origin[1]/game.terrain.rows as f32*100.),width:px(13),height:px(13),border:UiRect::all(px(2)),..default()},BorderColor::all(theme::PAPER),BackgroundColor(color)));
                    }
                    for unit in game.entities.iter().filter(|e| ["objective","town","village"].contains(&e.view.kind.as_str())) {
                        map.spawn((Node {position_type:PositionType::Absolute,left:percent(unit.view.x/game.terrain.cols as f32*100.),top:percent(unit.view.z/game.terrain.rows as f32*100.),width:px(7),height:px(7),..default()},BackgroundColor(if unit.view.kind=="objective" {theme::ACCENT}else{theme::MUTED})));
                    }
                });
                label(p,"RECRUITMENT / ALWAYS AVAILABLE IN THE HUD",14.,theme::ACCENT);
                label(p,"ASSAULT / SNIPER / MEDIC / SUPPORT\nINFANTRY: BARRACKS / VEHICLES: DEPOT\nAIRCRAFT: AIRFIELD\nChoose a production tab, then click units to queue them.\nUp to 5 per building; production runs in order.",19.,theme::PAPER);
                p.spawn((Node {width:percent(100),padding:UiRect::axes(px(0),px(10)),border:UiRect::top(px(1)),..default()},BorderColor::all(Color::srgb(0.25,0.30,0.25)))).with_children(|p| {label(p,"INFANTRY  /  ARMOUR  /  AIR SUPPORT",16.,theme::MUTED);});
            }
        });
        outer.spawn((Node { position_type:PositionType::Absolute,right:px(46),bottom:px(30),..default() },Text::new("FIELD OPERATIONS  /  0.1  ·  F11 FULLSCREEN"),TextFont {font_size:16.,..default()},TextColor(theme::MUTED)));
    });
}
