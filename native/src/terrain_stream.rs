// Included by landscape.rs. Persistent, indexed tiles keep camera movement from
// re-uploading the entire battlefield. Mesh generation remains off the main thread.
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const TILE: i32 = 64;
const FINE_CACHE: usize = 64;
const UPLOAD_BYTES: usize = 512 * 1024;
const UPLOAD_VERTICES: usize = 8192;
const JOB_TILES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TileLod {
    Coarse,
    Fine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileRequest {
    at: IVec2,
    lod: TileLod,
    epoch: u64,
}

struct TileMeshes {
    land: Handle<Mesh>,
    water: Option<Handle<Mesh>>,
    vertices: usize,
    epoch: u64,
}
#[derive(Default)]
struct TileState {
    epoch: u64,
    coarse: Option<TileMeshes>,
    fine: Option<TileMeshes>,
    land_entity: Option<Entity>,
    water_entity: Option<Entity>,
    active: Option<(TileLod, u64)>,
    active_vertices: usize,
    used: u64,
}
impl TileState {
    fn cache(&self, lod: TileLod) -> Option<&TileMeshes> {
        match lod {
            TileLod::Coarse => self.coarse.as_ref(),
            TileLod::Fine => self.fine.as_ref(),
        }
    }
    fn fresh(&self, lod: TileLod) -> bool {
        self.cache(lod)
            .is_some_and(|cache| cache.epoch == self.epoch)
    }
}
struct PreparedTile {
    request: TileRequest,
    generation: u64,
    land: Mesh,
    water: Mesh,
    bytes: usize,
    vertices: usize,
}
struct TerrainJob {
    receiver: mpsc::Receiver<Result<PreparedTile, String>>,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct StreamState {
    map: String,
    revision: u64,
    terrain: Option<Arc<Terrain>>,
    pending_edits: BTreeMap<String, Option<crate::terrain::Cell>>,
    towns: Arc<Vec<TownPaint>>,
    town_key: u64,
    tiles: HashMap<IVec2, TileState>,
    pending: Option<TerrainJob>,
    in_flight: HashSet<TileRequest>,
    staged: VecDeque<PreparedTile>,
    retired: VecDeque<Handle<Mesh>>,
    generation: u64,
    frame: u64,
    overview: bool,
    zoom: Option<f32>,
    zoom_changed: Option<Instant>,
}

fn tile_bounds(t: &Terrain, at: IVec2) -> [i32; 4] {
    [
        at.x * TILE,
        ((at.x + 1) * TILE).min(t.cols - 1),
        at.y * TILE,
        ((at.y + 1) * TILE).min(t.rows - 1),
    ]
}
fn tile_coords(t: &Terrain) -> impl Iterator<Item = IVec2> {
    let nx = (t.cols - 2).max(0) / TILE + 1;
    let nz = (t.rows - 2).max(0) / TILE + 1;
    (0..nz).flat_map(move |z| (0..nx).map(move |x| IVec2::new(x, z)))
}
fn tile_coarse(t: &Terrain) -> i32 {
    if t.cols.max(t.rows) > 1536 { 8 } else { 4 }
}
fn tile_wants_fine(
    t: &Terrain,
    at: IVec2,
    center: Vec3,
    overview: bool,
    detail: crate::graphics::Detail,
) -> bool {
    if t.cols.max(t.rows) <= 192 {
        // The coarse cache is already half-metre detail on small art/test maps.
        return false;
    }
    if overview {
        return false;
    }
    let focus = IVec2::new(
        (center.x / TILE as f32).floor() as i32,
        (center.z / TILE as f32).floor() as i32,
    );
    let radius = if detail == crate::graphics::Detail::Low {
        2
    } else {
        3
    };
    (focus.x - radius..focus.x + radius).contains(&at.x)
        && (focus.y - radius..focus.y + radius).contains(&at.y)
}
fn changed_tiles(changes: &[crate::terrain::EditDelta], t: &Terrain) -> HashSet<IVec2> {
    let margin = tile_coarse(t) + 2;
    changes
        .iter()
        .map(|(key, _)| key)
        .filter_map(|key| key.split_once(','))
        .filter_map(|(x, z)| Some((x.parse::<i32>().ok()?, z.parse::<i32>().ok()?)))
        .flat_map(|(x, z)| {
            let x0 = (x - margin).max(0) / TILE;
            let x1 = (x + margin).min(t.cols - 2).max(0) / TILE;
            let z0 = (z - margin).max(0) / TILE;
            let z1 = (z + margin).min(t.rows - 2).max(0) / TILE;
            (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| IVec2::new(x, z)))
        })
        .collect()
}
#[cfg(test)]
fn edited_tiles(
    before: &BTreeMap<String, crate::terrain::Cell>,
    after: &BTreeMap<String, crate::terrain::Cell>,
    t: &Terrain,
) -> HashSet<IVec2> {
    changed_tiles(
        &crate::terrain::edit_delta(before, &BTreeMap::new(), after),
        t,
    )
}
fn town_key(game: &NativeGame) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut key = std::collections::hash_map::DefaultHasher::new();
    for e in game
        .entities
        .iter()
        .filter(|e| !e.view.lots.is_empty() && e.view.label != "PRACTICE HOUSES")
    {
        e.view.kind.hash(&mut key);
        e.view.x.to_bits().hash(&mut key);
        e.view.z.to_bits().hash(&mut key);
        (e.view.town_plan as u8).hash(&mut key);
        e.view.lots.len().hash(&mut key);
        for lot in &e.view.lots {
            lot.at.map(f32::to_bits).hash(&mut key);
        }
    }
    key.finish()
}
fn source_key(t: &Terrain) -> String {
    // Source arrays are immutable between brush/crater revisions. Their backing
    // allocation identifies a newly imported/restarted map without hashing a
    // million height samples on each local crater edit.
    crate::terrain::render_source_key(t)
}
fn prepare_ground_lod(
    mut land: Mesh,
    terrain: &Terrain,
    towns: &[TownPaint],
    blend: Option<(i32, [i32; 4])>,
) -> Mesh {
    let grid = blend.map(|(coarse, bounds)| {
        SampleGrid::new(bounds, coarse as f32, |x, z| {
            Vec2::from_array(surface_blend(terrain, x, z, towns))
        })
    });
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        land.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        let blends: Vec<_> = positions
            .iter()
            .map(|p| {
                let value = Vec2::from_array(surface_blend(terrain, p[0], p[2], towns));
                if let (Some((coarse, bounds)), Some(grid)) = (blend, grid.as_ref()) {
                    let weight = edge_weight(p[0], p[2], coarse as f32, bounds);
                    if weight < 1. {
                        return grid
                            .sample(p[0], p[2], Vec2::lerp)
                            .lerp(value, weight)
                            .to_array();
                    }
                }
                value.to_array()
            })
            .collect();
        land.insert_attribute(Mesh::ATTRIBUTE_UV_1, blends);
    }
    if let Some(VertexAttributeValues::Float32x2(uv)) = land.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for p in uv {
            p[0] *= 4.;
            p[1] *= 4.;
        }
    }
    land
}
#[cfg(test)]
fn prepare_ground(land: Mesh, terrain: &Terrain, towns: &[TownPaint]) -> Mesh {
    prepare_ground_lod(land, terrain, towns, None)
}
fn prepare_tile(
    t: &Terrain,
    towns: &[TownPaint],
    request: TileRequest,
    generation: u64,
) -> PreparedTile {
    let _timing = crate::performance::span("terrain.prepare.tile");
    let bounds = tile_bounds(t, request.at);
    let small = t.cols.max(t.rows) <= 192;
    let blend = (request.lod == TileLod::Fine && !small).then_some((tile_coarse(t), bounds));
    let step = if small {
        0.5
    } else if request.lod == TileLod::Coarse {
        tile_coarse(t) as f32
    } else {
        1.
    };
    let (land, water) = mesh(t, bounds, step, None, blend);
    let land = prepare_ground_lod(land, t, towns, blend);
    let vertices = land.count_vertices() + water.count_vertices();
    let bytes = mesh_bytes(&land) + mesh_bytes(&water);
    PreparedTile {
        request,
        generation,
        land,
        water,
        bytes,
        vertices,
    }
}
fn mesh_bytes(mesh: &Mesh) -> usize {
    mesh.attributes()
        .map(|(_, v)| v.get_bytes().len())
        .sum::<usize>()
        + mesh.indices().map_or(0, |i| i.len() * 4)
}
fn retire(cache: TileMeshes, state: &mut StreamState) {
    state.retired.push_back(cache.land);
    state.retired.extend(cache.water);
}
fn valid_result(prepared: &PreparedTile, state: &StreamState) -> bool {
    prepared.generation == state.generation
        && state
            .tiles
            .get(&prepared.request.at)
            .is_some_and(|tile| tile.epoch == prepared.request.epoch)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects terrain, camera and mesh collections"
)]
pub fn stream(
    mut commands: Commands,
    game: Res<NativeGame>,
    controls: Res<Controls>,
    graphics: Option<Res<crate::graphics::Graphics>>,
    mut meshes: ResMut<Assets<Mesh>>,
    nature: Res<Nature>,
    ground: Query<(Entity, &Ground)>,
    mut state: Local<StreamState>,
    mut ready: ResMut<Ready>,
    mut menu: ResMut<crate::menu::MainMenu>,
) {
    let _timing = crate::performance::span("terrain.stream");
    let t = &game.terrain;
    let detail = graphics
        .as_ref()
        .map_or(crate::graphics::Detail::Balanced, |g| {
            g.settings.environment
        });
    let identity = source_key(t);
    state.frame += 1;
    let map_changed = state.map != identity;
    let layout = town_key(&game);
    let paint_changed = map_changed || layout != state.town_key;
    let revision_changed = map_changed || state.revision != t.revision;
    if map_changed {
        if let Some(job) = state.pending.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        for (entity, old) in &ground {
            commands.entity(entity).despawn();
            meshes.remove(old.0.id());
        }
        let old = std::mem::take(&mut state.tiles);
        for (_, tile) in old {
            for cache in tile.coarse.into_iter().chain(tile.fine) {
                retire(cache, &mut state);
            }
        }
        state.staged.clear();
        state.in_flight.clear();
        state.pending_edits.clear();
        state.generation += 1;
        state.map = identity;
        state.tiles = tile_coords(t)
            .map(|at| (at, TileState::default()))
            .collect();
        ready.ready = false;
        ready.error = None;
        ready.vertices = 0;
        state.terrain = Some(Arc::new(t.clone()));
        state.revision = t.revision;
    }
    if paint_changed {
        state.towns = Arc::new(
            game.entities
                .iter()
                .filter(|e| !e.view.lots.is_empty() && e.view.label != "PRACTICE HOUSES" && t.id != crate::gis_prototype::ID)
                .map(|e| TownPaint {
                    at: Vec2::new(e.view.x, e.view.z),
                    roads: crate::settlement_layout::roads_for(&e.view.kind, e.view.town_plan),
                    lots: e.view.lots.iter().map(|l| Vec2::from_array(l.at)).collect(),
                })
                .collect(),
        );
        state.town_key = layout;
    }
    if !map_changed && (revision_changed || paint_changed) {
        let changes = if revision_changed {
            crate::terrain::edit_delta(
                &state.terrain.as_ref().unwrap().edits,
                &state.pending_edits,
                &t.edits,
            )
        } else {
            vec![]
        };
        let dirty = if paint_changed && !map_changed {
            state.tiles.keys().copied().collect()
        } else {
            changed_tiles(&changes, t)
        };
        if !map_changed {
            for at in dirty {
                if let Some(tile) = state.tiles.get_mut(&at) {
                    tile.epoch += 1;
                }
            }
        }
        state.pending_edits.extend(changes);
        state.revision = t.revision;
        if !state.pending_edits.is_empty() || paint_changed {
            if let Some(job) = state.pending.take() {
                job.cancel.store(true, Ordering::Relaxed);
            }
            state.in_flight.clear();
        }
    }
    // A cancelled worker may still be finishing one tile. Keep its immutable
    // snapshot alive and the old visible meshes intact until ownership returns.
    if let Some(mut snapshot) = state.terrain.take() {
        crate::terrain::patch_snapshot(&mut snapshot, &mut state.pending_edits, t.revision);
        state.terrain = Some(snapshot);
    }
    if state
        .zoom
        .is_some_and(|zoom| (zoom - controls.zoom).abs() > 0.01)
    {
        state.zoom_changed = Some(Instant::now());
    }
    state.zoom = Some(controls.zoom);
    let upgrades = state
        .zoom_changed
        .is_none_or(|last| last.elapsed() >= Duration::from_millis(150));
    let small = t.cols.max(t.rows) <= 192;
    let overview = !small && controls.zoom >= if state.overview { 260. } else { 320. };
    state.overview = overview;
    // The worker sends independent tiles, so a slow tile never holds completed
    // neighbours hostage. Version checks precede mesh allocation / GPU admission.
    let mut disconnected = false;
    let mut completed = vec![];
    if let Some(job) = &state.pending {
        for _ in 0..JOB_TILES {
            match job.receiver.try_recv() {
                Ok(result) => completed.push(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
    }
    if disconnected {
        state.pending = None;
        state.in_flight.clear();
    }
    for result in completed {
        match result {
            Ok(prepared) => {
                state.in_flight.remove(&prepared.request);
                if valid_result(&prepared, &state) {
                    state.staged.push_back(prepared);
                } else {
                    ready.stale_tiles += 1;
                }
            }
            Err(error) => {
                ready.error = Some(error);
                menu.open = true;
            }
        }
    }
    let _install = crate::performance::span("terrain.install");
    let mut uploaded = 0usize;
    let mut vertices = 0usize;
    let mut installed = 0u32;
    while let Some(prepared) = state.staged.front() {
        if !valid_result(prepared, &state)
            || prepared.request.lod == TileLod::Fine
                && !tile_wants_fine(t, prepared.request.at, controls.center, overview, detail)
        {
            state.staged.pop_front();
            ready.stale_tiles += 1;
            continue;
        }
        if installed > 0
            && (uploaded + prepared.bytes > UPLOAD_BYTES
                || vertices + prepared.vertices > UPLOAD_VERTICES)
        {
            break;
        }
        let prepared = state.staged.pop_front().unwrap();
        uploaded += prepared.bytes;
        vertices += prepared.vertices;
        installed += 1;
        let request = prepared.request;
        let land = meshes.add(prepared.land);
        let water = (!prepared.water.indices().is_some_and(|i| i.is_empty())
            && prepared.water.count_vertices() > 0)
            .then(|| meshes.add(prepared.water));
        let cache = TileMeshes {
            land,
            water,
            vertices: prepared.vertices,
            epoch: request.epoch,
        };
        let tile = state.tiles.get_mut(&request.at).unwrap();
        let old = match request.lod {
            TileLod::Coarse => tile.coarse.replace(cache),
            TileLod::Fine => tile.fine.replace(cache),
        };
        if let Some(old) = old {
            retire(old, &mut state);
        }
    }
    if installed > 0 {
        ready.builds += installed;
    }
    ready.upload_bytes = uploaded;
    ready.peak_upload_bytes = ready.peak_upload_bytes.max(uploaded);
    let frame = state.frame;
    for (at, tile) in &mut state.tiles {
        let fine = tile_wants_fine(t, *at, controls.center, overview, detail);
        if fine {
            tile.used = frame;
        }
        let lod = if fine && tile.fresh(TileLod::Fine) {
            Some(TileLod::Fine)
        } else if tile.fresh(TileLod::Coarse) {
            Some(TileLod::Coarse)
        } else {
            None
        };
        let Some(lod) = lod else {
            continue;
        };
        let cache = tile.cache(lod).unwrap();
        if tile.active == Some((lod, cache.epoch)) {
            continue;
        }
        let (handle, water, count, epoch) = (
            cache.land.clone(),
            cache.water.clone(),
            cache.vertices,
            cache.epoch,
        );
        if let Some(entity) = tile.land_entity {
            commands
                .entity(entity)
                .insert((Ground(handle.clone()), Mesh3d(handle)));
        } else {
            tile.land_entity = Some(
                commands
                    .spawn((
                        Environment,
                        Ground(handle.clone()),
                        Mesh3d(handle),
                        MeshMaterial3d(nature.grass.clone()),
                        Transform::default(),
                    ))
                    .id(),
            );
        }
        match (tile.water_entity, water) {
            (Some(entity), Some(handle)) => {
                commands
                    .entity(entity)
                    .insert((Ground(handle.clone()), Mesh3d(handle)));
            }
            (None, Some(handle)) => {
                tile.water_entity = Some(
                    commands
                        .spawn((
                            Environment,
                            Ground(handle.clone()),
                            Mesh3d(handle),
                            MeshMaterial3d(nature.water.clone()),
                            Transform::default(),
                        ))
                        .id(),
                );
            }
            (Some(entity), None) => {
                commands.entity(entity).despawn();
                tile.water_entity = None;
            }
            (None, None) => {}
        }
        tile.active = Some((lod, epoch));
        tile.active_vertices = count;
    }
    ready.vertices = state.tiles.values().map(|tile| tile.active_vertices).sum();
    ready.ready = state.tiles.values().all(|tile| tile.land_entity.is_some());
    ready.cached_detail_tiles = state
        .tiles
        .values()
        .filter(|tile| tile.fine.is_some())
        .count();
    ready.pending_tiles =
        state.in_flight.len() + state.staged.len() + usize::from(!state.pending_edits.is_empty());
    // Fine meshes remain available on revisits. Retire one cold fine tile per
    // frame instead of freeing a burst of large vertex buffers on a zoom change.
    if state
        .tiles
        .values()
        .filter(|tile| tile.fine.is_some())
        .count()
        > FINE_CACHE
    {
        let cold = state
            .tiles
            .iter()
            .filter(|(_, tile)| {
                tile.fine.is_some() && tile.active.is_none_or(|(lod, _)| lod != TileLod::Fine)
            })
            .min_by_key(|(_, tile)| tile.used)
            .map(|(at, _)| *at);
        if let Some(at) = cold {
            let old = state.tiles.get_mut(&at).unwrap().fine.take().unwrap();
            retire(old, &mut state);
        }
    }
    for _ in 0..2 {
        if let Some(handle) = state.retired.pop_front() {
            meshes.remove(handle.id());
        }
    }
    if state.pending.is_none() && state.pending_edits.is_empty() {
        let staged: HashSet<_> = state.staged.iter().map(|tile| tile.request).collect();
        let mut requests = vec![];
        for (at, tile) in &state.tiles {
            let coarse = TileRequest {
                at: *at,
                lod: TileLod::Coarse,
                epoch: tile.epoch,
            };
            if !tile.fresh(TileLod::Coarse)
                && !staged.contains(&coarse)
                && !state.in_flight.contains(&coarse)
            {
                requests.push(coarse);
            }
            let fine = TileRequest {
                at: *at,
                lod: TileLod::Fine,
                epoch: tile.epoch,
            };
            if upgrades
                && tile_wants_fine(t, *at, controls.center, overview, detail)
                && !tile.fresh(TileLod::Fine)
                && !staged.contains(&fine)
                && !state.in_flight.contains(&fine)
            {
                requests.push(fine);
            }
        }
        requests.sort_by(|a, b| {
            let rank = |r: &TileRequest| {
                let center = r.at.as_vec2() * TILE as f32 + Vec2::splat(TILE as f32 * 0.5);
                let priority =
                    if r.lod == TileLod::Coarse && state.tiles[&r.at].land_entity.is_none() {
                        0
                    } else if r.lod == TileLod::Coarse {
                        1
                    } else {
                        2
                    };
                (
                    priority,
                    center.distance_squared(Vec2::new(controls.center.x, controls.center.z)),
                )
            };
            let (pa, da) = rank(a);
            let (pb, db) = rank(b);
            pa.cmp(&pb).then_with(|| da.total_cmp(&db))
        });
        requests.truncate(JOB_TILES);
        if !requests.is_empty() {
            let terrain = state.terrain.as_ref().unwrap().clone();
            let towns = state.towns.clone();
            let generation = state.generation;
            let (sender, receiver) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let cancelled = cancel.clone();
            state.in_flight.extend(requests.iter().copied());
            let spawned=std::thread::Builder::new().name("terrain-tiles".into()).spawn(move||{
                for request in requests {
                    if cancelled.load(Ordering::Relaxed){break;}
                    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||prepare_tile(&terrain,&towns,request,generation)))
                        .map_err(|_|"A battlefield tile could not be prepared. Re-enter the battlefield to retry.".into());
                    if sender.send(result).is_err(){break;}
                }
            });
            if spawned.is_ok() {
                state.pending = Some(TerrainJob { receiver, cancel });
            } else {
                state.in_flight.clear();
                ready.error = Some("Could not start terrain preparation.".into());
                menu.open = true;
            }
        }
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;
    fn flat(size: i32) -> Terrain {
        let mut t = Terrain::new("tile-test", size, size);
        t.authored = vec![0.56; (size * size) as usize];
        t.authored_scale = 32.;
        t.hydrate();
        t
    }
    fn nature() -> Nature {
        Nature {
            pebble: default(),
            trunk: default(),
            canopy: default(),
            canopy_low: default(),
            crown: default(),
            crown_low: default(),
            rock: default(),
            shrub: default(),
            groundcover: default(),
            low_growth: default(),
            bark: default(),
            leaves: default(),
            leaves_light: default(),
            stone: default(),
            grass: default(),
            water: default(),
        }
    }
    fn app() -> App {
        let mut app = App::new();
        let mut game = NativeGame::new();
        game.entities.clear();
        game.bases.clear();
        game.props.clear();
        game.terrain = flat(1024);
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Ready>()
            .init_resource::<crate::menu::MainMenu>()
            .insert_resource(game)
            .insert_resource(nature())
            .insert_resource(Controls {
                center: Vec3::new(512., 0., 512.),
                zoom: 60.,
                ..default()
            })
            .add_systems(Update, stream);
        app
    }
    fn wait(app: &mut App, condition: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            app.update();
            if condition(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(condition(app), "Terrain tiles did not finish");
    }
    fn resident(app: &mut App) -> HashMap<Entity, bevy::asset::AssetId<Mesh>> {
        app.world_mut()
            .query::<(Entity, &Ground)>()
            .iter(app.world())
            .map(|(e, g)| (e, g.0.id()))
            .collect()
    }
    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        let VertexAttributeValues::Float32x3(p) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("positions")
        };
        p
    }
    fn normals(mesh: &Mesh) -> &[[f32; 3]] {
        let VertexAttributeValues::Float32x3(p) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
        else {
            panic!("normals")
        };
        p
    }
    fn blends(mesh: &Mesh) -> &[[f32; 2]] {
        let VertexAttributeValues::Float32x2(p) = mesh.attribute(Mesh::ATTRIBUTE_UV_1).unwrap()
        else {
            panic!("blends")
        };
        p
    }
    #[test]
    fn tile_edges_match_coarse_height_normal_and_material_weights_including_last_map_edge() {
        let mut t = flat(256);
        for z in 0..256 {
            for x in 0..256 {
                t.authored[(z * 256 + x) as usize] =
                    0.56 + (x as f32 * 0.13).sin() * 0.025 + (z as f32 * 0.17).cos() * 0.03;
            }
        }
        for at in [
            IVec2::ZERO,
            IVec2::new(3, 0),
            IVec2::new(0, 3),
            IVec2::new(3, 3),
        ] {
            let coarse = prepare_tile(
                &t,
                &[],
                TileRequest {
                    at,
                    lod: TileLod::Coarse,
                    epoch: 0,
                },
                0,
            );
            let fine = prepare_tile(
                &t,
                &[],
                TileRequest {
                    at,
                    lod: TileLod::Fine,
                    epoch: 0,
                },
                0,
            );
            let b = tile_bounds(&t, at);
            for (i, p) in positions(&fine.land).iter().enumerate() {
                let edge_x = p[0] == b[0] as f32 || p[0] == b[1] as f32;
                let edge_z = p[2] == b[2] as f32 || p[2] == b[3] as f32;
                if !edge_x && !edge_z {
                    continue;
                }
                let mut edge: Vec<_> = positions(&coarse.land)
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| if edge_x { q[0] == p[0] } else { q[2] == p[2] })
                    .collect();
                let axis = if edge_x { 2 } else { 0 };
                edge.sort_by(|a, b| a.1[axis].total_cmp(&b.1[axis]));
                let pair = edge
                    .windows(2)
                    .find(|pair| pair[0].1[axis] <= p[axis] && pair[1].1[axis] >= p[axis])
                    .unwrap();
                let u = (p[axis] - pair[0].1[axis]) / (pair[1].1[axis] - pair[0].1[axis]);
                let h = pair[0].1[1].lerp(pair[1].1[1], u);
                let n = Vec3::from(normals(&coarse.land)[pair[0].0])
                    .lerp(Vec3::from(normals(&coarse.land)[pair[1].0]), u)
                    .normalize();
                let blend = Vec2::from(blends(&coarse.land)[pair[0].0])
                    .lerp(Vec2::from(blends(&coarse.land)[pair[1].0]), u);
                assert!((p[1] - h).abs() < 0.00002, "{at:?} {p:?}: height seam");
                assert!(
                    Vec3::from(normals(&fine.land)[i]).distance(n) < 0.00002,
                    "normal seam"
                );
                assert!(
                    Vec2::from(blends(&fine.land)[i]).distance(blend) < 0.00002,
                    "material seam"
                );
            }
        }
    }
    #[test]
    fn extended_terrain_detail_reaches_farther_but_low_and_overview_stay_bounded() {
        use crate::graphics::Detail;
        let terrain = flat(1024);
        let center = Vec3::new(512., 0., 512.);
        let farther = IVec2::new(10, 8);
        assert!(tile_wants_fine(
            &terrain,
            farther,
            center,
            false,
            Detail::High
        ));
        assert!(tile_wants_fine(
            &terrain,
            farther,
            center,
            false,
            Detail::Balanced
        ));
        assert!(!tile_wants_fine(
            &terrain,
            farther,
            center,
            false,
            Detail::Low
        ));
        assert!(!tile_wants_fine(
            &terrain,
            IVec2::new(14, 8),
            center,
            false,
            Detail::High
        ));
        assert!(!tile_wants_fine(
            &terrain,
            farther,
            center,
            true,
            Detail::High
        ));
    }
    #[test]
    fn camera_pan_and_zoom_reuse_gpu_handles_and_one_edit_replaces_only_its_tile() {
        let mut app = app();
        wait(&mut app, |a| {
            let r = a.world().resource::<Ready>();
            r.ready && r.cached_detail_tiles >= 16 && r.pending_tiles == 0
        });
        let before = resident(&mut app);
        let builds = app.world().resource::<Ready>().builds;
        app.world_mut().resource_mut::<Controls>().zoom = 1075.;
        for _ in 0..6 {
            app.update();
        }
        let overview = resident(&mut app);
        assert_eq!(
            before.keys().collect::<HashSet<_>>(),
            overview.keys().collect::<HashSet<_>>()
        );
        assert_eq!(app.world().resource::<Ready>().builds, builds);
        app.world_mut().resource_mut::<Controls>().center = Vec3::new(900., 0., 900.);
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(resident(&mut app), overview);
        app.world_mut().resource_mut::<Controls>().center = Vec3::new(512., 0., 512.);
        app.world_mut().resource_mut::<Controls>().zoom = 60.;
        app.update();
        assert_eq!(
            resident(&mut app),
            before,
            "cached detail must swap in immediately"
        );
        assert_eq!(app.world().resource::<Ready>().builds, builds);
        app.world_mut()
            .resource_mut::<NativeGame>()
            .terrain
            .crater(90., 90., 2);
        wait(&mut app, |a| a.world().resource::<Ready>().builds > builds);
        let edited = resident(&mut app);
        assert_eq!(
            before.keys().collect::<HashSet<_>>(),
            edited.keys().collect::<HashSet<_>>()
        );
        assert_eq!(
            before
                .iter()
                .filter(|(e, h)| edited.get(*e) != Some(*h))
                .count(),
            1
        );
        assert!(app.world().resource::<Ready>().peak_upload_bytes <= UPLOAD_BYTES);
        let old_source = source_key(&app.world().resource::<NativeGame>().terrain);
        app.world_mut().resource_mut::<NativeGame>().terrain = flat(1024);
        assert_ne!(
            source_key(&app.world().resource::<NativeGame>().terrain),
            old_source
        );
        app.update();
        assert!(!app.world().resource::<Ready>().ready);
        wait(&mut app, |a| a.world().resource::<Ready>().ready);
        assert!(resident(&mut app).keys().any(|e| !before.contains_key(e)));
    }
    #[test]
    fn edit_invalidation_covers_grid_boundaries_and_removed_edits_without_dirtying_the_world() {
        let mut t = flat(1024);
        let before = t.edits.clone();
        t.crater(64., 64., 2);
        let affected = edited_tiles(&before, &t.edits, &t);
        assert_eq!(
            affected,
            [
                IVec2::new(0, 0),
                IVec2::new(0, 1),
                IVec2::new(1, 0),
                IVec2::new(1, 1)
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(affected, edited_tiles(&t.edits, &before, &t));
    }
    #[test]
    fn stale_worker_results_are_rejected_before_gpu_asset_admission() {
        let t = flat(256);
        let at = IVec2::ZERO;
        let mut state = StreamState {
            generation: 3,
            ..default()
        };
        state.tiles.insert(
            at,
            TileState {
                epoch: 7,
                ..default()
            },
        );
        let mut prepared = prepare_tile(
            &t,
            &[],
            TileRequest {
                at,
                lod: TileLod::Coarse,
                epoch: 7,
            },
            3,
        );
        assert!(valid_result(&prepared, &state));
        prepared.generation = 2;
        assert!(!valid_result(&prepared, &state));
        prepared.generation = 3;
        prepared.request.epoch = 6;
        assert!(!valid_result(&prepared, &state));
        prepared.request.epoch = 7;
        state.tiles.clear();
        assert!(!valid_result(&prepared, &state));
        assert!(!tile_wants_fine(
            &t,
            at,
            Vec3::splat(900.),
            false,
            crate::graphics::Detail::Balanced
        ));
    }
}
