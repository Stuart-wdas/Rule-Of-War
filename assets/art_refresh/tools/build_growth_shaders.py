"""Pin Bevy 0.18.1 vertex paths and add shared track-driven turf flattening."""
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
BEVY=next((Path.home()/'.cargo/registry/src').glob('*/bevy_pbr-0.18.1/src'))
bindings='''
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var wear: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var wear_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> wear_world: vec4<f32>;
fn flatten_turf(position:vec4<f32>,base:f32)->vec4<f32> {
    let amount=textureSampleLevel(wear,wear_sampler,clamp(position.xz*wear_world.xy,vec2<f32>(0.),vec2<f32>(1.)),0.).r;
    return vec4<f32>(position.x,mix(position.y,base+0.018,amount*0.96),position.zw);
}
'''
for source,target in [('render/mesh.wgsl','growth_vertex.wgsl'),('prepass/prepass.wgsl','growth_prepass.wgsl')]:
    code=(BEVY/source).read_text()
    code='// Adapted from Bevy 0.18.1, MIT OR Apache-2.0. Licences: native/vendor/bevy_anti_alias/.\n'+bindings+code
    line='    out.position = position_world_to_clip(out.world_position.xyz);'
    code=code.replace(line,'#ifdef VERTEX_UVS_B\n    out.world_position=flatten_turf(out.world_position,vertex.uv_b.x);\n#endif\n'+line,1)
    if 'prepass' in source:
        line='''        vec4<f32>(prev_vertex.position, 1.0)
    );'''
        assert line in code
        code=code.replace(line,line+'\n#ifdef VERTEX_UVS_B\n    out.previous_world_position=flatten_turf(out.previous_world_position,prev_vertex.uv_b.x);\n#endif',1)
    (ROOT/'native/src'/target).write_text(code)
