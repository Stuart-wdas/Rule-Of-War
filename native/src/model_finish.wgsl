#import bevy_pbr::{
    mesh_functions,
    pbr_bindings,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{prepass_io::{VertexOutput, FragmentOutput},pbr_deferred_functions::deferred_output}
#else
#import bevy_pbr::{forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT}
#endif
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> finish_tuning:vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> atlas_tint:vec4<f32>;
fn wear_hash(p:vec2<f32>)->f32 {
    return fract(sin(dot(p,vec2<f32>(127.1,311.7)))*43758.5453);
}
fn wear_noise(p:vec2<f32>)->f32 {
    let cell=floor(p);let u=fract(p);let f=u*u*(3.-2.*u);
    return mix(mix(wear_hash(cell),wear_hash(cell+vec2<f32>(1.,0.)),f.x),
        mix(wear_hash(cell+vec2<f32>(0.,1.)),wear_hash(cell+vec2<f32>(1.)),f.x),f.y);
}
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing) is_front:bool)->FragmentOutput {
    var pbr_input=pbr_input_from_standard_material(in,is_front);
#ifdef VERTEX_UVS_A
    if atlas_tint.w>0.5 {
        let paint=textureSample(pbr_bindings::metallic_roughness_texture,
            pbr_bindings::metallic_roughness_sampler,in.uv).a;
        pbr_input.material.base_color=vec4<f32>(pbr_input.material.base_color.rgb*
            mix(vec3<f32>(1.),atlas_tint.rgb,paint),pbr_input.material.base_color.a);
    }
#endif
#ifdef VERTEX_UVS_B
    // Contact shading attenuates indirect light; direct sunlight stays crisp.
    let contact_visibility=clamp(in.uv_b.x,0.38,1.);
    pbr_input.diffuse_occlusion*=mix(1.,contact_visibility,finish_tuning.x);
    let speckle=wear_noise(in.uv*49.);
    let edge=clamp(in.uv_b.y,0.,1.)*smoothstep(0.34,0.78,speckle)*finish_tuning.y;
    pbr_input.material.base_color=vec4<f32>(mix(pbr_input.material.base_color.rgb,
        vec3<f32>(0.16,0.175,0.15),edge),pbr_input.material.base_color.a);
    pbr_input.material.metallic=mix(pbr_input.material.metallic,0.88,edge);
    pbr_input.material.perceptual_roughness=mix(pbr_input.material.perceptual_roughness,0.26,edge);
#endif
#ifdef VERTEX_COLORS
    // Existing warm vertex stains identify dirt on the lower hull and running
    // gear. Keep it diffuse while the clean metal and paint gain highlights.
    let dirt=clamp((in.color.r-in.color.b)*3.2,0.,0.85)*finish_tuning.z;
    pbr_input.material.perceptual_roughness=mix(pbr_input.material.perceptual_roughness,0.93,dirt);
    pbr_input.material.metallic*=1.-dirt;
#endif
    if finish_tuning.w>0. {
        // Recover bind-pose coordinates, including on the turning turret and
        // recoiling gun. Weathering must move with the vehicle, never the map.
        let p=(mesh_functions::get_local_from_world(in.instance_index)*in.world_position).xyz;
        let model=mesh_functions::get_world_from_local(in.instance_index);
        let normal=normalize(transpose(mat3x3<f32>(model[0].xyz,model[1].xyz,model[2].xyz))*in.world_normal);
        var uv=p.xz;
        var horizontal=p.x;
        if abs(normal.y)<max(abs(normal.x),abs(normal.z)) {
            uv=vec2<f32>(p.x,p.y);
            if abs(normal.x)>abs(normal.z) {uv=vec2<f32>(p.z,p.y);horizontal=p.z;}
        }
        let deposits=wear_noise(uv*3.7+vec2<f32>(11.7,3.1));
        let grain=wear_noise(uv*41.);
        let lower=1.-smoothstep(0.34,1.02+deposits*0.18,p.y);
        let mud=lower*smoothstep(0.28,0.70,deposits)*0.58;
        let top=max(normal.y,0.);
        let dust=(lower*0.30+top*0.19)*mix(0.45,1.,deposits);
        let streak=pow(wear_noise(vec2<f32>(horizontal*19.,p.y*0.9)),5.)
            *(1.-abs(normal.y))*0.42;
        var grime=streak;
#ifdef VERTEX_UVS_B
        // Recesses collect darker grime; the offline contact map locates the
        // vents, hinges and joints without inventing lines over the camouflage.
        grime+=smoothstep(0.05,0.55,1.-in.uv_b.x)*0.30;
#endif
        var colour=pbr_input.material.base_color.rgb*(1.-grime);
        colour=mix(colour,vec3<f32>(0.095,0.066,0.038)*mix(0.8,1.2,grain),mud);
        colour=mix(colour,vec3<f32>(0.24,0.195,0.125)*mix(0.86,1.08,grain),dust);
        pbr_input.material.base_color=vec4<f32>(colour,pbr_input.material.base_color.a);
        let deposit=clamp(mud+dust,0.,0.8);
        pbr_input.material.perceptual_roughness=mix(pbr_input.material.perceptual_roughness,0.94,deposit);
        pbr_input.material.metallic*=1.-deposit;
    }
#ifdef VERTEX_UVS_B
    // Rough paint, rubber and dusty recesses should not retain an unoccluded
    // sky reflection beneath the hull. Use the final weathered roughness so
    // polished chipped edges keep their highlights. No extra texture lookup.
    let contact_roughness=pbr_input.material.perceptual_roughness;
    pbr_input.specular_occlusion*=mix(1.,contact_visibility,
        finish_tuning.x*contact_roughness*contact_roughness*0.65);
#endif
    pbr_input.material.base_color=alpha_discard(pbr_input.material,pbr_input.material.base_color);
#ifdef PREPASS_PIPELINE
    return deferred_output(in,pbr_input);
#else
    var out:FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT)==0u {
        out.color=apply_pbr_lighting(pbr_input);
    } else {out.color=pbr_input.material.base_color;}
    out.color=main_pass_post_lighting_processing(pbr_input,out.color);
    return out;
#endif
}
