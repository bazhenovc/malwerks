// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[macro_use]
extern crate clap;

use malwerks_resources::*;

mod gltf_images;
mod gltf_material_instances;
mod gltf_materials;
mod gltf_meshes;
mod gltf_nodes;
mod gltf_shared;

mod meshopt;
mod texconv;

use gltf_images::*;
use gltf_material_instances::*;
use gltf_meshes::*;
use gltf_nodes::*;

fn import_gltf(file_name: &str, optimize_geometry: bool) -> DiskStaticScenery {
    let mut static_scenery = DiskStaticScenery {
        images: Vec::new(),
        buffers: Vec::new(),
        meshes: Vec::new(),
        samplers: Vec::new(),
        material_layouts: Vec::new(),
        material_instances: Vec::new(),
        materials: Vec::new(),
        buckets: Vec::new(),
        environment_probes: Vec::new(),
    };

    let gltf = gltf::Gltf::open(file_name).expect("failed to open gltf");
    let base_path = std::path::Path::new(file_name)
        .parent()
        .expect("failed to get file base path");

    import_material_instances(&mut static_scenery, gltf.materials());
    let primitive_remap_table = import_meshes(
        &mut static_scenery,
        &base_path,
        gltf.buffers(),
        gltf.views(),
        gltf.meshes(),
        gltf.materials(),
        optimize_geometry,
    );
    import_nodes(&mut static_scenery, primitive_remap_table, gltf.nodes());
    import_images(&mut static_scenery, &base_path, gltf.materials(), gltf.images());
    import_samplers(&mut static_scenery, gltf.samplers());
    import_probes(&mut static_scenery, &base_path);
    static_scenery
}

fn main() {
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        std::env::set_var("RUST_LOG", "info");
    }

    pretty_env_logger::init();

    let matches = clap::clap_app!(app =>
        (version: "0.1")
        (author: "Kyrylo Bazhenov <bazhenovc@gmail.com>")
        (about: "Converts a gltf scene into internal representation")
        (@arg INPUT_FILE: -i --input +takes_value +required "Sets input file to load")
        (@arg OUTPUT_FILE: -o --output +takes_value "Sets output file")
        (@arg COMPRESSION_LEVEL: -c --compression_level +takes_value "Sets the compression level for the output file")
        (@arg OPTIMIZE_GEOMETRY: -g --optimize_geometry +takes_value "Controls whether to optimize geometry or not"))
    .get_matches();

    let input_file = matches.value_of("INPUT_FILE").expect("no input file specified");
    let output_file = if let Some(file) = matches.value_of("OUTPUT_FILE") {
        std::path::PathBuf::from(file)
    } else {
        std::path::Path::new(&input_file).with_extension("world")
    };
    let compression_level = if let Some(level) = matches.value_of("COMPRESSION_LEVEL") {
        level
            .parse()
            .expect("compression level does not seem to be a valid number")
    } else {
        9
    };
    let optimize_geometry = if let Some(optimize_geometry) = matches.value_of("OPTIMIZE_GEOMETRY") {
        optimize_geometry
            .parse()
            .expect("optimize_geometry value does not seem to be a boolean")
    } else {
        true
    };

    let static_scenery = import_gltf(&input_file, optimize_geometry);
    log::info!(
        "saving {} buffers, {} meshes, {} images, {} samplers, {} layouts, {} instances, {} materials, {} buckets to {:?}",
        static_scenery.buffers.len(),
        static_scenery.meshes.len(),
        static_scenery.images.len(),
        static_scenery.samplers.len(),
        static_scenery.material_layouts.len(),
        static_scenery.material_instances.len(),
        static_scenery.materials.len(),
        static_scenery.buckets.len(),
        &output_file,
    );
    {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_file)
            .expect("failed to open output file");
        static_scenery.serialize_into(std::io::BufWriter::new(file), compression_level);
    }
}
