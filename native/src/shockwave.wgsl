#import bevy_pbr::{forward_io::VertexOutput, mesh_functions, mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler}}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> pixels: vec4<f32>;
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let age = f32(mesh_functions::get_tag(in.instance_index)) / 65535.;
    let envelope = sin(age * 3.14159265) * (1. - age);
    let normal = normalize(in.world_normal);
    let toward_camera = normalize(view.world_position - in.world_position.xyz);
    let rim = 1. - abs(dot(normal, toward_camera));
    let band = smoothstep(0.40, 0.72, rim) * (1. - smoothstep(0.94, 1., rim));
    let opacity = band * envelope * 0.65;
    if opacity < 0.001 { discard; }
    // Automated GPU mask: exposes the actual rasterized ring for alignment QA.
    if pixels.y > 0.5 { return vec4(1., 0., 1., 1.); }
    let projected = (view.clip_from_world * vec4(normal, 0.)).xy;
    let direction = normalize(projected + vec2(0.000001));
    // The transmission copy retains the full target size under DLSS. Only its
    // render viewport is populated: use physical texels, including its origin.
    let texture_size = vec2<f32>(textureDimensions(view_transmission_texture));
    let uv = in.position.xy / texture_size;
    let ripple = 0.86 + 0.14 * sin(rim * 80. - age * 25.);
    let offset = vec2(direction.x, -direction.y) * pixels.x * envelope * ripple / texture_size;
    let lower = (view.main_pass_viewport.xy + vec2(0.5)) / texture_size;
    let upper = (view.main_pass_viewport.xy + view.main_pass_viewport.zw - vec2(0.5)) / texture_size;
    let refracted = textureSampleLevel(view_transmission_texture, view_transmission_sampler,
                                      clamp(uv + offset, lower, upper), 0.);
    return vec4(refracted.rgb, opacity);
}
