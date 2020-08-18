// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "convert_metallic_roughness", about = "Converts metallic and roughness images into one")]
struct CommandLineOptions {
    #[structopt(short = "r", long = "roughness", parse(from_os_str))]
    roughness: std::path::PathBuf,

    #[structopt(short = "m", long = "metallic", parse(from_os_str))]
    metallic: Option<std::path::PathBuf>,

    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output: std::path::PathBuf,
}

fn main() {
    let command_line = {
        use structopt::StructOpt;
        CommandLineOptions::from_args()
    };

    let roughness_image = image::open(command_line.roughness)
        .expect("Failed to open roughness image")
        .into_luma();

    let metallic_image = if let Some(metallic_path) = command_line.metallic {
        image::open(metallic_path)
            .expect("Failed to open metallic image")
            .into_luma()
    } else {
        image::DynamicImage::new_luma8(roughness_image.width(), roughness_image.height()).into_luma()
    };

    assert_eq!(
        metallic_image.width(),
        roughness_image.width(),
        "Images width are not equal"
    );
    assert_eq!(
        metallic_image.height(),
        roughness_image.height(),
        "Images height are not equal"
    );

    let mut output_image = image::DynamicImage::new_rgba8(metallic_image.width(), metallic_image.height());
    for y in 0..metallic_image.height() {
        for x in 0..metallic_image.width() {
            use image::GenericImage;

            let metallic = metallic_image.get_pixel(x, y);
            let roughness = roughness_image.get_pixel(x, y);
            output_image.put_pixel(x, y, image::Rgba([metallic[0], roughness[0], 0, 255]));
        }
    }

    output_image.save(command_line.output).expect("Failed to save output image");
}
