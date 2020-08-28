// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

use crate::camera::*;
use crate::upload_batch::copy_to_mapped_memory;

pub struct SharedFrameData {
    descriptor_pool: vk::DescriptorPool,
    frame_data_descriptor_set_layout: vk::DescriptorSetLayout,

    frame_data_descriptor_set: FrameLocal<vk::DescriptorSet>,
    frame_data_buffer: FrameLocal<HeapAllocatedResource<vk::Buffer>>,

    view_projection: [f32; 16],
    view_position: [f32; 4],
}

impl SharedFrameData {
    pub fn new(factory: &mut DeviceFactory) -> Self {
        let frame_data_buffer = FrameLocal::new(|_| {
            factory.allocate_buffer(
                &vk::BufferCreateInfo::builder()
                    .size(std::mem::size_of::<PerFrameData>() as _)
                    .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
                    .build(),
                &vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::CpuToGpu,
                    ..Default::default()
                },
            )
        });

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(NUM_BUFFERED_GPU_FRAMES as _)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .build()])
                .build(),
        );
        let frame_data_descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&[vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                    .build()])
                .build(),
        );

        let per_descriptor_layouts: Vec<vk::DescriptorSetLayout> = (0..NUM_BUFFERED_GPU_FRAMES)
            .map(|_| frame_data_descriptor_set_layout)
            .collect();
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&per_descriptor_layouts)
                .build(),
        );

        let temp_buffer_infos: Vec<vk::DescriptorBufferInfo> = (0..NUM_BUFFERED_GPU_FRAMES)
            .map(|frame| {
                vk::DescriptorBufferInfo::builder()
                    .buffer(frame_data_buffer.get_frame(frame).0)
                    .offset(0)
                    .range(std::mem::size_of::<PerFrameData>() as _)
                    .build()
            })
            .collect();
        let temp_writes: Vec<vk::WriteDescriptorSet> = (0..NUM_BUFFERED_GPU_FRAMES)
            .map(|frame| {
                vk::WriteDescriptorSet::builder()
                    .dst_binding(0)
                    .dst_set(descriptor_sets[frame])
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&temp_buffer_infos[frame..=frame])
                    .build()
            })
            .collect();
        factory.update_descriptor_sets(&temp_writes, &[]);

        let frame_data_descriptor_set = FrameLocal::new(|frame| descriptor_sets[frame]);
        Self {
            descriptor_pool,
            frame_data_descriptor_set_layout,
            frame_data_descriptor_set,
            frame_data_buffer,
            view_projection: Default::default(),
            view_position: Default::default(),
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.frame_data_descriptor_set_layout);
        self.frame_data_buffer
            .destroy(|buffer| factory.deallocate_buffer(buffer));
    }

    pub fn update(&mut self, frame_context: &FrameContext, camera: &Camera, factory: &mut DeviceFactory) {
        let view_position = -camera.position;
        let view_projection = camera.get_view_projection();
        self.view_projection.copy_from_slice(view_projection.as_slice());
        self.view_position[0..3].copy_from_slice(view_position.as_slice());

        let mut per_frame_data = PerFrameData::default();
        per_frame_data
            .view_projection
            .copy_from_slice(view_projection.as_slice());
        per_frame_data
            .inverse_view_projection
            .copy_from_slice(view_projection.inversed().as_slice());
        per_frame_data.view_position[0..3].copy_from_slice(view_position.as_slice());
        //per_frame_data
        //    .camera_orientation
        //    .copy_from_slice(camera.orientation.as_slice());
        let frame_data_buffer = self.frame_data_buffer.get(frame_context);

        let per_frame_memory = factory.map_allocation_memory(&frame_data_buffer);
        copy_to_mapped_memory(&[per_frame_data], per_frame_memory);
        factory.unmap_allocation_memory(&frame_data_buffer);
    }

    pub fn get_view_projection(&self) -> &[f32] {
        &self.view_projection
    }

    pub fn get_view_position(&self) -> &[f32] {
        &self.view_position
    }

    pub fn get_frame_data_descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.frame_data_descriptor_set_layout
    }

    pub fn get_frame_data_descriptor_set(&self, frame_context: &FrameContext) -> &vk::DescriptorSet {
        self.frame_data_descriptor_set.get(frame_context)
    }
}

#[repr(C)]
#[derive(Default)]
struct PerFrameData {
    pub view_projection: [f32; 16],
    pub inverse_view_projection: [f32; 16],
    pub view_position: [f32; 4],
    pub camera_orientation: [f32; 4],
}
