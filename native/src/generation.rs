//! Default procedural terrain port: JS seed hashing, layered noise, thermal
//! erosion, river routing, water budgets and connected water surface levels.
use crate::terrain::{Cell, Data};
use std::collections::HashSet;

pub fn seed_from(text: &str) -> u32 {
    text.encode_utf16().fold(0x6d2b79f5u32, |hash, c| {
        (hash ^ c as u32).wrapping_mul(16777619)
    })
}
fn derive(seed: u32, channel: &str) -> u32 {
    seed_from(&format!("{}:{channel}", seed_from(&seed.to_string())))
}
fn random_at(x: i32, y: i32, seed: u32) -> f64 {
    let mut value = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
    value = value.wrapping_add(seed.wrapping_mul(1442695041));
    value = (value ^ (value >> 13)).wrapping_mul(1274126177);
    (value ^ (value >> 16)) as f64 / 4294967295.
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn noise(x: f64, y: f64, seed: u32) -> f64 {
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let fade = |t: f64| t * t * (3. - 2. * t);
    let (tx, ty) = (fade(x - cx as f64), fade(y - cy as f64));
    lerp(
        lerp(random_at(cx, cy, seed), random_at(cx + 1, cy, seed), tx),
        lerp(
            random_at(cx, cy + 1, seed),
            random_at(cx + 1, cy + 1, seed),
            tx,
        ),
        ty,
    )
}
struct Fractal {
    octaves: Vec<(u32, f64, f64)>,
    normalization: f64,
}
impl Fractal {
    fn new(seed: u32, count: usize, persistence: f64, lacunarity: f64) -> Self {
        let (mut amplitude, mut frequency, mut normalization) = (1., 1., 0.);
        let mut octaves = vec![];
        for i in 0..count {
            octaves.push((
                seed_from(&(seed as u64 + i as u64 * 1013).to_string()),
                amplitude,
                frequency,
            ));
            normalization += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        Self {
            octaves,
            normalization,
        }
    }
    fn sample(&self, x: f64, y: f64) -> f64 {
        self.octaves
            .iter()
            .map(|(seed, amplitude, frequency)| {
                noise(x * frequency, y * frequency, *seed) * amplitude
            })
            .sum::<f64>()
            / self.normalization
    }
}
fn normalize(values: &mut [f32]) {
    let minimum = values.iter().copied().fold(f32::INFINITY, f32::min) as f64;
    let maximum = values.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let span = maximum - minimum;
    if span <= 1e-9 {
        values.fill(0.5);
        return;
    }
    for v in values {
        *v = (((*v as f64 - minimum) / span).clamp(0., 1.)) as f32;
    }
}
fn field(width: usize, height: usize, seed: u32, moisture: bool) -> Vec<f32> {
    let seed = derive(seed, if moisture { "moisture" } else { "elevation" });
    let broad_seed = derive(seed, "broad");
    let detail_seed = derive(seed, "detail");
    let warp_seed = seed_from(&derive(seed, "warp").to_string());
    let ridge_seed = derive(seed, "ridges");
    let broad = Fractal::new(
        broad_seed,
        4,
        if moisture { 0.55 } else { 0.52 },
        if moisture { 2. } else { 2.03 },
    );
    let detail = Fractal::new(
        detail_seed,
        3,
        if moisture { 0.5 } else { 0.48 },
        if moisture { 2.07 } else { 2.11 },
    );
    let ridges = Fractal::new(ridge_seed, 5, 0.53, 2.02);
    let mut values = vec![0.; width * height];
    for y in 0..height {
        for x in 0..width {
            let (x, y) = (x as f64, y as f64);
            let warp = (noise(
                x / if moisture { 31. } else { 72. },
                y / if moisture { 31. } else { 72. },
                warp_seed,
            ) - 0.5)
                * if moisture { 4.5 } else { 14. };
            let b = broad.sample(
                (x + warp) / if moisture { 23. } else { 110. },
                (y - warp * 0.73) / if moisture { 23. } else { 110. },
            );
            let d = detail.sample(
                (x - warp * 0.31) / if moisture { 5. } else { 14. },
                (y + warp * 0.47) / if moisture { 5. } else { 14. },
            );
            let value = if moisture {
                b * 0.7 + d * 0.3
            } else {
                let ridge = (1.
                    - (ridges.sample((x + warp * 0.58) / 46., (y - warp * 0.41) / 46.) * 2. - 1.)
                        .abs())
                .powi(2);
                let mask = ((b - 0.48) / (0.76 - 0.48)).clamp(0., 1.);
                (b * 0.68 + d * 0.08 + ridge * mask * mask * (3. - 2. * mask) * 0.24)
                    / (0.68 + 0.08 + 0.24)
            };
            values[y as usize * width + x as usize] = value as f32;
        }
    }
    normalize(&mut values);
    if !moisture && width >= 3 && height >= 3 {
        for _ in 0..3 {
            let mut changes = vec![0f32; values.len()];
            for y in 1..height - 1 {
                for x in 1..width - 1 {
                    let index = y * width + x;
                    let mut lowest = index;
                    for candidate in [index - 1, index + 1, index - width, index + width] {
                        if values[candidate] < values[lowest] {
                            lowest = candidate;
                        }
                    }
                    let diff = values[index] as f64 - values[lowest] as f64;
                    if diff <= 0.045 {
                        continue;
                    }
                    let transfer = (diff * 0.45).min((diff - 0.045) * 0.18);
                    changes[index] = (changes[index] as f64 - transfer) as f32;
                    changes[lowest] = (changes[lowest] as f64 + transfer) as f32;
                }
            }
            for (value, change) in values.iter_mut().zip(changes) {
                *value = (*value as f64 + change as f64) as f32;
            }
        }
        normalize(&mut values);
    }
    if !moisture {
        for value in &mut values {
            *value = (*value as f64).powf(1.2) as f32;
        }
    }
    values
}
fn coverage(values: &[f32], amount: f64) -> f64 {
    if amount <= 0. || values.is_empty() {
        return 0.;
    }
    let mut buckets = [0usize; 2048];
    for v in values {
        buckets[(((*v as f64).clamp(0., 1.) * 2048.).floor() as usize).min(2047)] += 1;
    }
    let target = ((values.len() as f64 * amount.clamp(0., 1.)).floor() as usize).max(1);
    let mut count = 0;
    for (i, n) in buckets.into_iter().enumerate() {
        count += n;
        if count >= target {
            return i as f64 / 2048.;
        }
    }
    1.
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 15)).wrapping_mul(mixed | 1);
        mixed ^= mixed.wrapping_add((mixed ^ (mixed >> 7)).wrapping_mul(mixed | 61));
        (mixed ^ (mixed >> 14)) as f64 / 4294967296.
    }
}
fn rivers(
    elevation: &mut [f32],
    moisture: &[f32],
    width: usize,
    height: usize,
    seed: u32,
    water: f64,
) -> (Vec<bool>, Vec<f32>) {
    let mut random = Random(seed_from(&derive(seed, "sources").to_string()));
    let mut candidates = vec![];
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let i = y * width + x;
            if (elevation[i] as f64) < 0.68 {
                continue;
            }
            candidates.push((
                i,
                elevation[i] as f64 * 0.62 + moisture[i] as f64 * 0.25 + random.next() * 0.13,
            ));
        }
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let desired = ((width * height) as f64 * 0.0009).round().clamp(1., 40.) as usize;
    let mut sources: Vec<usize> = vec![];
    for (i, _) in candidates {
        if sources.iter().all(|s| {
            let dx = (s % width) as i64 - (i % width) as i64;
            let dy = (s / width) as i64 - (i / width) as i64;
            dx * dx + dy * dy >= 64
        }) {
            sources.push(i);
        }
        if sources.len() >= desired {
            break;
        }
    }
    let mut river = vec![false; elevation.len()];
    let mut flow = vec![0f32; elevation.len()];
    let mut downstream = vec![None; elevation.len()];
    for start in sources {
        if river[start] {
            continue;
        }
        let incoming = 1. + moisture[start] as f64 * 1.25;
        let mut visited = HashSet::new();
        let mut current = start;
        for _ in 0..(width * height).min((width + height) * 6) {
            if river[current] {
                add_flow(current, incoming, &river, &mut flow, &downstream);
                break;
            }
            if (elevation[current] as f64) < water || !visited.insert(current) {
                break;
            }
            river[current] = true;
            flow[current] = (flow[current] as f64 + incoming) as f32;
            let (x, y) = (current % width, current / width);
            if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                break;
            }
            let mut neighbors = vec![];
            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let i = ((y as i32 + dy) as usize) * width + (x as i32 + dx) as usize;
                if !visited.contains(&i) {
                    neighbors.push(i);
                }
            }
            neighbors.sort_by(|a, b| elevation[*a].total_cmp(&elevation[*b]).then(a.cmp(b)));
            let Some(&next) = neighbors.first() else {
                break;
            };
            if elevation[next] as f64 >= elevation[current] as f64 - 0.0005 {
                elevation[next] = (elevation[current] as f64 - 0.0025).max(0.) as f32;
            }
            if (elevation[next] as f64) < water {
                break;
            }
            downstream[current] = Some(next);
            if river[next] {
                add_flow(next, incoming, &river, &mut flow, &downstream);
                break;
            }
            current = next;
        }
    }
    let original = river.clone();
    let original_flow = flow.clone();
    for i in 0..original.len() {
        if original[i] {
            for j in cardinal(i, width, height)
                .into_iter()
                .chain(std::iter::once(i))
            {
                river[j] = true;
                flow[j] = (flow[j] as f64).max(original_flow[i] as f64 * 0.7) as f32;
            }
        }
    }
    (river, flow)
}
fn add_flow(
    start: usize,
    amount: f64,
    river: &[bool],
    flow: &mut [f32],
    downstream: &[Option<usize>],
) {
    let mut visited = HashSet::new();
    let mut current = Some(start);
    while let Some(i) = current {
        if !river[i] || !visited.insert(i) {
            break;
        }
        flow[i] = (flow[i] as f64 + amount) as f32;
        current = downstream[i];
    }
}
fn cardinal(i: usize, width: usize, height: usize) -> Vec<usize> {
    let (x, y) = (i % width, i / width);
    let mut out = vec![];
    if x > 0 {
        out.push(i - 1);
    }
    if x + 1 < width {
        out.push(i + 1);
    }
    if y > 0 {
        out.push(i - width);
    }
    if y + 1 < height {
        out.push(i + width);
    }
    out
}
fn components(
    kinds: &[u8],
    width: usize,
    height: usize,
    water: impl Fn(u8) -> bool,
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; kinds.len()];
    let mut result = vec![];
    for start in 0..kinds.len() {
        if visited[start] || !water(kinds[start]) {
            continue;
        }
        visited[start] = true;
        let mut component = vec![start];
        let mut cursor = 0;
        while cursor < component.len() {
            for next in cardinal(component[cursor], width, height) {
                if !visited[next] && water(kinds[next]) {
                    visited[next] = true;
                    component.push(next);
                }
            }
            cursor += 1;
        }
        result.push(component);
    }
    result
}
pub fn generate(width: usize, height: usize, text: &str) -> Data {
    let seed = seed_from(text);
    let mut elevation = field(width, height, derive(seed, "elevation-map"), false);
    let moisture = field(width, height, derive(seed, "moisture-map"), true);
    let water = 0.33f64.min(coverage(&elevation, 0.22));
    let (mut river, mut flow) = rivers(
        &mut elevation,
        &moisture,
        width,
        height,
        derive(seed, "river-map"),
        water,
    );
    let budget = ((width * height) as f64 * 0.26).floor() as usize;
    let mut indices: Vec<_> = river
        .iter()
        .enumerate()
        .filter(|(_, r)| **r)
        .map(|(i, _)| i)
        .collect();
    if indices.len() > budget {
        indices.sort_by(|a, b| flow[*a].total_cmp(&flow[*b]));
        for i in indices.iter().take(indices.len() - budget) {
            river[*i] = false;
            flow[*i] = 0.;
        }
    }
    let count = river.iter().filter(|r| **r).count();
    let shallow = water.min(coverage(
        &elevation,
        (budget - count) as f64 / (width * height) as f64,
    ));
    let deep = 0.2f64.min(shallow * (0.2 / 0.33));
    let mut kinds: Vec<u8> = elevation
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let e = *e as f64;
            if e < deep {
                0
            } else if e < shallow {
                1
            } else if river[i] {
                2
            } else if e < 0.42 {
                3
            } else if e < 0.65 {
                4
            } else if e < 0.82 {
                5
            } else {
                6
            }
        })
        .collect();
    for component in components(&kinds, width, height, |kind| kind < 2) {
        if component.len() < 6 {
            for i in component {
                kinds[i] = 3;
            }
        }
    }
    let mut surface = elevation.clone();
    for component in components(&kinds, width, height, |kind| kind <= 2) {
        if component.len() < 6 {
            continue;
        }
        let highest = component
            .iter()
            .map(|i| elevation[*i])
            .fold(f32::NEG_INFINITY, f32::max) as f64;
        let level = (shallow - 0.006).min(highest + 0.12) as f32;
        for i in component {
            surface[i] = level;
        }
    }
    Data {
        width,
        height,
        bounds: [-0.5, width as f32 - 0.5, -0.5, height as f32 - 0.5],
        scale: 3.,
        cells: (0..width * height)
            .map(|i| Cell {
                elevation: elevation[i],
                surface: surface[i],
                moisture: moisture[i],
                kind: kinds[i],
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_cells_match_javascript_fixtures() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../tests/web-procedural.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let data = generate(
                case["width"].as_u64().unwrap() as usize,
                case["height"].as_u64().unwrap() as usize,
                case["seed"].as_str().unwrap(),
            );
            assert_eq!(
                seed_from(case["seed"].as_str().unwrap()),
                case["numericSeed"].as_u64().unwrap() as u32
            );
            for (i, cell) in data.cells.iter().enumerate() {
                for (key, actual) in [
                    ("elevation", cell.elevation),
                    ("surface", cell.surface),
                    ("moisture", cell.moisture),
                ] {
                    let expected = case[key][i].as_f64().unwrap() as f32;
                    assert!(
                        (actual - expected).abs() < 0.000002,
                        "{} {key} {i}: {actual} != {expected}",
                        case["seed"]
                    );
                }
                assert_eq!(
                    cell.kind,
                    case["kinds"][i].as_u64().unwrap() as u8,
                    "{} kind {i}",
                    case["seed"]
                );
            }
        }
    }
}
