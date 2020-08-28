// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;
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
        bounding_cones: Vec<RawBoundingCone>,
    };

    let mut buckets = HashMap::<usize, HashMap<(usize, usize), InstanceData>>::new();
    for node in nodes {
        if let Some(mesh) = node.mesh() {
            log::info!("importing node {:?}", node.name().unwrap_or("<unnamed>"));
            let remap = &primitive_remap[mesh.index()];
            assert_eq!(remap.mesh_id, mesh.index());

            for (mesh_index, material_id, material_instance_id, bounding_cones) in &remap.primitives {
                // let material = &static_scenery.materials[*material_id];

                let mut instance_data = {
                    let node_transform = node.transform().matrix();

                    let transform = utv::mat::Mat4::new(
                        utv::vec::Vec4::from(node_transform[0]),
                        utv::vec::Vec4::from(node_transform[1]),
                        utv::vec::Vec4::from(node_transform[2]),
                        utv::vec::Vec4::from(node_transform[3]),
                    );
                    let mut transform_data = [0.0; 16];
                    transform_data.copy_from_slice(transform.as_slice());

                    let mut transformed_cones = Vec::with_capacity(bounding_cones.len());
                    for cone in bounding_cones {
                        let transformed_cone = cone.get_transformed(&transform);
                        let disk_cone = RawBoundingCone {
                            cone_apex: [
                                transformed_cone.cone_apex.x,
                                transformed_cone.cone_apex.y,
                                transformed_cone.cone_apex.z,
                                0.0,
                            ],
                            cone_axis: [
                                transformed_cone.cone_axis.x,
                                transformed_cone.cone_axis.y,
                                transformed_cone.cone_axis.z,
                                transformed_cone.cone_cutoff,
                                // TODO: Set cone_cutoff to 1.0 for double-sided materials
                                // if material.fragment_cull_flags == vk::CullModeFlags::NONE.as_raw() {
                                //     1.0
                                // } else {
                                //     transformed_cone.cone_cutoff
                                // },
                            ],
                        };
                        transformed_cones.push(disk_cone);
                    }

                    (transform_data, transformed_cones)
                };

                match buckets.get_mut(&material_id) {
                    Some(bucket) => match bucket.get_mut(&(*mesh_index, *material_instance_id)) {
                        Some(instance) => {
                            instance.transforms.push(instance_data.0);
                            instance.bounding_cones.append(&mut instance_data.1);
                        }
                        None => {
                            bucket.insert(
                                (*mesh_index, *material_instance_id),
                                InstanceData {
                                    transforms: vec![instance_data.0],
                                    bounding_cones: instance_data.1,
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
                                bounding_cones: instance_data.1,
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
        .map(|(material, instances)| {
            let mut draw_arguments_count = 0;
            for ((mesh_id, _), instance) in &instances {
                let mesh = &static_scenery.meshes[*mesh_id];
                draw_arguments_count += mesh.mesh_clusters.len() * instance.transforms.len();
            }

            let mut bounding_cone_data = vec![0u8; draw_arguments_count * std::mem::size_of::<RawBoundingCone>()];
            let dst_bounding_cones = unsafe {
                let memory = bounding_cone_data.as_mut_ptr();
                assert_eq!(
                    (memory as usize) & ((1 << (std::mem::align_of::<RawBoundingCone>() - 1)) - 1),
                    0
                );

                #[allow(clippy::cast_ptr_alignment)]
                std::slice::from_raw_parts_mut(memory as *mut RawBoundingCone, draw_arguments_count)
            };

            let mut draw_argument_data =
                vec![0u8; draw_arguments_count * std::mem::size_of::<vk::DrawIndexedIndirectCommand>()];
            let dst_draw_arguments = unsafe {
                let memory = draw_argument_data.as_mut_ptr();
                assert_eq!(
                    (memory as usize) & ((1 << (std::mem::align_of::<vk::DrawIndexedIndirectCommand>() - 1)) - 1),
                    0
                );

                #[allow(clippy::cast_ptr_alignment)]
                std::slice::from_raw_parts_mut(memory as *mut vk::DrawIndexedIndirectCommand, draw_arguments_count)
            };

            let mut current_argument = 0;
            for ((mesh_id, _), instance) in &instances {
                let mesh = &static_scenery.meshes[*mesh_id];

                let mut vertex_offset = 0;
                let mut first_index = 0;
                for cluster_id in 0..mesh.bounding_cones.len() {
                    let cluster = &mesh.mesh_clusters[cluster_id];
                    for instance_id in 0..instance.transforms.len() {
                        let bounding_cone =
                            instance.bounding_cones[cluster_id + instance_id * mesh.mesh_clusters.len()];
                        dst_bounding_cones[current_argument] = bounding_cone;
                        dst_draw_arguments[current_argument] = vk::DrawIndexedIndirectCommand {
                            instance_count: 0,
                            index_count: cluster.index_count as _,
                            first_index,
                            vertex_offset,
                            first_instance: instance_id as _,
                        };
                        current_argument += 1;
                    }
                    vertex_offset += cluster.vertex_count as i32;
                    first_index += cluster.index_count as u32;
                }
            }
            assert_eq!(current_argument, draw_arguments_count);

            let bounding_cone_buffer = static_scenery.buffers.len();
            {
                let stride = std::mem::size_of::<RawBoundingCone>() as u64;
                let usage_flags = vk::BufferUsageFlags::STORAGE_BUFFER.as_raw();
                static_scenery.buffers.push(DiskBuffer {
                    stride,
                    usage_flags,
                    data: bounding_cone_data,
                });
            }

            let draw_arguments_buffer = static_scenery.buffers.len();
            {
                let stride = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;
                let usage_flags =
                    (vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER).as_raw();
                static_scenery.buffers.push(DiskBuffer {
                    stride,
                    usage_flags,
                    data: draw_argument_data,
                });
            }

            DiskRenderBucket {
                material,
                instances: instances
                    .into_iter()
                    .map(|((mesh, material_instance), instance_data)| DiskRenderInstance {
                        mesh,
                        material_instance,
                        transforms: instance_data.transforms,
                    })
                    .collect(),
                bounding_cone_buffer,
                draw_arguments_buffer,
                draw_arguments_count,
            }
        })
        .collect();
}

#[repr(C)]
#[derive(Copy, Clone)]
struct RawBoundingCone {
    cone_apex: [f32; 4],
    cone_axis: [f32; 4],
}
