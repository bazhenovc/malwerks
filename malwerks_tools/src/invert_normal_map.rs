// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[derive(Debug, structopt::StructOpt)]
#[structopt(
    name = "invert_normal_map",
    about = "Inverts the green channel in the given normal map image"
)]
struct CommandLineOptions {
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input: std::path::PathBuf,

    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output: std::path::PathBuf,
}

fn main() {
    let command_line = {
        use structopt::StructOpt;
        CommandLineOptions::from_args()
    };

    let input_image = image::open(command_line.input)
        .expect("Failed to open input image")
        .into_rgb8();

    let mut output_image = image::DynamicImage::new_rgb8(input_image.width(), input_image.height());
    for y in 0..input_image.height() {
        for x in 0..input_image.width() {
            use image::GenericImage;

            let input_pixel = input_image.get_pixel(x, y);
            output_image.put_pixel(
                x,
                y,
                image::Rgba([input_pixel[0], 255 - input_pixel[1], input_pixel[2], 255]),
            );
        }
    }

    output_image
        .save(command_line.output)
        .expect("Failed to save output image");
}
