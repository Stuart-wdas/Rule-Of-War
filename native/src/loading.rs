//! Map generation and file IO never block the UI or the fixed tick.
use crate::{Controls, simulation::NativeGame};
use bevy::prelude::*;
use std::{
    sync::{Mutex, mpsc},
    time::Instant,
};

pub enum Request {
    Simulation(serde_json::Value),
    Map {
        id: String,
        cols: i32,
        rows: i32,
        config: serde_json::Value,
    },
    Restore(std::path::PathBuf),
    Import(std::path::PathBuf),
    Sector(Box<NativeGame>, usize),
}
#[derive(Resource, Default)]
pub struct Loading {
    receiver: Mutex<Option<mpsc::Receiver<Result<NativeGame, String>>>>,
    pub label: String,
    pub error: Option<String>,
    started: Option<Instant>,
}
impl Loading {
    pub fn busy(&self) -> bool {
        self.receiver.lock().unwrap().is_some()
    }
    pub fn seconds(&self) -> f32 {
        self.started
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.)
    }
    pub fn begin(&mut self, request: Request, label: String) {
        if self.busy() {
            return;
        }
        self.error = None;
        self.label = label;
        self.started = Some(Instant::now());
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("operation-loader".into())
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match request {
                        Request::Simulation(config) => Ok(NativeGame::simulation(config)),
                        Request::Map {
                            id,
                            cols,
                            rows,
                            config,
                        } => Ok(NativeGame::operation(&id, cols, rows, config)),
                        Request::Restore(path) => NativeGame::load_from(&path),
                        Request::Import(path) => NativeGame::import_file(&path),
                        Request::Sector(mut game, index) => {
                            game.start_sector(index).map(|()| *game)
                        }
                    }))
                    .unwrap_or_else(|_| {
                        Err(
                        "The operation could not be prepared. Your previous operation is retained."
                            .into(),
                    )
                    });
                let _ = sender.send(result);
            });
        match worker {
            Ok(_) => *self.receiver.lock().unwrap() = Some(receiver),
            Err(e) => self.error = Some(format!("Could not start operation loading: {e}")),
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn poll(
    mut loading: ResMut<Loading>,
    mut game: ResMut<NativeGame>,
    mut controls: ResMut<Controls>,
    mut history: ResMut<crate::editor::History>,
    mut front: ResMut<crate::frontend::Frontend>,
    mut menu: ResMut<crate::menu::MainMenu>,
    mut ready: ResMut<crate::landscape::Ready>,
    mut battle: ResMut<crate::runtime::Battle>,
    mut fog: ResMut<crate::fog::Fog>,
) {
    let result = {
        let mut receiver = loading.receiver.lock().unwrap();
        match receiver.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(result)) => {
                *receiver = None;
                Some(result)
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                *receiver = None;
                Some(Err("The loading worker stopped unexpectedly.".into()))
            }
            _ => None,
        }
    };
    let Some(result) = result else {
        return;
    };
    match result {
        Ok(mut next) => {
            fog.reset_exploration();
            next.terrain.revision = game.terrain.revision.saturating_add(1);
            *game = next;
            *controls = default();
            controls.training_tools = game.training.is_some();
            *history = default();
            front.world = false;
            front.editor = false;
            front.settings = false;
            front.lobby = false;
            front.draft_config = None;
            menu.open = false;
            menu.started = true;
            ready.ready = false;
            ready.error = None;
            battle.error = None;
        }
        Err(error) => {
            game.notice = error.clone();
            loading.error = Some(error);
            menu.open = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut app = App::new();
        let mut game = NativeGame::new();
        game.tick = 37;
        app.add_plugins(MinimalPlugins)
            .insert_resource(game)
            .init_resource::<Loading>()
            .init_resource::<Controls>()
            .init_resource::<crate::editor::History>()
            .init_resource::<crate::frontend::Frontend>()
            .init_resource::<crate::menu::MainMenu>()
            .init_resource::<crate::landscape::Ready>()
            .init_resource::<crate::runtime::Battle>()
            .init_resource::<crate::fog::Fog>()
            .add_systems(Update, poll);
        app
    }
    fn finish(app: &mut App) {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while app.world().resource::<Loading>().busy() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.update();
        }
        assert!(!app.world().resource::<Loading>().busy());
    }
    #[test]
    fn failed_restore_keeps_the_existing_operation() {
        let mut app = app();
        let before = app.world().resource::<NativeGame>().entities.len();
        app.world_mut().resource_mut::<Loading>().begin(
            Request::Restore(
                std::env::temp_dir()
                    .join(format!("row-missing-{}/operation.json", std::process::id())),
            ),
            "restore".into(),
        );
        finish(&mut app);
        assert_eq!(app.world().resource::<NativeGame>().tick, 37);
        assert_eq!(app.world().resource::<NativeGame>().entities.len(), before);
        assert!(app.world().resource::<Loading>().error.is_some());
        assert!(app.world().resource::<crate::menu::MainMenu>().open);
    }
    #[test]
    fn simulation_load_and_restart_preserve_spectator_mode() {
        let mut app = app();
        let config = crate::simulation::core_config(&app.world().resource::<NativeGame>().config);
        app.world_mut()
            .resource_mut::<Loading>()
            .begin(Request::Simulation(config), "AI vs AI".into());
        finish(&mut app);
        assert!(app.world().resource::<NativeGame>().simulation);
        assert!(!app.world().resource::<NativeGame>().human_opponent);
        assert!(app.world().resource::<NativeGame>().training.is_none());
        assert!(app.world().resource::<Controls>().selected.is_empty());
        app.world_mut()
            .resource_mut::<NativeGame>()
            .command(serde_json::json!({"type":"new"}));
        let game = app.world().resource::<NativeGame>();
        assert!(game.simulation);
        assert_eq!(game.tick, 0);
        assert_eq!(game.terrain.id, crate::battlefield::ID);
    }
    #[test]
    fn completed_load_resets_controls_and_waits_for_geometry() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<Controls>()
            .selected
            .push("old-selection".into());
        let config = app.world().resource::<NativeGame>().config.clone();
        app.world_mut().resource_mut::<Loading>().begin(
            Request::Map {
                id: "grand-front".into(),
                cols: 128,
                rows: 128,
                config,
            },
            "Grand Front".into(),
        );
        finish(&mut app);
        assert_eq!(app.world().resource::<NativeGame>().terrain.cols, 128);
        assert_eq!(app.world().resource::<NativeGame>().tick, 0);
        assert!(app.world().resource::<Controls>().selected.is_empty());
        assert!(!app.world().resource::<crate::menu::MainMenu>().open);
        assert!(!app.world().resource::<crate::landscape::Ready>().ready);
    }
}
