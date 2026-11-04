//! A persistent operation result, with a dismissible view of the final battlefield.
use crate::{
    menu::MainMenu,
    runtime::{Battle, CommandQueue},
};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Debrief {
    pub open: bool,
    finished: bool,
}
#[derive(Component)]
pub struct ResultPanel;
#[derive(Component)]
pub struct ResultCaption;
#[derive(Component)]
pub struct ResultReturn;
#[derive(Component, Clone, Copy)]
pub enum ResultAction {
    New,
    Inspect,
    Menu,
    Show,
}

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            ResultReturn,
            Button,
            ResultAction::Show,
            GlobalZIndex(80),
            Node {
                position_type: PositionType::Absolute,
                top: px(112),
                right: px(20),
                display: Display::None,
                padding: UiRect::all(px(12)),
                ..default()
            },
            BackgroundColor(crate::theme::INK),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("BATTLE RESULTS"),
                TextFont {
                    font_size: 15.,
                    ..default()
                },
                TextColor(crate::scene::GOLD),
            ));
        });
    commands
        .spawn((
            ResultPanel,
            GlobalZIndex(90),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.025, 0.03, 0.84)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(620),
                    max_width: percent(90),
                    padding: UiRect::all(px(32)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(22),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                crate::theme::Panel,
                BorderColor::all(crate::scene::GOLD),
                BackgroundColor(crate::theme::INK),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("OPERATION COMPLETE"),
                    TextFont {
                        font_size: 17.,
                        ..default()
                    },
                    TextColor(crate::scene::GOLD),
                ));
                p.spawn((
                    ResultCaption,
                    Text::new(""),
                    TextFont {
                        font_size: 27.,
                        ..default()
                    },
                    TextColor(crate::theme::PAPER),
                ));
                for (label, action) in [
                    ("INSPECT BATTLEFIELD", ResultAction::Inspect),
                    ("NEW OPERATION [F2]", ResultAction::New),
                    ("COMMAND MENU", ResultAction::Menu),
                ] {
                    p.spawn((
                        Button,
                        action,
                        Node {
                            padding: UiRect::all(px(14)),
                            ..default()
                        },
                        crate::theme::Panel,
                        BackgroundColor(Color::srgb(0.12, 0.16, 0.16)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: 17.,
                                ..default()
                            },
                            TextColor(crate::theme::PAPER),
                        ));
                    });
                }
            });
        });
}

fn caption(s: &crate::runtime::Snapshot, owner: i32) -> String {
    let winner = s.winner.as_i64().unwrap_or(-1) as i32;
    if s.simulation {
        let title = match winner {
            0 => "FIELD COMMAND (BLUE) WINS",
            1 => "IRON FRONT (RED) WINS",
            _ => "DRAW",
        };
        let reason = if winner < 0 {
            "Both forces eliminated."
        } else if s.outcome_reason == "objectives" {
            "All command posts secured and held."
        } else {
            "Opposing forces eliminated."
        };
        return format!(
            "SIMULATION COMPLETE\n\n{title}\n{reason}\n\nDuration {:02}:{:02}\n\nF2: watch a new AI battle.",
            s.tick / 1200,
            s.tick / 20 % 60
        );
    }
    let title = if winner == owner {
        "VICTORY"
    } else if winner == 1 - owner {
        "DEFEAT"
    } else {
        "DRAW"
    };
    let secured = s.objectives.iter().filter(|o| o["owner"] == winner).count();
    let reason = if winner < 0 {
        "Both forces eliminated.".into()
    } else if s.outcome_reason == "objectives" {
        format!(
            "All {} command posts secured and held for {:.0} seconds.",
            secured,
            crate::simulation::VICTORY_HOLD_SECONDS
        )
    } else if winner == owner {
        "Enemy forces have been eliminated.".into()
    } else {
        "Your force has been eliminated.".into()
    };
    let survivors = s
        .entities
        .iter()
        .filter(|e| e.owner == owner && !e.building && e.hp > 0.)
        .map(|e| {
            1 + if crate::simulation::transport_capacity(&e.kind) > 0 {
                e.occupants
            } else {
                0
            }
        })
        .sum::<usize>();
    format!(
        "{title}\n\n{reason}\n\nDuration {:02}:{:02}  /  Your surviving units: {survivors}\n\nThe operation has ended.",
        s.tick / 1200,
        s.tick / 20 % 60
    )
}

#[expect(clippy::too_many_arguments, reason = "Bevy system parameters")]
pub fn update(
    mut state: ResMut<Debrief>,
    battle: Res<Battle>,
    session: Res<crate::network::Session>,
    menu: Res<MainMenu>,
    loading: Res<crate::loading::Loading>,
    mut panel: Single<&mut Node, With<ResultPanel>>,
    mut reopen: Single<&mut Node, (With<ResultReturn>, Without<ResultPanel>)>,
    mut text: Single<&mut Text, With<ResultCaption>>,
) {
    let ended = !battle.snapshot.winner.is_null() && battle.snapshot.training.is_none();
    if ended && !state.finished {
        state.open = true;
    }
    if !ended && state.open {
        state.open = false;
    }
    if state.finished != ended {
        state.finished = ended;
    }
    let visible = ended && !menu.open && !loading.busy();
    let panel_display = if visible && state.open {
        Display::Flex
    } else {
        Display::None
    };
    if panel.display != panel_display {
        panel.display = panel_display;
    }
    let reopen_display = if visible && !state.open {
        Display::Flex
    } else {
        Display::None
    };
    if reopen.display != reopen_display {
        reopen.display = reopen_display;
    }
    if ended {
        let next = caption(&battle.snapshot, session.owner);
        if ***text != next {
            ***text = next;
        }
    }
}

pub fn input(
    mut state: ResMut<Debrief>,
    mut menu: ResMut<MainMenu>,
    mut game: ResMut<crate::simulation::NativeGame>,
    session: Res<crate::network::Session>,
    queue: Res<CommandQueue>,
    mut controls: ResMut<crate::Controls>,
    buttons: Query<(&Interaction, &ResultAction), Changed<Interaction>>,
) {
    if game.winner.is_none() || menu.open {
        return;
    }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            ResultAction::Inspect => state.open = false,
            ResultAction::Show => state.open = true,
            ResultAction::Menu => menu.show(&mut game, session.owner),
            ResultAction::New if session.owner == 0 => {
                queue.send(serde_json::json!({"type":"new"}));
                controls.selected.clear();
                controls.groups.clear();
                controls.build = None;
                controls.base_return = None;
                controls.roster_click = None;
            }
            ResultAction::New => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simulation_results_name_the_winning_faction() {
        let mut snapshot = crate::runtime::Snapshot {
            simulation: true,
            winner: serde_json::json!(0),
            ..default()
        };
        assert!(caption(&snapshot, 0).contains("FIELD COMMAND (BLUE) WINS"));
        snapshot.winner = serde_json::json!(1);
        assert!(caption(&snapshot, 0).contains("IRON FRONT (RED) WINS"));
        snapshot.winner = serde_json::json!(-1);
        assert!(caption(&snapshot, 0).contains("DRAW"));
    }
    #[test]
    fn both_teams_see_the_correct_result_and_training_has_no_result_panel() {
        let mut app = App::new();
        let mut s = crate::runtime::Snapshot {
            winner: serde_json::json!(0),
            tick: 4800,
            ..default()
        };
        s.objectives = vec![serde_json::json!({"owner":0}); 3];
        s.outcome_reason = "objectives".into();
        assert!(caption(&s, 0).starts_with("VICTORY"));
        assert!(caption(&s, 1).starts_with("DEFEAT"));
        assert!(caption(&s, 0).contains("3 command posts"));
        // Embarked troops are represented by their carrier in a snapshot;
        // wounded survivors also remain alive when the operation ends.
        s.entities = vec![
            crate::runtime::Piece {
                kind: "troopCarrier".into(),
                owner: 0,
                hp: 1.,
                occupants: 6,
                ..default()
            },
            crate::runtime::Piece {
                kind: "medic".into(),
                owner: 0,
                hp: 0.5,
                downed: true,
                ..default()
            },
            crate::runtime::Piece {
                kind: "troopCarrier".into(),
                owner: 1,
                hp: 1.,
                occupants: 6,
                ..default()
            },
        ];
        assert!(caption(&s, 0).contains("Your surviving units: 8"));
        let mut menu = MainMenu::default();
        menu.open = false;
        app.add_plugins(MinimalPlugins)
            .insert_resource(Battle {
                snapshot: s,
                ..default()
            })
            .insert_resource(menu)
            .init_resource::<Debrief>()
            .init_resource::<crate::network::Session>()
            .init_resource::<crate::loading::Loading>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.update();
        assert!(app.world().resource::<Debrief>().open);
        let panel = app
            .world_mut()
            .query_filtered::<Entity, With<ResultPanel>>()
            .single(app.world())
            .unwrap();
        let return_button = app
            .world_mut()
            .query_filtered::<Entity, With<ResultReturn>>()
            .single(app.world())
            .unwrap();
        let caption = app
            .world_mut()
            .query_filtered::<Entity, With<ResultCaption>>()
            .single(app.world())
            .unwrap();
        let node_ticks = [panel, return_button].map(|id| {
            app.world()
                .entity(id)
                .get_ref::<Node>()
                .unwrap()
                .last_changed()
        });
        let text_tick = app
            .world()
            .entity(caption)
            .get_ref::<Text>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            [panel, return_button].map(|id| app
                .world()
                .entity(id)
                .get_ref::<Node>()
                .unwrap()
                .last_changed()),
            node_ticks
        );
        assert_eq!(
            app.world()
                .entity(caption)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            text_tick
        );
        app.world_mut().resource_mut::<Debrief>().open = false;
        app.update();
        assert!(!app.world().resource::<Debrief>().open);
        app.world_mut().resource_mut::<Battle>().snapshot.winner = serde_json::Value::Null;
        app.update();
        app.world_mut().resource_mut::<Battle>().snapshot.winner = serde_json::json!(1);
        app.update();
        assert!(app.world().resource::<Debrief>().open);
        app.world_mut().resource_mut::<Battle>().snapshot.training = Some(Default::default());
        app.update();
        assert!(!app.world().resource::<Debrief>().open);
    }
    #[test]
    fn result_buttons_inspect_reopen_and_open_the_menu_without_changing_the_outcome() {
        let mut app = App::new();
        let mut game = crate::simulation::NativeGame::new();
        game.winner = Some(0);
        let mut menu = MainMenu::default();
        menu.open = false;
        app.add_plugins(MinimalPlugins)
            .insert_resource(game)
            .insert_resource(menu)
            .init_resource::<Debrief>()
            .init_resource::<crate::network::Session>()
            .init_resource::<CommandQueue>()
            .init_resource::<crate::Controls>()
            .add_systems(Update, input);
        let button = app
            .world_mut()
            .spawn((Interaction::Pressed, ResultAction::Inspect))
            .id();
        app.world_mut().resource_mut::<Debrief>().open = true;
        app.update();
        assert!(!app.world().resource::<Debrief>().open);
        app.world_mut()
            .entity_mut(button)
            .insert((Interaction::Pressed, ResultAction::Show));
        app.update();
        assert!(app.world().resource::<Debrief>().open);
        app.world_mut()
            .entity_mut(button)
            .insert((Interaction::Pressed, ResultAction::Menu));
        app.update();
        assert!(app.world().resource::<MainMenu>().open);
        assert_eq!(
            app.world()
                .resource::<crate::simulation::NativeGame>()
                .winner,
            Some(0)
        );
        let mut s = crate::runtime::Snapshot {
            winner: serde_json::json!(1),
            ..default()
        };
        assert!(caption(&s, 0).contains("Your force"));
        s.winner = serde_json::json!(0);
        assert!(caption(&s, 0).contains("Enemy forces"));
    }
}
