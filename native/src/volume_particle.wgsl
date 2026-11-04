#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::{position_ndc_to_world},
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils::prepass_depth
#endif

// Fragment coordinates belong to the internal main-pass resolution when DLSS
// is active; the output viewport remains at the monitor's native resolution.
fn frag_coord_to_ndc(p: vec4<f32>) -> vec3<f32> {
    let uv=(p.xy-view.main_pass_viewport.xy)/view.main_pass_viewport.zw;
    return vec3(uv*vec2(2.,-2.)+vec2(-1.,1.),p.z);
}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> settings: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> profile: vec4<f32>;

fn hash(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p); let f = fract(p); let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash(i), hash(i + vec3(1.,0.,0.)), u.x), mix(hash(i + vec3(0.,1.,0.)), hash(i + vec3(1.,1.,0.)), u.x), u.y), mix(mix(hash(i + vec3(0.,0.,1.)), hash(i + vec3(1.,0.,1.)), u.x), mix(hash(i + vec3(0.,1.,1.)), hash(i + vec3(1.,1.,1.)), u.x), u.y), u.z);
}
fn billow(p: vec3<f32>, seed: f32) -> f32 {
    let q = p * 4.3 + vec3(seed, seed * 0.37, seed * 0.71);
    var n = noise(q) * 0.64 + noise(q * 2.13 + vec3(17.1)) * 0.26;
    if settings.z > 0.5 { n += noise(q * 4.7) * 0.1; }
    else { n += 0.05; }
    return n;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tag = mesh_functions::get_tag(in.instance_index);
    let opacity = f32(tag & 255u) / 255.0;
    if opacity < 0.005 { discard; }
    let heat = f32((tag >> 8u) & 255u) / 255.0;
    let seed = f32((tag >> 16u) & 16383u) * 0.017;
    let dust = tag >> 30u == 2u;
    let inverse = mesh_functions::get_local_from_world(in.instance_index);
    var origin = (inverse * vec4(view.world_position, 1.)).xyz;
    // Orthographic rays have parallel directions and different origins.
    // Using the eye position makes volumes slide/flatten against this camera.
    if view.clip_from_view[3][3] > 0.5 {
        let near_world = position_ndc_to_world(frag_coord_to_ndc(vec4(in.position.xy, 1., 1.)));
        origin = (inverse * vec4(near_world, 1.)).xyz;
    }
    let surface = (inverse * in.world_position).xyz;
    let direction = normalize(surface - origin);
    let b = dot(origin, direction);
    let c = dot(origin, origin) - 1.0;
    let discriminant = b * b - c;
    if discriminant <= 0. { discard; }
    let entry = max(0., -b - sqrt(discriminant));
    var exit = -b + sqrt(discriminant);
#ifdef DEPTH_PREPASS
    let depth = prepass_depth(in.position, 0u);
    if depth > 0.000001 {
        let opaque_world = position_ndc_to_world(frag_coord_to_ndc(vec4(in.position.xy, depth, 1.)));
        let opaque_local = (inverse * vec4(opaque_world, 1.)).xyz;
        exit = min(exit, dot(opaque_local - origin, direction));
    }
#endif
    if exit <= entry { discard; }
    let steps = u32(settings.y);
    let stride = (exit - entry) / f32(steps);
    // A screen-pixel seed changes abruptly as a moving lobe crosses pixels.
    // On long smoke volumes that moves the sampled density up/down the ray.
    // Keep a stratified phase attached to the particle instead of the screen.
    var jitter = hash(vec3(seed, 1.7, 8.3));
    if settings.w > 0.5 { jitter = hash(vec3(floor(in.position.xy), seed)); }
    var sum = vec3(0.);
    var alpha = 0.;
    for (var i = 0u; i < steps; i += 1u) {
        let p = origin + direction * (entry + (f32(i) + jitter) * stride);
        let n = billow(p, seed);
        let envelope = 1. - smoothstep(0.48, 0.98, length(p) + (0.5 - n) * 0.5);
        let density = max(0., n - 0.28) * mix(4.5, 7.2, profile.x) * envelope * opacity;
        let a = 1. - exp(-density * stride * 3.5);
        // Bright pockets lie within cooler soot, instead of painting every
        // lobe orange. Noise is evaluated in 3D local space, never in screen UVs.
        let fire = pow(heat, 1.3) * smoothstep(0.5, 0.68, n + heat * 0.1) * (1. - smoothstep(0.6, 1., length(p)));
        let shadow = billow(p + vec3(-0.2, 0.3, 0.15), seed);
        let shade = clamp(0.8 + p.y * 0.35 - p.x * 0.13 - shadow * 0.9 + n * 0.4, 0.15, 1.);
        var smoke = vec3(0.075, 0.09, 0.11) * shade;
        if dust { smoke = vec3(0.28, 0.23, 0.16) * shade; }
        let radiance = mix(vec3(8., 0.25, 0.003), vec3(22., 5.5, 0.12), smoothstep(0.1, 0.65, fire));
        var color = smoke + radiance * fire * 0.7 * select(1., 0., dust);
        if profile.x > 0.5 {
            let sun = normalize((inverse * vec4(-0.444, 0.778, 0.444, 0.)).xyz);
            // Integrate a short light ray through each billow. This creates a
            // sun-facing rim and dark interiors, independently of the camera.
            var optical = 0.;
            for (var j=1u; j<=3u; j+=1u) {
                let q = p + sun * f32(j) * 0.20;
                let sample = billow(q, seed);
                let body = 1. - smoothstep(0.48, 0.98, length(q)+(0.5-sample)*0.5);
                optical += max(0.,sample-0.28)*body*opacity;
            }
            let transmission = exp(-optical*8.0);
            let edge = clamp(dot(normalize(p+vec3(0.001)),sun)*0.5+0.5,0.,1.);
            let illumination = 0.065 + transmission*(0.50+edge*1.4);
            var pigment=vec3(0.32,0.295,0.26);
            if dust {pigment=vec3(0.46,0.39,0.28);}
            let flame = pow(heat,0.85)*smoothstep(0.40,0.62,n)
                *(1.-smoothstep(0.55,0.97,length(p)));
            let glow = mix(vec3(5.,0.35,0.025),vec3(17.,6.,0.7),smoothstep(0.15,0.75,flame));
            color = pigment*illumination + glow*flame*0.9*select(1.,0.,dust);
        }
        sum += (1. - alpha) * a * color;
        alpha += (1. - alpha) * a;
        if alpha > 0.985 { break; }
    }
    if alpha < 0.006 { discard; }
    return vec4(sum / max(alpha, 0.0001), alpha);
}
