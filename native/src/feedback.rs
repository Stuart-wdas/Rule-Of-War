//! Immediate order confirmation and short-lived battlefield effects.
use crate::{Controls, runtime::Battle, simulation::NativeGame};
use bevy::prelude::*;
#[derive(Resource, Default)]
pub struct Feedback {
    pub sequence: u64,
    pub markers: Vec<Marker>,
    pub last_click: Option<(String, f32)>,
}
pub struct Marker {
    pub at: Vec3,
    pub attack: bool,
    pub age: f32,
}
fn order_route(p: &crate::runtime::Piece) -> impl Iterator<Item = [f32; 2]> + '_ {
    // Aircraft fly outside the ground navigation grid. Their accepted active
    // destination still needs a confirmation line, even without ground nodes.
    let flight_goal = (p.category == "aircraft" && p.waypoints.is_empty())
        .then(|| p.orders.first().copied())
        .flatten();
    p.waypoints.iter().copied().chain(flight_goal)
}
impl Feedback {
    pub fn acknowledge(&mut self) {
        self.sequence = self.sequence.wrapping_add(1);
    }
    pub fn order(&mut self, at: Vec3, attack: bool) {
        self.acknowledge();
        if self.markers.len() >= 16 {
            self.markers.remove(0);
        }
        self.markers.push(Marker {
            at,
            attack,
            age: 0.,
        });
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each system parameter independently"
)]
pub fn draw(
    mut feedback: ResMut<Feedback>,
    battle: Res<Battle>,
    game: Res<NativeGame>,
    controls: Res<Controls>,
    menu: Res<crate::menu::MainMenu>,
    front: Res<crate::frontend::Frontend>,
    time: Res<Time>,
    mut gizmos: Gizmos,
    cursor: Res<crate::cursor::TacticalCursor>,
) {
    for marker in &mut feedback.markers {
        marker.age += time.delta_secs();
    }
    feedback.markers.retain(|m| m.age < 1.1);
    if menu.open || front.world || controls.hud_hidden {
        return;
    }
    let ground = |at: [f32; 2]| Vec3::new(at[0], game.terrain.height(at[0], at[1]) + 0.25, at[1]);
    let ring = |p: Vec3| Isometry3d::new(p, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    if cursor.mode == crate::cursor::Mode::Attack
        && let Some(p) = battle
            .snapshot
            .entities
            .iter()
            .find(|p| Some(&p.id) == cursor.hovered.as_ref())
    {
        gizmos.circle(
            ring(ground([p.x, p.z])),
            if p.category == "troop" {
                0.85
            } else {
                p.model_scale.max(1.)
            },
            crate::scene::RED,
        );
    }
    for m in &feedback.markers {
        let color = if m.attack {
            crate::scene::RED
        } else {
            crate::scene::BLUE
        };
        let p = ground([m.at.x, m.at.z]);
        gizmos.circle(ring(p), 0.65 + m.age * 1.8, color);
        gizmos.line(p - Vec3::X * 0.6, p + Vec3::X * 0.6, color);
        gizmos.line(p - Vec3::Z * 0.6, p + Vec3::Z * 0.6, color);
    }
    for p in &battle.snapshot.entities {
        if controls.selected.contains(&p.id) {
            let mut from = p.position() + Vec3::Y * 0.25;
            for at in order_route(p) {
                let to = ground(at);
                gizmos.line(from, to, Color::srgba(0.35, 0.70, 0.76, 0.7));
                from = to;
            }
            // Queue destinations are markers; only the planned route is drawn as a path.
            for at in &p.orders {
                gizmos.circle(ring(ground(*at)), 0.6, crate::scene::BLUE);
            }
        }
    }
    for e in &battle.snapshot.events {
        let p = ground(e.at) + Vec3::Y * 0.5;
        if e.kind == "rescue" {
            gizmos.line(p - Vec3::X, p + Vec3::X, Color::srgb(0.38, 0.82, 0.52));
            gizmos.line(p - Vec3::Y, p + Vec3::Y, Color::srgb(0.38, 0.82, 0.52));
        }
    }
    // Short contact flashes remain legible above dust and the armor deck.
    // The cap keeps a 60-round burst from growing the overlay without a bound.
    for e in battle
        .snapshot
        .events
        .iter()
        .rev()
        .filter(|e| {
            e.kind == "impact"
                && e.weapon == "plane"
                && e.tick <= battle.snapshot.tick
                && battle.snapshot.tick.saturating_sub(e.tick) <= 2
        })
        .take(4)
    {
        let age = battle.snapshot.tick.saturating_sub(e.tick) as f32 / 2.;
        let p = Vec3::new(
            e.at[0],
            e.release_height
                .unwrap_or_else(|| game.terrain.height(e.at[0], e.at[1]))
                + 0.2,
            e.at[1],
        );
        let color = Color::srgba(1., 0.85 - age * 0.3, 0.38, 1. - age * 0.8);
        for direction in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
            gizmos.line(
                p + direction * 0.25,
                p + direction * (0.85 + age * 0.8) + Vec3::Y * 0.35,
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flight_order_line_follows_only_the_current_accepted_destination() {
        let mut p = crate::runtime::Piece {
            category: "aircraft".into(),
            orders: vec![[100., 100.], [900., 900.]],
            ..default()
        };
        assert_eq!(order_route(&p).collect::<Vec<_>>(), vec![[100., 100.]]);
        p.orders = vec![[900., 900.]];
        assert_eq!(order_route(&p).collect::<Vec<_>>(), vec![[900., 900.]]);
        p.orders.clear();
        assert_eq!(order_route(&p).count(), 0);
        p.category = "vehicle".into();
        p.orders = vec![[900., 900.]];
        assert_eq!(
            order_route(&p).count(),
            0,
            "ground orders require a traversable route"
        );
        p.waypoints = vec![[100., 150.], [900., 900.]];
        assert_eq!(order_route(&p).collect::<Vec<_>>(), p.waypoints);
    }
}
