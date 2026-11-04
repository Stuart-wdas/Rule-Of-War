//! Compact field-command dock. Panels share the existing painted field hardware.
use super::*;

#[derive(Component)]
pub(crate) struct Drawer;
#[derive(Component)]
pub(crate) struct DockTab(&'static str);
#[derive(Component)]
pub(crate) struct TabArrow(&'static str);
#[derive(Component)]
pub(crate) struct Tooltip(pub String);
#[derive(Component)]
pub(crate) struct TooltipSurface;
#[derive(Component)]
pub(crate) struct TooltipText;
#[derive(Component)]
pub(crate) struct DetailScroll;
#[derive(Component)]
pub(crate) struct DrawerHeader;
#[derive(Component)]
pub(crate) struct DrawerBody;

pub(super) const DOCK_LEFT: f32 = 184.;
pub(super) const DRAWER_BOTTOM: f32 = 122.;
pub(super) const DRAWER_HEIGHT: f32 = 256.;
pub(super) const COMPACT_WIDTH: f32 = 520.;
pub(super) fn compact_orders(battle: &Battle, controls: &Controls) -> bool {
    !controls.training_tools
        && battle
            .snapshot
            .entities
            .iter()
            .any(|p| controls.selected.first() == Some(&p.id) && !p.building)
}
pub(super) fn drawer_width(size: Vec2) -> f32 {
    (size.x - DOCK_LEFT - 12.).clamp(0., 720.)
}
pub(super) fn drawer_height(size: Vec2) -> f32 {
    (size.y - DRAWER_BOTTOM - 86.).clamp(0., DRAWER_HEIGHT)
}

fn icon(icons: &crate::unit_icons::UnitIcons, kind: &str, size: f32) -> impl Bundle {
    (
        icons.image(kind),
        Node {
            width: px(size),
            height: px(size),
            min_width: px(size),
            ..default()
        },
    )
}

pub fn setup(mut commands: Commands, icons: Res<crate::unit_icons::UnitIcons>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                width: percent(100),
                height: px(44),
                padding: UiRect::axes(px(12), px(6)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                border: UiRect::bottom(px(1)),
                column_gap: px(12),
                ..default()
            },
            BorderColor::all(EDGE),
            crate::theme::Panel,
            BackgroundColor(PANEL),
            crate::menu::TacticalUi,
        ))
        .with_children(|p| {
            p.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|p| {
                p.spawn(text("RW", 23., scene::GOLD));
                p.spawn((
                    text("FIELD COMMAND", 16., crate::theme::PAPER),
                    FactionTitle,
                ));
            });
            p.spawn(Node {
                column_gap: px(6),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|p| {
                p.spawn(text("CP", 12., SUBTLE));
                for i in 0..3 {
                    p.spawn((
                        ObjectiveLight(i),
                        Node {
                            width: px(12),
                            height: px(12),
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BorderColor::all(scene::GOLD),
                        BackgroundColor(EDGE),
                    ));
                }
            });
            p.spawn((text("SUPPLY 000 / 00:00", 15., crate::theme::PAPER), Hud));
        });

    // The minimap remains available even when every command panel is closed.
    commands.spawn((
        Button, Minimap,
        Tooltip("Tactical map\nClick or drag to move the camera. Blue: friendly. Red: enemy. Gold: command post.".into()),
        Node { position_type: PositionType::Absolute, left: px(12), bottom: px(12), width: px(160), height: px(160),
            border: UiRect::all(px(1)), overflow: Overflow::clip(), ..default() },
        BorderColor::all(scene::GOLD), BackgroundColor(Color::srgb(0.15, 0.22, 0.2)), crate::menu::TacticalUi,
    ));

    commands.spawn((
        Node { position_type: PositionType::Absolute, left: px(DOCK_LEFT), right: px(12), max_width: px(720),
            bottom: px(12), height: px(48), padding: UiRect::all(px(4)), column_gap: px(4),
            border: UiRect::all(px(1)), ..default() },
        BorderColor::all(EDGE), crate::theme::Panel, BackgroundColor(PANEL), crate::menu::TacticalUi,
    )).with_children(|p| {
        for (label, kind, symbol) in [
            ("BARRACKS", "barracks", "short"),
            ("DEPOT", "vehicleDepot", "medium"),
            ("AIRFIELD", "airfield", "plane"),
            ("AC-130", "gunship", "c130"),
            ("ORDERS", "orders", "attack"),
        ] {
            let action = if kind == "orders" { Action::ToggleDrawer } else if kind == "gunship" { Action::GunshipCallIn } else { Action::Factory(kind.into()) };
            let tip = if kind == "gunship" { "AC-130 CALL-IN [J] / 60 SUPPLY\nClick a target area. Enters from your map edge, fires during five orbits, then leaves. One active gunship per side.".into() } else { format!("{label}\nClick to expand or collapse. Production uses the least busy available facility.") };
            p.spawn((Button, action, DockTab(kind), PlayerControls, crate::menu::TacticalUi, Tooltip(tip),
                Node { flex_grow: 1., flex_basis: px(0), min_width: px(78), height: percent(100), column_gap: px(5),
                    padding: UiRect::axes(px(6), px(3)), align_items: AlignItems::Center, justify_content: JustifyContent::Center,
                    border: UiRect::bottom(px(2)), ..default() },
                BorderColor::all(EDGE), BackgroundColor(Color::srgb(0.10, 0.12, 0.11)),
            )).with_children(|p| {
                p.spawn(icon(&icons, symbol, 20.));
                p.spawn(text(label, 12., crate::theme::PAPER));
                p.spawn((text("+", 14., scene::GOLD), TabArrow(kind)));
            });
        }
        p.spawn((SimulationControls, crate::menu::TacticalUi,
            Node { flex_grow: 1., align_items: AlignItems::Center, padding: UiRect::horizontal(px(8)), ..default() }))
            .with_children(|p| { p.spawn(text("WATCHING AI vs AI  /  Space: pause  /  F2: restart  /  Esc: menu", 13., crate::theme::PAPER)); });
        for (action, symbol, tip, training) in [
            (Action::BaseFocus, "home", "Base / return [Home]", false),
            (Action::Pause, "pause", "Pause / resume [Space]", false),
            (Action::TrainingTools, "engineer", "Training shop / tools [T]", true),
        ] {
            let mut button = p.spawn((Button, action, Tooltip(tip.into()),
                Node { width: px(36), min_width: px(36), height: percent(100), align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center, ..default() },
                BackgroundColor(Color::srgb(0.10, 0.12, 0.11))));
            if training { button.insert(TrainingToolsButton); }
            button.with_children(|p| { p.spawn(icon(&icons, symbol, 22.)); });
        }
    });

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(DOCK_LEFT),
            right: px(12),
            max_width: px(720),
            bottom: px(70),
            height: px(42),
            column_gap: px(4),
            overflow: Overflow::scroll_x(),
            ..default()
        },
        Roster,
        PlayerControls,
        ScrollPosition::default(),
        crate::menu::TacticalUi,
    ));

    let drawer = commands
        .spawn((
            Drawer,
            Node {
                position_type: PositionType::Absolute,
                left: px(DOCK_LEFT),
                bottom: px(DRAWER_BOTTOM),
                width: px(720),
                height: px(0),
                display: Display::None,
                overflow: Overflow::clip(),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(px(1)),
                ..default()
            },
            BorderColor::all(EDGE),
            BackgroundColor(PANEL),
            crate::theme::Panel,
        ))
        .with_children(|p| {
            p.spawn((
                DrawerHeader,
                Node {
                    height: px(34),
                    min_height: px(34),
                    padding: UiRect::axes(px(10), px(4)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|p| {
                p.spawn((text("FIELD ORDERS", 14., scene::GOLD), ActionTitle));
                p.spawn((
                    Button,
                    Action::CloseDrawer,
                    Tooltip("Collapse panel [Escape]".into()),
                    Node {
                        width: px(26),
                        height: px(26),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.10, 0.12, 0.11)),
                ))
                .with_children(|p| {
                    p.spawn(icon(&icons, "close", 18.));
                });
            });
            p.spawn((
                DrawerBody,
                Node {
                    flex_grow: 1.,
                    min_height: px(0),
                    padding: UiRect::new(px(10), px(10), px(0), px(10)),
                    column_gap: px(12),
                    ..default()
                },
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        width: px(172),
                        min_width: px(172),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(5),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    DetailScroll,
                    ScrollPosition::default(),
                ))
                .with_children(|p| {
                    p.spawn(Node {
                        min_height: px(28),
                        column_gap: px(7),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|p| {
                        p.spawn((icon(&icons, "commander", 26.), UnitPortrait));
                        p.spawn((text("FIELD OPERATIONS", 14., crate::theme::PAPER), UnitName));
                    });
                    p.spawn((
                        Node {
                            width: percent(100),
                            height: px(3),
                            min_height: px(3),
                            ..default()
                        },
                        BackgroundColor(EDGE),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            HealthFill,
                            Node {
                                width: percent(0),
                                height: percent(100),
                                ..default()
                            },
                            BackgroundColor(scene::BLUE),
                        ));
                    });
                    p.spawn((
                        text("Select a unit or a production building.", 12., SUBTLE),
                        Details,
                    ));
                });
                p.spawn((
                    Node {
                        flex_grow: 1.,
                        flex_basis: px(0),
                        min_width: px(0),
                        height: percent(100),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        align_content: AlignContent::FlexStart,
                        row_gap: px(6),
                        column_gap: px(6),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ActionPanel,
                    ScrollPosition::default(),
                ));
            });
        })
        .id();

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(51),
                right: px(12),
                height: px(22),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::menu::TacticalUi,
        ))
        .with_children(|p| {
            p.spawn((text("", 12., scene::GOLD), Status));
        });
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(1)),
            ..default()
        },
        BorderColor::all(scene::BLUE),
        BackgroundColor(Color::srgba(0.2, 0.7, 0.9, 0.12)),
        SelectionBox,
    ));
    let tooltip = commands
        .spawn((
            TooltipSurface,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                max_width: px(300),
                padding: UiRect::all(px(9)),
                border: UiRect::all(px(1)),
                ..default()
            },
            ZIndex(100),
            BorderColor::all(scene::GOLD),
            BackgroundColor(PANEL),
        ))
        .with_children(|p| {
            p.spawn((TooltipText, text("", 13., crate::theme::PAPER)));
        })
        .id();
    // Menu visibility belongs to the wrapper; each panel retains its own
    // collapsed/hover display state when the menu restores the HUD.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            crate::menu::TacticalUi,
        ))
        .add_children(&[drawer, tooltip]);
}

type DetailText<'w, 's> =
    Query<'w, 's, &'static Text, (With<Details>, Without<TabArrow>, Without<TooltipText>)>;
type DrawerGeometry<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        Option<&'static DrawerHeader>,
        Option<&'static DrawerBody>,
        Option<&'static DetailScroll>,
        Option<&'static ActionPanel>,
    ),
    (
        Without<Drawer>,
        Without<TooltipSurface>,
        Or<(
            With<DrawerHeader>,
            With<DrawerBody>,
            With<DetailScroll>,
            With<ActionPanel>,
        )>,
    ),
>;

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects independent layout and hover queries"
)]
pub fn layout(
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut controls: ResMut<Controls>,
    battle: Res<Battle>,
    menu: Res<crate::menu::MainMenu>,
    mut drawer: Single<&mut Node, (With<Drawer>, Without<TooltipSurface>)>,
    mut tabs: Query<(
        &DockTab,
        &Interaction,
        &mut BorderColor,
        &mut BackgroundColor,
    )>,
    mut arrows: Query<(&TabArrow, &mut Text), Without<TooltipText>>,
    hovered: Query<(&Interaction, &Tooltip, &ComputedNode, &UiGlobalTransform)>,
    mut tip: Single<&mut Node, (With<TooltipSurface>, Without<Drawer>)>,
    mut tip_text: Single<&mut Text, (With<TooltipText>, Without<TabArrow>)>,
    mut hover_time: Local<(String, f32)>,
    panel: Single<Option<&Children>, With<ActionPanel>>,
    details: DetailText,
    mut geometry: DrawerGeometry,
) {
    if battle.snapshot.simulation {
        controls.command_drawer = false;
    }
    let target = if controls.command_drawer { 1. } else { 0. };
    // A short constant-speed slide remains reversible even mid-animation.
    let step = time.delta_secs() / 0.18;
    controls.drawer_reveal += (target - controls.drawer_reveal).clamp(-step, step);
    let reveal = controls.drawer_reveal;
    let eased = reveal * reveal * (3. - 2. * reveal);
    let mut next = (*drawer).clone();
    let compact = compact_orders(&battle, &controls);
    controls.drawer_compact = compact;
    let width = drawer_width(window.size()).min(if compact { COMPACT_WIDTH } else { 720. });
    next.width = px(width);
    for (mut node, header, body, detail, actions) in &mut geometry {
        let mut next = node.clone();
        if header.is_some() {
            next.height = px(if compact { 26. } else { 34. });
            next.min_height = next.height;
        }
        if body.is_some() {
            let pad = if compact { 8. } else { 10. };
            next.padding = UiRect::new(px(pad), px(pad), px(0), px(pad));
            next.column_gap = px(if compact { 8. } else { 12. });
        }
        if detail.is_some() {
            next.width = px(if compact { 136. } else { 172. });
            next.min_width = next.width;
        }
        if actions.is_some() {
            next.column_gap = px(if compact { 4. } else { 6. });
            next.row_gap = next.column_gap;
        }
        node.set_if_neq(next);
    }
    let columns = ((width - if compact { 162. } else { 204. } + if compact { 4. } else { 6. })
        / if compact { 60. } else { 92. })
    .floor()
    .max(1.) as usize;
    let count = panel.as_ref().map_or(0, |children| children.len());
    let rows = count.div_ceil(columns).max(1);
    let detail_lines: usize = details
        .iter()
        .flat_map(|t| t.0.lines())
        .map(|line| line.chars().count().div_ceil(24).max(1))
        .sum();
    controls.drawer_height = if compact {
        (rows as f32 * 48. - 4. + 36.).clamp(128., 148.)
    } else {
        (rows as f32 * 88. - 6. + 44.)
            .max(detail_lines as f32 * 14. + 88.)
            .clamp(148., DRAWER_HEIGHT)
    };
    next.height = px(controls.drawer_height.min(drawer_height(window.size())) * eased);
    next.display = if reveal == 0. {
        Display::None
    } else {
        Display::Flex
    };
    drawer.set_if_neq(next);

    let selected = battle
        .snapshot
        .entities
        .iter()
        .find(|p| controls.selected.first() == Some(&p.id));
    let active_kind = if controls.training_tools {
        "training"
    } else {
        selected
            .filter(|p| ["barracks", "vehicleDepot", "airfield"].contains(&p.kind.as_str()))
            .map_or("orders", |p| p.kind.as_str())
    };
    for (tab, interaction, mut border, mut background) in &mut tabs {
        let active = controls.command_drawer && tab.0 == active_kind;
        border.set_if_neq(BorderColor::all(if active { scene::GOLD } else { EDGE }));
        background.set_if_neq(BackgroundColor(if active {
            Color::srgb(0.10, 0.24, 0.29)
        } else if *interaction == Interaction::Hovered {
            Color::srgb(0.22, 0.27, 0.24)
        } else {
            Color::srgb(0.10, 0.12, 0.11)
        }));
    }
    for (tab, mut label) in &mut arrows {
        let next = if controls.command_drawer && tab.0 == active_kind {
            "−"
        } else {
            "+"
        };
        if label.0 != next {
            label.0 = next.into();
        }
    }
    let cursor = window.cursor_position();
    let current = if menu.open || controls.hud_hidden {
        None
    } else {
        hovered
            .iter()
            .find(|(interaction, _, node, transform)| {
                **interaction == Interaction::Hovered
                    && cursor.is_some_and(|p| {
                        node.contains_point(**transform, p * window.scale_factor())
                    })
            })
            .map(|(_, tip, _, _)| tip.0.as_str())
    };
    if let Some(value) = current {
        if hover_time.0 != value {
            *hover_time = (value.into(), 0.);
        }
        hover_time.1 += time.delta_secs();
        if hover_time.1 >= 0.35
            && let Some(cursor) = cursor
        {
            if tip_text.0 != value {
                tip_text.0 = value.into();
            }
            tip.display = Display::Flex;
            tip.left = px(cursor.x.clamp(12., (window.width() - 312.).max(12.)));
            tip.top = px((cursor.y - 100.).max(80.));
            return;
        }
    } else {
        *hover_time = default();
    }
    if tip.display != Display::None {
        tip.display = Display::None;
    }
}
