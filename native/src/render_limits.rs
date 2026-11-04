//! Measure the live device budget without changing textures, effects, or GPU limits.
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{
            BindGroupLayoutDescriptor, BindingType, CachedPipelineState, PipelineCache,
            PipelineDescriptor, ShaderStages,
        },
        renderer::{RenderAdapter, RenderDevice},
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};

static REPORT: OnceLock<Mutex<Value>> = OnceLock::new();

pub fn report() -> Value {
    REPORT
        .get()
        .map_or(Value::Null, |r| r.lock().unwrap().clone())
}

pub fn register(app: &mut App) {
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.add_systems(
            Render,
            audit
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
        );
    }
}

#[derive(Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StageBudget {
    sampled_textures: u32,
    samplers: u32,
    binding_array_elements: u32,
    binding_array_sampler_elements: u32,
}
impl StageBudget {
    fn fits(self, limits: &bevy::render::settings::WgpuLimits) -> bool {
        self.sampled_textures <= limits.max_sampled_textures_per_shader_stage
            && self.samplers <= limits.max_samplers_per_shader_stage
            && self.binding_array_elements <= limits.max_binding_array_elements_per_shader_stage
            && self.binding_array_sampler_elements
                <= limits.max_binding_array_sampler_elements_per_shader_stage
    }
}

/// Match wgpu-core 27's BindingTypeMaxCountValidator. Binding arrays have a
/// separate element budget, rather than spending ordinary sampled-texture slots.
fn count(layouts: &[BindGroupLayoutDescriptor]) -> [StageBudget; 3] {
    let mut stages = [StageBudget::default(); 3];
    for layout in layouts {
        for entry in &layout.entries {
            for (budget, stage) in stages.iter_mut().zip([
                ShaderStages::VERTEX,
                ShaderStages::FRAGMENT,
                ShaderStages::COMPUTE,
            ]) {
                if !entry.visibility.contains(stage) {
                    continue;
                }
                if let Some(elements) = entry.count {
                    budget.binding_array_elements += elements.get();
                    if matches!(entry.ty, BindingType::Sampler(_)) {
                        budget.binding_array_sampler_elements += elements.get();
                    }
                } else {
                    match entry.ty {
                        BindingType::Texture { .. } => budget.sampled_textures += 1,
                        BindingType::Sampler(_) => budget.samplers += 1,
                        BindingType::ExternalTexture => {
                            budget.sampled_textures += 4;
                            budget.samplers += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    stages
}

fn limits_json(limits: &bevy::render::settings::WgpuLimits) -> Value {
    json!({
        "sampledTexturesPerShaderStage":limits.max_sampled_textures_per_shader_stage,
        "samplersPerShaderStage":limits.max_samplers_per_shader_stage,
        "bindingArrayElementsPerShaderStage":limits.max_binding_array_elements_per_shader_stage,
        "bindingArraySamplerElementsPerShaderStage":limits.max_binding_array_sampler_elements_per_shader_stage,
        "maximumTextureDimension2d":limits.max_texture_dimension_2d,
        "maximumTextureArrayLayers":limits.max_texture_array_layers,
    })
}

#[derive(Default)]
struct AuditState {
    frames: u64,
    signature: Option<(usize, usize, usize)>,
    last_fit: Option<bool>,
    announced: bool,
}

fn audit(
    adapter: Res<RenderAdapter>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    mut state: Local<AuditState>,
) {
    state.frames += 1;
    if state.frames != 1 && !state.frames.is_multiple_of(30) {
        return;
    }
    let signature = cache
        .pipelines()
        .fold((0, 0, 0), |(total, ready, failed), p| {
            (
                total + 1,
                ready + usize::from(matches!(p.state, CachedPipelineState::Ok(_))),
                failed + usize::from(matches!(p.state, CachedPipelineState::Err(_))),
            )
        });
    if state.signature == Some(signature) {
        return;
    }
    state.signature = Some(signature);
    let (_, ready, failed) = signature;
    let mut rows = vec![];
    let mut max_textures = 0;
    let mut max_view_textures = 0;
    let limits = device.limits();
    for pipeline in cache.pipelines() {
        let (kind, label, layouts) = match &pipeline.descriptor {
            PipelineDescriptor::RenderPipelineDescriptor(p) => ("render", &p.label, &p.layout),
            PipelineDescriptor::ComputePipelineDescriptor(p) => ("compute", &p.label, &p.layout),
        };
        let status = match &pipeline.state {
            CachedPipelineState::Ok(_) => "ready",
            CachedPipelineState::Err(_) => "error",
            _ => "pending",
        };
        let stages = count(layouts);
        let view_textures = layouts
            .iter()
            .filter(|l| l.label.starts_with("mesh_view_layout"))
            .flat_map(|l| &l.entries)
            .filter(|e| matches!(e.ty, BindingType::Texture { .. }))
            .count();
        max_view_textures = max_view_textures.max(view_textures);
        max_textures =
            max_textures.max(stages.iter().map(|s| s.sampled_textures).max().unwrap_or(0));
        rows.push(json!({"kind":kind,"label":label,"state":status,
            "allTextureAndSamplerBudgetsFit":stages.iter().all(|s|s.fits(&limits)),
            "vertex":stages[0],"fragment":stages[1],"compute":stages[2],
            "portableViewTextureBindingCount":view_textures,
            "bindGroups":layouts.iter().map(|l|l.label.as_ref()).collect::<Vec<_>>()
        }));
    }
    let fits = rows
        .iter()
        .all(|r| r["allTextureAndSamplerBudgetsFit"] == true);
    let info = adapter.get_info();
    let report = json!({
        "adapter":{"name":info.name,"backend":format!("{:?}",info.backend),
            "driver":info.driver,"driverInfo":info.driver_info},
        "adapterLimits":limits_json(&adapter.limits()),
        "deviceLimits":limits_json(&limits),
        "pipelinesInspected":rows.len(),"readyPipelines":ready,"failedPipelines":failed,
        "allTextureAndSamplerBudgetsFit":fits,
        "maximumSampledTexturesPerStage":max_textures,
        "sampledTextureHeadroom":limits.max_sampled_textures_per_shader_stage.saturating_sub(max_textures),
        "maximumPortableViewTextureBindingCount":max_view_textures,
        "portableViewWarningThreshold":10,
        "portableViewThresholdExceeded":max_view_textures>10,
        "scope":"Declared layouts across all bind groups of cached render and compute pipelines; binding arrays use their own budgets.",
        "pipelines":rows,
    });
    *REPORT
        .get_or_init(|| Mutex::new(Value::Null))
        .lock()
        .unwrap() = report;
    if ready > 0 && !state.announced {
        info!(
            "GPU texture budget: {} / {:?}; device allows {} sampled textures per shader stage. Texture quality and effects are preserved.",
            info.name, info.backend, limits.max_sampled_textures_per_shader_stage
        );
        state.announced = true;
    }
    if !fits && state.last_fit != Some(false) {
        error!(
            "A rendering pipeline exceeds this device's texture/sampler budget; see the renderTextureBudget diagnostic. No quality settings have been changed."
        );
    }
    state.last_fit = Some(fits);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{
        BindGroupLayoutEntry, SamplerBindingType, TextureSampleType, TextureViewDimension,
    };
    use std::num::NonZeroU32;

    fn texture(
        binding: u32,
        visibility: ShaderStages,
        elements: Option<u32>,
    ) -> BindGroupLayoutEntry {
        BindGroupLayoutEntry {
            binding,
            visibility,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: elements.and_then(NonZeroU32::new),
        }
    }
    #[test]
    fn materials_and_view_textures_share_a_stage_budget_but_vertex_is_separate() {
        let view: Vec<_> = (0..10)
            .map(|i| texture(i, ShaderStages::FRAGMENT, None))
            .collect();
        let mut material: Vec<_> = (0..7)
            .map(|i| texture(i, ShaderStages::FRAGMENT, None))
            .collect();
        material.push(texture(8, ShaderStages::VERTEX, None));
        let budgets = count(&[
            BindGroupLayoutDescriptor::new("view", &view),
            BindGroupLayoutDescriptor::new("material", &material),
        ]);
        assert_eq!(budgets[0].sampled_textures, 1);
        assert_eq!(budgets[1].sampled_textures, 17);
        assert!(!budgets[1].fits(&Default::default()));
        let desktop = bevy::render::settings::WgpuLimits {
            max_sampled_textures_per_shader_stage: 32,
            ..Default::default()
        };
        assert!(budgets[1].fits(&desktop));
    }
    #[test]
    fn binding_arrays_spend_their_separate_element_and_sampler_budgets() {
        let mut entries = vec![
            texture(0, ShaderStages::FRAGMENT, Some(48)),
            texture(1, ShaderStages::FRAGMENT, None),
        ];
        entries.push(BindGroupLayoutEntry {
            binding: 2,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::Filtering),
            count: NonZeroU32::new(4),
        });
        let budgets = count(&[BindGroupLayoutDescriptor::new("arrays", &entries)]);
        assert_eq!(budgets[1].sampled_textures, 1);
        assert_eq!(budgets[1].samplers, 0);
        assert_eq!(budgets[1].binding_array_elements, 52);
        assert_eq!(budgets[1].binding_array_sampler_elements, 4);
        let mut limits = bevy::render::settings::WgpuLimits {
            max_binding_array_elements_per_shader_stage: 52,
            max_binding_array_sampler_elements_per_shader_stage: 4,
            ..Default::default()
        };
        assert!(budgets[1].fits(&limits));
        limits.max_binding_array_elements_per_shader_stage = 51;
        assert!(!budgets[1].fits(&limits));
    }
}
