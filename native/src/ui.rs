mod hud;
pub(crate) use hud::Tooltip;
pub use hud::{layout, setup};

use crate::{
    Controls,
    runtime::{Battle, CommandQueue},
    scene,
};
use bevy::{prelude::*, window::PrimaryWindow};
use serde_json::json;

#[derive(Component)]
pub struct Hud;
#[derive(Component)]
pub struct FactionTitle;
#[derive(Component)]
pub struct Details;
#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct ActionPanel;
#[derive(Component)]
pub struct ActionTitle;
#[derive(Component)]
pub struct SelectionBox;
#[derive(Component, Clone)]
pub enum Action {
    GunshipCallIn,
    AttackMode,
    Order(String),
    Select(String),
    BaseFocus,
    Recruit(String),
    Factory(String),
    ToggleDrawer,
    CloseDrawer,
    CancelProduction(String),
    Build(String),
    Pause,
    Save,
    Load,
    New,
    TrainingTools,
    TrainingTab(bool),
    TrainingBuy(String),
    TrainingPlace(String),
    TrainingCommand(String),
}
#[derive(Component)]
pub struct TrainingToolsButton;
#[derive(Component)]
pub struct PlayerControls;
#[derive(Component)]
pub struct SimulationControls;

fn text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

pub fn over_ui(point: Vec2, size: Vec2, controls: &Controls) -> bool {
    if controls.hud_hidden {
        return false;
    }
    let in_rect = |left: f32, top: f32, width: f32, height: f32| {
        width > 0.
            && height > 0.
            && Rect::from_corners(Vec2::new(left, top), Vec2::new(left + width, top + height))
                .contains(point)
    };
    let reveal = controls.drawer_reveal;
    let height =
        controls.drawer_height.min(hud::drawer_height(size)) * reveal * reveal * (3. - 2. * reveal);
    point.y < 44.
        || in_rect(12., size.y - 172., 160., 160.)
        || in_rect(hud::DOCK_LEFT, size.y - 60., hud::drawer_width(size), 48.)
        || in_rect(hud::DOCK_LEFT, size.y - 112., hud::drawer_width(size), 42.)
        || in_rect(size.x - 164., size.y - 60., 152., 48.)
        || in_rect(
            hud::DOCK_LEFT,
            size.y - hud::DRAWER_BOTTOM - height,
            hud::drawer_width(size).min(if controls.drawer_compact {
                hud::COMPACT_WIDTH
            } else {
                720.
            }),
            height,
        )
}
#[derive(Component)]
pub struct Minimap;
#[derive(Component)]
pub struct MapMarker;
#[derive(Component)]
pub struct MapUnit(String);
#[derive(Component)]
struct MapViewport;
type MinimapMarkers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static MapUnit>,
        &'static mut Node,
        Option<&'static mut BackgroundColor>,
    ),
    With<MapMarker>,
>;
#[derive(Component)]
pub struct Roster;
#[derive(Component)]
pub struct UnitName;

#[cfg(test)]
mod update_regressions {
    use super::*;
    #[test]
    fn compact_hud_only_blocks_its_visible_surfaces() {
        let mut controls = Controls::default();
        let size = Vec2::new(1920., 1080.);
        assert!(over_ui(Vec2::new(900., 20.), size, &controls));
        assert!(over_ui(Vec2::new(30., 1000.), size, &controls));
        assert!(over_ui(Vec2::new(300., 1030.), size, &controls));
        assert!(!over_ui(Vec2::new(1200., 1000.), size, &controls));
        assert!(!over_ui(Vec2::new(300., 850.), size, &controls));
        assert!(!over_ui(Vec2::new(100., 80.), size, &controls));
        controls.drawer_reveal = 1.;
        controls.drawer_height = hud::DRAWER_HEIGHT;
        assert!(over_ui(Vec2::new(300., 850.), size, &controls));
        assert!(!over_ui(Vec2::new(1000., 850.), size, &controls));
        controls.drawer_reveal = 0.5;
        assert!(!over_ui(Vec2::new(300., 750.), size, &controls));
        assert!(over_ui(Vec2::new(300., 900.), size, &controls));
        controls.hud_hidden = true;
        assert!(!over_ui(Vec2::new(30., 1000.), size, &controls));
        assert!(!over_ui(Vec2::new(300., 900.), size, &controls));
    }
    #[test]
    fn minimap_pan_and_zoom_keep_markers_and_do_not_reupload_unchanged_terrain() {
        let mut game = crate::simulation::NativeGame::new();
        game.entities.clear();
        game.config["fogEnabled"] = json!(false);
        let unit = game.spawn("short", 0, [40., 40.]);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Controls>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::frontend::Frontend>()
            .insert_resource({
                let mut menu = crate::menu::MainMenu::default();
                menu.open = false;
                menu
            })
            .insert_resource(Battle {
                snapshot: game.snapshot(),
                ..default()
            })
            .insert_resource(game)
            .add_systems(Update, minimap);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut()
            .spawn((Minimap, Node::default(), ImageNode::default()));
        app.update();
        let ids: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<MapMarker>>()
            .iter(app.world())
            .collect();
        assert_eq!(ids.len(), 2);
        let upload_tick = app.world().resource_ref::<Assets<Image>>().last_changed();
        for step in 0..40 {
            {
                let mut controls = app.world_mut().resource_mut::<Controls>();
                controls.center = Vec3::new(20. + step as f32, 0., 35.);
                controls.zoom = 30. + step as f32;
            }
            app.world_mut().resource_mut::<Battle>().snapshot.tick += 4;
            app.update();
            let current: Vec<_> = app
                .world_mut()
                .query_filtered::<Entity, With<MapMarker>>()
                .iter(app.world())
                .collect();
            assert_eq!(current, ids, "panning must not respawn minimap nodes");
            assert_eq!(
                app.world().resource_ref::<Assets<Image>>().last_changed(),
                upload_tick,
                "moving the camera must not regenerate the terrain texture"
            );
        }
        let viewport = app
            .world_mut()
            .query_filtered::<&Node, With<MapViewport>>()
            .single(app.world())
            .unwrap();
        assert_eq!(viewport.width, px(69. * 0.8 / 160. * 160.));
        app.world_mut()
            .resource_mut::<Battle>()
            .snapshot
            .entities
            .retain(|p| p.id != unit);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<MapMarker>>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn identical_hud_values_do_not_invalidate_text_layout() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Battle>()
            .init_resource::<Controls>()
            .init_resource::<crate::network::Session>()
            .add_systems(Update, update);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let selection = app.world_mut().spawn((SelectionBox, Node::default())).id();
        let hud = app.world_mut().spawn((Hud, Text::default())).id();
        let title = app.world_mut().spawn((FactionTitle, Text::default())).id();
        app.update();
        let before = [hud, title].map(|id| {
            app.world()
                .entity(id)
                .get_ref::<Text>()
                .unwrap()
                .last_changed()
        });
        let selection_tick = app
            .world()
            .entity(selection)
            .get_ref::<Node>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            [hud, title].map(|id| app
                .world()
                .entity(id)
                .get_ref::<Text>()
                .unwrap()
                .last_changed()),
            before
        );
        assert_eq!(
            app.world()
                .entity(selection)
                .get_ref::<Node>()
                .unwrap()
                .last_changed(),
            selection_tick
        );
        app.world_mut().resource_mut::<Battle>().snapshot.supply = 81.;
        app.update();
        assert!(app.world().get::<Text>(hud).unwrap().0.contains("081"));
        assert_ne!(
            app.world()
                .entity(hud)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            before[0]
        );
        assert_eq!(
            app.world()
                .entity(title)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            before[1]
        );
    }
}
#[derive(Component)]
pub struct UnitPortrait;
#[derive(Component)]
pub struct HealthFill;
#[derive(Component)]
pub struct ObjectiveLight(pub usize);
const PANEL: Color = Color::srgba(0.047, 0.059, 0.060, 0.97);
const EDGE: Color = Color::srgb(0.23, 0.26, 0.25);
const SUBTLE: Color = Color::srgb(0.61, 0.65, 0.61);
type SelectedTextData = (
    &'static mut Text,
    Option<&'static UnitName>,
    Option<&'static ActionTitle>,
);

/// Keep terse card faces readable; full descriptions and shortcuts live in tooltips.
fn action_face(
    action: &Action,
    label: &str,
    battle: &Battle,
    producer: Option<&str>,
) -> (String, String, String, &'static str) {
    let face = |icon: &str, name: &str, key| (icon.into(), name.into(), String::new(), key);
    match action {
        Action::GunshipCallIn => face("plane", "AC-130 call-in", "J"),
        Action::Recruit(kind) | Action::TrainingBuy(kind) => {
            let spec = &battle.catalog["units"][kind];
            let price = if matches!(action, Action::TrainingBuy(_)) {
                crate::simulation::training_unit_price(kind).unwrap_or(0.)
            } else {
                spec["price"].as_f64().unwrap_or(0.) as f32
            };
            let queued = battle
                .snapshot
                .jobs
                .iter()
                .filter(|j| {
                    matches!(action, Action::Recruit(_))
                        && j["type"] == kind.as_str()
                        && j["producer"].as_str() == producer
                })
                .count();
            (
                kind.clone(),
                spec["label"].as_str().unwrap_or(kind).into(),
                if queued > 0 {
                    format!("{price:.0} · Q{queued}")
                } else {
                    format!("{price:.0} supply")
                },
                "",
            )
        }
        Action::AttackMode => face("attack", "Advance", "Q"),
        Action::Order(kind) => match kind.as_str() {
            "stop" => face("stop", "Stop", "X"),
            "attack-stance" => face(
                "attack",
                if label.contains("OFF") {
                    "Fire: off"
                } else {
                    "Fire: on"
                },
                "E",
            ),
            "retreat" => face("retreat", "Retreat", "R"),
            "hold-fire" => face("hold", "Hold fire", "H"),
            "garrison" => face("garrison", "Garrison", "G"),
            "load" => face("garrison", "Load", "L"),
            "unload" => face("unload", "Unload", "U"),
            _ => face("attack", label, ""),
        },
        Action::Build(kind) => {
            let spec = &battle.catalog["buildings"][kind];
            let name = match kind.as_str() {
                "vehicleDepot" => "Depot",
                "fieldHospital" => "Hospital",
                "machineGunner" => "MG nest",
                _ => spec["label"].as_str().unwrap_or(kind),
            };
            let price = if kind == "bridge" {
                format!("{}/m", spec["pricePerMeter"])
            } else if kind == "machineGunner" {
                "16 supply".into()
            } else {
                format!("{} supply", spec["price"])
            };
            ("build".into(), name.into(), price, "")
        }
        Action::CancelProduction(_) => face("close", "Cancel last", ""),
        Action::Pause => face("pause", "Pause", "Space"),
        Action::Save => face("save", "Save", "F5"),
        Action::Load => face("load", "Load", "F9"),
        Action::New => face("home", "New battle", "F2"),
        Action::TrainingTab(true) => face("short", "Buy units", ""),
        Action::TrainingTab(false) => face("engineer", "Test tools", ""),
        Action::TrainingTools => face("attack", "Unit orders", "T"),
        Action::TrainingPlace(kind) => {
            let (category, item) = kind.split_once(':').unwrap_or(("", kind));
            let name = match item {
                "short" => "Infantry",
                "medium" => "Armour",
                "helicopter" => "Aircraft",
                "sandbags" => "Sandbags",
                "wall" => "Wall",
                _ => "Crates",
            };
            (
                if category == "target" { item } else { "build" }.into(),
                name.into(),
                if category == "target" {
                    "Target"
                } else {
                    "Cover"
                }
                .into(),
                "",
            )
        }
        Action::TrainingCommand(kind) => face(
            match kind.as_str() {
                "heal" => "medic",
                "wound" => "attack",
                "refill" => "load",
                _ => "scout",
            },
            match kind.as_str() {
                "reset-targets" => "Reset targets",
                "refill" => "Refill",
                "clear-targets" => "Clear targets",
                "heal" => "Heal",
                "wound" => "Wound",
                "fog" => {
                    if label.contains("ON") {
                        "Fog: on"
                    } else {
                        "Fog: off"
                    }
                }
                "live-fire" => {
                    if label.contains("ON") {
                        "Fire: on"
                    } else {
                        "Fire: off"
                    }
                }
                _ => label,
            },
            "",
        ),
        _ => face("commander", label, ""),
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects independent UI queries"
)]
pub fn selected_frame(
    battle: Res<Battle>,
    controls: Res<Controls>,
    icons: Res<crate::unit_icons::UnitIcons>,
    mut portraits: Query<&mut ImageNode, With<UnitPortrait>>,
    mut names: Query<SelectedTextData>,
    mut health: Query<(&mut Node, &mut BackgroundColor), With<HealthFill>>,
    mut objectives: Query<(&ObjectiveLight, &mut BackgroundColor), Without<HealthFill>>,
    mut range_buttons: Query<&mut Node, (With<TrainingToolsButton>, Without<HealthFill>)>,
) {
    let _timing = crate::performance::span("ui.selected_frame");
    for mut node in &mut range_buttons {
        let display = if battle.snapshot.training.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    let p = battle
        .snapshot
        .entities
        .iter()
        .find(|p| controls.selected.first() == Some(&p.id));
    for mut image in &mut portraits {
        let next = icons.image(p.map(|p| p.kind.as_str()).unwrap_or("commander"));
        if image.image != next.image {
            *image = next;
        }
    }
    for (mut text, name, action_title) in &mut names {
        if action_title.is_some() {
            let value = if controls.training_tools {
                if controls.training_shop {
                    "TRAINING / UNIT SHOP"
                } else {
                    "TRAINING / TEST TOOLS"
                }
                .into()
            } else {
                p.filter(|p| ["barracks", "vehicleDepot", "airfield"].contains(&p.kind.as_str()))
                    .map(|p| {
                        format!(
                            "{} / QUEUE {}/5",
                            p.label.to_uppercase(),
                            battle
                                .snapshot
                                .jobs
                                .iter()
                                .filter(|j| j["producer"].as_str() == Some(&p.id))
                                .count()
                        )
                    })
                    .unwrap_or("FIELD ORDERS".into())
            };
            if **text != value {
                **text = value;
            }
        }
        if name.is_some() {
            let value = p
                .map(|p| p.label.to_uppercase())
                .unwrap_or("FIELD OPERATIONS".into());
            if **text != value {
                **text = value;
            }
        }
    }
    for (mut node, mut color) in &mut health {
        let value = p.map(|p| p.hp / p.max_hp.max(1.)).unwrap_or(0.);
        let width = percent(value * 100.);
        if node.width != width {
            node.width = width;
        }
        color.set_if_neq(BackgroundColor(if value < 0.3 {
            scene::RED
        } else {
            scene::BLUE
        }));
    }
    for (light, mut color) in &mut objectives {
        let next = match battle
            .snapshot
            .objectives
            .get(light.0)
            .and_then(|o| o["owner"].as_i64())
        {
            Some(0) => scene::BLUE,
            Some(1) => scene::RED,
            _ => EDGE,
        };
        color.set_if_neq(BackgroundColor(next));
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects UI and input resources"
)]
pub fn rebuild_actions(
    mut commands: Commands,
    mut panel: Single<(Entity, &mut ScrollPosition), With<ActionPanel>>,
    battle: Res<Battle>,
    icons: Res<crate::unit_icons::UnitIcons>,
    controls: Res<Controls>,
    front: Res<crate::frontend::Frontend>,
    session: Res<crate::network::Session>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut last: Local<String>,
    mut context: Local<(String, bool, bool)>,
) {
    let _timing = crate::performance::span("ui.rebuild_actions");
    let selected = battle
        .snapshot
        .entities
        .iter()
        .find(|p| controls.selected.first() == Some(&p.id));
    let key = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        selected.map(|p| p.id.as_str()).unwrap_or(""),
        selected.map(|p| p.owner).unwrap_or(-1),
        battle.catalog.is_null(),
        front.world,
        front.editor,
        front.settings,
        front.lobby,
        session.owner
    );
    let key = format!(
        "{key}:{}:{}:{:?}:{}",
        controls.training_tools,
        controls.training_shop,
        battle.snapshot.training.as_ref().map(|t| t.live_fire),
        battle.snapshot.fog_enabled
    );
    let key = format!(
        "{key}:{:.0}:{:?}:{}",
        battle.snapshot.supply,
        battle
            .snapshot
            .jobs
            .iter()
            .map(|j| j["id"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        battle.snapshot.entities.len()
    );
    let key = format!("{key}:{}", selected.is_none_or(|p| p.auto_attack));
    // Keep pressed buttons alive until release, so a queue refresh cannot turn
    // one held click into several purchases.
    if mouse.pressed(MouseButton::Left) || *last == key {
        return;
    }
    *last = key;
    // Supply and queue updates retain the user's scroll position.
    let next_context = (
        selected.map_or("", |p| p.id.as_str()).to_owned(),
        controls.training_tools,
        controls.training_shop,
    );
    if *context != next_context {
        panel.1.0.y = 0.;
        *context = next_context;
    }
    commands.entity(panel.0).despawn_children();
    let mut actions: Vec<(String, Action)> = vec![];
    if let Some(p) = selected.filter(|p| {
        p.owner == session.owner && !front.world && !front.editor && !front.settings && !front.lobby
    }) {
        if p.building || p.kind == "aircraftCarrier" {
            if let Some(units) = battle.catalog["units"].as_object() {
                for (kind, spec) in units {
                    if !spec["price"].is_number() || spec["recruitable"] == false {
                        continue;
                    }
                    let compatible = spec["productionBuildings"]
                        .as_array()
                        .map(|a| a.iter().any(|v| v.as_str() == Some(&p.kind)))
                        .unwrap_or_else(|| {
                            spec["productionBuilding"].as_str().unwrap_or("barracks") == p.kind
                        });
                    if compatible {
                        actions.push((
                            format!(
                                "{} / {} SUPPLY / Q{}{}",
                                spec["label"].as_str().unwrap_or(kind),
                                spec["price"],
                                battle
                                    .snapshot
                                    .jobs
                                    .iter()
                                    .filter(|j| j["producer"].as_str() == Some(&p.id)
                                        && j["type"] == kind.as_str())
                                    .count(),
                                match spec["infantryClass"].as_str() {
                                    Some("sniper") => "\nSCOPED RIFLE / LONG RANGE",
                                    Some("assault") => "\nRIFLE / FRONT LINE",
                                    Some("medic") => "\nHEAL / RESCUE",
                                    Some("support") => "\nRIFLE + AA LAUNCHER",
                                    _ => "",
                                }
                            ),
                            Action::Recruit(kind.clone()),
                        ));
                    }
                }
            }
            if let Some(job) = battle
                .snapshot
                .jobs
                .iter()
                .rev()
                .find(|j| j["producer"].as_str() == Some(&p.id))
            {
                actions.push((
                    "CANCEL LAST / REFUND SUPPLY".into(),
                    Action::CancelProduction(job["id"].as_str().unwrap_or("").into()),
                ));
            }
        } else if p.kind != "c130" {
            actions.push(("ATTACK / ADVANCE [Q]".into(), Action::AttackMode));
            if !p.building && p.capacity > 0 {
                actions.push((
                    format!("LOAD TROOPS [L] / {}/{}", p.occupants, p.capacity),
                    Action::Order("load".into()),
                ));
                actions.push(("UNLOAD TROOPS [U]".into(), Action::Order("unload".into())));
            }
            for (label, kind) in [
                ("STOP / CEASE FIRE [X]", "stop"),
                ("AUTO ATTACK [E]", "attack-stance"),
                ("RETREAT [R]", "retreat"),
                ("HOLD FIRE [H]", "hold-fire"),
                ("ENTER NEARBY BUILDING [G]", "garrison"),
                ("LEAVE BUILDING / UNLOAD [U]", "unload"),
            ] {
                if !(p.capacity > 0 && kind == "unload") {
                    let label = if kind == "attack-stance" {
                        if p.auto_attack {
                            "AUTO ATTACK: ON [E]"
                        } else {
                            "AUTO ATTACK: OFF [E]"
                        }
                    } else {
                        label
                    };
                    actions.push((label.into(), Action::Order(kind.into())));
                }
            }
            if ["commander", "engineer"].contains(&p.kind.as_str())
                && let Some(buildings) = battle.catalog["buildings"].as_object()
            {
                for (kind, spec) in buildings {
                    if ![
                        "mine",
                        "barracks",
                        "vehicleDepot",
                        "airfield",
                        "fieldHospital",
                        "watchtower",
                        "bridge",
                    ]
                    .contains(&kind.as_str())
                    {
                        continue;
                    }
                    if spec["commanderOnly"] == true && p.kind != "commander" {
                        continue;
                    }
                    if spec["engineerOnly"] == true && p.kind != "engineer" {
                        continue;
                    }
                    actions.push((
                        format!(
                            "+ {} | {}",
                            spec["label"].as_str().unwrap_or(kind),
                            if kind == "bridge" {
                                format!("{}/m", spec["pricePerMeter"])
                            } else {
                                spec["price"].to_string()
                            }
                        ),
                        Action::Build(kind.clone()),
                    ));
                }
                if p.kind == "engineer" {
                    actions.push((
                        "+ MACHINE GUN NEST | 16".into(),
                        Action::Build("machineGunner".into()),
                    ));
                }
            }
        }
    }
    if session.owner == 0 && selected.is_none() {
        actions.extend([
            ("PAUSE / RESUME [Space]".into(), Action::Pause),
            ("SAVE [F5]".into(), Action::Save),
            ("LOAD [F9]".into(), Action::Load),
            ("NEW OPERATION [F2]".into(), Action::New),
        ]);
    }
    if let Some(training) = &battle.snapshot.training
        && controls.training_tools
    {
        actions = vec![
            ("BUY UNITS".into(), Action::TrainingTab(true)),
            ("TEST TOOLS".into(), Action::TrainingTab(false)),
            ("UNIT ORDERS [T]".into(), Action::TrainingTools),
        ];
        if controls.training_shop {
            if let Some(units) = battle.catalog["units"].as_object() {
                for kind in [
                    "short",
                    "medium",
                    "scout",
                    "medic",
                    "engineer",
                    "commander",
                    "long",
                    "xtralong",
                    "antiAir",
                    "troopCarrier",
                    "huey",
                    "helicopter",
                    "plane",
                    "machineGunner",
                ] {
                    let Some(spec) = units.get(kind) else {
                        continue;
                    };
                    if let Some(price) = crate::simulation::training_unit_price(kind) {
                        actions.push((
                            format!(
                                "BUY {} | {price:.0} supply",
                                spec["label"].as_str().unwrap_or(kind)
                            ),
                            Action::TrainingBuy(kind.into()),
                        ));
                    }
                }
            }
        } else {
            for (label, action) in [
                ("RESET TARGETS", "reset-targets"),
                (
                    if training.live_fire {
                        "RETURN FIRE: ON"
                    } else {
                        "RETURN FIRE: OFF"
                    },
                    "live-fire",
                ),
                ("REFILL SUPPLY", "refill"),
            ] {
                actions.push((label.into(), Action::TrainingCommand(action.into())));
            }
            for (label, kind) in [
                ("PLACE INFANTRY TARGET", "target:short"),
                ("PLACE ARMOUR TARGET", "target:medium"),
                ("PLACE AIR TARGET", "target:helicopter"),
                ("PLACE SANDBAGS", "cover:sandbags"),
                ("PLACE WALL", "cover:wall"),
                ("PLACE CRATES", "cover:crates"),
            ] {
                actions.push((label.into(), Action::TrainingPlace(kind.into())));
            }
            for (label, action) in [
                ("CLEAR TARGETS", "clear-targets"),
                ("HEAL SELECTED", "heal"),
                ("WOUND SELECTED", "wound"),
                (
                    if battle.snapshot.fog_enabled {
                        "FOG: ON"
                    } else {
                        "FOG: OFF"
                    },
                    "fog",
                ),
            ] {
                actions.push((label.into(), Action::TrainingCommand(action.into())));
            }
            actions.push(("RESET WHOLE RANGE [F2]".into(), Action::New));
        }
    }
    if session.multiplayer() {
        actions.retain(|(_, a)| !matches!(a, Action::Load));
    }
    if front.world || front.editor || front.settings || front.lobby {
        actions.retain(|(_, action)| matches!(action, Action::Save | Action::Load));
    }
    commands.entity(panel.0).with_children(|parent| {
        let compact = hud::compact_orders(&battle, &controls);
        for (label, action) in actions {
            let (symbol, caption, meta, hotkey) = action_face(
                &action,
                &label,
                &battle,
                controls.selected.first().map(String::as_str),
            );
            let affordable = match &action {
                Action::Recruit(kind) | Action::TrainingBuy(kind) => battle.catalog["units"][kind]
                    ["price"]
                    .as_f64()
                    .is_none_or(|price| price <= battle.snapshot.supply as f64),
                _ => true,
            };
            parent
                .spawn((
                    Button,
                    action,
                    hud::Tooltip(label),
                    Node {
                        width: px(if compact { 56. } else { 86. }),
                        min_width: px(if compact { 56. } else { 86. }),
                        height: px(if compact { 44. } else { 82. }),
                        padding: UiRect::all(px(if compact { 3. } else { 5. })),
                        border: UiRect::all(px(1)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        row_gap: px(2),
                        ..default()
                    },
                    BorderColor::all(EDGE),
                    BackgroundColor(Color::srgb(0.10, 0.12, 0.11)),
                ))
                .with_children(|p| {
                    p.spawn((
                        icons.image(&symbol),
                        Node {
                            width: px(if compact { 18. } else { 26. }),
                            height: px(if compact { 18. } else { 26. }),
                            min_width: px(if compact { 18. } else { 26. }),
                            ..default()
                        },
                    ));
                    p.spawn(text(
                        &caption,
                        if compact { 10. } else { 12. },
                        crate::theme::PAPER,
                    ));
                    if !meta.is_empty() && !compact {
                        p.spawn(text(
                            &meta,
                            12.,
                            if affordable { scene::GOLD } else { scene::RED },
                        ));
                    }
                    if !hotkey.is_empty() {
                        p.spawn((
                            text(hotkey, if compact { 8. } else { 10. }, SUBTLE),
                            Node {
                                position_type: PositionType::Absolute,
                                top: px(3),
                                right: px(5),
                                ..default()
                            },
                        ));
                    }
                });
        }
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects UI, simulation and feedback resources"
)]
pub fn buttons(
    mut query: Query<(&Interaction, &Action, &mut BackgroundColor), Changed<Interaction>>,
    front: Res<crate::frontend::Frontend>,
    battle: Res<Battle>,
    session: Res<crate::network::Session>,
    queue: Option<Res<CommandQueue>>,
    mut controls: ResMut<Controls>,
    menu: Res<crate::menu::MainMenu>,
    loading: Res<crate::loading::Loading>,
    mut feedback: ResMut<crate::feedback::Feedback>,
    time: Res<Time<Real>>,
) {
    // World selection and keyboard shortcuts open the same drawer as dock tabs.
    if controls.selected != controls.drawer_selection {
        controls.command_drawer = !controls.selected.is_empty() || controls.training_tools;
        controls.drawer_selection = controls.selected.clone();
    }
    if controls.training_tools != controls.drawer_training {
        controls.command_drawer = controls.training_tools || !controls.selected.is_empty();
        controls.drawer_training = controls.training_tools;
    }
    if menu.open || loading.busy() || controls.hud_hidden {
        return;
    }
    let Some(queue) = queue else {
        return;
    };
    for (interaction, action, mut color) in &mut query {
        *color = BackgroundColor(if *interaction == Interaction::Hovered {
            Color::srgb(0.22, 0.27, 0.24)
        } else {
            Color::srgb(0.10, 0.12, 0.11)
        });
        if *interaction != Interaction::Pressed {
            continue;
        }
        if battle.snapshot.simulation
            && !matches!(
                action,
                Action::Pause | Action::Save | Action::Load | Action::New | Action::BaseFocus
            )
        {
            continue;
        }
        if !battle.snapshot.winner.is_null()
            && !matches!(
                action,
                Action::Save | Action::Load | Action::New | Action::BaseFocus | Action::Select(_)
            )
        {
            continue;
        }
        if (front.world || front.editor || front.settings || front.lobby)
            && !matches!(action, Action::Save | Action::Load)
        {
            continue;
        }
        match action {
            Action::GunshipCallIn => {
                controls.build = Some("callin:c130".into());
                controls.attack_move = false;
                controls.patrol = false;
                controls.move_only = false;
                controls.drag = None;
                controls.command_drawer = false;
            }
            Action::Factory(kind) => {
                let already_selected = battle.snapshot.entities.iter().any(|p| {
                    controls.selected.first() == Some(&p.id)
                        && p.owner == session.owner
                        && &p.kind == kind
                });
                if already_selected {
                    controls.command_drawer = !controls.command_drawer || controls.training_tools;
                    controls.training_tools = false;
                    continue;
                }
                let factory = battle
                    .snapshot
                    .entities
                    .iter()
                    .filter(|p| p.owner == session.owner && p.hp > 0. && &p.kind == kind)
                    .min_by_key(|p| {
                        battle
                            .snapshot
                            .jobs
                            .iter()
                            .filter(|j| j["producer"].as_str() == Some(&p.id))
                            .count()
                    });
                if let Some(p) = factory {
                    controls.selected = vec![p.id.clone()];
                    controls.training_tools = false;
                    controls.build = None;
                    controls.attack_move = false;
                    controls.command_drawer = true;
                    feedback.acknowledge();
                }
            }
            Action::ToggleDrawer => {
                let production = battle
                    .snapshot
                    .entities
                    .iter()
                    .any(|p| controls.selected.first() == Some(&p.id) && p.building);
                if production {
                    controls.selected.clear();
                }
                controls.command_drawer =
                    !controls.command_drawer || production || controls.training_tools;
                controls.training_tools = false;
            }
            Action::CloseDrawer => {
                controls.command_drawer = false;
            }
            Action::CancelProduction(id) => {
                queue.send(json!({"type":"cancel-production","jobId":id}))
            }
            Action::AttackMode => {
                controls.move_only = false;
                controls.attack_move = true;
                controls.patrol = false;
                controls.build = None;
            }
            Action::TrainingTools => {
                controls.training_tools = !controls.training_tools || !controls.command_drawer;
                controls.command_drawer = controls.training_tools || !controls.selected.is_empty();
                controls.build = None;
            }
            Action::TrainingTab(shop) => {
                controls.training_shop = *shop;
                controls.build = None;
            }
            Action::TrainingBuy(kind) => {
                queue.send(json!({"type":"training","action":"buy","kind":kind}));
                feedback.acknowledge();
            }
            Action::TrainingPlace(kind) => {
                controls.build = Some(format!("range:{kind}"));
                controls.attack_move = false;
                controls.patrol = false;
                controls.bridge_start = None;
                controls.command_drawer = false;
            }
            Action::TrainingCommand(action) => {
                queue.send(json!({"type":"training","action":action,"unitIds":controls.selected}));
            }
            Action::Order(kind) => {
                if kind == "attack-stance" {
                    controls.move_only = false;
                }
                queue.order(&controls.selected, kind, None, None, false);
                if !controls.selected.is_empty() {
                    feedback.acknowledge();
                }
            }
            Action::Select(id) => {
                if let Some(p) = battle
                    .snapshot
                    .entities
                    .iter()
                    .find(|p| p.id == *id && p.owner == session.owner && p.hp > 0.)
                {
                    controls.select_card(id, p.position(), time.elapsed_secs_f64());
                    controls.command_drawer = true;
                    controls.training_tools = false;
                }
            }
            Action::BaseFocus => {
                if let Some(base) = battle
                    .snapshot
                    .bases
                    .iter()
                    .find(|b| b.owner == session.owner)
                {
                    controls.toggle_base(Vec3::new(base.origin[0], 0., base.origin[1]));
                } else if let Some(p) = battle
                    .snapshot
                    .entities
                    .iter()
                    .find(|p| p.owner == session.owner && p.building)
                {
                    controls.toggle_base(p.position());
                }
            }
            Action::Recruit(kind) => {
                queue.send(json!({"type":"recruit","unitType":kind,"producerId":controls.selected.first()}));
                feedback.acknowledge();
            }
            Action::Build(kind) => {
                controls.build = Some(kind.clone());
                controls.bridge_start = None;
                controls.attack_move = false;
                controls.patrol = false;
                controls.command_drawer = false;
            }
            Action::Pause => queue.send(json!({"type":"pause"})),
            Action::Save => queue.send(json!({"type":"save"})),
            Action::Load | Action::New => {
                queue.send(json!({"type":if matches!(action,Action::Load){"load"}else{"new"}}));
                controls.selected.clear();
                controls.groups.clear();
                controls.base_return = None;
                controls.roster_click = None;
                controls.build = None;
            }
        }
    }
    // A deliberate collapse must survive the next selection synchronization.
    controls.drawer_selection = controls.selected.clone();
    controls.drawer_training = controls.training_tools;
}

type HudTextData = (
    &'static mut Text,
    Option<&'static Hud>,
    Option<&'static Details>,
    Option<&'static Status>,
    Option<&'static FactionTitle>,
);
type ScrollPanels<'w, 's> = Query<
    'w,
    's,
    (
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static mut ScrollPosition,
        Option<&'static Roster>,
    ),
    Or<(With<ActionPanel>, With<Roster>, With<hud::DetailScroll>)>,
>;

pub fn scroll_actions(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut panels: ScrollPanels,
    menu: Res<crate::menu::MainMenu>,
    controls: Res<Controls>,
) {
    for event in wheel.read() {
        if menu.open || controls.hud_hidden {
            continue;
        }
        for (node, transform, mut scroll, roster) in &mut panels {
            if !window
                .cursor_position()
                .is_some_and(|p| node.contains_point(*transform, p * window.scale_factor()))
                || (roster.is_none() && (!controls.command_drawer || controls.drawer_reveal < 1.))
            {
                continue;
            }
            let unit = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                32.
            } else {
                1.
            };
            if roster.is_some() {
                scroll.0.x = (scroll.0.x - (event.y + event.x) * unit).max(0.);
            } else {
                scroll.0.y = (scroll.0.y - event.y * unit).max(0.);
            }
        }
    }
}
pub fn update(
    battle: Res<Battle>,
    controls: Res<Controls>,
    window: Single<&Window, With<PrimaryWindow>>,
    session: Res<crate::network::Session>,
    mut texts: Query<HudTextData>,
    mut selection: Single<&mut Node, With<SelectionBox>>,
) {
    let _timing = crate::performance::span("ui.update");
    let s = &battle.snapshot;
    for (mut t, hud, details, status, title) in &mut texts {
        let next_text = if title.is_some() {
            if s.simulation {
                "SIMULATION / AI vs AI"
            } else if session.owner == 0 {
                "FIELD COMMAND"
            } else {
                "IRON FRONT"
            }
            .to_owned()
        } else if hud.is_some() {
            if s.simulation {
                format!(
                    "BLUE {:.0} / RED {:.0} SUPPLY   /   {:02}:{:02}   /   HOLD {:.0}s : {:.0}s {}",
                    s.team_supply[0],
                    s.team_supply[1],
                    s.tick / 1200,
                    s.tick / 20 % 60,
                    s.objective_hold[0],
                    s.objective_hold[1],
                    if s.paused { "PAUSED" } else { "" }
                )
            } else if let Some(training) = &s.training {
                format!(
                    "SUPPLY {:.0}   /   {} SHOTS · {} HITS · {:.0} DMG {}",
                    s.supply,
                    training.shots,
                    training.hits,
                    training.damage,
                    if s.paused { "PAUSED" } else { "" }
                )
            } else {
                format!(
                    "SUPPLY {:03.0}   /   {:02}:{:02}   /   {} {}",
                    s.supply,
                    s.tick / 1200,
                    s.tick / 20 % 60,
                    if s.tick < crate::simulation::DEPLOYMENT_TICKS {
                        format!(
                            "DEPLOY {}s",
                            (crate::simulation::DEPLOYMENT_TICKS - s.tick).div_ceil(20)
                        )
                    } else {
                        format!(
                            "{} HOLD {:.0}/{:.0}s",
                            if s.objective_hold[1 - session.owner as usize] > 0. {
                                "ENEMY"
                            } else {
                                "YOUR"
                            },
                            s.objective_hold[0].max(s.objective_hold[1]),
                            crate::simulation::VICTORY_HOLD_SECONDS
                        )
                    },
                    if s.paused { "PAUSED" } else { "" }
                )
            }
        } else if details.is_some() {
            let p = s
                .entities
                .iter()
                .find(|p| controls.selected.first() == Some(&p.id));
            let mut info = p
                .map(|p| {
                    format!(
                        "HP {:.0}/{:.0} · SIGHT {:.0}m\n{} selected · {}",
                        p.hp,
                        p.max_hp,
                        p.sight,
                        controls.selected.len(),
                        if p.category == "troop" {
                            "1 SOLDIER"
                        } else {
                            "CREW READY"
                        }
                    )
                })
                .unwrap_or("Choose a building below to recruit.\n\nSelect a unit for orders.\n\nEsc: menu · F10: hide HUD\nF5: save · F9: load".into());
            if let Some(post) = p.filter(|p| p.kind == "objective") {
                let posts: Vec<_> = s
                    .entities
                    .iter()
                    .filter(|p| p.kind == "objective")
                    .collect();
                if let Some(state) = posts
                    .iter()
                    .position(|p| p.id == post.id)
                    .and_then(|i| s.objectives.get(i))
                {
                    info.push_str(&if state["locked"].as_bool().unwrap_or(false) {
                        "\nDEPLOYMENT: POSTS LOCKED".into()
                    } else {
                        format!("\nSECURING {:.0}% / 25s UNCONTESTED\nInfantry captures; armour denies.\nEnemy posts: neutralize, then secure.",state["progress"].as_f64().unwrap_or(0.)*100.)
                    });
                }
            }
            if let Some(p) = p.filter(|p| p.category == "troop") {
                info.push_str(&format!(
                    "\n{} · {}",
                    match p.cover {
                        2 => "HARD COVER",
                        1 => "LIGHT COVER",
                        _ => "EXPOSED",
                    },
                    if p.downed {
                        "AWAITING MEDIC"
                    } else if p.activity == "stopped" {
                        "STOPPED / WEAPONS SAFE"
                    } else if p.retreating {
                        "RETREATING"
                    } else if p.suppression >= 0.8 {
                        "PINNED"
                    } else if p.suppression >= 0.45 {
                        "SUPPRESSED"
                    } else if p.activity == "treating" {
                        "TREATING WOUNDED"
                    } else if p.activity == "seeking-cover" {
                        "SEEKING COVER"
                    } else if p.activity == "in-cover" {
                        "HOLDING COVER"
                    } else if p.activity == "shooting" {
                        "FIRING"
                    } else {
                        "READY"
                    }
                ));
            }
            if let Some(p) = p {
                if !p.building && p.capacity > 0 {
                    info.push_str(&format!("\nPASSENGERS {}/{}", p.occupants, p.capacity));
                }
                if !p.tactical_role.is_empty() {
                    info.push_str(&format!(
                        "\n{}",
                        p.tactical_role.replace('-', " ").to_uppercase()
                    ));
                }
                if p.kind == "engineer" {
                    info.push_str(&format!(
                        "\nAA 160m / {}",
                        if p.aa_elevated {
                            "LOCKING / LAUNCHING".to_owned()
                        } else if p.aa_reload > 0. {
                            format!("RELOAD {:.0}s", p.aa_reload)
                        } else {
                            "READY".to_owned()
                        }
                    ));
                }
                if p.kind == "scout" {
                    info.push_str("\nRIFLE 110m / RELOAD 4.5s");
                }
                if p.category == "aircraft" {
                    if p.kind == "c130" {
                        info.push_str(&format!("\n{} / {} OF 5 ORBITS\n30 mm: 1.2s / 105 mm: 3.8s\nALTITUDE {:.0} m\nAUTOMATIC DEPARTURE", p.activity.to_uppercase(), p.gunship_laps, p.flight_height()));
                    } else {
                        info.push_str(&format!(
                            "\nFUEL {:.0}% | {}\nALTITUDE {:.0} m",
                            p.fuel_fraction * 100.,
                            p.activity.replace('-', " ").to_uppercase(),
                            p.flight_height()
                        ));
                    }
                    info.push_str(&format!(
                        "\nFLARES {}/3 / {}",
                        p.flare_salvos,
                        if p.flare_cooldown > 0. {
                            format!("{:.0}s", p.flare_cooldown)
                        } else if p.flare_salvos == 0 {
                            "EMPTY".into()
                        } else {
                            "READY".into()
                        }
                    ));
                    if p.kind == "plane" {
                        info.push_str(&format!("\nCANNON {}/60", p.cannon_ammo));
                        if p.strafe_target.is_some() {
                            info.push_str(if p.strafe_locked {
                                " / TARGET LOCKED"
                            } else {
                                " / RETARGET AVAILABLE"
                            });
                        }
                    } else if p.kind == "helicopter" {
                        info.push_str("\nMG 85m / ROCKETS 120m");
                    } else if p.kind == "huey" {
                        info.push_str("\nTROOP TRANSPORT / L LOAD / U LAND & UNLOAD");
                    }
                }
                if p.building && p.capacity > 0 {
                    info.push_str(&format!("\nWINDOWS {}/{}", p.occupants, p.capacity));
                }
                if let Some(g) = &p.garrison {
                    info.push_str(&format!(
                        "\n{} / WINDOW {}",
                        if g.entered { "IN BUILDING" } else { "ENTERING" },
                        g.window + 1
                    ));
                }
                if let Some(squad) = p.squad {
                    info.push_str(&format!(
                        "\nSQUAD {} / Ctrl+1-9 assign, 1-9 select",
                        squad + 1
                    ));
                }
                if p.activity == "falling-back" {
                    info.push_str("\nFALLING BACK UNDER PRESSURE");
                }
            }
            for j in s
                .jobs
                .iter()
                .filter(|j| p.is_none_or(|p| !p.building || j["producer"].as_str() == Some(&p.id)))
                .take(3)
            {
                let kind = j["type"].as_str().unwrap_or("Production");
                let label = battle.catalog["units"][kind]["label"]
                    .as_str()
                    .or_else(|| battle.catalog["buildings"][kind]["label"].as_str())
                    .unwrap_or(kind);
                info.push_str(&format!(
                    "\n{}: {}",
                    label.to_uppercase(),
                    if j["queued"] == true {
                        "WAITING".into()
                    } else {
                        format!("{:.0}%", j["progress"].as_f64().unwrap_or(0.) * 100.)
                    }
                ));
            }
            info
        } else if status.is_some() {
            if let Some(error) = &battle.error {
                error.clone()
            } else if !s.winner.is_null() {
                (if s.simulation {
                    match s.winner.as_i64() {
                        Some(0) => "FIELD COMMAND (BLUE) WINS",
                        Some(1) => "IRON FRONT (RED) WINS",
                        _ => "DRAW",
                    }
                } else if s.winner == session.owner {
                    "VICTORY"
                } else if s.winner == (1 - session.owner) {
                    "DEFEAT"
                } else {
                    "DRAW"
                })
                .to_string()
            } else if let Some(kind) = &controls.build {
                if let Some(item) = kind.strip_prefix("range:") {
                    format!(
                        "PLACE {}: left-click clear ground. Shift-click repeats. Right-click or Escape cancels.",
                        item.replace(':', " ")
                    )
                } else if kind == "callin:c130" {
                    "AC-130 / 60 SUPPLY: click target area for five firing orbits. Right-click or Escape cancels.".into()
                } else if kind == "bridge" {
                    "BRIDGE: click the first dry bank, then the opposite bank. Right click cancels."
                        .into()
                } else {
                    format!(
                        "Place {kind} near your commander or support soldier. Right click to cancel."
                    )
                }
            } else {
                if controls.patrol {
                    "PATROL: click a destination".into()
                } else if controls.attack_move {
                    "ATTACK-MOVE: click ground or an enemy".into()
                } else if controls.move_only {
                    "MOVE ONLY: click a destination".into()
                } else {
                    s.notice.clone()
                }
            }
        } else {
            continue;
        };
        if **t != next_text {
            **t = next_text;
        }
    }
    if let (Some(start), Some(end)) = (controls.drag, window.cursor_position())
        && !controls.hud_hidden
    {
        if selection.display != Display::Flex {
            selection.display = Display::Flex;
        }
        let (left, top, width, height) = (
            px(start.x.min(end.x)),
            px(start.y.min(end.y)),
            px((start.x - end.x).abs()),
            px((start.y - end.y).abs()),
        );
        if selection.left != left {
            selection.left = left;
        }
        if selection.top != top {
            selection.top = top;
        }
        if selection.width != width {
            selection.width = width;
        }
        if selection.height != height {
            selection.height = height;
        }
    } else if selection.display != Display::None {
        selection.display = Display::None;
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn minimap(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Res<crate::simulation::NativeGame>,
    battle: Res<Battle>,
    mut controls: ResMut<Controls>,
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    map: Single<Entity, With<Minimap>>,
    mut markers: MinimapMarkers,
    menu: Res<crate::menu::MainMenu>,
    front: Res<crate::frontend::Frontend>,
    mut last: Local<String>,
    mut texture: Local<Option<Handle<Image>>>,
) {
    let _timing = crate::performance::span("ui.minimap");
    if menu.open || front.world || front.lobby || front.settings || controls.hud_hidden {
        return;
    }
    let size = 160.;
    if mouse.pressed(MouseButton::Left)
        && let Some(p) = window.cursor_position()
        && (12. ..172.).contains(&p.x)
        && (window.height() - 172. ..window.height() - 12.).contains(&p.y)
    {
        if mouse.just_pressed(MouseButton::Left) {
            controls.camera_revision = controls.camera_revision.wrapping_add(1);
        }
        controls.center = Vec3::new(
            (p.x - 12.) / size * game.terrain.cols as f32,
            0.,
            (p.y - (window.height() - 172.)) / size * game.terrain.rows as f32,
        );
    }
    let key = format!(
        "{}:{}:{}:{}:{}",
        game.terrain.id,
        game.terrain.cols,
        game.terrain.rows,
        game.terrain.seed,
        game.terrain.revision
    );
    // Fog no longer shades the map. Only actual terrain changes require new pixels.
    if *last != key {
        let mut pixels = Vec::with_capacity(64 * 64 * 4);
        for z in 0..64 {
            for x in 0..64 {
                let c = game.terrain.cell(
                    x as f32 / 63. * (game.terrain.cols - 1) as f32,
                    z as f32 / 63. * (game.terrain.rows - 1) as f32,
                );
                let color = if c.water() {
                    [41, 77, 90, 255]
                } else {
                    let shade = (c.elevation * 50.) as u8;
                    [70 + shade, 86 + shade, 57 + shade, 255]
                };
                pixels.extend(color);
            }
        }
        let image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            pixels,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        if let Some(handle) = texture.as_ref() {
            if let Some(current) = images.get_mut(handle) {
                *current = image;
            }
        } else {
            let handle = images.add(image);
            commands.entity(*map).insert(ImageNode::new(handle.clone()));
            *texture = Some(handle);
        }
        *last = key;
    }
    let pieces: std::collections::HashMap<_, _> = battle
        .snapshot
        .entities
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let unit_node = |p: &crate::runtime::Piece| Node {
        position_type: PositionType::Absolute,
        left: px((p.x / game.terrain.cols as f32 * size - 2.).clamp(0., size - 4.)),
        top: px((p.z / game.terrain.rows as f32 * size - 2.).clamp(0., size - 4.)),
        width: px(if p.kind == "objective" { 6. } else { 4. }),
        height: px(if p.kind == "objective" { 6. } else { 4. }),
        ..default()
    };
    let unit_color = |p: &crate::runtime::Piece| {
        BackgroundColor(if p.kind == "objective" {
            scene::GOLD
        } else if p.owner == 0 {
            scene::BLUE
        } else {
            scene::RED
        })
    };
    let w = (controls.zoom * 0.8 / game.terrain.cols as f32 * size).clamp(8., size);
    let h = (controls.zoom * 0.55 / game.terrain.rows as f32 * size).clamp(8., size);
    let viewport_node = Node {
        position_type: PositionType::Absolute,
        left: px(
            (controls.center.x / game.terrain.cols as f32 * size - w * 0.5).clamp(0., size - w),
        ),
        top: px(
            (controls.center.z / game.terrain.rows as f32 * size - h * 0.5).clamp(0., size - h),
        ),
        width: px(w),
        height: px(h),
        border: UiRect::all(px(1)),
        ..default()
    };
    let mut present = std::collections::HashSet::new();
    let mut viewport_present = false;
    // Camera movement touches only the retained viewport rectangle. Unit marker
    // identities survive panning, zooming, ticks and capture colour changes.
    for (entity, unit, mut node, color) in &mut markers {
        if let Some(unit) = unit {
            if let Some(piece) = pieces.get(unit.0.as_str()) {
                node.set_if_neq(unit_node(piece));
                if let Some(mut color) = color {
                    color.set_if_neq(unit_color(piece));
                }
                present.insert(unit.0.as_str());
            } else {
                commands.entity(entity).despawn();
            }
        } else {
            node.set_if_neq(viewport_node.clone());
            viewport_present = true;
        }
    }
    commands.entity(*map).with_children(|root| {
        for p in battle
            .snapshot
            .entities
            .iter()
            .filter(|p| !present.contains(p.id.as_str()))
        {
            root.spawn((
                MapMarker,
                MapUnit(p.id.clone()),
                unit_node(p),
                unit_color(p),
            ));
        }
        if !viewport_present {
            root.spawn((
                MapMarker,
                MapViewport,
                viewport_node,
                BorderColor::all(Color::srgb(0.86, 0.90, 0.82)),
            ));
        }
    });
}
pub fn roster(
    mut commands: Commands,
    root: Single<Entity, With<Roster>>,
    battle: Res<Battle>,
    icons: Res<crate::unit_icons::UnitIcons>,
    controls: Res<Controls>,
    session: Res<crate::network::Session>,
    mut last: Local<String>,
) {
    let _timing = crate::performance::span("ui.roster");
    let units: Vec<_> = battle
        .snapshot
        .entities
        .iter()
        .filter(|p| p.owner == session.owner && !p.building)
        .collect();
    let key = units
        .iter()
        .map(|p| {
            format!(
                "{}:{:.1}:{}:{}:{}:{}",
                p.id,
                p.hp,
                controls.selected.contains(&p.id),
                p.downed,
                (p.suppression * 4.) as i32,
                p.retreating
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    if *last == key {
        return;
    }
    *last = key;
    commands.entity(*root).despawn_children();
    commands.entity(*root).with_children(|root| {
        for p in units {
            let selected = controls.selected.contains(&p.id);
            let color = if p.downed || p.suppression >= 0.8 {
                scene::RED
            } else if p.suppression >= 0.45 {
                scene::GOLD
            } else if selected {
                scene::BLUE
            } else {
                Color::srgb(0.20, 0.29, 0.30)
            };
            root.spawn((
                Button,
                Action::Select(p.id.clone()),
                hud::Tooltip(format!(
                    "{} · {:.0}% HP\n{}\nClick to select. Double-click to focus.",
                    p.label,
                    p.hp / p.max_hp.max(1.) * 100.,
                    if p.downed {
                        "MEDIC NEEDED"
                    } else if p.retreating {
                        "RETREATING"
                    } else if p.suppression >= 0.8 {
                        "PINNED"
                    } else if p.suppression >= 0.45 {
                        "SUPPRESSED"
                    } else {
                        "READY"
                    }
                )),
                Node {
                    width: px(44),
                    min_width: px(44),
                    height: px(42),
                    padding: UiRect::all(px(5)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.045, 0.075, 0.085, 0.94)),
                BorderColor::all(color),
            ))
            .with_children(|root| {
                root.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    height: px(25),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        icons.image(&p.kind),
                        Node {
                            width: px(25),
                            height: px(25),
                            min_width: px(25),
                            ..default()
                        },
                    ));
                });
                root.spawn((
                    Node {
                        height: px(3),
                        width: percent(100),
                        ..default()
                    },
                    BackgroundColor(EDGE),
                ))
                .with_children(|pbar| {
                    pbar.spawn((
                        Node {
                            height: percent(100),
                            width: percent((p.hp / p.max_hp.max(1.) * 100.).clamp(0., 100.)),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                });
            });
        }
    });
}

#[cfg(test)]
mod training_ui_tests {
    use super::*;
    #[test]
    fn factory_shortcut_selects_owned_depot_and_queues_compatible_units() {
        let mut app = App::new();
        let game = crate::simulation::NativeGame::default();
        let depot = game
            .entities
            .iter()
            .find(|e| e.view.owner == 0 && e.view.kind == "vehicleDepot")
            .unwrap()
            .view
            .id
            .clone();
        let mut menu = crate::menu::MainMenu::default();
        menu.open = false;
        let battle = Battle {
            snapshot: game.snapshot(),
            catalog: crate::simulation::active_catalog(),
            ..default()
        };
        app.add_plugins(MinimalPlugins)
            .insert_resource(game)
            .insert_resource(battle)
            .init_resource::<crate::runtime::CommandQueue>()
            .init_resource::<crate::network::Session>()
            .init_resource::<Controls>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::loading::Loading>()
            .init_resource::<crate::feedback::Feedback>()
            .insert_resource(menu)
            .add_systems(Update, (buttons, crate::runtime::receive).chain());
        let tab = app
            .world_mut()
            .spawn((
                Interaction::Pressed,
                Action::Factory("vehicleDepot".into()),
                BackgroundColor(Color::BLACK),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<Controls>().selected,
            vec![depot.clone()]
        );
        assert!(app.world().resource::<Controls>().command_drawer);
        app.world_mut().entity_mut(tab).insert(Interaction::None);
        app.update();
        app.world_mut().entity_mut(tab).insert(Interaction::Pressed);
        app.update();
        assert!(!app.world().resource::<Controls>().command_drawer);
        assert_eq!(
            app.world().resource::<Controls>().selected,
            vec![depot.clone()]
        );
        app.world_mut().entity_mut(tab).insert(Interaction::None);
        app.update();
        app.update();
        assert!(
            !app.world().resource::<Controls>().command_drawer,
            "collapsed selections must stay collapsed"
        );
        app.world_mut().entity_mut(tab).insert(Interaction::Pressed);
        app.update();
        assert!(app.world().resource::<Controls>().command_drawer);
        for _ in 0..2 {
            app.world_mut().spawn((
                Interaction::Pressed,
                Action::Recruit("medium".into()),
                BackgroundColor(Color::BLACK),
            ));
        }
        app.update();
        let snapshot = app
            .world()
            .resource::<crate::simulation::NativeGame>()
            .snapshot();
        assert_eq!(snapshot.jobs.len(), 2);
        assert!(
            snapshot
                .jobs
                .iter()
                .all(|j| j["type"] == "medium" && j["producer"] == depot)
        );
        assert_eq!(snapshot.jobs[1]["queued"], true);
        let job = snapshot.jobs[1]["id"].as_str().unwrap().to_string();
        let game = &mut *app
            .world_mut()
            .resource_mut::<crate::simulation::NativeGame>();
        let supply = game.supply[0];
        game.command_as(1, json!({"type":"cancel-production","jobId":job}));
        assert_eq!(game.snapshot().jobs.len(), 2);
        game.command_as(0, json!({"type":"cancel-production","jobId":job}));
        assert_eq!(game.snapshot().jobs.len(), 1);
        assert_eq!(game.supply[0], supply + 20.);
    }
    #[test]
    fn shop_button_purchases_and_selects_a_real_unit() {
        let mut app = App::new();
        let mut config = crate::simulation::core_config(&serde_json::Value::Null);
        config["fogEnabled"] = json!(false);
        let game = crate::simulation::NativeGame::operation(crate::training::ID, 384, 384, config);
        let mut main_menu = crate::menu::MainMenu::default();
        main_menu.open = false;
        app.add_plugins(MinimalPlugins)
            .insert_resource(game)
            .init_resource::<crate::runtime::CommandQueue>()
            .init_resource::<crate::runtime::Battle>()
            .init_resource::<crate::network::Session>()
            .init_resource::<Controls>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::loading::Loading>()
            .init_resource::<crate::feedback::Feedback>()
            .insert_resource(main_menu)
            .add_systems(Update, (buttons, crate::runtime::receive).chain());
        app.world_mut().spawn((
            Interaction::Pressed,
            Action::TrainingBuy("medium".into()),
            BackgroundColor(Color::BLACK),
        ));
        app.update();
        let game = app.world().resource::<crate::simulation::NativeGame>();
        assert!(
            game.entities
                .iter()
                .any(|e| e.view.kind == "medium" && e.view.owner == 0)
        );
        let controls = app.world().resource::<Controls>();
        assert_eq!(controls.selected.len(), 1);
        assert_eq!(
            game.training.as_ref().unwrap().last_purchase.as_ref(),
            controls.selected.first()
        );
    }
}
