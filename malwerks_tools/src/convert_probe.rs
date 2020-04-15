// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod dds;
use dds::*;

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

                let mut dds_file = std::fs::File::open(dds_path).expect("failed to open resulting dds file");
                let dds_header = {
                    use std::io::Read;

                    let mut header = [0u8; 148];
                    dds_file.read_exact(&mut header[..]).expect("failed to read dds header");

                    let header: DirectDrawHeader =
                        bincode::deserialize(&header[..]).expect("failed to parse dds header");
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

                temp_specular_images[name_id].push((dds_header, dds_data));
            }
        }

        let cube_width = temp_specular_images[0][0].0.width;
        let cube_height = temp_specular_images[0][0].0.height;
        let cube_data_size = {
            let mut size = 0;
            for image in &temp_specular_images {
                for (_header, data) in image.iter() {
                    size += data.len();
                }
            }
            size
        };

        let dds_header = bincode::serialize(&DirectDrawHeader {
            magic: *b"DDS ",
            size: 124,
            flags: DDSD_CAPS | DDSD_WIDTH | DDSD_HEIGHT | DDSD_PIXELFORMAT,
            height: cube_height,
            width: cube_width,
            pitch_or_linear_size: ((cube_width * 64 + 7) / 8) as _,
            depth: 1,
            mipmap_count: num_specular_mips as _,
            reserved: [0; 11],
            pixel_format: DirectDrawPixelFormat {
                size: 32,
                flags: DDPF_FOURCC,
                four_cc: *b"DX10",
                rgb_bit_count: 0,
                red_bit_mask: 0,
                green_bit_mask: 0,
                blue_bit_mask: 0,
                alpha_bit_mask: 0,
            },
            caps: 0,
            caps2: 0,
            caps3: 0,
            caps4: 0,
            reserved2: 0,
            dxt10: DirectDrawHeader10 {
                dxgi_format: DXGI_FORMAT_R32G32B32_FLOAT,
                resource_dimension: D3D10_RESOURCE_DIMENSION_TEXTURE2D,
                misc_flag: DDS_RESOURCE_MISC_TEXTURECUBE,
                array_size: 1,
                misc_flags2: 0,
            },
        })
        .expect("failed to serialize dds header");

        let mut dds_data = vec![0; cube_data_size];
        let mut data_offset = 0;
        for image in &temp_specular_images {
            for mip in image.iter() {
                let src_slice = &mip.1;
                let dst_slice = &mut dds_data[data_offset..data_offset + src_slice.len()];

                dst_slice.copy_from_slice(src_slice);
                data_offset += src_slice.len();
            }
        }

        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open("output_pmrem.dds")
                .expect("failed to open output file");

            file.write_all(&dds_header[..]).expect("failed to write dds header");
            file.write_all(&dds_data[..]).expect("failed to write pixels");
        }
    }
}
