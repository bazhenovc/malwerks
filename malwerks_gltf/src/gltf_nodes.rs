// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;
use ultraviolet as utv;

use crate::gltf_shared::*;

pub fn import_nodes(
    primitive_remap: Vec<PrimitiveRemap>,
    nodes: gltf::iter::Nodes,
    in_buffers: &mut Vec<DiskBuffer>,
) -> Vec<DiskRenderBucket> {
    use std::collections::HashMap;

    struct InstanceData {
        transforms: Vec<[f32; 16]>,
    };

    let mut buckets = HashMap::<usize, HashMap<(usize, usize), InstanceData>>::new();
    for node in nodes {
        if let Some(mesh) = node.mesh() {
            log::info!("importing node {:?}", node.name().unwrap_or("<unnamed>"));
            let remap = &primitive_remap[mesh.index()];
            assert_eq!(remap.mesh_id, mesh.index());

            for (mesh_index, material_id, material_instance_id) in &remap.primitives {
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

                    transform_data
                };

                match buckets.get_mut(&material_id) {
                    Some(bucket) => match bucket.get_mut(&(*mesh_index, *material_instance_id)) {
                        Some(instance) => {
                            instance.transforms.push(instance_data);
                        }
                        None => {
                            bucket.insert(
                                (*mesh_index, *material_instance_id),
                                InstanceData {
                                    transforms: vec![instance_data],
                                },
                            );
                        }
                    },
                    None => {
                        let mut new_value = HashMap::new();
                        new_value.insert(
                            (*mesh_index, *material_instance_id),
                            InstanceData {
                                transforms: vec![instance_data],
                            },
                        );
                        buckets.insert(*material_id, new_value);
                    }
                }
            }
        }
    }

    buckets
        .into_iter()
        .map(|(material, instances)| {
            let mut total_instance_count = 0usize;
            let mut total_draw_count = 0usize;
            for ((_, _), instance) in &instances {
                total_instance_count += instance.transforms.len();
                total_draw_count += instance.transforms.len();
            }

            let mut instance_transform_data = vec![0u8; total_instance_count * std::mem::size_of::<[f32; 16]>()];
            let dst_instance_transforms = unsafe {
                let memory = instance_transform_data.as_mut_ptr();
                assert_eq!(
                    (memory as usize) & ((1 << (std::mem::align_of::<[f32; 16]>() - 1)) - 1),
                    0
                );

                #[allow(clippy::cast_ptr_alignment)]
                std::slice::from_raw_parts_mut(memory as *mut [f32; 16], total_draw_count)
            };

            let mut current_transform = 0;
            for instance in instances.values() {
                for instance_id in 0..instance.transforms.len() {
                    dst_instance_transforms[current_transform] = instance.transforms[instance_id];
                    current_transform += 1;
                }
            }

            let instance_transform_buffer = in_buffers.len();
            {
                let stride = std::mem::size_of::<[f32; 16]>() as u64;
                let usage_flags = vk::BufferUsageFlags::STORAGE_BUFFER.as_raw();
                in_buffers.push(DiskBuffer {
                    stride,
                    usage_flags,
                    data: instance_transform_data,
                });
            }

            DiskRenderBucket {
                material,
                instances: instances
                    .into_iter()
                    .map(|((mesh, material_instance), instance_data)| DiskRenderInstance {
                        mesh,
                        material_instance,

                        total_instance_count: instance_data.transforms.len(),
                        total_draw_count: instance_data.transforms.len(),
                    })
                    .collect(),

                instance_transform_buffer,
            }
        })
        .collect()
}
