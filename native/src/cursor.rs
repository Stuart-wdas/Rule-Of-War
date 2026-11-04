//! One hover rule for visible targeting, click orders and the tactical pointer.
use crate::{
    Controls,
    runtime::{Battle, Piece, Snapshot},
};
use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Hidden,
    Select,
    Move,
    Attack,
    Advance,
    Place,
    Pan,
    Inspect,
    Strafe,
}
#[derive(Resource, Default)]
pub struct TacticalCursor {
    pub mode: Mode,
    pub hovered: Option<String>,
}
#[derive(Resource)]
pub struct CursorArt(Vec<Handle<Image>>);
#[derive(Component)]
pub struct Pointer;
#[derive(Component)]
pub struct Hint;
type HintFilter = (With<Hint>, Without<Pointer>);

pub fn hostile(p: &Piece, owner: i32) -> bool {
    p.hp > 0.
        && !p.downed
        && p.owner != owner
        && p.kind != "objective"
        && (p.owner >= 0 || p.lots.iter().any(|l| l.integrity > 0.))
}
pub fn selected_units<'a>(
    s: &'a Snapshot,
    c: &'a Controls,
    owner: i32,
) -> impl Iterator<Item = &'a Piece> {
    s.entities.iter().filter(move |p| {
        p.owner == owner && !p.building && !p.downed && p.hp > 0. && c.selected.contains(&p.id)
    })
}
pub fn can_attack(s: &Snapshot, c: &Controls, catalog: &serde_json::Value, owner: i32) -> bool {
    selected_units(s, c, owner)
        .any(|p| catalog["units"][&p.kind]["damage"].as_f64().unwrap_or(0.) > 0.)
}
pub fn warthog<'a>(s: &'a Snapshot, c: &'a Controls, owner: i32) -> Option<&'a Piece> {
    selected_units(s, c, owner).find(|p| p.kind == "plane")
}
pub fn ground_order(
    s: &Snapshot,
    c: &Controls,
    catalog: &serde_json::Value,
    owner: i32,
) -> &'static str {
    if c.patrol {
        "patrol"
    } else if c.move_only {
        "move"
    } else if c.attack_move
        || selected_units(s, c, owner).any(|p| {
            p.auto_attack && catalog["units"][&p.kind]["damage"].as_f64().unwrap_or(0.) > 0.
        })
    {
        "attack-move"
    } else {
        "move"
    }
}
pub fn can_board(s: &Snapshot, c: &Controls, owner: i32, p: &Piece) -> bool {
    !c.attack_move
        && !c.patrol
        && !p.building
        && p.capacity > 0
        && p.owner == owner
        && p.hp > 0.
        && p.occupants < p.capacity
        && selected_units(s, c, owner).any(|u| u.category == "troop" && u.garrison.is_none())
}
pub fn can_garrison(s: &Snapshot, c: &Controls, owner: i32, p: &Piece) -> bool {
    !c.attack_move
        && !c.patrol
        && p.building
        && p.hp > 0.
        && p.capacity > 0
        && (p.owner == owner || p.owner < 0)
        && selected_units(s, c, owner).any(|u| u.category == "troop")
}
/// Ray-test house wings and their pitched roofs, including rotated/scaled towns.
/// Screen-space anchor radii alone miss large roofs and can pick the house behind.
fn house_hit(p: &Piece, origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
    let inverse = Quat::from_rotation_y(-p.heading);
    let scale = p.model_scale.max(0.1);
    let origin_local = inverse * (origin - p.position()) / scale;
    let direction_local = inverse * direction / scale;
    let mut nearest: Option<f32> = None;
    for lot in p.lots.iter().filter(|l| l.integrity > 0.) {
        let rotation = Quat::from_rotation_y(-lot.yaw);
        let o = rotation * (origin_local - Vec3::new(lot.at[0], 0., lot.at[1]));
        let d = rotation * direction_local;
        for wing in lot.wings() {
            let width = wing.size[0].min(wing.size[1]);
            let rise = (width * 0.38).clamp(0.8, 1.75);
            let top = lot.ground + crate::settlement_layout::HOUSE_HEIGHT + rise + 0.12;
            let low = Vec3::new(
                wing.at[0] - wing.size[0] * 0.5 - 0.24,
                lot.ground,
                wing.at[1] - wing.size[1] * 0.5 - 0.24,
            );
            let high = Vec3::new(
                wing.at[0] + wing.size[0] * 0.5 + 0.24,
                top,
                wing.at[1] + wing.size[1] * 0.5 + 0.24,
            );
            let (mut enter, mut exit) = (0_f32, f32::INFINITY);
            for axis in 0..3 {
                if d[axis].abs() < 0.000001 {
                    if o[axis] < low[axis] || o[axis] > high[axis] {
                        exit = -1.;
                        break;
                    }
                } else {
                    let a = (low[axis] - o[axis]) / d[axis];
                    let b = (high[axis] - o[axis]) / d[axis];
                    enter = enter.max(a.min(b));
                    exit = exit.min(a.max(b));
                }
            }
            if enter > exit {
                continue;
            }
            let across_axis = if wing.size[0] > wing.size[1] { 2 } else { 0 };
            let across_origin = o[across_axis]
                - if across_axis == 0 {
                    wing.at[0]
                } else {
                    wing.at[1]
                };
            let slope = rise / (width * 0.5 + 0.22);
            let roof_at_entry = top - slope * (across_origin + d[across_axis] * enter).abs();
            let mut hit = (o.y + d.y * enter <= roof_at_entry + 0.001).then_some(enter);
            for sign in [-1., 1.] {
                let denominator = d.y + sign * slope * d[across_axis];
                if denominator.abs() < 0.000001 {
                    continue;
                }
                let t = (top - o.y - sign * slope * across_origin) / denominator;
                if t >= enter && t <= exit && sign * (across_origin + d[across_axis] * t) >= -0.001
                {
                    hit = Some(hit.map_or(t, |old| old.min(t)));
                }
            }
            if let Some(t) = hit {
                nearest = Some(nearest.map_or(t, |old| old.min(t)));
            }
        }
    }
    nearest.map(|t| (t, origin + direction * t))
}
pub fn pick<'a>(
    s: &'a Snapshot,
    cursor: Vec2,
    camera: &Camera,
    transform: &GlobalTransform,
) -> Option<&'a Piece> {
    let ray = camera.viewport_to_world(transform, cursor).ok();
    let house = ray.as_ref().and_then(|ray| {
        s.entities
            .iter()
            .filter(|p| p.hp > 0. && p.building)
            .filter_map(|p| house_hit(p, ray.origin, *ray.direction).map(|(t, _)| (p, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    });
    let anchor = s
        .entities
        .iter()
        .filter(|p| p.hp > 0.)
        .filter_map(|p| {
            p.pick_positions()
                .filter_map(|at| camera.world_to_viewport(transform, at).ok())
                .map(|at| at.distance(cursor))
                .min_by(f32::total_cmp)
                .map(|d| (p, d))
        })
        .filter(|(_, d)| *d < 24.)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match (anchor, house, ray) {
        (Some((p, d)), Some((building, t)), Some(ray)) => {
            if !p.building
                && (p.position().distance(ray.origin) < t
                    || d < 12. && p.owner == s.viewer && p.garrison.is_some())
            {
                Some(p)
            } else {
                Some(building)
            }
        }
        (_, Some((p, _)), _) => Some(p),
        (anchor, _, _) => anchor.map(|(p, _)| p),
    }
}
/// Keep the house/wing under the pointer when a town contains several houses.
pub fn attack_position(
    p: &Piece,
    cursor: Vec2,
    camera: &Camera,
    transform: &GlobalTransform,
) -> Vec3 {
    if p.lots.is_empty() {
        return p.position();
    }
    if let Ok(ray) = camera.viewport_to_world(transform, cursor)
        && let Some((_, hit)) = house_hit(p, ray.origin, *ray.direction)
    {
        return hit;
    }
    p.pick_positions()
        .filter_map(|at| {
            camera
                .world_to_viewport(transform, at)
                .ok()
                .map(|screen| (at, screen.distance_squared(cursor)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
        .unwrap_or(p.position())
}
pub fn left_order(
    s: &Snapshot,
    c: &Controls,
    catalog: &serde_json::Value,
    owner: i32,
    hovered: Option<&Piece>,
) -> Option<&'static str> {
    if !c.move_only
        && hovered.is_some_and(|p| hostile(p, owner))
        && can_attack(s, c, catalog, owner)
    {
        Some("attack")
    } else if selected_units(s, c, owner).next().is_some() {
        if c.attack_move
            || hovered.is_none() && !c.move_only && !c.patrol && warthog(s, c, owner).is_some()
        {
            Some("attack-move")
        } else if c.move_only {
            Some("move")
        } else if c.patrol {
            Some("patrol")
        } else {
            None
        }
    } else {
        None
    }
}
fn mode(
    s: &Snapshot,
    c: &Controls,
    catalog: &serde_json::Value,
    owner: i32,
    hovered: Option<&Piece>,
) -> Mode {
    if c.camera_drag.as_ref().is_some_and(|g| g.active) {
        return Mode::Pan;
    }
    if c.build.is_some() {
        return Mode::Place;
    }
    if !c.move_only && !c.patrol && warthog(s, c, owner).is_some() {
        return Mode::Strafe;
    }
    if left_order(s, c, catalog, owner, hovered) == Some("attack") {
        return Mode::Attack;
    }
    if c.attack_move && selected_units(s, c, owner).next().is_some() {
        return Mode::Advance;
    }
    if c.patrol && selected_units(s, c, owner).next().is_some() {
        return Mode::Move;
    }
    if hovered.is_some() {
        return if hovered.is_some_and(|p| hostile(p, owner)) {
            Mode::Inspect
        } else {
            Mode::Select
        };
    }
    if selected_units(s, c, owner).next().is_some() {
        Mode::Move
    } else {
        Mode::Select
    }
}
fn color(mode: Mode) -> [u8; 4] {
    match mode {
        Mode::Attack => [255, 104, 83, 255],
        Mode::Place | Mode::Advance | Mode::Inspect | Mode::Strafe => [255, 199, 97, 255],
        _ => [109, 224, 239, 255],
    }
}
// Original code-drawn icons with a dark outline, so they remain clear over light terrain.
fn pixels(mode: Mode) -> Vec<u8> {
    let mut data = vec![0; 36 * 36 * 4];
    let mut segments = vec![];
    let center = Vec2::splat(16.);
    match mode {
        Mode::Strafe => {
            for i in 0..32 {
                if i % 8 < 2 {
                    continue;
                }
                let a = i as f32 * std::f32::consts::TAU / 32.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 32.;
                segments.push((
                    center + Vec2::new(a.cos() * 11., a.sin() * 8.),
                    center + Vec2::new(b.cos() * 11., b.sin() * 8.),
                ));
            }
            for x in [-5., 0., 5.] {
                segments.push((center + Vec2::new(x, -14.), center + Vec2::new(x, -10.)));
            }
            segments.extend([
                (center - Vec2::X * 4., center + Vec2::X * 4.),
                (center - Vec2::Y * 3., center + Vec2::Y * 3.),
                (center + Vec2::new(-4., 13.), center + Vec2::new(0., 9.)),
                (center + Vec2::new(4., 13.), center + Vec2::new(0., 9.)),
            ]);
        }
        Mode::Attack | Mode::Advance => {
            for i in 0..32 {
                let a = i as f32 * std::f32::consts::TAU / 32.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 32.;
                segments.push((
                    center + Vec2::new(a.cos(), a.sin()) * 9.,
                    center + Vec2::new(b.cos(), b.sin()) * 9.,
                ));
            }
            for d in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                segments.push((center + d * 6., center + d * 14.));
            }
            segments.push((center - Vec2::X * 1., center + Vec2::X * 1.));
        }
        Mode::Move => {
            for (a, b) in [
                ([16., 16.], [21., 31.]),
                ([21., 31.], [24., 25.]),
                ([24., 25.], [31., 24.]),
                ([31., 24.], [16., 16.]),
                ([7., 6.], [13., 6.]),
                ([13., 6.], [13., 12.]),
                ([6., 13.], [13., 6.]),
            ] {
                segments.push((Vec2::from(a), Vec2::from(b)));
            }
        }
        Mode::Place => {
            for (a, b) in [
                ([5., 5.], [27., 5.]),
                ([27., 5.], [27., 27.]),
                ([27., 27.], [5., 27.]),
                ([5., 27.], [5., 5.]),
                ([16., 10.], [16., 22.]),
                ([10., 16.], [22., 16.]),
            ] {
                segments.push((Vec2::from(a), Vec2::from(b)));
            }
        }
        Mode::Pan => {
            for d in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                let perpendicular = Vec2::new(-d.y, d.x);
                segments.extend([
                    (center, center + d * 12.),
                    (center + d * 12., center + d * 8. + perpendicular * 3.),
                    (center + d * 12., center + d * 8. - perpendicular * 3.),
                ]);
            }
        }
        _ => {
            for d in [
                Vec2::new(-1., -1.),
                Vec2::new(1., -1.),
                Vec2::new(1., 1.),
                Vec2::new(-1., 1.),
            ] {
                let at = center + d * 8.;
                segments.extend([
                    (at, at - Vec2::new(d.x * 5., 0.)),
                    (at, at - Vec2::new(0., d.y * 5.)),
                ]);
            }
        }
    }
    for y in 0..36 {
        for x in 0..36 {
            let at = Vec2::new(x as f32, y as f32);
            let distance = segments
                .iter()
                .map(|(a, b)| {
                    let ab = *b - *a;
                    at.distance(*a + ab * ((at - *a).dot(ab) / ab.length_squared()).clamp(0., 1.))
                })
                .fold(f32::INFINITY, f32::min);
            if distance <= 2.7 {
                let rgba = if distance <= 1.25 {
                    color(mode)
                } else {
                    [9, 16, 18, 255]
                };
                data[(y * 36 + x) * 4..(y * 36 + x) * 4 + 4].copy_from_slice(&rgba);
            }
        }
    }
    data
}
pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handles = [
        Mode::Select,
        Mode::Move,
        Mode::Attack,
        Mode::Advance,
        Mode::Place,
        Mode::Pan,
        Mode::Inspect,
        Mode::Strafe,
    ]
    .map(|m| {
        images.add(Image::new(
            bevy::render::render_resource::Extent3d {
                width: 36,
                height: 36,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            pixels(m),
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        ))
    });
    commands
        .spawn((
            Pointer,
            ImageNode::new(handles[0].clone()),
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                width: px(36),
                height: px(36),
                ..default()
            },
            GlobalZIndex(200),
            bevy::ui::FocusPolicy::Pass,
        ))
        .with_children(|p| {
            p.spawn((
                Hint,
                Text::new(""),
                TextFont {
                    font_size: 11.,
                    ..default()
                },
                TextColor(crate::theme::PAPER),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(32),
                    top: px(23),
                    width: px(190),
                    padding: UiRect::all(px(4)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.04, 0.05, 0.92)),
                bevy::ui::FocusPolicy::Pass,
            ));
        });
    commands.insert_resource(CursorArt(handles.to_vec()));
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects independent resources and UI queries"
)]
pub fn update(
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::RtsCamera>>,
    battle: Res<Battle>,
    controls: Res<Controls>,
    session: Res<crate::network::Session>,
    menu: Res<crate::menu::MainMenu>,
    front: Res<crate::frontend::Frontend>,
    loading: Res<crate::loading::Loading>,
    ready: Res<crate::landscape::Ready>,
    art: Res<CursorArt>,
    mut state: ResMut<TacticalCursor>,
    icon: Single<(&mut Node, &mut ImageNode), With<Pointer>>,
    hint: Single<(&mut Text, &mut Node), HintFilter>,
) {
    let (window, mut native) = window.into_inner();
    let (mut node, mut image) = icon.into_inner();
    let (mut text, mut hint_node) = hint.into_inner();
    let cursor = window.cursor_position();
    let show = window.focused
        && !controls.hud_hidden
        && (!crate::visual_demo::enabled() || crate::visual_demo::command_mode())
        && !menu.open
        && !loading.busy()
        && ready.ready
        && !front.editor
        && !front.world
        && !front.settings
        && !front.lobby
        && cursor.is_some_and(|p| !crate::ui::over_ui(p, window.size(), &controls));
    native.visible = !show
        && !(controls.hud_hidden
            && !menu.open
            && !front.editor
            && !front.world
            && !front.settings
            && !front.lobby
            && !loading.busy());
    node.display = if show { Display::Flex } else { Display::None };
    state.mode = Mode::Hidden;
    state.hovered = None;
    if !show {
        return;
    }
    let at = cursor.unwrap();
    let (camera, transform) = *camera;
    let hovered = pick(&battle.snapshot, at, camera, transform);
    state.mode = mode(
        &battle.snapshot,
        &controls,
        &battle.catalog,
        session.owner,
        hovered,
    );
    state.hovered = hovered.map(|p| p.id.clone());
    node.left = px(at.x - 16.);
    node.top = px(at.y - 16.);
    hint_node.left = px(if at.x > window.width() - 170. {
        -150.
    } else {
        32.
    });
    let (index, label) = match state.mode {
        Mode::Attack => (2, "ATTACK | LEFT / RIGHT CLICK"),
        Mode::Strafe => (
            7,
            if warthog(&battle.snapshot, &controls, session.owner).is_some_and(|p| p.strafe_locked)
            {
                "A-10 FINAL RUN | TARGET LOCKED"
            } else {
                "A-10 STRAFE | CLICK TARGET"
            },
        ),
        Mode::Advance => (3, "ATTACK-MOVE | CLICK"),
        Mode::Place if controls.build.as_deref() == Some("callin:c130") => {
            (7, "AC-130 | 60 SUPPLY | CLICK AREA")
        }
        Mode::Place => (4, "PLACE | LEFT CLICK"),
        Mode::Pan => (5, "PAN | DRAG"),
        Mode::Inspect => (6, "INSPECT | LEFT CLICK"),
        Mode::Move => (
            1,
            if controls.patrol {
                "PATROL | CLICK"
            } else {
                if ground_order(&battle.snapshot, &controls, &battle.catalog, session.owner)
                    == "attack-move"
                {
                    "AUTO ENGAGE | RIGHT CLICK"
                } else {
                    "MOVE ONLY | RIGHT CLICK"
                }
            },
        ),
        _ => (0, "SELECT | LEFT CLICK"),
    };
    image.image = art.0[index].clone();
    **text = label.into();
    if hovered.is_some_and(|p| can_board(&battle.snapshot, &controls, session.owner, p)) {
        **text = "BOARD CARRIER | RIGHT CLICK".into();
    } else if hovered.is_some_and(|p| can_garrison(&battle.snapshot, &controls, session.owner, p)) {
        **text = "ENTER BUILDING | RIGHT CLICK".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roof_rays_pick_the_clicked_house_in_rotated_scaled_towns_and_leave_gaps_open() {
        let lot = crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 10.,
            depth: 10.,
            height: 3.2,
            yaw: 0.4,
            style: 0,
            ground: 0.,
            integrity: 1.,
        };
        let mut next = lot.clone();
        next.at = [20., 0.];
        let p = Piece {
            x: 100.,
            z: 100.,
            model_scale: 1.3,
            heading: 0.7,
            lots: vec![lot, next],
            building: true,
            ..default()
        };
        let wing = p.lots[1].wings()[0];
        let local = p.lots[1].to_world(wing.at);
        let point = p.position()
            + Quat::from_rotation_y(p.heading) * Vec3::new(local[0], 0., local[1]) * p.model_scale;
        let (_, hit) = house_hit(&p, point + Vec3::Y * 30., Vec3::NEG_Y).unwrap();
        assert!(Vec2::new(hit.x - point.x, hit.z - point.z).length() < 0.001);
        assert!(hit.y > p.elevation + 3.2 * p.model_scale);
        let gap = p.position()
            + Quat::from_rotation_y(p.heading) * Vec3::new(10., 0., 0.) * p.model_scale;
        assert!(house_hit(&p, gap + Vec3::Y * 30., Vec3::NEG_Y).is_none());
    }
    #[test]
    fn ordinary_ground_orders_engage_and_move_only_is_explicit() {
        let mut s = Snapshot {
            entities: vec![Piece {
                id: "soldier".into(),
                kind: "short".into(),
                owner: 0,
                hp: 1.,
                ..default()
            }],
            ..default()
        };
        let catalog = crate::simulation::active_catalog();
        let mut c = Controls {
            selected: vec!["soldier".into()],
            ..default()
        };
        assert_eq!(ground_order(&s, &c, &catalog, 0), "attack-move");
        assert_eq!(left_order(&s, &c, &catalog, 0, Some(&s.entities[0])), None);
        c.move_only = true;
        assert_eq!(ground_order(&s, &c, &catalog, 0), "move");
        c.move_only = false;
        c.patrol = true;
        assert_eq!(ground_order(&s, &c, &catalog, 0), "patrol");
        c.patrol = false;
        s.entities[0].auto_attack = false;
        assert_eq!(ground_order(&s, &c, &catalog, 0), "move");
    }
    #[test]
    fn only_owned_selected_warthogs_enable_the_strafe_pointer() {
        let mut s = Snapshot {
            entities: vec![Piece {
                id: "jet".into(),
                kind: "plane".into(),
                owner: 0,
                hp: 1.,
                ..default()
            }],
            ..default()
        };
        let catalog = crate::simulation::active_catalog();
        let mut c = Controls {
            selected: vec!["jet".into()],
            ..default()
        };
        assert_eq!(mode(&s, &c, &catalog, 0, None), Mode::Strafe);
        assert_eq!(left_order(&s, &c, &catalog, 0, None), Some("attack-move"));
        assert_eq!(left_order(&s, &c, &catalog, 0, Some(&s.entities[0])), None);
        c.move_only = true;
        assert_eq!(mode(&s, &c, &catalog, 0, None), Mode::Move);
        c.move_only = false;
        s.entities[0].owner = 1;
        assert_eq!(mode(&s, &c, &catalog, 0, None), Mode::Select);
    }
    #[test]
    fn cursor_and_left_click_distinguish_orders_from_inspection() {
        let mut g = crate::simulation::NativeGame::new();
        g.config["fogEnabled"] = serde_json::json!(false);
        let mut s = g.snapshot();
        let friendly = s
            .entities
            .iter()
            .find(|p| p.owner == 0 && p.kind == "short")
            .unwrap()
            .clone();
        let enemy = s
            .entities
            .iter()
            .find(|p| p.owner == 1 && p.kind == "short")
            .unwrap()
            .clone();
        let catalog = crate::simulation::active_catalog();
        let mut c = Controls::default();
        assert_eq!(mode(&s, &c, &catalog, 0, Some(&enemy)), Mode::Inspect);
        assert_eq!(left_order(&s, &c, &catalog, 0, Some(&enemy)), None);
        c.selected = vec![friendly.id.clone()];
        assert_eq!(mode(&s, &c, &catalog, 0, Some(&enemy)), Mode::Attack);
        assert_eq!(
            left_order(&s, &c, &catalog, 0, Some(&enemy)),
            Some("attack")
        );
        assert_eq!(mode(&s, &c, &catalog, 0, None), Mode::Move);
        assert_eq!(left_order(&s, &c, &catalog, 0, None), None);
        assert_eq!(mode(&s, &c, &catalog, 0, Some(&friendly)), Mode::Select);
        c.attack_move = true;
        assert_eq!(left_order(&s, &c, &catalog, 0, None), Some("attack-move"));
        c.attack_move = false;
        c.move_only = true;
        assert_eq!(left_order(&s, &c, &catalog, 0, None), Some("move"));
        assert_eq!(left_order(&s, &c, &catalog, 0, Some(&enemy)), Some("move"));
        c.move_only = false;
        c.patrol = true;
        assert_eq!(mode(&s, &c, &catalog, 0, Some(&friendly)), Mode::Move);
        assert_eq!(
            left_order(&s, &c, &catalog, 0, Some(&friendly)),
            Some("patrol")
        );
        c.patrol = false;
        c.build = Some("range:cover:wall".into());
        assert_eq!(mode(&s, &c, &catalog, 0, Some(&enemy)), Mode::Place);
        c.build = None;
        c.attack_move = false;
        s.entities
            .iter_mut()
            .find(|p| p.id == friendly.id)
            .unwrap()
            .downed = true;
        assert_eq!(left_order(&s, &c, &catalog, 0, Some(&enemy)), None);
        assert!(!hostile(
            &Piece {
                kind: "objective".into(),
                owner: 1,
                hp: 1.,
                ..default()
            },
            0
        ));
    }
}
