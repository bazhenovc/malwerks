// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_bundles::*;

use ash::vk;

pub fn optimize_mesh(
    raw_vertex_data: &[u8],
    raw_vertex_stride: usize,
    raw_vertex_count: usize,

    raw_index_data: &[u8],
    raw_index_stride: usize,
    _raw_index_count: usize,
) -> (DiskBuffer, DiskBuffer) {
    let (mut vertex_remap, mut index_buffer) = {
        let u32_index_data = match raw_index_stride {
            1 => make_wide_index_buffer::<u8>(&raw_index_data),
            2 => make_wide_index_buffer::<u16>(&raw_index_data),
            4 => make_wide_index_buffer::<u32>(&raw_index_data),
            _ => unimplemented!("unsupported index stride"),
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

    let final_vertex_buffer = DiskBuffer {
        stride: raw_vertex_stride as _,
        usage_flags: vk::BufferUsageFlags::VERTEX_BUFFER.as_raw(),
        data: vertex_buffer,
    };

    let mut final_index_buffer = DiskBuffer {
        stride: raw_index_stride as _,
        usage_flags: vk::BufferUsageFlags::INDEX_BUFFER.as_raw(),
        data: Vec::new(),
    };
    match raw_index_stride {
        1 => convert_to_narrow_index_buffer::<u8>(&index_buffer, &mut final_index_buffer),
        2 => convert_to_narrow_index_buffer::<u16>(&index_buffer, &mut final_index_buffer),
        4 => convert_to_narrow_index_buffer::<u32>(&index_buffer, &mut final_index_buffer),
        _ => unimplemented!("unsupported index stride"),
    }

    (final_vertex_buffer, final_index_buffer)
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
