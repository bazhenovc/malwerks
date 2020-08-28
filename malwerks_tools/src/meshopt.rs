// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;

pub fn optimize_mesh(
    raw_vertex_data: &[u8],
    raw_vertex_stride: usize,
    raw_vertex_count: usize,

    raw_index_data: &[u8],
    raw_index_stride: usize,
    _raw_index_count: usize,
) -> (DiskBuffer, DiskBuffer, Vec<DiskMeshCluster>, Vec<DiskBoundingCone>) {
    let (mut vertex_remap, mut index_buffer) = {
        let u32_index_data = match raw_index_stride {
            1 => make_wide_index_buffer::<u8>(&raw_index_data),
            2 => make_wide_index_buffer::<u16>(&raw_index_data),
            4 => make_wide_index_buffer::<u32>(&raw_index_data),
            _ => panic!("unsupported index stride"),
        };
        (vec![0u32; u32_index_data.len()], u32_index_data)
    };

    let vertex_count = unsafe {
        meshopt::ffi::meshopt_generateVertexRemap(
            vertex_remap.as_mut_ptr(),
            index_buffer.as_ptr() as _,
            index_buffer.len(),
            raw_vertex_data.as_ptr() as _,
            raw_vertex_count,
            raw_vertex_stride as _,
        )
    };

    let mut vertex_buffer = vec![0u8; vertex_count * raw_vertex_stride as usize];
    unsafe {
        meshopt::ffi::meshopt_remapVertexBuffer(
            vertex_buffer.as_mut_ptr() as _,
            raw_vertex_data.as_ptr() as _,
            raw_vertex_count,
            raw_vertex_stride as _,
            vertex_remap.as_ptr(),
        );
        meshopt::ffi::meshopt_remapIndexBuffer(
            index_buffer.as_mut_ptr() as _,
            index_buffer.as_ptr() as _,
            index_buffer.len(),
            vertex_remap.as_ptr(),
        );
        meshopt::ffi::meshopt_optimizeVertexCache(
            index_buffer.as_mut_ptr() as _,
            index_buffer.as_ptr() as _,
            index_buffer.len(),
            vertex_count,
        );
        meshopt::ffi::meshopt_optimizeVertexFetch(
            vertex_buffer.as_mut_ptr() as _,
            index_buffer.as_mut_ptr() as _,
            index_buffer.len(),
            vertex_buffer.as_mut_ptr() as _,
            vertex_count,
            raw_vertex_stride as _,
        );
    }

    let meshlets = meshopt::clusterize::build_meshlets(&index_buffer, vertex_count, 64, 126);
    let mut mesh_clusters = Vec::with_capacity(meshlets.len());
    let mut mesh_bounds = Vec::with_capacity(meshlets.len());

    let mut final_vertex_count = 0usize;
    let mut final_index_count = 0usize;
    for meshlet in &meshlets {
        mesh_clusters.push(DiskMeshCluster {
            vertex_count: meshlet.vertex_count as u16,
            index_count: (meshlet.triangle_count as u16) * 3,
        });

        final_vertex_count += meshlet.vertex_count as usize;
        final_index_count += (meshlet.triangle_count as usize) * 3;
    }

    let mut final_vertex_data = vec![0u8; final_vertex_count * raw_vertex_stride];
    let mut temp_index_data = Vec::with_capacity(final_index_count);

    let mut final_vertex_offset = 0;
    for meshlet in &meshlets {
        for local_vertex_index in 0..meshlet.vertex_count {
            let vertex_id = meshlet.vertices[local_vertex_index as usize] as usize;
            let source_vertex_offset = vertex_id * raw_vertex_stride;
            let source_vertex_slice = &vertex_buffer[source_vertex_offset..source_vertex_offset + raw_vertex_stride];
            let target_vertex_slice =
                &mut final_vertex_data[final_vertex_offset..final_vertex_offset + raw_vertex_stride];
            target_vertex_slice.copy_from_slice(source_vertex_slice);
            final_vertex_offset += raw_vertex_stride;
        }

        for local_triangle_index in 0..meshlet.triangle_count {
            let index0 = meshlet.indices[local_triangle_index as usize][0] as u16;
            let index1 = meshlet.indices[local_triangle_index as usize][1] as u16;
            let index2 = meshlet.indices[local_triangle_index as usize][2] as u16;

            temp_index_data.push(index0);
            temp_index_data.push(index1);
            temp_index_data.push(index2);
        }

        let bounds = unsafe {
            let memory = vertex_buffer.as_ptr();
            assert_eq!((memory as usize) & ((1 << (std::mem::align_of::<f32>() - 1)) - 1), 0);

            #[allow(clippy::cast_ptr_alignment)]
            meshopt::ffi::meshopt_computeMeshletBounds(
                meshlet as *const _,
                memory as *const f32,
                raw_vertex_count,
                raw_vertex_stride,
            )
        };
        mesh_bounds.push(DiskBoundingCone {
            cone_apex: [bounds.cone_apex[0], bounds.cone_apex[1], bounds.cone_apex[2], 0.0],
            cone_axis: [
                bounds.cone_axis[0],
                bounds.cone_axis[1],
                bounds.cone_axis[2],
                bounds.cone_cutoff,
            ],
        });
    }
    assert_eq!(final_vertex_offset, final_vertex_data.len());

    // TOD: get rid of this extra copy
    let mut final_index_data = vec![0u8; final_index_count * std::mem::size_of::<u16>()];
    final_index_data.copy_from_slice(bytemuck::cast_slice(&temp_index_data));

    let final_vertex_buffer = DiskBuffer {
        stride: raw_vertex_stride as _,
        usage_flags: vk::BufferUsageFlags::VERTEX_BUFFER.as_raw(),
        data: final_vertex_data,
    };
    let final_index_buffer = DiskBuffer {
        stride: std::mem::size_of::<u16>() as _,
        usage_flags: vk::BufferUsageFlags::INDEX_BUFFER.as_raw(),
        data: final_index_data,
    };

    (final_vertex_buffer, final_index_buffer, mesh_clusters, mesh_bounds)
}

fn make_wide_index_buffer<FROM>(raw_source: &[u8]) -> Vec<u32>
where
    FROM: bytemuck::Pod + Into<u32>,
{
    let source = bytemuck::cast_slice::<u8, FROM>(raw_source);
    let mut target = Vec::with_capacity(source.len());
    for v in source {
        target.push((*v).into());
    }
    target
}

// fn convert_to_narrow_index_buffer<TO>(source: &[u32], target: &mut DiskBuffer)
// where
//     TO: bytemuck::Pod + std::convert::TryFrom<u32>,
// {
//     let mut temp = Vec::with_capacity(source.len());
//     for v in source {
//         temp.push(match TO::try_from(*v) {
//             Ok(v) => v,
//             _ => panic!("narrowing index value conversion failed"),
//         });
//     }
//
//     copy_to_buffer::<TO>(&temp, target);
// }
//
// fn copy_to_buffer<TO>(source: &[TO], target: &mut DiskBuffer)
// where
//     TO: bytemuck::Pod,
// {
//     target.stride = std::mem::size_of::<TO>() as _;
//     target.data.resize(source.len() * std::mem::size_of::<TO>(), 0u8);
//     target.data.copy_from_slice(bytemuck::cast_slice(source));
// }
