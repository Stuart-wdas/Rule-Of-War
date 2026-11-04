// CPU-only samples shared by both terrain LODs. Every tile edge is on the same
// coarse grid: height, normal, wetness and material weights all agree there.
#[derive(Clone, Copy)]
struct SurfaceSample {
    height: f32,
    normal: Vec3,
    wet: f32,
    color: Vec4,
}
impl SurfaceSample {
    fn at(t: &Terrain, x: f32, z: f32) -> Self {
        let wet = wet(t, x, z);
        Self {
            height: visual_height(t, x, z),
            wet,
            color: Vec4::from_array(color(t, x, z, wet)),
            normal: Vec3::new(
                visual_height(t, x - 0.5, z) - visual_height(t, x + 0.5, z),
                1.,
                visual_height(t, x, z - 0.5) - visual_height(t, x, z + 0.5),
            )
            .normalize(),
        }
    }
    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            height: self.height.lerp(other.height, t),
            normal: self.normal.lerp(other.normal, t),
            wet: self.wet.lerp(other.wet, t),
            color: self.color.lerp(other.color, t),
        }
    }
}
struct SampleGrid<T> {
    points: Vec<T>,
    bounds: [i32; 4],
    step: f32,
    nx: usize,
    nz: usize,
}
impl<T: Copy> SampleGrid<T> {
    fn new(bounds: [i32; 4], step: f32, mut sample: impl FnMut(f32, f32) -> T) -> Self {
        let nx = ((bounds[1] - bounds[0]) as f32 / step).ceil() as usize;
        let nz = ((bounds[3] - bounds[2]) as f32 / step).ceil() as usize;
        let mut points = Vec::with_capacity((nx + 1) * (nz + 1));
        for z in 0..=nz {
            for x in 0..=nx {
                points.push(sample(
                    (bounds[0] as f32 + x as f32 * step).min(bounds[1] as f32),
                    (bounds[2] as f32 + z as f32 * step).min(bounds[3] as f32),
                ));
            }
        }
        Self {
            points,
            bounds,
            step,
            nx,
            nz,
        }
    }
    fn sample(&self, x: f32, z: f32, lerp: fn(T, T, f32) -> T) -> T {
        let ix = (((x - self.bounds[0] as f32) / self.step).floor() as usize)
            .min(self.nx.saturating_sub(1));
        let iz = (((z - self.bounds[2] as f32) / self.step).floor() as usize)
            .min(self.nz.saturating_sub(1));
        let x0 = self.bounds[0] as f32 + ix as f32 * self.step;
        let z0 = self.bounds[2] as f32 + iz as f32 * self.step;
        let x1 = (x0 + self.step).min(self.bounds[1] as f32);
        let z1 = (z0 + self.step).min(self.bounds[3] as f32);
        let u = ((x - x0) / (x1 - x0).max(0.001)).clamp(0., 1.);
        let v = ((z - z0) / (z1 - z0).max(0.001)).clamp(0., 1.);
        let stride = self.nx + 1;
        lerp(
            lerp(
                self.points[iz * stride + ix],
                self.points[iz * stride + ix + 1],
                u,
            ),
            lerp(
                self.points[(iz + 1) * stride + ix],
                self.points[(iz + 1) * stride + ix + 1],
                u,
            ),
            v,
        )
    }
}
fn edge_weight(x: f32, z: f32, coarse: f32, bounds: [i32; 4]) -> f32 {
    ((x - bounds[0] as f32)
        .min(bounds[1] as f32 - x)
        .min(z - bounds[2] as f32)
        .min(bounds[3] as f32 - z)
        / (coarse * 2.))
        .clamp(0., 1.)
}
