// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ash::vk;
use malwerks_bundles::*;
use malwerks_dds::*;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImageUsage {
    SrgbColor,
    MetallicRoughnessMap,
    NormalMap,
    AmbientOcclusionMap,

    EnvironmentSkybox,
    EnvironmentIem,
    EnvironmentPmrem,
    EnvironmentBrdf,
}

pub fn compress_image(
    image_usage: ImageUsage,
    output_path: &std::path::Path,
    image_path: &std::path::Path,
) -> DiskImage {
    std::fs::create_dir_all(output_path).expect("failed to create output folder for texconv");

    let dds_path = output_path.join(image_path.with_extension("dds").file_name().unwrap());
    assert_ne!(dds_path, image_path); // make sure we're not writing compressed output to the source texture

    const FORCE_TEXCONV: bool = false;
    let need_texconv = FORCE_TEXCONV || {
        let image_meta = std::fs::metadata(&image_path).expect("failed to get image file metadata");
        let dds_meta = std::fs::metadata(&dds_path);

        if let Ok(dds_meta) = dds_meta {
            let image_modified = image_meta.modified().expect("failed to get image timestamp");
            let dds_modified = dds_meta.modified().expect("failed to get image timestamp");

            image_modified > dds_modified
        } else {
            true
        }
    };

    let mut texconv_args = vec!["-nologo", "-dx10", "-y", "-o", output_path.to_str().unwrap()];
    let (image_format, expected_block_size, is_cube_map) = match image_usage {
        ImageUsage::SrgbColor => {
            texconv_args.push("-srgb");
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM_SRGB");
            (vk::Format::BC7_SRGB_BLOCK, 16, false)
        }

        ImageUsage::MetallicRoughnessMap => {
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM"); // TODO: compress with BC5
            (vk::Format::BC7_UNORM_BLOCK, 16, false)
        }

        ImageUsage::NormalMap => {
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM"); // TODO: compress with BC5
            (vk::Format::BC7_UNORM_BLOCK, 16, false)
        }

        ImageUsage::AmbientOcclusionMap => {
            texconv_args.push("-f");
            texconv_args.push("BC4_UNORM");
            (vk::Format::BC4_UNORM_BLOCK, 8, false)
        }

        ImageUsage::EnvironmentSkybox => {
            texconv_args.push("-srgbo");
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM_SRGB");
            (vk::Format::BC7_SRGB_BLOCK, 16, true)
        }

        ImageUsage::EnvironmentIem => {
            texconv_args.push("-f");
            texconv_args.push("BC6H_UF16");
            texconv_args.push("-m");
            texconv_args.push("1");
            (vk::Format::BC6H_UFLOAT_BLOCK, 16, true)
        }

        ImageUsage::EnvironmentPmrem => {
            texconv_args.push("-f");
            texconv_args.push("BC6H_UF16");
            texconv_args.push("-m");
            texconv_args.push("0");
            (vk::Format::BC6H_UFLOAT_BLOCK, 16, true)
        }

        ImageUsage::EnvironmentBrdf => {
            texconv_args.push("-f");
            texconv_args.push("R16G16_FLOAT"); // TODO: is it worth compressing?
            texconv_args.push("-m");
            texconv_args.push("1");
            (vk::Format::R16G16_SFLOAT, 16, false)
        }
    };
    texconv_args.push(image_path.to_str().expect("failed to convert image path"));

    if need_texconv {
        log::info!("texconv.exe {:?}", &texconv_args);
        let texconv = std::process::Command::new("texconv.exe")
            .args(&texconv_args)
            .current_dir(std::env::current_dir().expect("failed to get current process dir"))
            .output()
            .expect("failed to spawn texconv.exe process");
        if !texconv.status.success() {
            panic!("texconv finished with status {:?}", texconv.status);
        }
    }

    let scratch_image = ScratchImage::from_file(&dds_path);
    let image_size = scratch_image.image_size();

    let block_size = scratch_image.block_size();
    assert_eq!(block_size, expected_block_size);

    let (image_type, view_type) = if is_cube_map {
        (vk::ImageType::TYPE_2D, vk::ImageViewType::CUBE)
    } else if image_size.2 > 1 {
        (vk::ImageType::TYPE_3D, vk::ImageViewType::TYPE_3D)
    } else if image_size.1 > 1 {
        (vk::ImageType::TYPE_2D, vk::ImageViewType::TYPE_2D)
    } else {
        (vk::ImageType::TYPE_1D, vk::ImageViewType::TYPE_1D)
    };

    let image_layer_count = if is_cube_map {
        scratch_image.layer_count() * 6
    } else {
        scratch_image.layer_count()
    };

    DiskImage {
        width: image_size.0,
        height: image_size.1,
        depth: image_size.2,
        block_size: block_size as _,
        mipmap_count: scratch_image.mipmap_count() as _,
        layer_count: image_layer_count as _,
        image_type: image_type.as_raw(),
        view_type: view_type.as_raw(),
        format: image_format.as_raw(),
        pixels: scratch_image.as_slice().to_vec(),
    }
}
