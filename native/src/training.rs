//! A fixed practice range; its tools use the same units and authoritative combat.
pub const ID: &str = "training-ground";
pub const SIZE: i32 = 1024;
pub const STAGING: [f32; 2] = [84., 176.];
pub const BUILDINGS: [f32; 2] = [134., 192.];

/// Fixed practice houses between the infantry/armour and aircraft lanes.
pub fn practice_houses() -> Vec<crate::settlement_layout::Lot> {
    [
        ([-30., 0.], 0),
        ([-10., 4.], 1),
        ([10., 0.], 2),
        ([30., 4.], 3),
    ]
    .into_iter()
    .map(|(at, style)| crate::settlement_layout::Lot {
        at,
        width: crate::settlement_layout::HOUSE_PLOT,
        depth: crate::settlement_layout::HOUSE_PLOT,
        height: crate::settlement_layout::HOUSE_HEIGHT,
        yaw: 0.,
        style,
        ground: 0.,
        integrity: 1.,
    })
    .collect()
}

pub fn elevation(x: f32, z: f32) -> f32 {
    // Level firing lanes, a gentle rear hill, and a small water/bridge practice basin.
    let hill = (-((x - 246.) / 65.).powi(2) - ((z - 326.) / 36.).powi(2)).exp();
    let basin = (-((x - 316.) / 18.).powi(2) - ((z - 314.) / 13.).powi(2)).exp();
    (0.46 + 0.07 * hill - 0.19 * basin).clamp(0.25, 0.65)
}
