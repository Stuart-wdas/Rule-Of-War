//! Opt-in, repeatable input/render stress. Drives the real camera control path.
use crate::{Controls, RtsCamera, landscape::Ready};
use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use serde_json::{Value, json};

#[derive(Resource, Default)]
pub struct Study {
    initialized: bool,
    warmup: Option<f64>,
    started: Option<f64>,
    pub complete: bool,
    phase: &'static str,
    previous_phase: &'static str,
    next_wheel: f64,
    outward: bool,
    min_zoom: f32,
    max_zoom: f32,
    last_center: Option<Vec3>,
    travel: f32,
    drag_events: usize,
    wheel_events: usize,
    samples: Vec<(String, f64, f32, [f32; 2])>,
}
impl Study {
    pub fn report(&self) -> Value {
        json!({"complete":self.complete,"minZoom":self.min_zoom,"maxZoom":self.max_zoom,
            "cameraTravel":self.travel,"wheelEvents":self.wheel_events,"dragFrames":self.drag_events,
            "frameSamples":self.samples.iter().map(|(phase,ms,zoom,center)|json!({"phase":phase,"ms":ms,"zoom":zoom,"center":center})).collect::<Vec<_>>()})
    }
}
pub fn enabled() -> bool {
    std::env::args().any(|a| a == "--camera-stress")
}
pub fn register(app: &mut App) {
    if enabled() {
        app.init_resource::<Study>().add_systems(
            Update,
            drive.after(crate::smoke::run).before(crate::camera_control),
        );
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects benchmark input and camera state"
)]
fn drive(
    mut study: ResMut<Study>,
    ready: Res<Ready>,
    menu: Res<crate::menu::MainMenu>,
    mut game: ResMut<crate::simulation::NativeGame>,
    mut controls: ResMut<Controls>,
    time: Res<Time<Real>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut wheel: MessageWriter<MouseWheel>,
    window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    camera: Single<&mut Transform, With<RtsCamera>>,
) {
    if menu.open || !ready.ready {
        return;
    }
    let (window_id, mut window) = window.into_inner();
    let window_size = window.size();
    window.focused = true;
    let now = time.elapsed_secs_f64();
    if !study.initialized {
        game.paused = true;
        controls.center = Vec3::new(512., game.terrain.height(512., 512.), 512.);
        controls.zoom = 1075.;
        *camera.into_inner() = crate::fixed_view::transform(controls.center, controls.zoom);
        study.initialized = true;
        study.warmup = Some(now);
        study.min_zoom = 1075.;
        study.max_zoom = 1075.;
        return;
    }
    if study.started.is_none() {
        if now - study.warmup.unwrap() < 3. {
            return;
        }
        study.started = Some(now);
        crate::performance::reset();
        crate::render_profile::reset();
    }
    let elapsed = now - study.started.unwrap();
    if !study.previous_phase.is_empty() {
        let ms = time.delta_secs_f64() * 1000.;
        let phase = study.previous_phase;
        crate::performance::record(phase, ms);
        if study.samples.len() < 20000 {
            study.samples.push((
                phase.into(),
                ms,
                controls.zoom,
                [controls.center.x, controls.center.z],
            ));
        }
        if ms >= 33.3 {
            crate::performance::event(
                json!({"stage":"camera.frame_hitch","phase":phase,"ms":ms,"zoom":controls.zoom,"center":[controls.center.x,controls.center.z],"terrainBuilds":ready.builds}),
            );
        }
    }
    let phase = if elapsed < 6. {
        "camera.full_pan"
    } else if elapsed < 14. {
        "camera.scroll"
    } else if elapsed < 22. {
        "camera.near_pan"
    } else if elapsed < 30. {
        "camera.drag"
    } else if elapsed < 42. {
        "camera.mixed"
    } else {
        "camera.settle"
    };
    let changed = study.phase != phase;
    if changed {
        crate::performance::event(
            json!({"stage":"camera.phase","phase":phase,"elapsed":elapsed,"zoom":controls.zoom,"terrainBuilds":ready.builds}),
        );
        study.phase = phase;
        study.next_wheel = now;
        if phase == "camera.near_pan" {
            controls.zoom = 65.;
        }
        if phase == "camera.drag" {
            controls.zoom = 140.;
        }
    }
    for key in [
        KeyCode::KeyW,
        KeyCode::KeyA,
        KeyCode::KeyS,
        KeyCode::KeyD,
        KeyCode::ShiftLeft,
    ] {
        keys.release(key);
    }
    if phase != "camera.drag" {
        mouse.release(MouseButton::Middle);
    }
    window.set_cursor_position(Some(Vec2::new(window_size.x * 0.52, window_size.y * 0.4)));
    match phase {
        "camera.full_pan" => {
            if elapsed % 1.5 < 0.35 {
                keys.press(if ((elapsed / 1.5) as usize).is_multiple_of(2) {
                    KeyCode::KeyD
                } else {
                    KeyCode::KeyA
                });
            }
        }
        "camera.scroll" | "camera.mixed" => {
            if controls.zoom < 42. {
                study.outward = true;
            }
            if controls.zoom > 1100. {
                study.outward = false;
            }
            if now >= study.next_wheel {
                wheel.write(MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: 0.,
                    y: if study.outward { -2. } else { 2. },
                    window: window_id,
                });
                study.wheel_events += 1;
                study.next_wheel = now + 0.075;
            }
            if phase == "camera.mixed" {
                let offset = elapsed - 30.;
                if offset % 2. < 0.6 {
                    keys.press(if ((offset / 2.) as usize).is_multiple_of(2) {
                        KeyCode::KeyD
                    } else {
                        KeyCode::KeyA
                    });
                }
            }
        }
        "camera.near_pan" => {
            let offset = elapsed - 14.;
            keys.press(
                [KeyCode::KeyD, KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS]
                    [((offset / 2.) as usize).min(3)],
            );
            keys.press(KeyCode::ShiftLeft);
        }
        "camera.drag" => {
            let offset = elapsed - 22.;
            let cycle = offset % 1.;
            let direction = if (offset as usize).is_multiple_of(2) {
                1.
            } else {
                -1.
            };
            if cycle < 0.72 {
                window.set_cursor_position(Some(Vec2::new(
                    window_size.x * 0.5 + direction * 210. * (cycle / 0.72) as f32,
                    window_size.y * 0.36 + direction * 70. * (cycle / 0.72) as f32,
                )));
                mouse.press(MouseButton::Middle);
                study.drag_events += 1;
            } else {
                mouse.release(MouseButton::Middle);
            }
        }
        _ => {}
    }
    study.min_zoom = study.min_zoom.min(controls.zoom);
    study.max_zoom = study.max_zoom.max(controls.zoom);
    if let Some(last) = study.last_center {
        study.travel += last.distance(controls.center);
    }
    study.last_center = Some(controls.center);
    study.previous_phase = phase;
    if elapsed >= 45. {
        study.complete = true;
    }
}
