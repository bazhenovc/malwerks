// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;

pub fn optimize_mesh(
    buffers: &mut [DiskBuffer],
    vertex_buffer_id: usize,
    index_buffer_id: (usize, usize),
) -> (vk::IndexType, u32) {
    unsafe {
        let raw_vertex_buffer = &buffers[vertex_buffer_id];
        let raw_vertex_count = raw_vertex_buffer.data.len() / (raw_vertex_buffer.stride as usize);

        let (mut vertex_remap, mut index_buffer) = if index_buffer_id.1 != 0 {
            let u32_index_data = match index_buffer_id.1 {
                1 => make_wide_index_buffer::<u8>(&buffers[index_buffer_id.0].data),
                2 => make_wide_index_buffer::<u16>(&buffers[index_buffer_id.0].data),
                4 => make_wide_index_buffer::<u32>(&buffers[index_buffer_id.0].data),
                _ => panic!("unsupported index stride"),
            };
            (vec![0u32; u32_index_data.len()], u32_index_data)
        } else {
            (vec![0u32; raw_vertex_count * 3], vec![0u32; raw_vertex_count * 3])
        };

        let vertex_count = meshopt::ffi::meshopt_generateVertexRemap(
            vertex_remap.as_mut_ptr(),
            index_buffer.as_ptr() as _,
            index_buffer.len(),
            raw_vertex_buffer.data.as_ptr() as _,
            raw_vertex_count,
            raw_vertex_buffer.stride as _,
        );

        let mut vertex_buffer = vec![0u8; vertex_count * raw_vertex_buffer.stride as usize];
        meshopt::ffi::meshopt_remapVertexBuffer(
            vertex_buffer.as_mut_ptr() as _,
            raw_vertex_buffer.data.as_ptr() as _,
            raw_vertex_count,
            raw_vertex_buffer.stride as _,
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
            raw_vertex_buffer.stride as _,
        );

        let raw_vertex_buffer = &mut buffers[vertex_buffer_id];
        raw_vertex_buffer.data.resize(vertex_buffer.len(), 0u8);
        raw_vertex_buffer.data.copy_from_slice(&vertex_buffer);

        let raw_index_buffer = &mut buffers[index_buffer_id.0];
        // TODO: enable uint8 index format when AMD starts supporting it
        // if vertex_count <= u8::max_value() as usize {
        //     convert_to_narrow_index_buffer::<u8>(&index_buffer, raw_index_buffer);
        //     (vk::IndexType::UINT8_EXT, index_buffer.len() as _)
        // } else

        if vertex_count <= u16::max_value() as usize {
            convert_to_narrow_index_buffer::<u16>(&index_buffer, raw_index_buffer);
            (vk::IndexType::UINT16, index_buffer.len() as _)
        } else if vertex_count <= u32::max_value() as usize {
            copy_to_buffer::<u32>(&index_buffer, raw_index_buffer);
            (vk::IndexType::UINT32, index_buffer.len() as _)
        } else {
            panic!("vertex count exceeds u32 limits")
        }
    }
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

fn convert_to_narrow_index_buffer<TO>(source: &[u32], target: &mut DiskBuffer)
where
    TO: bytemuck::Pod + std::convert::TryFrom<u32>,
{
    let mut temp = Vec::with_capacity(source.len());
    for v in source {
        temp.push(match TO::try_from(*v) {
            Ok(v) => v,
            _ => panic!("narrowing index value conversion failed"),
        });
    }

    copy_to_buffer::<TO>(&temp, target);
}

fn copy_to_buffer<TO>(source: &[TO], target: &mut DiskBuffer)
where
    TO: bytemuck::Pod,
{
    target.stride = std::mem::size_of::<TO>() as _;
    target.data.resize(source.len() * std::mem::size_of::<TO>(), 0u8);
    target.data.copy_from_slice(bytemuck::cast_slice(source));
}
