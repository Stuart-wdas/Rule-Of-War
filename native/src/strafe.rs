//! World-space cannon pipper: the inner circle uses the weapon's actual spread and splash.
use crate::{
    Controls, cursor,
    runtime::{Battle, Piece, Snapshot},
    simulation::NativeGame,
};
use bevy::{prelude::*, window::PrimaryWindow};

#[derive(Resource, Default)]
pub struct Overlay {
    pub previews: usize,
    pub missions: usize,
}

fn footprint(catalog: &serde_json::Value) -> f32 {
    crate::simulation::A10_SPREAD_RADIUS
        + catalog["units"]["plane"]["damageRadius"]
            .as_f64()
            .unwrap_or(1.1) as f32
}

fn approach(s: &Snapshot, plane: &Piece, target: Vec3) -> f32 {
    let base = s
        .entities
        .iter()
        .find(|p| plane.home_airfield.as_ref() == Some(&p.id))
        .or_else(|| {
            s.entities
                .iter()
                .find(|p| p.kind == "airfield" && p.owner == plane.owner)
        });
    let origin = base
        .map(Piece::position)
        .unwrap_or_else(|| plane.position());
    (target.z - origin.z).atan2(target.x - origin.x)
}

fn pipper(gizmos: &mut Gizmos, at: Vec3, heading: f32, radius: f32, color: Color) {
    let forward = Vec3::new(heading.cos(), 0., heading.sin());
    let right = Vec3::new(-forward.z, 0., forward.x);
    gizmos.circle(
        Isometry3d::new(at, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        radius,
        color,
    );
    for sign in [-1., 1.] {
        gizmos.line(at + right * sign * 0.5, at + right * sign * 1.5, color);
        gizmos.line(at + forward * sign * 0.5, at + forward * sign * 1.5, color);
        let corner = at + right * sign * (radius + 1.8);
        gizmos.line(corner - forward * 2., corner + forward * 2., color);
        gizmos.line(
            corner + forward * 2.,
            corner + forward * 2. - right * sign,
            color,
        );
        gizmos.line(
            corner - forward * 2.,
            corner - forward * 2. - right * sign,
            color,
        );
    }
    // Approach arrow ends outside the hit circle; it is not a second damage corridor.
    let tip = at - forward * (radius + 2.);
    gizmos.line(at - forward * (radius + 13.), tip, color);
    gizmos.line(tip, tip - forward * 2. + right * 1.5, color);
    gizmos.line(tip, tip - forward * 2. - right * 1.5, color);
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects independent overlay resources"
)]
pub fn draw(
    battle: Res<Battle>,
    game: Res<NativeGame>,
    controls: Res<Controls>,
    session: Res<crate::network::Session>,
    cursor: Res<cursor::TacticalCursor>,
    menu: Res<crate::menu::MainMenu>,
    front: Res<crate::frontend::Frontend>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::RtsCamera>>,
    mut overlay: ResMut<Overlay>,
    mut gizmos: Gizmos,
) {
    overlay.previews = 0;
    overlay.missions = 0;
    if menu.open
        || front.world
        || front.settings
        || front.lobby
        || front.editor
        || controls.hud_hidden
    {
        return;
    }
    let s = &battle.snapshot;
    for p in s
        .entities
        .iter()
        .filter(|p| p.owner == session.owner && p.gunship_target.is_some())
    {
        let target = p.gunship_target.unwrap();
        let at = Vec3::new(
            target[0],
            game.terrain.height(target[0], target[1]) + 0.35,
            target[1],
        );
        gizmos.circle(
            Isometry3d::new(at, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            crate::simulation::GUNSHIP_AREA,
            Color::srgba(1., 0.64, 0.18, 0.8),
        );
    }
    if controls.build.as_deref() == Some("callin:c130") {
        if let Some(at) = controls.ground {
            pipper(
                &mut gizmos,
                at + Vec3::Y * 0.3,
                0.,
                crate::simulation::GUNSHIP_AREA,
                Color::srgb(1., 0.72, 0.23),
            );
        }
        return;
    }
    let radius = footprint(&battle.catalog);
    let surface = |at: Vec3, owner| {
        at.with_y(
            crate::runtime::cannon_contact_height(
                s.entities.iter(),
                [at.x, at.z],
                owner,
                game.terrain.height(at.x, at.z),
            ) + 0.28,
        )
    };
    for plane in cursor::selected_units(s, &controls, session.owner)
        .filter(|p| p.kind == "plane")
        .take(8)
    {
        if let Some(target) = plane.strafe_target {
            let at = surface(Vec3::new(target[0], 0., target[1]), plane.owner);
            pipper(
                &mut gizmos,
                at,
                plane
                    .strafe_heading
                    .unwrap_or_else(|| approach(s, plane, at)),
                radius,
                if plane.strafe_locked {
                    Color::srgb(1., 0.65, 0.19)
                } else {
                    Color::srgb(0.25, 0.92, 1.)
                },
            );
            overlay.missions += 1;
        }
    }
    if cursor.mode != cursor::Mode::Strafe {
        return;
    }
    let Some(plane) = cursor::warthog(s, &controls, session.owner) else {
        return;
    };
    let Some(mut at) = controls.ground else {
        return;
    };
    if let Some(p) = s
        .entities
        .iter()
        .find(|p| Some(&p.id) == cursor.hovered.as_ref())
    {
        if !cursor::hostile(p, session.owner) {
            return;
        }
        if let Some(pos) = window.cursor_position() {
            at = cursor::attack_position(p, pos, camera.0, camera.1);
        }
    } else if let Some(p) = s
        .entities
        .iter()
        .filter(|p| {
            p.owner >= 0
                && p.owner != session.owner
                && !p.airborne
                && p.hp > 0.
                && !p.downed
                && p.kind != "objective"
                && Vec2::new(p.x - at.x, p.z - at.z).length() < 35.
        })
        .min_by(|a, b| {
            Vec2::new(a.x - at.x, a.z - at.z)
                .length_squared()
                .total_cmp(&Vec2::new(b.x - at.x, b.z - at.z).length_squared())
        })
    {
        // Ground attack orders acquire a visible enemy within 35 m at launch.
        at = p.position();
    }
    at = surface(at, plane.owner);
    pipper(
        &mut gizmos,
        at,
        approach(s, plane, at),
        radius,
        Color::srgb(1., 0.72, 0.23),
    );
    overlay.previews = 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_circle_includes_maximum_round_spread_and_damage_radius() {
        let catalog = crate::simulation::active_catalog();
        assert!((footprint(&catalog) - 2.7).abs() < 0.001);
    }
    #[test]
    fn preview_approach_uses_the_assigned_airfield() {
        let plane = Piece {
            home_airfield: Some("home".into()),
            owner: 0,
            ..default()
        };
        let s = Snapshot {
            entities: vec![Piece {
                id: "home".into(),
                kind: "airfield".into(),
                x: 10.,
                z: 20.,
                ..default()
            }],
            ..default()
        };
        assert!(
            (approach(&s, &plane, Vec3::new(10., 0., 50.)) - std::f32::consts::FRAC_PI_2).abs()
                < 0.001
        );
    }
}
