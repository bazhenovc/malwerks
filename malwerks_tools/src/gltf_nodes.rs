// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ultraviolet as utv;

use crate::gltf_shared::*;

pub fn import_nodes(
    static_scenery: &mut DiskStaticScenery,
    primitive_remap: Vec<PrimitiveRemap>,
    nodes: gltf::iter::Nodes,
) {
    use std::collections::HashMap;

    struct InstanceData {
        transforms: Vec<[f32; 16]>,
        bounding_boxes: Vec<([f32; 3], [f32; 3])>,
    };

    let mut buckets = HashMap::<usize, HashMap<(usize, usize), InstanceData>>::new();
    for node in nodes {
        if let Some(mesh) = node.mesh() {
            log::info!("importing node {:?}", node.name().unwrap_or("<unnamed>"));
            let remap = &primitive_remap[mesh.index()];
            assert_eq!(remap.mesh_id, mesh.index());

            for (mesh_index, material_id, material_instance_id, bounding_box) in &remap.primitives {
                let instance_data = {
                    let node_transform = node.transform().matrix();

                    let transform = utv::mat::Mat4::new(
                        utv::vec::Vec4::from(node_transform[0]),
                        utv::vec::Vec4::from(node_transform[1]),
                        utv::vec::Vec4::from(node_transform[2]),
                        utv::vec::Vec4::from(node_transform[3]),
                    );
                    let mut transform_data = [0.0; 16];
                    transform_data.copy_from_slice(transform.as_slice());

                    let transformed_box = bounding_box.get_transformed(&transform);
                    let mut bounding_box_data = ([0.0; 3], [0.0; 3]);
                    bounding_box_data.0.copy_from_slice(transformed_box.min.as_slice());
                    bounding_box_data.1.copy_from_slice(transformed_box.max.as_slice());

                    (transform_data, bounding_box_data)
                };

                match buckets.get_mut(&material_id) {
                    Some(bucket) => match bucket.get_mut(&(*mesh_index, *material_instance_id)) {
                        Some(instance) => {
                            instance.transforms.push(instance_data.0);
                            instance.bounding_boxes.push(instance_data.1);
                        }
                        None => {
                            bucket.insert(
                                (*mesh_index, *material_instance_id),
                                InstanceData {
                                    transforms: vec![instance_data.0],
                                    bounding_boxes: vec![instance_data.1],
                                },
                            );
                        }
                    },
                    None => {
                        let mut new_value = HashMap::new();
                        new_value.insert(
                            (*mesh_index, *material_instance_id),
                            InstanceData {
                                transforms: vec![instance_data.0],
                                bounding_boxes: vec![instance_data.1],
                            },
                        );
                        buckets.insert(*material_id, new_value);
                    }
                }
            }
        }
    }

    static_scenery.buckets = buckets
        .into_iter()
        .map(|(material, instances)| DiskRenderBucket {
            material,
            instances: instances
                .into_iter()
                .map(|((mesh, material_instance), instance_data)| DiskRenderInstance {
                    mesh,
                    material_instance,
                    transforms: instance_data.transforms,
                    bounding_boxes: instance_data.bounding_boxes,
                })
                .collect(),
        })
        .collect();
}
