// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ash::vk;
use malwerks_resources::*;

use crate::dds::*;

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

pub fn compress_image(image_usage: ImageUsage, base_path: &std::path::Path, image_path: &std::path::Path) -> DiskImage {
    let dds_path = base_path.join(image_path.with_extension("dds").file_name().unwrap());
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

    let mut texconv_args = vec!["-nologo", "-dx10", "-y", "-o", base_path.to_str().unwrap()];
    let (image_format, block_size, is_cube_map, compressed) = match image_usage {
        ImageUsage::SrgbColor => {
            texconv_args.push("-srgb");
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM_SRGB");
            (vk::Format::BC7_SRGB_BLOCK, 16, false, true)
        }

        ImageUsage::MetallicRoughnessMap => {
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM"); // TODO: compress with BC5
            (vk::Format::BC7_UNORM_BLOCK, 16, false, true)
        }

        ImageUsage::NormalMap => {
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM"); // TODO: compress with BC5
            (vk::Format::BC7_UNORM_BLOCK, 16, false, true)
        }

        ImageUsage::AmbientOcclusionMap => {
            texconv_args.push("-f");
            texconv_args.push("BC4_UNORM");
            (vk::Format::BC4_UNORM_BLOCK, 8, false, true)
        }

        ImageUsage::EnvironmentSkybox => {
            texconv_args.push("-srgbo");
            texconv_args.push("-f");
            texconv_args.push("BC7_UNORM_SRGB");
            (vk::Format::BC7_SRGB_BLOCK, 16, true, true)
        }

        ImageUsage::EnvironmentIem => {
            texconv_args.push("-f");
            texconv_args.push("BC6H_UF16");
            texconv_args.push("-m");
            texconv_args.push("1");
            (vk::Format::BC6H_UFLOAT_BLOCK, 16, true, true)
        }

        ImageUsage::EnvironmentPmrem => {
            texconv_args.push("-f");
            texconv_args.push("BC6H_UF16");
            texconv_args.push("-m");
            texconv_args.push("0");
            (vk::Format::BC6H_UFLOAT_BLOCK, 16, true, true)
        }

        ImageUsage::EnvironmentBrdf => {
            texconv_args.push("-f");
            texconv_args.push("R16G16_FLOAT"); // TODO: is it worth compressing?
            texconv_args.push("-m");
            texconv_args.push("1");
            (vk::Format::R16G16_SFLOAT, 16, false, false)
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
            log::info!("texconv finished with status {:?}", texconv.status);
        }
    }

    let mut dds_file = std::fs::File::open(dds_path).expect("failed to open resulting dds file");
    let dds_header = {
        use std::io::Read;

        let mut header = [0u8; 148];
        dds_file.read_exact(&mut header[..]).expect("failed to read dds header");

        let header: DirectDrawHeader = bincode::deserialize(&header[..]).expect("failed to parse dds header");
        assert_eq!(&header.magic, b"DDS ");
        assert_eq!(header.size, 124);
        assert_eq!(header.pixel_format.size, 32);

        header
    };
    let dds_data = {
        use std::io::Read;
        let mut buffer = Vec::new();
        dds_file.read_to_end(&mut buffer).expect("failed to read dds data");
        buffer
    };

    let (image_type, view_type) = if is_cube_map {
        (vk::ImageType::TYPE_2D, vk::ImageViewType::CUBE)
    } else if dds_header.depth > 1 {
        (vk::ImageType::TYPE_3D, vk::ImageViewType::TYPE_3D)
    } else if dds_header.height > 1 {
        (vk::ImageType::TYPE_2D, vk::ImageViewType::TYPE_2D)
    } else {
        (vk::ImageType::TYPE_1D, vk::ImageViewType::TYPE_1D)
    };
    let image_layer_count = if is_cube_map {
        dds_header.dxt10.array_size * 6
    } else {
        dds_header.dxt10.array_size
    };

    if compressed {
        let row_pitch = block_size * ((dds_header.width + 3) / 4).max(1);
        let linear_pitch = row_pitch * ((dds_header.height + 3) / 4).max(1);
        assert_eq!(linear_pitch, dds_header.pitch_or_linear_size);

        let mut image_data_size = 0;
        for mip in 0..dds_header.mipmap_count {
            let mip_pitch = block_size * (((dds_header.width >> mip) + 3) / 4).max(1);
            let mip_size = mip_pitch * (((dds_header.height >> mip) + 3) / 4).max(1);
            image_data_size += mip_size;
        }
        assert_eq!(image_data_size * image_layer_count, dds_data.len() as u32);
    }

    DiskImage {
        width: dds_header.width,
        height: dds_header.height,
        depth: dds_header.depth,
        block_size: block_size as _,
        mipmap_count: dds_header.mipmap_count as _,
        layer_count: image_layer_count as _,
        image_type: image_type.as_raw(),
        view_type: view_type.as_raw(),
        format: image_format.as_raw(),
        pixels: dds_data,
    }
}
