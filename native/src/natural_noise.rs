//! Stable continuous world-space variation, independent of camera and chunk boundaries.
pub fn value(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let hash = |x: i32, z: i32| {
        let mut n = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ seed;
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        (n ^ (n >> 16)) as f64 / u32::MAX as f64
    };
    let smooth = |v: f32| v * v * (3. - 2. * v);
    let (u, v) = (smooth(x - x.floor()), smooth(z - z.floor()));
    let a = hash(ix, iz) as f32;
    let b = hash(ix + 1, iz) as f32;
    let c = hash(ix, iz + 1) as f32;
    let d = hash(ix + 1, iz + 1) as f32;
    (a + (b - a) * u) * (1. - v) + (c + (d - c) * u) * v
}
