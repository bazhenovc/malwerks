// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod gltf_images;
mod gltf_material_instances;
mod gltf_materials;
mod gltf_meshes;
mod gltf_nodes;
mod gltf_shared;

mod global_resources;

pub use gltf_images::*;
pub use gltf_material_instances::*;
pub use gltf_meshes::*;
pub use gltf_nodes::*;

pub use global_resources::*;

pub fn import_gltf(input_file: &std::path::Path) -> malwerks_resources::DiskStaticScenery {
    let mut static_scenery = malwerks_resources::DiskStaticScenery {
        images: Vec::new(),
        buffers: Vec::new(),
        meshes: Vec::new(),
        samplers: Vec::new(),
        material_layouts: Vec::new(),
        material_instances: Vec::new(),
        materials: Vec::new(),
        buckets: Vec::new(),
        environment_probes: Vec::new(),
        global_resources: Default::default(),
    };

    let gltf = gltf::Gltf::open(&input_file).expect("failed to open gltf");
    let base_path = std::path::Path::new(&input_file)
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
