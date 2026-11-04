# Narrow DLSS compatibility backport

These are published `bevy_anti_alias` 0.18.1 and `dlss_wgpu` 2.0.0 sources,
retaining their original MIT/Apache notices. Only the Super Resolution path is
patched. The rest of Bevy remains pinned to 0.18.1.

The newer upstream wrapper isolates the raw NGX dispatch into its own command
buffer and requires it immediately after the parent wgpu encoder. The matching
Bevy node flushes/queues that buffer using `RenderContext::add_command_buffer`.
This fixes the incorrect command ordering that omitted transparent units' badges,
targeting lines and explosions in the initial native integration.

Resource descriptors passed through raw NGX pointers also have explicit local
bindings that remain alive through evaluation, following the newer upstream code.
For fixed quality modes, this backport uses NGX's **optimal** input resolution;
the original wrapper returned the dynamic-resolution minimum even in Quality.
The dynamic range getter is unchanged. Ray reconstruction is not exposed by the
game and is not part of this backport.

Reference implementations inspected on 24 October 2026:
https://github.com/bevyengine/dlss_wgpu/blob/main/src/super_resolution.rs
https://github.com/bevyengine/bevy/blob/main/crates/bevy_anti_alias/src/dlss/node.rs

Re-evaluate/remove these patches when upgrading the engine; do not silently
replace this wrapper with a newer incompatible wgpu or NGX SDK version.
