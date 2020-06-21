// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;
// use malwerks_resources::*;

pub struct ShaderBindingTable {
    table_group_count: usize,
    table_size: usize,
    table_stride: usize,
    table_buffer: HeapAllocatedResource<vk::Buffer>,
}

impl ShaderBindingTable {
    pub fn new(
        // disk_scenery: &DiskStaticScenery,
        table_group_count: usize,
        ray_tracing_properties: &vk::PhysicalDeviceRayTracingPropertiesNV,
        factory: &mut DeviceFactory,
    ) -> Self {
        let table_stride = ray_tracing_properties.shader_group_handle_size as usize;
        let table_size = table_group_count * table_stride;
        let table_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size(table_size as _)
                .usage(vk::BufferUsageFlags::RAY_TRACING_NV | vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        );

        log::info!("allocated shader binding table with size {}", table_size);

        Self {
            table_group_count,
            table_size,
            table_stride,
            table_buffer,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.deallocate_buffer(&self.table_buffer);
    }

    pub fn build(
        &mut self,
        ray_tracing_pipelines: &[vk::Pipeline],
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        let mut table_data = vec![0u8; ray_tracing_pipelines.len() * self.table_size];
        let mut table_offset = 0;
        for pipeline in ray_tracing_pipelines.iter() {
            let mut table_slice = &mut table_data[table_offset..table_offset + self.table_size];
            table_offset += self.table_size;

            factory.get_ray_tracing_shader_group_handles_nv(
                *pipeline,
                0,
                self.table_group_count as _,
                &mut table_slice,
            );
        }

        let mut upload_batch = UploadBatch::new(command_buffer);
        upload_batch.upload_buffer_memory(
            vk::PipelineStageFlags::RAY_TRACING_SHADER_NV,
            &self.table_buffer,
            &table_data,
            0,
            factory,
        );
        upload_batch.flush(factory, queue);
    }

    pub fn get_table_buffer(&self) -> &HeapAllocatedResource<vk::Buffer> {
        &self.table_buffer
    }

    pub fn get_table_stride(&self) -> usize {
        self.table_stride
    }

    pub fn get_table_size(&self) -> usize {
        self.table_size
    }
}
