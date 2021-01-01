// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_core::*;
use malwerks_vk::*;

use crate::camera::*;

pub struct SharedFrameData {
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,

    frame_data_descriptor_set: FrameLocal<vk::DescriptorSet>,
    frame_data_buffer: FrameLocal<HeapAllocatedResource<vk::Buffer>>,

    view_subsample_offset: [f32; 2],
    view_subsample_index: usize,

    previous_view_projection: ultraviolet::mat::Mat4,
    view_projection: ultraviolet::mat::Mat4,
    subsample_view_projection: ultraviolet::mat::Mat4,
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
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&[vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                    .build()])
                .build(),
        );

        let per_descriptor_layouts: Vec<vk::DescriptorSetLayout> =
            (0..NUM_BUFFERED_GPU_FRAMES).map(|_| descriptor_set_layout).collect();
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
            descriptor_set_layout,
            frame_data_descriptor_set,
            frame_data_buffer,
            view_subsample_offset: Default::default(),
            view_subsample_index: Default::default(),
            previous_view_projection: ultraviolet::mat::Mat4::identity(),
            view_projection: ultraviolet::mat::Mat4::identity(),
            subsample_view_projection: ultraviolet::mat::Mat4::identity(),
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        self.frame_data_buffer
            .destroy(|buffer| factory.deallocate_buffer(buffer));
    }

    pub fn advance_subsample_offset(&mut self) {
        self.view_subsample_offset = SUBSAMPLE_OFFSETS[self.view_subsample_index];
        self.view_subsample_index = (self.view_subsample_index + 1) % SUBSAMPLE_OFFSETS.len();
    }

    pub fn reset_subsample_offset(&mut self) {
        self.view_subsample_offset = Default::default();
    }

    pub fn update(&mut self, frame_context: &FrameContext, camera: &Camera, factory: &mut DeviceFactory) {
        let view_position = -camera.position;
        let (view_projection, subsample_view_projection) = camera.calculate_view_projection(self.view_subsample_offset);
        let inverted_view_projection = view_projection.inversed();
        let view_reprojection = self.previous_view_projection * inverted_view_projection;

        let viewport = camera.get_viewport();
        let viewport_size = [
            (viewport.width as i32 - viewport.x) as f32,
            (viewport.height as i32 - viewport.y) as f32,
        ];

        let mut per_frame_data = PerFrameData::default();
        per_frame_data
            .view_projection
            .copy_from_slice(view_projection.as_slice());
        per_frame_data
            .inverse_view_projection
            .copy_from_slice(inverted_view_projection.as_slice());
        per_frame_data
            .view_reprojection
            .copy_from_slice(view_reprojection.as_slice());
        per_frame_data.view_position[0..3].copy_from_slice(view_position.as_slice());
        per_frame_data.viewport_size = [
            viewport_size[0],
            viewport_size[1],
            1.0 / viewport_size[0],
            1.0 / viewport_size[1],
        ];
        // per_frame_data
        //    .camera_orientation
        //    .copy_from_slice(camera.orientation.as_slice());
        let frame_data_buffer = self.frame_data_buffer.get(frame_context);

        let per_frame_memory = factory.map_allocation_memory(&frame_data_buffer);
        copy_to_mapped_memory(&[per_frame_data], per_frame_memory);
        factory.unmap_allocation_memory(&frame_data_buffer);

        self.previous_view_projection = self.view_projection;
        self.view_projection = view_projection;
        self.subsample_view_projection = subsample_view_projection;
    }

    pub fn get_subsample_view_projection(&self) -> &ultraviolet::mat::Mat4 {
        &self.subsample_view_projection
    }

    // pub fn get_view_position(&self) -> &[f32] {
    //     &self.view_position
    // }

    pub fn get_frame_data_descriptor_set(&self, frame_context: &FrameContext) -> &vk::DescriptorSet {
        self.frame_data_descriptor_set.get(frame_context)
    }
}

#[repr(C)]
#[derive(Default)]
struct PerFrameData {
    pub view_projection: [f32; 16],
    pub inverse_view_projection: [f32; 16],
    pub view_reprojection: [f32; 16],
    pub view_position: [f32; 4],
    pub camera_orientation: [f32; 4],
    pub viewport_size: [f32; 4],
}

const SUBSAMPLE_OFFSETS: [[f32; 2]; 8] = [
    [-0.5, 0.33333337],
    [0.5, -0.7777778],
    [-0.75, -0.111111104],
    [0.25, 0.5555556],
    [-0.25, -0.5555556],
    [0.75, 0.111111164],
    [-0.875, 0.7777778],
    [0.125, -0.9259259],
];
