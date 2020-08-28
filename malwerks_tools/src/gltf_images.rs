// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;

use crate::texconv::*;

pub fn import_images(
    static_scenery: &mut DiskStaticScenery,
    base_path: &std::path::Path,
    materials: gltf::iter::Materials,
    images: gltf::iter::Images,
) {
    macro_rules! update_image_usage {
        ($image_usage: ident, $texture: expr, $usage: expr) => {
            if let Some(info) = $texture {
                if let Some(old_usage) = $image_usage[info.texture().index()] {
                    assert_eq!(old_usage, $usage);
                } else {
                    $image_usage[info.texture().index()] = Some($usage);
                }
            }
        };
    }

    let mut images_usage = Vec::with_capacity(images.len());
    images_usage.resize(images.len(), None);

    for material in materials {
        let pbr_metallic_roughness = material.pbr_metallic_roughness();
        update_image_usage!(
            images_usage,
            pbr_metallic_roughness.base_color_texture(),
            ImageUsage::SrgbColor
        );
        update_image_usage!(
            images_usage,
            pbr_metallic_roughness.metallic_roughness_texture(),
            ImageUsage::MetallicRoughnessMap
        );
        update_image_usage!(images_usage, material.normal_texture(), ImageUsage::NormalMap);
        update_image_usage!(
            images_usage,
            material.occlusion_texture(),
            ImageUsage::AmbientOcclusionMap
        );
        update_image_usage!(images_usage, material.emissive_texture(), ImageUsage::SrgbColor);
    }

    static_scenery.images.reserve_exact(images.len());
    for image in images {
        let image_path = match image.source() {
            gltf::image::Source::View { .. } => panic!("buffer image views are not supported right now"),
            gltf::image::Source::Uri { uri, .. } => base_path.join(uri),
        };
        let image_index = static_scenery.images.len();
        let image_usage = match images_usage[image_index] {
            Some(usage) => usage,
            None => {
                log::warn!("unused texture: {:?}", image.source());
                ImageUsage::SrgbColor
            }
        };

        log::info!("importing image: {:?} as {:?}", &image_path, image_usage);
        static_scenery
            .images
            .push(compress_image(image_usage, base_path, &image_path));
    }
}

// TODO: make this less specific
pub fn import_probes(static_scenery: &mut DiskStaticScenery, base_path: &std::path::Path) {
    let probe_image_id = static_scenery.images.len();
    let probe_path = base_path.join("probe");
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentSkybox,
        base_path,
        &probe_path.join("output_skybox.dds"),
    ));
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentIem,
        base_path,
        &probe_path.join("output_iem.dds"),
    ));
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentPmrem,
        base_path,
        &probe_path.join("output_pmrem.dds"),
    ));

    static_scenery.environment_probes.reserve_exact(1);
    static_scenery.environment_probes.push(DiskEnvironmentProbe {
        skybox_image: probe_image_id,
        iem_image: probe_image_id + 1,
        pmrem_image: probe_image_id + 2,
    });
}

pub fn import_samplers(static_scenery: &mut DiskStaticScenery, samplers: gltf::iter::Samplers) {
    if samplers.len() == 0 {
        static_scenery.samplers.reserve_exact(1);
        static_scenery.samplers.push(DiskSampler {
            mag_filter: vk::Filter::LINEAR.as_raw(),
            min_filter: vk::Filter::LINEAR.as_raw(),
            mipmap_mode: vk::SamplerMipmapMode::LINEAR.as_raw(),
            address_mode_u: vk::SamplerAddressMode::REPEAT.as_raw(),
            address_mode_v: vk::SamplerAddressMode::REPEAT.as_raw(),
            address_mode_w: vk::SamplerAddressMode::REPEAT.as_raw(),
        });
    } else {
        static_scenery.samplers.reserve_exact(samplers.len());
        for sampler in samplers {
            let disk_sampler = DiskSampler {
                mag_filter: match sampler.mag_filter() {
                    Some(filter) => match filter {
                        gltf::texture::MagFilter::Nearest => vk::Filter::NEAREST,
                        gltf::texture::MagFilter::Linear => vk::Filter::LINEAR,
                    }
                    .as_raw(),
                    None => vk::Filter::LINEAR.as_raw(),
                },
                min_filter: match sampler.min_filter() {
                    Some(filter) => {
                        match filter {
                            gltf::texture::MinFilter::Nearest => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::Linear => vk::Filter::LINEAR,

                            // These filters are used in combination with mipmap mode below
                            gltf::texture::MinFilter::NearestMipmapNearest => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::NearestMipmapLinear => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::LinearMipmapNearest => vk::Filter::LINEAR,
                            gltf::texture::MinFilter::LinearMipmapLinear => vk::Filter::LINEAR,
                        }
                        .as_raw()
                    }
                    None => vk::Filter::LINEAR.as_raw(),
                },
                mipmap_mode: match sampler.min_filter() {
                    Some(filter) => {
                        match filter {
                            // These filters are used in combination with min filter above
                            gltf::texture::MinFilter::NearestMipmapNearest => vk::SamplerMipmapMode::NEAREST,
                            gltf::texture::MinFilter::LinearMipmapNearest => vk::SamplerMipmapMode::NEAREST,
                            gltf::texture::MinFilter::NearestMipmapLinear => vk::SamplerMipmapMode::LINEAR,
                            gltf::texture::MinFilter::LinearMipmapLinear => vk::SamplerMipmapMode::LINEAR,

                            _ => vk::SamplerMipmapMode::LINEAR,
                        }
                        .as_raw()
                    }
                    None => vk::SamplerMipmapMode::LINEAR.as_raw(),
                },
                address_mode_u: convert_wrap_mode(sampler.wrap_s()).as_raw(),
                address_mode_v: convert_wrap_mode(sampler.wrap_t()).as_raw(),
                address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE.as_raw(),
            };
            static_scenery.samplers.push(disk_sampler);
        }
    }
}

fn convert_wrap_mode(mode: gltf::texture::WrappingMode) -> vk::SamplerAddressMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        gltf::texture::WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        gltf::texture::WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
    }
}
