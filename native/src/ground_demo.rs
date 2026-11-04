//! Opt-in terrain material comparison under the ordinary game's camera and sun.
use bevy::prelude::*;
use std::sync::OnceLock;

pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::args().any(|a| a == "--ground-demo"))
}
pub fn photographic() -> bool {
    static PHOTO: OnceLock<bool> = OnceLock::new();
    *PHOTO.get_or_init(|| {
        !std::env::args().any(|a| a == "--ground-baseline" || a == "--legacy-ground-materials")
    })
}
pub fn layers(photographic: bool) -> [&'static str; 5] {
    if photographic {
        [
            "natural_meadow_v3",
            "meadow_relief_v1",
            "natural_shoulder_v3",
            "asphalt",
            "natural_stony_v3",
        ]
    } else {
        let [grass, scan, soil, asphalt] = crate::surfaces::TERRAIN_LAYERS;
        [grass, scan, soil, asphalt, grass]
    }
}

pub fn setup(
    mut game: ResMut<crate::simulation::NativeGame>,
    mut menu: ResMut<crate::menu::MainMenu>,
    mut controls: ResMut<crate::Controls>,
) {
    *game = crate::simulation::NativeGame::operation(
        crate::training::ID,
        384,
        384,
        game.config.clone(),
    );
    game.entities.clear();
    game.props.clear();
    game.bases.clear();
    game.config["fogEnabled"] = serde_json::json!(false);
    game.human_opponent = true;
    // Normal live actors supply silhouettes, physical shading and real shadows.
    game.spawn("medium", 0, [155., 180.]);
    game.spawn("troopCarrier", 0, [174., 190.]);
    game.spawn("helicopter", 0, [175., 173.]);
    if let Some(e) = game.entities.last_mut() {
        e.view.airborne = true;
        e.view.altitude = Some(9.);
    }
    game.paused = true;
    menu.open = false;
    menu.started = true;
    controls.hud_hidden = true;
    controls.center = Vec3::new(165., game.terrain.height(165., 180.), 180.);
    controls.zoom = if std::env::args().any(|a| a == "--ground-close") {
        42.
    } else {
        100.
    };
    controls.yaw = crate::fixed_view::YAW;
    controls.last_map = format!(
        "{}:{}:{}:{}",
        game.terrain.id, game.terrain.cols, game.terrain.rows, game.terrain.seed
    );
}
