//! One fixed, deliberately laid-out proving ground. Distances are world metres.
pub const ID: &str = "forward-base";
pub const SIZE: i32 = 1024;
pub const COLS: i32 = 2048;
pub const ROWS: i32 = 1024;
pub const SCENERY_MARGIN: f32 = 2048.;
pub fn expanded(cols: i32, rows: i32) -> bool {
    cols >= 1800 && rows >= 900
}
/// Four authored sites between the bases: two hamlets and two small farmyards.
pub fn additional_sites(cols: i32, rows: i32) -> Vec<([f32; 2], bool)> {
    if !expanded(cols, rows) {
        return vec![];
    }
    [
        [-512., 65., 0.],
        [512., -60., 0.],
        [-270., -250., 1.],
        [270., 250., 1.],
    ]
    .into_iter()
    .map(|p| {
        (
            [cols as f32 * 0.5 + p[0], rows as f32 * 0.5 + p[1]],
            p[2] > 0.,
        )
    })
    .collect()
}
/// Continue the underlying rolling ground outside the playable rectangle.
pub fn scenic_height(t: &crate::terrain::Terrain, x: f32, z: f32) -> f32 {
    if t.id == crate::gis_prototype::ID {
        return t.height(x, z);
    }
    let p = [
        x.clamp(0., (t.cols - 1) as f32),
        z.clamp(0., (t.rows - 1) as f32),
    ];
    let distance = (x - p[0]).hypot(z - p[1]);
    let land = elevation(x, z, t.cols, t.rows) * t.authored_scale;
    let edge = elevation(p[0], p[1], t.cols, t.rows) * t.authored_scale;
    land + (t.height(p[0], p[1]) - edge) * (-distance / 80.).exp()
}

fn mound(x: f32, z: f32, cx: f32, cz: f32, rx: f32, rz: f32) -> f32 {
    (-((x - cx) / rx).powi(2) - ((z - cz) / rz).powi(2)).exp()
}

/// Broad ridgelines, a central lowland, and a shallow drainage basin. No random seed.
pub fn elevation(x: f32, z: f32, cols: i32, rows: i32) -> f32 {
    // Keep terrain, roads and fields in their shared authored 384-metre coordinates.
    let (x, z) = (x * 384. / cols as f32, z * 384. / rows as f32);
    let h = 0.47
        + 0.16 * mound(x, z, 110., 57., 82., 38.)
        + 0.13 * mound(x, z, 280., 327., 76., 42.)
        + 0.07 * mound(x, z, 301., 76., 50., 70.)
        - 0.045 * mound(x, z, 193., 193., 70., 142.)
        - 0.15 * mound(x, z, 253., 46., 20., 12.);
    let road = road_distance(x, z);
    // Cut gently into the land, without the old corrugated sine-wave hills.
    (h - 0.008 * (1. - road / 6.).clamp(0., 1.)).clamp(0.34, 0.8)
}

pub fn road_distance(x: f32, z: f32) -> f32 {
    // Continue the village's main road through the surrounding countryside,
    // replacing the previous oversized triangular road network.
    let p = crate::rural_layout::local([
        (x - 192.) * SIZE as f32 / 384.,
        (z - 192.) * SIZE as f32 / 384.,
    ]);
    crate::rural_layout::main_road_distance(p)
}

/// Warped, feathered cultivated edges shared by terrain blending and woodland density.
pub fn field_weight(x: f32, z: f32) -> f32 {
    let wx = x + (crate::natural_noise::value(x / 43., z / 43., 71) - 0.5) * 22.;
    let wz = z + (crate::natural_noise::value(x / 39., z / 39., 213) - 0.5) * 18.;
    let inside = (wx - 90.).min(294. - wx).min(wz - 70.).min(314. - wz);
    let t = ((inside + 9.) / 18.).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// Fixed field boundaries leave vehicle lanes and woodland on the ridges.
#[cfg(test)]
pub fn field(x: f32, z: f32) -> bool {
    (90. ..294.).contains(&x) && (70. ..314.).contains(&z)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_battlefield_has_gentle_relief_and_clear_connected_roads() {
        let mut lo = 1f32;
        let mut hi = 0f32;
        for z in 1..SIZE - 1 {
            for x in 1..SIZE - 1 {
                let h = elevation(x as f32, z as f32, SIZE, SIZE);
                lo = lo.min(h);
                hi = hi.max(h);
                assert!((h - elevation(x as f32 + 1., z as f32, SIZE, SIZE)).abs() * 48. < 0.7);
            }
        }
        assert!((hi - lo) * 48. > 7.);
        for road in crate::rural_layout::roads().into_iter().take(7) {
            for step in 0..=100 {
                let t = step as f32 / 100.;
                let x = road.a[0] + (road.b[0] - road.a[0]) * t;
                let z = road.a[1] + (road.b[1] - road.a[1]) * t;
                assert!(
                    road_distance(192. + x * 384. / SIZE as f32, 192. + z * 384. / SIZE as f32)
                        < 0.001
                );
            }
        }
    }
}
