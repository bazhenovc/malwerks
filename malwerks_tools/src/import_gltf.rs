// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_gltf::*;

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "import_gltf", about = "glTF import tool")]
struct CommandLineOptions {
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input_file: std::path::PathBuf,

    #[structopt(short = "t", long = "temp_folder", parse(from_os_str))]
    temp_folder: std::path::PathBuf,

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

    let disk_bundle = import_gltf_bundle(&command_line.input_file, &command_line.temp_folder);
    let output_file = if let Some(file) = command_line.output_file {
        file
    } else {
        std::path::Path::new(&command_line.input_file).with_extension("render_bundle")
    };
    log::info!(
        "saving {} buffers, {} meshes, {} images, {} samplers, {} layouts, {} instances, {} materials, {} buckets to {:?}",
        disk_bundle.buffers.len(),
        disk_bundle.meshes.len(),
        disk_bundle.images.len(),
        disk_bundle.samplers.len(),
        disk_bundle.material_layouts.len(),
        disk_bundle.material_instances.len(),
        disk_bundle.materials.len(),
        disk_bundle.buckets.len(),
        &output_file,
    );
    {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_file)
            .expect("failed to open output file");
        disk_bundle
            .serialize_into(std::io::BufWriter::new(file), command_line.compression_level)
            .expect("failed to serialize render bundle");
    }
}
