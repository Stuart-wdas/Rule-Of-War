//! World-anchored grass blending, retaining the standard lighting and filtered mipmaps.
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef},
};

const SHADER: Handle<Shader> = uuid_handle!("22a8b24b-e48d-4dc1-a0b4-aa2ca8e48798");
const GROWTH_VERTEX: Handle<Shader> = uuid_handle!("dba9c78e-0ab1-49d4-a1ea-6e958ddc0001");
const GROWTH_PREPASS: Handle<Shader> = uuid_handle!("dba9c78e-0ab1-49d4-a1ea-6e958ddc0002");
pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassBlend>;
pub type GrowthMaterial = ExtendedMaterial<StandardMaterial, GrowthBlend>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GrowthBlend {
    #[texture(100)]
    #[sampler(101)]
    pub tracks: Handle<Image>,
    #[uniform(102)]
    pub track_world: Vec4,
}
impl MaterialExtension for GrowthBlend {
    fn vertex_shader() -> ShaderRef {
        GROWTH_VERTEX.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        GROWTH_PREPASS.into()
    }
    fn deferred_vertex_shader() -> ShaderRef {
        GROWTH_PREPASS.into()
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GrassBlend {
    #[uniform(113)]
    pub rural: RuralPaint,
    #[uniform(106)]
    pub debug: Vec4,
    #[uniform(107)]
    pub repeats: Vec4,
    #[uniform(111)]
    pub variation: Vec4,
    #[uniform(112)]
    pub photo_means: [Vec4; 3],
    #[texture(108)]
    #[sampler(109)]
    pub tracks: Handle<Image>,
    #[uniform(110)]
    pub track_world: Vec4,
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    pub texture: Handle<Image>,
    #[texture(102, dimension = "2d_array")]
    #[sampler(103)]
    pub normals: Handle<Image>,
    #[texture(104, dimension = "2d_array")]
    #[sampler(105)]
    pub orm: Handle<Image>,
}

/// Below the 16 KiB minimum uniform-buffer limit. These are
/// polygon coordinates, never a generated terrain texture or a new art asset.
#[derive(Clone, Debug, Reflect, ShaderType)]
pub struct RuralPaint {
    pub map: Vec4,
    pub corners: [Vec4; 224],
    pub styles: [UVec4; 28],
    pub bins: [UVec4; 512],
    pub yards: [Vec4; 4],
}
impl RuralPaint {
    pub fn new() -> Self {
        assert!(
            Self::min_size().get() <= 16 * 1024,
            "rural uniforms must fit the minimum GPU limit"
        );
        let mut result = Self {
            map: Vec4::ZERO,
            corners: [Vec4::ZERO; 224],
            styles: [UVec4::ZERO; 28],
            bins: [UVec4::ZERO; 512],
            yards: [Vec4::ZERO; 4],
        };
        result.set_fields(1024, 1024);
        result
    }
    fn set_fields(&mut self, cols: i32, rows: i32) {
        let (fields, bins) = crate::rural_layout::field_paint(cols, rows);
        assert!(fields.len() <= 112 && bins.len() == 256);
        self.corners.fill(Vec4::ZERO);
        self.styles.fill(UVec4::ZERO);
        self.bins.fill(UVec4::ZERO);
        for (i, field) in fields.iter().enumerate() {
            let p = field.corners;
            self.corners[i * 2] = Vec4::new(p[0][0], p[0][1], p[1][0], p[1][1]);
            self.corners[i * 2 + 1] = Vec4::new(p[2][0], p[2][1], p[3][0], p[3][1]);
            self.styles[i / 4][i % 4] = if field.harvested {
                2
            } else {
                u32::from(field.crop)
            };
        }
        for (bin, indices) in bins.iter().enumerate() {
            assert!(indices.len() <= 8, "field spatial index capacity");
            for (i, index) in indices.iter().enumerate() {
                self.bins[bin * 2 + i / 4][i % 4] = *index as u32 + 1;
            }
        }
    }
}
fn rural_map(
    game: Option<Res<crate::simulation::NativeGame>>,
    nature: Option<Res<crate::landscape::Nature>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    let (Some(game), Some(nature)) = (game, nature) else {
        return;
    };
    let map = Vec4::new(
        game.terrain.cols as f32 * 0.5,
        game.terrain.rows as f32 * 0.5,
        if game.terrain.id == crate::battlefield::ID {
            1.
        } else {
            0.
        },
        crate::rural_layout::field_cell_size(game.terrain.cols, game.terrain.rows),
    );
    if materials
        .get(&nature.grass)
        .is_some_and(|m| m.extension.rural.map != map)
    {
        let mut yards = [Vec4::ZERO; 4];
        if map.z > 0.5 {
            for (i, (at, farm)) in
                crate::battlefield::additional_sites(game.terrain.cols, game.terrain.rows)
                    .into_iter()
                    .enumerate()
            {
                let size = if farm { 38. } else { 54. };
                yards[i] = Vec4::new(at[0], at[1], size, size);
            }
        }
        let material = materials.get_mut(&nature.grass).unwrap();
        let rural = &mut material.extension.rural;
        rural.set_fields(game.terrain.cols, game.terrain.rows);
        rural.map = map;
        rural.yards = yards;
    }
}
impl MaterialExtension for GrassBlend {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<GrassMaterial>::default());
    app.add_plugins(MaterialPlugin::<GrowthMaterial>::default());
    app.add_systems(Update, rural_map);
    load_internal_asset!(app, SHADER, "grass_blend.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, GROWTH_VERTEX, "growth_vertex.wgsl", Shader::from_wgsl);
    load_internal_asset!(
        app,
        GROWTH_PREPASS,
        "growth_prepass.wgsl",
        Shader::from_wgsl
    );
}
