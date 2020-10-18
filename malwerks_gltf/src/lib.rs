// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod gltf_images;
mod gltf_material_instances;
mod gltf_materials;
mod gltf_meshes;
mod gltf_nodes;
mod gltf_shaders;
mod gltf_shared;

use gltf_images::*;
use gltf_material_instances::*;
use gltf_meshes::*;
use gltf_nodes::*;
// use gltf_shaders::*;

pub fn import_gltf_bundle(
    input_file: &std::path::Path,
    temp_folder: &std::path::Path,
) -> malwerks_resources::DiskRenderBundle {
    let gltf = gltf::Gltf::open(&input_file).expect("failed to open gltf");
    let base_path = std::path::Path::new(&input_file)
        .parent()
        .expect("failed to get file base path");

    let (material_layouts, material_instances) = import_material_instances(gltf.materials());
    let (mut buffers, meshes, materials, primitive_remap_table) = import_meshes(
        &base_path,
        gltf.buffers(),
        gltf.views(),
        gltf.meshes(),
        gltf.materials(),
        &material_layouts,
    );
    let buckets = import_nodes(primitive_remap_table, gltf.nodes(), &mut buffers);
    let images = import_images(&base_path, temp_folder, gltf.materials(), gltf.images());
    let samplers = import_samplers(gltf.samplers());

    malwerks_resources::DiskRenderBundle {
        buffers,
        meshes,
        images,
        samplers,
        material_layouts,
        material_instances,
        materials,
        buckets,
    }
}

pub use gltf_shaders::compile_gltf_shaders;
