#import bevy_pbr::{
    pbr_types, pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::{prepass_io::{VertexOutput, FragmentOutput}, pbr_deferred_functions::deferred_output}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var grass_texture: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var terrain_normals: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var terrain_orm: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var orm_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var<uniform> terrain_debug: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var<uniform> repeats: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var tracks: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var track_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var<uniform> track_world: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var<uniform> variation: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var<uniform> photo_means: array<vec4<f32>,3>;
struct RuralPaint {
    map: vec4<f32>,
    corners: array<vec4<f32>,224>,
    styles: array<vec4<u32>,28>,
    bins: array<vec4<u32>,512>,
    yards: array<vec4<f32>,4>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var<uniform> rural: RuralPaint;

fn parcel_inset(index: u32, p: vec2<f32>) -> f32 {
    let ab = rural.corners[index * 2u];
    let cd = rural.corners[index * 2u + 1u];
    var corners = array<vec2<f32>,4>(ab.xy, ab.zw, cd.xy, cd.zw);
    var inset = 100000.;
    for (var i = 0u; i < 4u; i += 1u) {
        let a = corners[i];
        let d = corners[(i + 1u) % 4u] - a;
        inset = min(inset, (d.x * (p.y - a.y) - d.y * (p.x - a.x)) / length(d));
    }
    return inset;
}
struct FieldSurface {
    paint: vec4<f32>,
    slope: vec2<f32>,
    roughness: f32,
}
// Colour, planted rows and soil coverage share exact polygon edges at every LOD.
fn rural_ground(world: vec2<f32>, footprint: f32) -> FieldSurface {
    let q = world - rural.map.xy;
    let p = vec2<f32>(q.x - q.y, q.x + q.y) * 0.70710678118;
    let village = all(abs(p) < vec2<f32>(154.,102.));
    var parcel = 112u;
    var inset = 0.;
    if village {
        for (var i = 0u; i < 11u; i += 1u) {
            let distance = parcel_inset(i,p);
            if distance > 0. { parcel = i; inset = distance; break; }
        }
    }
    if parcel == 112u {
        // Field paint, hedges and tracks share the village's un-sheared axes.
        let cell = vec2<i32>(floor(p / rural.map.w + vec2<f32>(8.)));
        if all(cell >= vec2<i32>(0)) && all(cell < vec2<i32>(16)) {
            let bin = u32(cell.y * 16 + cell.x);
            for (var i = 0u; i < 8u; i += 1u) {
                let entry = rural.bins[bin * 2u + i / 4u][i % 4u];
                if entry == 0u { break; }
                let distance = parcel_inset(entry - 1u,p);
                if distance >= 0. { parcel = entry - 1u; inset = distance; break; }
            }
        }
    }
    if parcel == 112u { return FieldSurface(vec4<f32>(0.93,1.04,0.84,0.035),vec2<f32>(0.),0.); }
    let style = rural.styles[parcel / 4u][parcel % 4u];
    let seed = hash(vec2<f32>(f32(parcel)+3.7,f32(parcel)*1.91));
    let ab = rural.corners[parcel * 2u];
    let along = normalize(ab.zw-ab.xy);
    let across = vec2<f32>(-along.y,along.x);
    let coordinate = dot(p-ab.xy,across);
    // Dry and vigorous patches remain several metres wide at tactical zoom.
    // They affect material response as well as colour, not a screen-space filter.
    let fertility = noise(p / 34. + seed * 79.) * 0.68
        + noise(p / 9. + seed.yx * 43.) * 0.32;
    let rows_visible = 1.-smoothstep(0.18,0.65,footprint*0.72);
    let phase = coordinate * 4.5 + seed.x * 6.28;
    let rows = sin(phase) * rows_visible;
    var tint = vec3<f32>(0.91,1.04,0.72);
    var dirt = 0.07;
    var slope = vec2<f32>(0.);
    var roughness = (fertility-0.5)*0.07;
    if style == 2u {
        tint = vec3<f32>(1.12,0.94,0.66) * (1.+rows*0.022);
        dirt = 0.80 + rows * 0.065;
    } else if style == 1u {
        tint = vec3<f32>(0.58,0.87,0.47) * (1.+rows*0.025);
        dirt = 0.03;
    }
    tint *= mix(vec3<f32>(0.88,0.92,0.79),vec3<f32>(1.06,1.045,0.99),fertility);
    tint *= mix(0.96,1.04,seed.y);
    if style != 0u {
        // Paired tractor tramlines and a worked headland reuse exposed soil.
        // Their spacing follows each parcel, with no new images or geometry.
        let wheel_distance = abs(abs(fract((coordinate+seed.x*11.)/20.)-0.5)*20.-1.4);
        let tram = (1.-smoothstep(0.32,0.72+footprint*0.35,wheel_distance))
            * (1.-smoothstep(0.45,1.3,footprint));
        let headland = 1.-smoothstep(3.,8.,inset);
        let interior = smoothstep(4.,9.,inset);
        dirt = clamp(dirt+tram*interior*0.23+headland*0.06,0.,1.);
        tint *= 1.-tram*interior*0.075-headland*0.035;
        let world_across = vec2<f32>(across.x+across.y,across.y-across.x)*0.70710678118;
        slope = world_across * cos(phase) * rows_visible * interior * select(0.12,0.08,style==1u);
        roughness += tram*0.025;
    } else {
        dirt += smoothstep(0.56,0.78,fertility)*0.10;
    }
    let edge = clamp(inset / max(2.5,footprint * 0.85),0.,1.);
    return FieldSurface(vec4<f32>(mix(vec3<f32>(1.),tint,edge),dirt*edge),slope*edge,roughness*edge);
}

fn hash(p: vec2<f32>) -> vec2<f32> {
    return fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
}
fn field_corner(cell: vec2<f32>, delta: vec2<f32>) -> f32 {
    let falloff = max(0.5 - dot(delta, delta), 0.0);
    let gradient = hash(cell) * 2.0 - vec2<f32>(1.0);
    return falloff * falloff * falloff * falloff * dot(gradient, delta);
}
fn noise(p: vec2<f32>) -> f32 {
    // Radial simplex kernels avoid the square plateaus of thresholded value
    // noise. The colour field stays continuous across terrain mesh boundaries.
    let skew = (p.x + p.y) * 0.366025404;
    let cell = floor(p + vec2<f32>(skew));
    let delta = p - cell + vec2<f32>((cell.x + cell.y) * 0.211324865);
    let corner = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), delta.x > delta.y);
    let value = field_corner(cell, delta)
        + field_corner(cell + corner, delta - corner + vec2<f32>(0.211324865))
        + field_corner(cell + vec2<f32>(1.0), delta - vec2<f32>(0.577350270));
    return clamp(0.5 + value * 42.0, 0.0, 1.0);
}
fn filtered_field(world: vec2<f32>, frequency: f32, footprint: f32) -> f32 {
    // Only retain variation large enough to resolve. Shoulder grit becomes its
    // filtered average at overview distance instead of crawling as white dots.
    let resolved = 1. - smoothstep(0.30, 0.95, footprint * frequency);
    return mix(0.5, noise(world * frequency), resolved);
}
struct GroundSample {
    color: vec3<f32>,
    normal: vec3<f32>,
    data: vec3<f32>,
}
fn ground_sample(uv: vec2<f32>, layer: i32, dx: vec2<f32>, dy: vec2<f32>) -> GroundSample {
    return GroundSample(
        textureSampleGrad(grass_texture, grass_sampler, uv, layer, dx, dy).rgb,
        textureSampleGrad(terrain_normals, normal_sampler, uv, layer, dx, dy).rgb * 2. - 1.,
        textureSampleGrad(terrain_orm, orm_sampler, uv, layer, dx, dy).rgb);
}
fn photo_sample(uv: vec2<f32>, layer: i32, dx: vec2<f32>, dy: vec2<f32>) -> GroundSample {
    // At overview scale each pixel covers many source texels: the filtered
    // mip already removes tile repetition. Avoid nine PBR fetches per layer
    // for detail smaller than a screen pixel, with a continuous transition.
    let distant = smoothstep(0.04, 0.16, max(length(dx), length(dy)));
    if distant >= 0.999 {
        return ground_sample(uv, layer, dx, dy);
    }
    // Continuous triangular blending removes visible grid repetition. All PBR
    // channels use identical offsets: a coloured pebble retains its own normal.
    let skew = vec2<f32>(uv.x - uv.y * 0.57735027, uv.y * 1.15470054);
    let cell = floor(skew);
    let f = fract(skew);
    var points = array<vec2<f32>,3>(cell, cell + vec2<f32>(1.,0.), cell + vec2<f32>(0.,1.));
    var w = vec3<f32>(1. - f.x - f.y, f.x, f.y);
    if f.x + f.y > 1. {
        points = array<vec2<f32>,3>(cell + vec2<f32>(1.,1.), cell + vec2<f32>(0.,1.), cell + vec2<f32>(1.,0.));
        w = vec3<f32>(f.x + f.y - 1., 1. - f.x, 1. - f.y);
    }
    // Mostly retain a single detailed patch; soften only the joins. Translation
    // preserves the fixed sunlight direction in the small baked contact shadows.
    w = pow(max(w,vec3<f32>(0.)),vec3<f32>(5.));
    w /= w.x + w.y + w.z;
    var c = vec3<f32>(0.);
    var n = vec3<f32>(0.);
    var d = vec3<f32>(0.);
    for (var i = 0; i < 3; i += 1) {
        let sampled = ground_sample(uv + hash(points[i]) * 13.7, layer, dx, dy);
        c += sampled.color * w[i];
        n += sampled.normal * w[i];
        d += sampled.data * w[i];
    }
    // Preserve local contrast lost by averaging three independently offset
    // samples. Avoid a flat grey veil across the generated colour detail.
    var mean_index = 0u;
    if layer == 2 { mean_index = 1u; }
    if layer == 4 { mean_index = 2u; }
    let mean = photo_means[mean_index].rgb;
    c = clamp(mean + (c - mean) / sqrt(dot(w,w)),vec3<f32>(0.),vec3<f32>(1.));
    if distant > 0.001 {
        let far_sample = ground_sample(uv, layer, dx, dy);
        return GroundSample(mix(c,far_sample.color,distant),
            normalize(mix(normalize(n),far_sample.normal,distant)),mix(d,far_sample.data,distant));
    }
    return GroundSample(c, normalize(n), d);
}
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let world = in.world_position.xz;
    let footprint = max(length(dpdx(world)), length(dpdy(world)));
    let broad = noise(world / 61.0);
    var coverage = vec2<f32>(0.0);
#ifdef VERTEX_UVS_B
    coverage = clamp(in.uv_b, vec2<f32>(0.), vec2<f32>(1.));
#endif
    var field_tint = vec3<f32>(1.);
    var field_slope = vec2<f32>(0.);
    var field_roughness = 0.;
    if rural.map.z > 0.5 {
        var field = rural_ground(world,footprint);
        for (var i=0u;i<4u;i+=1u) {
            let yard=rural.yards[i];
            if yard.z>0.5 {
                let d=abs(world-yard.xy)-yard.zw;
                let cleared=1.-smoothstep(0.,7.,max(d.x,d.y));
                field.paint=mix(field.paint,vec4<f32>(0.93,1.04,0.84,0.035),cleared);
                field.slope*=1.-cleared;
                field.roughness*=1.-cleared;
            }
        }
        coverage = vec2<f32>(field.paint.a,0.);
        field_tint = pow(field.paint.rgb,vec3<f32>(2.2));
        field_slope = field.slope;
        field_roughness = field.roughness;
    }
    // Fine meadow albedo supplies most of the visible grass, soil and gravel.
    // Retain a smaller share of the aerial scan for broad, irregular variation.
    let photographic = terrain_debug.z > 0.5;
    let legacy_noise = !photographic || terrain_debug.w > 0.5;
    let scan_range = select(vec2<f32>(0.42,0.60), vec2<f32>(0.04,0.10), photographic);
    let scanned = mix(scan_range.x, scan_range.y, smoothstep(0.28, 0.74, noise(world / 17.0)));
    let stony = select(0., smoothstep(0.29,0.73,noise(world / 19.)), photographic);
    // The authored masks define roads and exposed earth. Break only their
    // mixed boundaries: full meadow and the asphalt core remain untouched.
    // Unwarped, filtered fields avoid the previous pale contour-like swirls.
    let aggregate = filtered_field(world + vec2<f32>(7.1,19.3), 0.72, footprint) - 0.5;
    let fine_edge = filtered_field(world + vec2<f32>(31.7,4.8), 2.6, footprint) - 0.5;
    let road_edge = 4. * coverage.y * (1. - coverage.y);
    let soil_edge = smoothstep(0.18,0.32,coverage.x) * (1. - smoothstep(0.82,0.98,coverage.x));
    let boundary = aggregate * 0.23 + fine_edge * 0.09;
    let dirt_jitter = boundary * soil_edge * 0.70;
    let gravel_jitter = boundary * road_edge;
    let dirt_amount = smoothstep(0.12, 0.88, clamp(coverage.x + dirt_jitter, 0.0, 1.0));
    let gravel_amount = smoothstep(0.26, 0.82, clamp(coverage.y + gravel_jitter, 0.0, 1.0));
    let meadow_weight = (1. - dirt_amount) * (1. - gravel_amount);
    let weights = vec4<f32>((1. - scanned) * meadow_weight * (1. - stony), scanned * meadow_weight,
        dirt_amount * (1. - gravel_amount), gravel_amount);
    let stony_weight = (1. - scanned) * meadow_weight * stony;
    var ground = vec3<f32>(0.);
    var surface_normal = vec3<f32>(0.);
    var surface_data = vec3<f32>(0.);
    var detail = vec3<f32>(0.);
    var meadow_normal = vec3<f32>(0.,0.,1.);
    let track_uv = world * track_world.xy;
    let rut = textureSampleLevel(tracks, track_sampler, clamp(track_uv,vec2<f32>(0.),vec2<f32>(1.)), 0.).r;
    var disturbed = GroundSample(vec3<f32>(0.),vec3<f32>(0.,0.,1.),vec3<f32>(1.,0.95,0.));
    for (var layer = 0; layer < 5; layer += 1) {
        var layer_weight = stony_weight;
        var physical_repeat = variation.x;
        if layer < 4 {
            layer_weight = weights[layer];
            physical_repeat = repeats[layer];
        }
        var uv = world * physical_repeat;
        // Keep the measured meadow height, normal and geometry in register.
        let dx = dpdx(uv);
        let dy = dpdy(uv);
        let track_soil = layer == 2 && rut > 0.01 && meadow_weight > 0.002;
        if photographic && (layer == 0 || layer == 2 || layer == 4) {
            if layer_weight > 0.002 || track_soil {
                var sampled = photo_sample(uv, layer, dx, dy);
                // Calibrate reflectance per source, after stochastic contrast
                // compensation. Preserve the artist's colour detail and PBR UVs.
                var reflectance_gain = variation.y;
                if layer == 2 { reflectance_gain = variation.z; }
                if layer == 4 { reflectance_gain = variation.w; }
                var mean_index = 0u;
                if layer == 2 { mean_index = 1u; }
                if layer == 4 { mean_index = 2u; }
                // The gameplay view filters fine strands. Retain the source's
                // broad clump/stone contrast without sharpening alpha or normals.
                let source_mean = photo_means[mean_index].rgb;
                sampled.color = max(source_mean + (sampled.color - source_mean) * 1.40, vec3<f32>(0.));
                sampled.color *= reflectance_gain;
                ground += sampled.color * layer_weight;
                surface_data += sampled.data * layer_weight;
                surface_normal += sampled.normal * layer_weight;
                if layer == 0 { detail = sampled.color; }
                if layer == 2 { disturbed = sampled; }
            }
            continue;
        }
        if layer == 4 { continue; }
        var sampled_normal = vec3<f32>(0.,0.,1.);
        var normal_sampled = false;
        if layer_weight > 0.002 || track_soil {
            let sampled_color = textureSampleGrad(grass_texture, grass_sampler, uv, layer, dx, dy).rgb;
            let sampled_data = textureSampleGrad(terrain_orm, orm_sampler, uv, layer, dx, dy).rgb;
            ground += sampled_color * layer_weight;
            surface_data += sampled_data * layer_weight;
            if layer == 0 { detail = sampled_color; }
            if layer != 0 {
                sampled_normal = textureSampleGrad(terrain_normals, normal_sampler, uv, layer, dx, dy).rgb * 2. - 1.;
                normal_sampled = true;
            }
            if layer == 2 { disturbed = GroundSample(sampled_color,sampled_normal,sampled_data); }
        }
        // Both meadow colour layers share the measured scan normal at its
        // original 90 m scale. The generated albedo's neutral normal must not
        // flatten this relief; soil and asphalt keep their coverage weights.
        let normal_weight = select(layer_weight, meadow_weight, layer == 1 && !photographic);
        if (layer != 0 || photographic) && (normal_weight > 0.002 || (layer == 1 && meadow_weight + weights.z > 0.002)) {
            if !normal_sampled {
                sampled_normal = textureSampleGrad(terrain_normals, normal_sampler, uv, layer, dx, dy).rgb * 2. - 1.;
            }
            surface_normal += sampled_normal * normal_weight;
            if layer == 1 { meadow_normal = sampled_normal; }
        }
    }
    // Rough soil keeps its own small-scale grain and gains the same measured
    // meadow relief as grass. Asphalt and prepared lanes contribute no mask.
    let measured_relief = weights.z * select(0.72,0.08,photographic) + meadow_weight * select(0.,0.12,photographic);
    surface_normal = normalize(surface_normal + vec3<f32>(meadow_normal.xy * measured_relief,0.));
    // A readable macro palette survives mip filtering at RTS distances. The
    // same field is sampled at every LOD, so patches never pop on chunk changes.
    let meadow = mix(vec3<f32>(0.94, 0.97, 0.90), vec3<f32>(1.045, 1.025, 0.96), smoothstep(0.25, 0.77, broad));
    let micro = select(1.,mix(0.98, 1.02, filtered_field(world,0.37,footprint)),legacy_noise);
    ground *= mix(vec3<f32>(1.0), meadow, meadow_weight) * micro;
    // Broad worn patches survive distance filtering; fractured edges blend
    // into the soil shoulder rather than terminating at a clean painted line.
    let wear = noise(world * 0.085) * 0.7 + filtered_field(world + vec2<f32>(23.4,5.6),0.31,footprint) * 0.3;
    let road_tint = mix(vec3<f32>(0.94, 0.955, 0.96), vec3<f32>(1.06, 1.045, 1.015), wear);
    ground *= mix(vec3<f32>(1.0), road_tint, gravel_amount);
    surface_data.g = clamp(surface_data.g + (wear - 0.5) * gravel_amount * 0.06,0.,1.);
    // World-space wear survives terrain LOD changes. The mask is updated only
    // when vehicles move, and has no authority over navigation or collision.
    if rut > 0.01 {
        // Exposed earth reuses the same stochastic sample as the shoulder,
        // including its normal/roughness. It cannot slide against its own grit.
        let pressure = smoothstep(0.02,0.96,rut);
        let exposure = pressure * meadow_weight * 0.78;
        ground = mix(ground, disturbed.color * vec3<f32>(0.88,0.85,0.79), exposure);
        ground *= 1. - pressure * mix(0.11,0.04,gravel_amount);
        let pressed_normal = normalize(mix(vec3<f32>(0.,0.,1.),disturbed.normal,0.28));
        surface_normal = normalize(mix(surface_normal,pressed_normal,pressure * 0.58));
        surface_data = mix(surface_data,disturbed.data,exposure);
        surface_data.g = mix(surface_data.g,0.86,pressure * (1. - gravel_amount * 0.65) * 0.45);
    }
    if terrain_debug.y > 0.5 {
        let luma = dot(ground, vec3<f32>(0.22, 0.68, 0.10));
        ground = mix(vec3<f32>(luma), ground, 0.94) * vec3<f32>(0.97, 1.0, 0.94);
    }
    pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb * ground * field_tint, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = clamp(surface_data.g+field_roughness, 0.55, 1.0);
    pbr_input.material.metallic = 0.;
    pbr_input.diffuse_occlusion *= mix(1., clamp(surface_data.r,0.65,1.), 0.70);
    surface_normal = normalize(vec3<f32>(surface_normal.xy * select(1.35,1.10,photographic), surface_normal.z));
    surface_normal = normalize(vec3<f32>(surface_normal.xy+field_slope,surface_normal.z));
    // OpenGL normals use +Y along the source's V/world-Z direction. Keep the
    // same physical UVs for all three maps and blend vectors before lighting.
    let tangent_x = normalize(vec3<f32>(1.,0.,0.) - pbr_input.N * pbr_input.N.x);
    let tangent_z = normalize(cross(tangent_x, pbr_input.N));
    pbr_input.N = normalize(tangent_x * surface_normal.x + tangent_z * surface_normal.y + pbr_input.N * surface_normal.z);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    apply_decals(&pbr_input);
#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    if terrain_debug.x > 0.5 {
        var diagnostic = ground;
        if terrain_debug.x > 1.5 { diagnostic = vec3<f32>(coverage, 0.0); }
        if terrain_debug.x > 2.5 { diagnostic = detail; }
#ifdef VERTEX_COLORS
        if terrain_debug.x > 3.5 { diagnostic = in.color.rgb; }
#endif
        if terrain_debug.x > 4.5 { diagnostic = vec3<f32>(rut); }
        if terrain_debug.x > 5.5 { diagnostic = vec3<f32>(weights.x,stony_weight,0.); }
        if terrain_debug.x > 6.5 { diagnostic = vec3<f32>(dirt_amount,gravel_amount,0.); }
        out.color = vec4<f32>(diagnostic, 1.0);
    }
    return out;
#endif
}
