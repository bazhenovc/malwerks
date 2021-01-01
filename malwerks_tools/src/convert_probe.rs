// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_dds::*;

fn main() {
    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let in_folder = std::path::Path::new(&args[1]);

    {
        let diffuse_folder = in_folder.join("diffuse");
        let diffuse_names = vec![
            diffuse_folder.join("diffuse_right_0.hdr"),
            diffuse_folder.join("diffuse_left_0.hdr"),
            diffuse_folder.join("diffuse_top_0.hdr"),
            diffuse_folder.join("diffuse_bottom_0.hdr"),
            diffuse_folder.join("diffuse_front_0.hdr"),
            diffuse_folder.join("diffuse_back_0.hdr"),
        ];

        let mut texassemble_args = vec!["cube", "-nologo", "-y", "-dx10", "-o", "output_iem.dds"];
        for name in &diffuse_names {
            texassemble_args.push(name.to_str().unwrap());
        }

        log::info!("texassemble.exe {:?}", &texassemble_args);
        let texassemble = std::process::Command::new("texassemble.exe")
            .args(&texassemble_args)
            .current_dir(std::env::current_dir().expect("failed to get current process dir"))
            .output()
            .expect("failed to spawn texassemble.exe process");

        if !texassemble.status.success() {
            log::info!("texassemble finished with status {:?}", texassemble.status);
        }
    }

    {
        let specular_folder = in_folder.join("specular");

        let num_specular_mips = 11;
        let temp_specular_names = vec![
            "specular_right",
            "specular_left",
            "specular_top",
            "specular_bottom",
            "specular_front",
            "specular_back",
        ];
        let temp_specular_folder = in_folder.join("temp_dds");

        let mut temp_specular_images = vec![
            Vec::new(), // mips for "specular_right",
            Vec::new(), // mips for "specular_left",
            Vec::new(), // mips for "specular_top",
            Vec::new(), // mips for "specular_bottom",
            Vec::new(), // mips for "specular_front",
            Vec::new(), // mips for "specular_back",
        ];

        for name_id in 0..temp_specular_names.len() {
            for mip in 0..num_specular_mips {
                let hdr_path = specular_folder.join(format!("{}_{}.hdr", &temp_specular_names[name_id], mip));
                let dds_path = temp_specular_folder.join(format!("{}_{}.dds", &temp_specular_names[name_id], mip));

                let mut texconv_args = vec!["-nologo", "-y", "-dx10", "-o", temp_specular_folder.to_str().unwrap()];
                texconv_args.push("-f");
                texconv_args.push("R32G32B32_FLOAT");
                texconv_args.push("-m");
                texconv_args.push("1");
                texconv_args.push(hdr_path.to_str().unwrap());

                log::info!("texconv.exe {:?}", &texconv_args);
                let texconv = std::process::Command::new("texconv.exe")
                    .args(&texconv_args)
                    .current_dir(std::env::current_dir().expect("failed to get current process dir"))
                    .output()
                    .expect("failed to spawn texconv.exe process");
                if !texconv.status.success() {
                    log::info!("texconv finished with status {:?}", texconv.status);
                }

                temp_specular_images[name_id].push(ScratchImage::from_file(&dds_path));
            }
        }

        let cube_width = temp_specular_images[0][0].image_size().0;
        let cube_height = temp_specular_images[0][0].image_size().1;

        let mut scratch_image = ScratchImage::new(
            cube_width,
            cube_height,
            1,
            num_specular_mips,
            1,
            DXGI_FORMAT_R32G32B32_FLOAT,
            true,
        );

        let image_data = scratch_image.as_slice_mut();
        let mut image_data_offset = 0;
        for image in &temp_specular_images {
            for mip in image.iter() {
                let src_slice = mip.as_slice();
                let dst_slice = &mut image_data[image_data_offset..image_data_offset + src_slice.len()];

                dst_slice.copy_from_slice(src_slice);
                image_data_offset += src_slice.len();
            }
        }

        scratch_image.save_to_file(&std::path::Path::new("output_pmrem.dds"));
    }
}
