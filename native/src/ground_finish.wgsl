#import bevy_pbr::{pbr_functions::alpha_discard,pbr_fragment::pbr_input_from_standard_material}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{prepass_io::{VertexOutput,FragmentOutput},pbr_deferred_functions::deferred_output}
#else
#import bevy_pbr::{forward_io::{VertexOutput,FragmentOutput},
    pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT}
#endif
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tuning:vec4<f32>;
fn hash(p:vec2<f32>)->f32 {
    return fract(sin(dot(p,vec2<f32>(127.1,311.7)))*43758.5453);
}
fn field(p:vec2<f32>)->f32 {
    let i=floor(p);let u=fract(p);let f=u*u*(3.-2.*u);
    return mix(mix(hash(i),hash(i+vec2<f32>(1.,0.)),f.x),
        mix(hash(i+vec2<f32>(0.,1.)),hash(i+vec2<f32>(1.)),f.x),f.y);
}
fn weather(p:vec2<f32>)->f32 {
    return field(p)*0.56+field(p*2.07+vec2<f32>(11.3,5.7))*0.29
        +field(p*4.13+vec2<f32>(3.1,17.8))*0.15;
}
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing) front:bool)->FragmentOutput {
    var pbr=pbr_input_from_standard_material(in,front);
    let world=in.world_position.xz;
    let aa=max(fwidth(world.x)+fwidth(world.y),0.006);
    // Metre-scale weathering survives the normal gameplay camera. Warping
    // breaks up square value-noise patches; the scan retains finer aggregate.
    let warp=vec2<f32>(field(world/19.),field(world/23.+vec2<f32>(9.7)))-vec2<f32>(0.5);
    let worn=weather((world+warp*5.)/7.3);
    let dust=weather((world-warp*3.)/3.8+vec2<f32>(23.6,7.1));
    let bleach=weather(world/19.+vec2<f32>(41.2,3.8));
    let speckle=field(world*1.8+warp*2.);
    let stain=smoothstep(0.44,0.75,worn)*(0.55+speckle*0.45);
    let chalk=smoothstep(0.52,0.78,bleach);
    let dust_mask=smoothstep(0.45,0.76,dust);
    var colour=pbr.material.base_color.rgb;
    colour*=0.80+worn*0.40;
    colour*=1.-stain*0.19;
    colour=mix(colour,colour*vec3<f32>(1.16,1.10,0.94),dust_mask*0.55);
    colour*=1.+chalk*0.13;
    var oil=0.;
    var tyre=0.;
    var joint=0.;
    var edge=0.;
    if tuning.y<0.5 {
        let grid=world/tuning.x;let cell=floor(grid);let local=fract(grid)*tuning.x;
        let slab=hash(cell);
        let distance=min(min(local.x,tuning.x-local.x),min(local.y,tuning.x-local.y));
        joint=(1.-smoothstep(0.008,0.018+aa*0.5,distance))*min(1.,0.045/aa);
        edge=1.-smoothstep(0.015,0.11,distance);
        let spill_at=vec2<f32>(1.+hash(cell+vec2<f32>(13.1))*3.5,1.+hash(cell+vec2<f32>(3.7))*3.5);
        let spill_r=length((local-spill_at)*vec2<f32>(1.,1.4))+(speckle-0.5)*0.34;
        oil=(1.-smoothstep(0.08,0.72,spill_r))*step(0.87,hash(cell+vec2<f32>(37.)));
        let radius=length(local-vec2<f32>(2.6,2.4));
        tyre=(1.-smoothstep(0.035,0.09+aa,abs(radius-2.05)))
            *smoothstep(0.38,0.7,field(world*0.9))*step(0.68,hash(cell+vec2<f32>(71.)));
        let fracture=abs(local.x-(0.6+local.y*0.55+sin(local.y*8.+slab*31.)*0.035));
        let crack=(1.-smoothstep(0.004,0.012+aa*0.5,fracture))*min(1.,0.025/aa)
            *step(0.73,hash(cell+vec2<f32>(91.)))*(1.-smoothstep(2.5,3.8,local.y));
        colour*=(0.93+slab*0.13)*(1.-joint*0.38-edge*0.045-crack*0.30);
    } else {
        // Continuous hardstanding uses dusty perimeter deposits, not slab joints.
#ifdef VERTEX_UVS_B
        let uv_edge=min(in.uv_b,vec2<f32>(1.)-in.uv_b)*tuning.zw;
        let perimeter=min(uv_edge.x,uv_edge.y);
        let rim=1.-smoothstep(0.12,2.5+speckle*1.9,perimeter);
        colour=mix(colour,colour*vec3<f32>(0.96,0.85,0.65),rim*0.50);
        colour*=1.-rim*(0.04+stain*0.10);
#endif
        let streak=field(vec2<f32>(world.x*0.65,world.y*0.07)+vec2<f32>(7.3,13.8));
        tyre=smoothstep(0.62,0.85,streak)*smoothstep(0.28,0.65,dust)*0.6;
        oil=smoothstep(0.70,0.88,worn)*(0.55+speckle*0.45);
    }
    colour*=1.-oil*0.27-tyre*0.17;
    pbr.material.base_color=vec4<f32>(colour,pbr.material.base_color.a);
    pbr.diffuse_occlusion*=1.-joint*0.12-edge*0.025;
    pbr.material.perceptual_roughness=clamp(pbr.material.perceptual_roughness
        +(dust_mask-0.5)*0.10-chalk*0.04-oil*0.15,0.58,1.);
    pbr.material.base_color=alpha_discard(pbr.material,pbr.material.base_color);
#ifdef PREPASS_PIPELINE
    return deferred_output(in,pbr);
#else
    var out:FragmentOutput;
    if (pbr.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT)==0u {out.color=apply_pbr_lighting(pbr);}
    else {out.color=pbr.material.base_color;}
    out.color=main_pass_post_lighting_processing(pbr,out.color);
    return out;
#endif
}
