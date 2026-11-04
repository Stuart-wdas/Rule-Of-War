//! Opt-in CPU stage and asset-upload measurements, including render-thread backpressure.
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        mesh::RenderMesh,
        pipelined_rendering::RenderExtractApp,
        render_asset::{
            ExtractedAssets, RenderAsset, RenderAssetBytesPerFrameLimiter, RenderAssets,
        },
        texture::GpuImage,
    },
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Default, Clone, Copy)]
struct ExtractedCounts {
    meshes: usize,
    mesh_bytes: usize,
    images: usize,
    image_bytes: usize,
    removed: usize,
}
#[derive(Resource, Default)]
struct StageClocks {
    starts: [Option<Instant>; 4],
    extracted: ExtractedCounts,
    frames: u64,
    gpu_meshes: usize,
    gpu_images: usize,
}
#[derive(Default)]
struct Counters {
    frames: u64,
    mesh_count: u64,
    image_count: u64,
    mesh_bytes: u64,
    image_bytes: u64,
    peak_extracted_meshes: usize,
    peak_extracted_images: usize,
    peak_extracted_bytes: usize,
    uploaded_bytes: u64,
    peak_upload_bytes: usize,
    upload_frames: u64,
    gpu_meshes: usize,
    gpu_images: usize,
    peak_gpu_meshes: usize,
    peak_gpu_images: usize,
    budget_bytes: Option<usize>,
}
static COUNTERS: OnceLock<Mutex<Counters>> = OnceLock::new();

pub fn reset() {
    if let Some(counters) = COUNTERS.get() {
        *counters.lock().unwrap() = Counters::default();
    }
}
pub fn report() -> Value {
    let Some(counters) = COUNTERS.get() else {
        return json!({});
    };
    let c = counters.lock().unwrap();
    json!({
        "frames":c.frames,
        "extractedMeshes":c.mesh_count,"extractedImages":c.image_count,
        "extractedMeshBytes":c.mesh_bytes,"extractedImageBytes":c.image_bytes,
        "peakExtractedMeshesPerFrame":c.peak_extracted_meshes,
        "peakExtractedImagesPerFrame":c.peak_extracted_images,
        "peakExtractedBytesPerFrame":c.peak_extracted_bytes,
        "uploadedBytes":c.uploaded_bytes,"peakUploadedBytesPerFrame":c.peak_upload_bytes,
        "framesWithUploads":c.upload_frames,
        "activeGpuMeshes":c.gpu_meshes,"activeGpuImages":c.gpu_images,
        "peakActiveGpuMeshes":c.peak_gpu_meshes,"peakActiveGpuImages":c.peak_gpu_images,
        "uploadBudgetBytes":c.budget_bytes,
        "budgetIsSoft":true,"pendingAssetCountsAvailable":false
    })
}
fn sample(name: &'static str, start: Instant, details: Value) {
    let ms = start.elapsed().as_secs_f64() * 1000.;
    crate::performance::record(name, ms);
    if ms >= 33.3 {
        crate::performance::event(json!({"stage":name,"ms":ms,"details":details}));
    }
}
fn finish(clocks: &mut StageClocks, slot: usize, name: &'static str) {
    if let Some(start) = clocks.starts[slot].take() {
        let e = clocks.extracted;
        sample(
            name,
            start,
            json!({"meshes":e.meshes,"meshBytes":e.mesh_bytes,
            "images":e.images,"imageBytes":e.image_bytes}),
        );
    }
}
fn begin_frame(mut clocks: ResMut<StageClocks>) {
    clocks.starts[0] = Some(Instant::now());
}
fn end_frame(mut clocks: ResMut<StageClocks>) {
    finish(&mut clocks, 0, "render.schedule");
}
fn begin_assets(
    mut clocks: ResMut<StageClocks>,
    meshes: Option<Res<ExtractedAssets<RenderMesh>>>,
    images: Option<Res<ExtractedAssets<GpuImage>>>,
) {
    clocks.starts[1] = Some(Instant::now());
    let mut counts = ExtractedCounts::default();
    if let Some(meshes) = meshes {
        counts.meshes = meshes.extracted.len();
        counts.mesh_bytes = meshes
            .extracted
            .iter()
            .map(|(_, m)| RenderMesh::byte_len(m).unwrap_or(0))
            .sum();
        counts.removed += meshes.removed.len();
    }
    if let Some(images) = images {
        counts.images = images.extracted.len();
        counts.image_bytes = images
            .extracted
            .iter()
            .map(|(_, i)| GpuImage::byte_len(i).unwrap_or(0))
            .sum();
        counts.removed += images.removed.len();
    }
    clocks.extracted = counts;
}
fn end_assets(mut clocks: ResMut<StageClocks>) {
    finish(&mut clocks, 1, "render.prepare_assets");
}
fn begin_queue(mut clocks: ResMut<StageClocks>) {
    clocks.starts[2] = Some(Instant::now());
}
fn end_queue(mut clocks: ResMut<StageClocks>) {
    finish(&mut clocks, 2, "render.queue");
}
fn begin_render(mut clocks: ResMut<StageClocks>) {
    clocks.starts[3] = Some(Instant::now());
}
fn end_render(
    mut clocks: ResMut<StageClocks>,
    budget: Option<Res<RenderAssetBytesPerFrameLimiter>>,
    meshes: Option<Res<RenderAssets<RenderMesh>>>,
    images: Option<Res<RenderAssets<GpuImage>>>,
) {
    finish(&mut clocks, 3, "render.submit");
    clocks.frames += 1;
    let e = clocks.extracted;
    if e.meshes + e.images + e.removed > 0 || clocks.frames.is_multiple_of(60) {
        clocks.gpu_meshes = meshes.as_ref().map_or(0, |m| m.iter().count());
        clocks.gpu_images = images.as_ref().map_or(0, |i| i.iter().count());
    }
    let uploaded = budget
        .as_ref()
        .map_or(0, |b| b.bytes_written.load(Ordering::Relaxed));
    let mut c = COUNTERS.get_or_init(Default::default).lock().unwrap();
    c.frames += 1;
    c.mesh_count += e.meshes as u64;
    c.image_count += e.images as u64;
    c.mesh_bytes += e.mesh_bytes as u64;
    c.image_bytes += e.image_bytes as u64;
    c.peak_extracted_meshes = c.peak_extracted_meshes.max(e.meshes);
    c.peak_extracted_images = c.peak_extracted_images.max(e.images);
    c.peak_extracted_bytes = c.peak_extracted_bytes.max(e.mesh_bytes + e.image_bytes);
    c.uploaded_bytes += uploaded as u64;
    c.peak_upload_bytes = c.peak_upload_bytes.max(uploaded);
    c.upload_frames += u64::from(uploaded > 0);
    c.gpu_meshes = clocks.gpu_meshes;
    c.gpu_images = clocks.gpu_images;
    c.peak_gpu_meshes = c.peak_gpu_meshes.max(clocks.gpu_meshes);
    c.peak_gpu_images = c.peak_gpu_images.max(clocks.gpu_images);
    c.budget_bytes = budget.as_ref().and_then(|b| b.max_bytes);
}

pub fn register(app: &mut App) {
    if !std::env::args().any(|a| a == "--profile") {
        return;
    }
    let extracted_ns = Arc::new(AtomicU64::new(0));
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        let mut original = render.take_extract();
        let elapsed = extracted_ns.clone();
        render.set_extract(move |main, world| {
            let start = Instant::now();
            if let Some(extract) = original.as_mut() {
                extract(main, world);
            }
            elapsed.store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            sample("render.extract", start, Value::Null);
        });
        render.init_resource::<StageClocks>().add_systems(
            Render,
            (
                begin_frame.before(RenderSystems::ExtractCommands),
                end_frame.after(RenderSystems::PostCleanup),
                begin_assets
                    .after(RenderSystems::ExtractCommands)
                    .before(RenderSystems::PrepareAssets),
                end_assets
                    .after(RenderSystems::PrepareAssets)
                    .before(RenderSystems::PrepareMeshes),
                begin_queue
                    .after(RenderSystems::ManageViews)
                    .before(RenderSystems::Queue),
                end_queue
                    .after(RenderSystems::Queue)
                    .before(RenderSystems::PhaseSort),
                begin_render
                    .after(RenderSystems::Prepare)
                    .before(RenderSystems::Render),
                end_render
                    .after(RenderSystems::Render)
                    .before(RenderSystems::Cleanup),
            ),
        );
    }
    if let Some(extract_app) = app.get_sub_app_mut(RenderExtractApp) {
        let mut original = extract_app.take_extract();
        extract_app.set_extract(move |main, world| {
            extracted_ns.store(0, Ordering::Relaxed);
            let start = Instant::now();
            if let Some(extract) = original.as_mut() {
                extract(main, world);
            }
            let total_ms = start.elapsed().as_secs_f64() * 1000.;
            let extract_ms = extracted_ns.load(Ordering::Relaxed) as f64 / 1_000_000.;
            let wait_ms = (total_ms - extract_ms).max(0.);
            crate::performance::record("render.sync_wait", wait_ms);
            if wait_ms >= 33.3 {
                crate::performance::event(json!({"stage":"render.sync_wait","ms":wait_ms,
                    "syncAndExtractMs":total_ms,"extractMs":extract_ms}));
            }
        });
    }
}
