//! Keep refraction in the same pixel coordinates as the opaque battlefield.
//!
//! Bevy 0.18.1 applies MainPassResolutionOverride in the non-copying branch of
//! its transmission pass, but misses it when transmission steps are enabled.
//! DLSS then enlarges and shifts pressure shells before upscaling them again.
use bevy::{
    camera::{MainPassResolutionOverride, Viewport},
    core_pipeline::core_3d::{
        Transmissive3d, ViewTransmissionTexture,
        graph::{Core3d, Node3d},
    },
    ecs::query::QueryItem,
    prelude::*,
    render::{
        RenderApp,
        camera::ExtractedCamera,
        render_graph::{NodeRunError, RenderGraph, RenderGraphContext, ViewNode, ViewNodeRunner},
        render_phase::ViewSortedRenderPhases,
        render_resource::{RenderPassDescriptor, StoreOp},
        renderer::RenderContext,
        view::{ExtractedView, ViewDepthTexture, ViewTarget},
    },
};

pub fn register(app: &mut App) {
    let world = app.sub_app_mut(RenderApp).world_mut();
    let runner = ViewNodeRunner::new(TransmissionPass, world);
    let mut graph = world.resource_mut::<RenderGraph>();
    let state = graph
        .sub_graph_mut(Core3d)
        .get_node_state_mut(Node3d::MainTransmissivePass)
        .unwrap();
    // Preserve the existing graph edges, including prepasses and DLSS.
    state.node = Box::new(runner);
    state.type_name = std::any::type_name::<ViewNodeRunner<TransmissionPass>>();
}

struct TransmissionPass;
impl ViewNode for TransmissionPass {
    type ViewQuery = (
        &'static ExtractedCamera,
        &'static ExtractedView,
        &'static Camera3d,
        &'static ViewTarget,
        Option<&'static ViewTransmissionTexture>,
        &'static ViewDepthTexture,
        Option<&'static MainPassResolutionOverride>,
    );

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        context: &mut RenderContext,
        (camera, view, settings, target, transmission, depth, resolution): QueryItem<
            Self::ViewQuery,
        >,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(phases) = world.get_resource::<ViewSortedRenderPhases<Transmissive3d>>() else {
            return Ok(());
        };
        let Some(phase) = phases.get(&view.retained_view_entity) else {
            return Ok(());
        };
        if phase.items.is_empty() {
            return Ok(());
        }
        let copies = settings.screen_space_specular_transmission_steps;
        let viewport = Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution);
        let descriptor = RenderPassDescriptor {
            label: Some("battlefield_transmission"),
            color_attachments: &[Some(target.get_color_attachment())],
            depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        for range in ranges(phase.items.len(), copies) {
            if copies > 0 {
                let transmission =
                    transmission.expect("transmission texture prepared for refraction");
                // Bevy stores the render area in the upper-left of an output-
                // sized texture. Keep that layout; the shader samples physical
                // texels, rather than stretching this area to the whole image.
                context.command_encoder().copy_texture_to_texture(
                    target.main_texture().as_image_copy(),
                    transmission.texture.as_image_copy(),
                    target.main_texture().size(),
                );
            }
            let mut pass = context.begin_tracked_render_pass(descriptor.clone());
            if let Some(viewport) = &viewport {
                pass.set_camera_viewport(viewport);
            }
            if let Err(error) = phase.render_range(&mut pass, world, graph.view_entity(), range) {
                error!("Error rendering battlefield transmission: {error:?}");
            }
        }
        Ok(())
    }
}

fn ranges(len: usize, steps: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
    let count = steps.max(1).min(len);
    (0..count).map(move |i| i * len / count..(i + 1) * len / count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refraction_viewport_matches_opaque_rendering_at_every_upscale_ratio() {
        let full = Viewport {
            physical_position: UVec2::new(24, 16),
            physical_size: UVec2::new(1920, 1080),
            ..default()
        };
        for size in [
            UVec2::new(1920, 1080),
            UVec2::new(1280, 720),
            UVec2::new(960, 540),
        ] {
            let resolution = MainPassResolutionOverride(size);
            let viewport =
                Viewport::from_viewport_and_override(Some(&full), Some(&resolution)).unwrap();
            assert_eq!(viewport.physical_size, size);
            assert_eq!(viewport.physical_position, full.physical_position);
            // The centre must survive rendering then upscaling without moving.
            let render_center = viewport.physical_size.as_vec2() * 0.5;
            let output_center = render_center * full.physical_size.as_vec2() / size.as_vec2();
            assert_eq!(output_center, full.physical_size.as_vec2() * 0.5);
        }
        assert!(Viewport::from_viewport_and_override(None, None).is_none());
    }
    #[test]
    fn transmission_steps_cover_each_draw_once_even_when_disabled_or_oversubscribed() {
        for len in 0..15 {
            for steps in 0..18 {
                let draws: Vec<_> = ranges(len, steps).flatten().collect();
                assert_eq!(draws, (0..len).collect::<Vec<_>>());
            }
        }
    }
}
