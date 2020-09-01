// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

mod gltf_images;
mod gltf_material_instances;
mod gltf_materials;
mod gltf_meshes;
mod gltf_nodes;
mod gltf_shared;

mod global_resources;

mod meshopt;
mod texconv;

use gltf_images::*;
use gltf_material_instances::*;
use gltf_meshes::*;
use gltf_nodes::*;

use global_resources::*;

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

fn import_gltf(command_line: &CommandLineOptions) -> DiskStaticScenery {
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
        global_resources: DiskGlobalResources::default(),
    };

    let gltf = gltf::Gltf::open(&command_line.input_file).expect("failed to open gltf");
    let base_path = std::path::Path::new(&command_line.input_file)
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
    );
    import_nodes(&mut static_scenery, primitive_remap_table, gltf.nodes());
    import_images(&mut static_scenery, &base_path, gltf.materials(), gltf.images());
    import_samplers(&mut static_scenery, gltf.samplers());
    import_probes(&mut static_scenery, &base_path);
    import_global_resources(&mut static_scenery, &base_path);
    static_scenery
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

    let static_scenery = import_gltf(&command_line);

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
