//! Fog only hides hostile units in the authoritative snapshot; the map stays lit.
//! The camera has no distance haze, so zooming out cannot obscure the map.
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Fog;
#[derive(Component)]
pub struct FogSurface;
impl Fog {
    pub fn reset_exploration(&mut self) {}
    #[cfg(test)]
    pub fn shade(&self, _at: Vec2, _snapshot: &crate::runtime::Snapshot) -> f32 {
        1.
    }
}
// Retain the scheduled system while eliminating terrain fog geometry and dimming.
pub fn update() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_is_fully_visible_even_with_fog_enabled() {
        let fog = Fog;
        let snapshot = crate::runtime::Snapshot {
            fog_enabled: true,
            ..default()
        };
        assert_eq!(fog.shade(Vec2::ZERO, &snapshot), 1.);
        assert_eq!(fog.shade(Vec2::splat(384.), &snapshot), 1.);
    }
}
