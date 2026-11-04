// Included by surfaces.rs. Build mip levels once, leaving source PNG files intact.
pub(crate) fn filtered_mips(image: &mut Image, normal: bool) {
    let srgb = image.texture_descriptor.format == TextureFormat::Rgba8UnormSrgb;
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
    ) || image.texture_descriptor.mip_level_count != 1
        || image.texture_descriptor.size.depth_or_array_layers != 1
    {
        return;
    }
    let Some(mut level) = image.data.take() else {
        return;
    };
    let (mut width, mut height) = (image.width() as usize, image.height() as usize);
    let decode: [f32; 256] = std::array::from_fn(|i| {
        let v = i as f32 / 255.;
        if srgb {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        } else {
            v
        }
    });
    let encode: [u8; 4097] = std::array::from_fn(|i| {
        let v = i as f32 / 4096.;
        let v = if srgb {
            if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        } else {
            v
        };
        (v * 255.).round() as u8
    });
    let mut data = level.clone();
    let mut levels = 1;
    while width > 1 || height > 1 {
        let (nw, nh) = ((width / 2).max(1), (height / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                let mut rgb = Vec3::ZERO;
                let (mut alpha, mut count) = (0., 0.);
                for sy in y * height / nh..(y + 1) * height / nh {
                    for sx in x * width / nw..(x + 1) * width / nw {
                        let pixel = &level[(sy * width + sx) * 4..][..4];
                        let a = pixel[3] as f32 / 255.;
                        let color = if normal {
                            Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 127.5
                                - Vec3::ONE
                        } else {
                            Vec3::new(
                                decode[pixel[0] as usize],
                                decode[pixel[1] as usize],
                                decode[pixel[2] as usize],
                            )
                        };
                        rgb += color * a;
                        alpha += a;
                        count += 1.;
                    }
                }
                if normal {
                    rgb = rgb.normalize_or(Vec3::Z) * 0.5 + Vec3::splat(0.5);
                    next.extend(rgb.to_array().map(|v| (v * 255.).round() as u8));
                } else {
                    rgb /= alpha.max(0.000001);
                    next.extend(
                        rgb.to_array()
                            .map(|v| encode[(v.clamp(0., 1.) * 4096.).round() as usize]),
                    );
                }
                next.push((alpha / count * 255.).round() as u8);
            }
        }
        data.extend(&next);
        level = next;
        (width, height) = (nw, nh);
        levels += 1;
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = repeat_sampler();
}

#[cfg(test)]
mod filtering_tests {
    use super::*;
    fn texture(width: u32, height: u32, data: Vec<u8>, srgb: bool) -> Image {
        Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            if srgb {
                TextureFormat::Rgba8UnormSrgb
            } else {
                TextureFormat::Rgba8Unorm
            },
            RenderAssetUsages::default(),
        )
    }
    #[test]
    fn checkerboard_mips_remove_aliasing_in_linear_light_and_filter_between_levels() {
        let mut image = texture(
            2,
            2,
            vec![
                0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
            ],
            true,
        );
        filtered_mips(&mut image, false);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert!((186..=189).contains(&image.data.as_ref().unwrap()[16]));
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("filtered sampler required");
        };
        assert_eq!(sampler.min_filter, bevy::image::ImageFilterMode::Linear);
        assert_eq!(sampler.mipmap_filter, bevy::image::ImageFilterMode::Linear);
        assert_eq!(sampler.anisotropy_clamp, 16);
    }
    #[test]
    fn odd_images_preserve_edges_alpha_colour_and_normal_directions() {
        let mut image = texture(3, 1, vec![255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0], true);
        filtered_mips(&mut image, false);
        assert_eq!(&image.data.as_ref().unwrap()[12..], &[255, 0, 0, 85]);
        let mut normal = texture(2, 1, vec![255, 128, 128, 255, 128, 128, 255, 255], false);
        filtered_mips(&mut normal, true);
        let v = &normal.data.as_ref().unwrap()[8..];
        let n = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 127.5 - Vec3::ONE;
        assert!((n.length() - 1.).abs() < 0.01 && n.x > 0.7 && n.z > 0.7);
    }
}
