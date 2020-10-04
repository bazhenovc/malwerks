// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_gltf::*;

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "import_gltf", about = "glTF import tool")]
struct CommandLineOptions {
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input_file: std::path::PathBuf,

    #[structopt(short = "o", long = "output")]
    output_file: Option<std::path::PathBuf>,

    #[structopt(short = "c", long = "compression_level", default_value = "9")]
    compression_level: u32,
}

fn main() {
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        std::env::set_var("RUST_LOG", "info");
    }

    pretty_env_logger::init();

    let command_line = {
        use structopt::StructOpt;
        CommandLineOptions::from_args()
    };

    let static_scenery = import_gltf(&command_line.input_file);

    let output_file = if let Some(file) = command_line.output_file {
        file
    } else {
        std::path::Path::new(&command_line.input_file).with_extension("world")
    };
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
        static_scenery.serialize_into(std::io::BufWriter::new(file), command_line.compression_level);
    }
}
