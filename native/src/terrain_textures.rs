// Included by surfaces.rs. Two grassy finishes retain the measured macro scan.
pub const TERRAIN_LAYERS: [&str; 4] = ["field_meadow_v1", "meadow_relief_v1", "soil", "asphalt"];

pub fn terrain_array(suffix: &str, size: u32) -> Image {
    let root = std::path::Path::new("materials");
    let names = crate::ground_demo::layers(crate::ground_demo::photographic());
    let cache = terrain_cache_path(root, suffix, size, names);
    if let Some(image) = cache.as_ref().and_then(|path| read_terrain_cache(path, suffix, size)) {
        crate::performance::record("terrain.texture.cache_hit", 0.);
        return image;
    }
    let _timing = crate::performance::span("terrain.texture.prepare");
    let image = terrain_array_from(root, suffix, size, names);
    if let Some(path) = cache { write_terrain_cache(&path, &image); }
    image
}

fn terrain_cache_path(root: &std::path::Path, suffix: &str, size: u32, names: [&str;5]) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut key = std::collections::hash_map::DefaultHasher::new();
    // Version covers filtering, array layout and sampler changes. Source edits
    // invalidate only derived data; the artist's original maps are untouched.
    "terrain-array-v2".hash(&mut key);
    root.canonicalize().ok()?.hash(&mut key);
    suffix.hash(&mut key); size.hash(&mut key);
    for name in names {
        name.hash(&mut key);
        let file = root.join("pbr").join(format!("{name}_{suffix}.png"));
        let meta = file.metadata().ok()?;
        meta.len().hash(&mut key);
        meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos().hash(&mut key);
    }
    Some(root.parent()?.join(".data/derived/terrain").join(format!("{:016x}.rowtex", key.finish())))
}
fn array_bytes(size: u32) -> usize {
    (0..=size.ilog2()).map(|m| ((size >> m).max(1) as usize).pow(2) * 4).sum::<usize>() * 5
}
fn read_terrain_cache(path: &std::path::Path, suffix: &str, size: u32) -> Option<Image> {
    let mut bytes = std::fs::read(path).ok()?;
    if bytes.len() != 16 + array_bytes(size) || &bytes[..8] != b"ROWTEX02"
        || u32::from_le_bytes(bytes[8..12].try_into().ok()?) != size
        || u32::from_le_bytes(bytes[12..16].try_into().ok()?) != size.ilog2() + 1 {
        return None;
    }
    let mut image = Image::new_uninit(
        Extent3d { width:size, height:size, depth_or_array_layers:5 },
        TextureDimension::D2,
        if suffix == "albedo" { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm },
        RenderAssetUsages::RENDER_WORLD);
    image.data = Some(bytes.split_off(16));
    image.texture_descriptor.mip_level_count = size.ilog2() + 1;
    image.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::D2Array), ..default()
    });
    image.data_order = bevy::render::render_resource::TextureDataOrder::LayerMajor;
    image.sampler = repeat_sampler();
    Some(image)
}
fn write_terrain_cache(path: &std::path::Path, image: &Image) {
    use std::io::Write;
    let Some(data) = image.data.as_ref() else { return; };
    let Some(parent) = path.parent() else { return; };
    if std::fs::create_dir_all(parent).is_err() { return; }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(b"ROWTEX02")?;
        file.write_all(&image.width().to_le_bytes())?;
        file.write_all(&image.texture_descriptor.mip_level_count.to_le_bytes())?;
        file.write_all(data)?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() { let _ = std::fs::remove_file(temporary); }
}

fn terrain_array_from(root: &std::path::Path, suffix: &str, size: u32, names: [&str;5]) -> Image {
    use bevy::render::render_resource::{TextureDataOrder, TextureViewDescriptor, TextureViewDimension};
    let srgb=suffix=="albedo";
    let format=if srgb {TextureFormat::Rgba8UnormSrgb} else {TextureFormat::Rgba8Unorm};
    let layers:Vec<_>=names.into_iter().map(|name| {
        let source=read_texture(root,name,suffix,srgb).or_else(|| {
            (name.starts_with("meadow") || name=="field_meadow_v1").then(|| read_texture(root,"grass",suffix,srgb)).flatten()
        });
        let fallback=match suffix {
            "normal" => [128,128,255,255],
            "orm" => [255,240,0,255],
            _ => [150,150,130,255],
        };
        let mut pixels=vec![0;(size*size*4) as usize];
        if let Some(source)=source {
            // Use an existing filtered mip before resampling legacy-size sources.
            let (mut w,mut h,mut offset)=(source.width(),source.height(),0usize);
            for _ in 1..source.texture_descriptor.mip_level_count {
                if w/2<size || h/2<size {break;}
                offset+=(w*h*4) as usize; w/=2; h/=2;
            }
            let data=source.data.as_ref().unwrap();
            for y in 0..size {for x in 0..size {
                let from=offset+(((y*h/size)*w+x*w/size)*4) as usize;
                let to=((y*size+x)*4) as usize;
                pixels[to..to+4].copy_from_slice(&data[from..from+4]);
            }}
        } else {for pixel in pixels.chunks_exact_mut(4) {pixel.copy_from_slice(&fallback);}}
        let mut image=Image::new(Extent3d{width:size,height:size,depth_or_array_layers:1},
            TextureDimension::D2,pixels,format,RenderAssetUsages::default());
        filtered_mips(&mut image,suffix=="normal");
        image
    }).collect();
    let mut image=layers[0].clone();
    image.texture_descriptor.size.depth_or_array_layers=layers.len() as u32;
    image.texture_view_descriptor=Some(TextureViewDescriptor{dimension:Some(TextureViewDimension::D2Array),..default()});
    image.data_order=TextureDataOrder::LayerMajor;
    image.data=Some(layers.into_iter().flat_map(|layer|layer.data.unwrap()).collect());
    image.asset_usage=RenderAssetUsages::RENDER_WORLD;
    image
}

pub fn terrain_repeats() -> (Vec4,Vec4) {
    let names=crate::ground_demo::layers(crate::ground_demo::photographic());
    let values=names.map(|name| 1. / pbr_repeat(name).map_or(2.5,|r|r.x));
    let catalog:serde_json::Value=std::fs::read_to_string("materials/pbr/catalog.json").ok()
        .and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default();
    let gain=|i:usize|catalog["materials"][names[i]]["linear_reflectance_gain"].as_f64()
        .filter(|v|v.is_finite()&&*v>0.&&*v<=2.).unwrap_or(1.) as f32;
    (Vec4::new(values[0],values[1],values[2],values[3]),Vec4::new(values[4],gain(0),gain(2),gain(4)))
}

pub fn terrain_means() -> [Vec4;3] {
    let catalog:serde_json::Value=std::fs::read_to_string("materials/pbr/catalog.json").ok()
        .and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default();
    let layers=crate::ground_demo::layers(crate::ground_demo::photographic());
    [0,2,4].map(|i| {
        let rgb=&catalog["materials"][layers[i]]["albedo_linear_mean_rgb"];
        let channel=|n|rgb[n].as_f64().filter(|v|v.is_finite() && (0. ..=1.).contains(v)).unwrap_or(0.15) as f32;
        Vec4::new(channel(0),channel(1),channel(2),0.)
    })
}

#[cfg(test)]
mod terrain_texture_tests {
    use super::*;
    #[test]
    fn prepared_cache_preserves_every_mip_and_rejects_truncated_data() {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../materials");
        let path=std::env::temp_dir().join(format!("row-terrain-cache-{}.rowtex",std::process::id()));
        let image=terrain_array_from(&root,"normal",8,crate::ground_demo::layers(true));
        write_terrain_cache(&path,&image);
        let cached=read_terrain_cache(&path,"normal",8).unwrap();
        assert_eq!(image.data,cached.data);
        assert_eq!(image.texture_descriptor.mip_level_count,cached.texture_descriptor.mip_level_count);
        assert_eq!(image.texture_descriptor.size,cached.texture_descriptor.size);
        assert_eq!(image.texture_view_descriptor,cached.texture_view_descriptor);
        assert_eq!(image.sampler,cached.sampler);
        assert!(read_terrain_cache(&path,"normal",16).is_none());
        std::fs::write(&path,b"ROWTEX02").unwrap();
        assert!(read_terrain_cache(&path,"normal",8).is_none());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn five_pbr_layers_keep_color_data_normals_and_macro_scan_in_register() {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../materials");
        let names=crate::ground_demo::layers(true);
        for suffix in ["albedo","normal","orm"] {
            let image=terrain_array_from(&root,suffix,32,names);
            assert_eq!(image.texture_descriptor.size.depth_or_array_layers,5);
            assert_eq!(image.texture_descriptor.mip_level_count,6);
            assert_eq!(image.texture_descriptor.format,if suffix=="albedo" {TextureFormat::Rgba8UnormSrgb} else {TextureFormat::Rgba8Unorm});
            let stride=(0..6).map(|m|(32usize>>m).pow(2)*4).sum::<usize>();
            let data=image.data.as_ref().unwrap();
            assert_eq!(data.len(),stride*5);
            for (layer,name) in names.iter().enumerate() {
                let source=read_texture(&root,name,suffix,suffix=="albedo").unwrap();
                // Each array slice really begins with its matching material,
                // including the extra grass slice, rather than a padded repeat.
                let (mut w,mut h,mut offset)=(source.width(),source.height(),0);
                for _ in 1..source.texture_descriptor.mip_level_count {
                    if w/2<32 || h/2<32 {break;}
                    offset+=(w*h*4) as usize; w/=2; h/=2;
                }
                assert_eq!(&data[layer*stride..layer*stride+4],&source.data.as_ref().unwrap()[offset..offset+4]);
            }
        }
        assert_eq!(repeat_from(&root,names[1]),Some(Vec2::splat(90.)));
        assert_eq!(repeat_from(&root,names[0]),Some(Vec2::splat(4.)));
        assert_eq!(repeat_from(&root,names[4]),Some(Vec2::splat(3.)));
    }
}
