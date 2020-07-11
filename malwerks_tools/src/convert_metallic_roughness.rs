// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[macro_use]
extern crate clap;

fn main() {
    let matches = clap::clap_app!(app =>
            (version: "0.1")
            (author: "Kyrylo Bazhenov <bazhenovc@gmail.com>")
            (about: "Converts metallic and roughness textures into one")
            (@arg METALLIC: -m --metallic +takes_value "Sets metallic image to use")
            (@arg ROUGHNESS: -r --roughness +required +takes_value "Sets roughness image to use")
            (@arg OUTPUT: -o --output +required +takes_value "Sets output file name"))
    .get_matches();

    //let metallic_path = std::path::PathBuf::from(matches.value_of("METALLIC").expect("No metallic image provided"));
    let roughness_path = std::path::PathBuf::from(matches.value_of("ROUGHNESS").expect("No roughness image provided"));
    let output_path = std::path::PathBuf::from(matches.value_of("OUTPUT").expect("No output path provided"));

    let roughness_image = image::open(roughness_path)
        .expect("Failed to open roughness image")
        .into_luma();

    let metallic_image = if let Some(metallic_path) = matches.value_of("METALLIC") {
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

    output_image.save(output_path).expect("Failed to save output image");
}
