// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::shared_frame_data::*;

struct ComputeBucket {
    // bounding_cone_buffer: usize,
    draw_arguments_buffer: usize,
    dispatch_size: u32,
}

#[derive(Default)]
pub(crate) struct MeshClusterCulling {
    apex_culling_shader_module: vk::ShaderModule,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    buckets: Vec<ComputeBucket>,

    debug_parameters: [u32; 4],
    debug_apex_culling_paused: bool,
}

impl MeshClusterCulling {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_shader_module(self.apex_culling_shader_module);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        factory.destroy_pipeline_layout(self.pipeline_layout);
        factory.destroy_pipeline(self.pipeline);
    }

    pub fn new(
        disk_scenery: &DiskStaticScenery,
        buffers: &[HeapAllocatedResource<vk::Buffer>],
        factory: &mut DeviceFactory,
        pipeline_cache: vk::PipelineCache,
    ) -> Self {
        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(disk_scenery.buckets.len() as _)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(2)
                    .build()])
                .build(),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&[
                    vk::DescriptorSetLayoutBinding::builder()
                        .binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .build(),
                    vk::DescriptorSetLayoutBinding::builder()
                        .binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .build(),
                ])
                .build(),
        );
        let temp_per_descriptor_layouts = vec![descriptor_set_layout; disk_scenery.buckets.len()];
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut temp_buffer_infos = Vec::with_capacity(disk_scenery.buckets.len() * 2);
        let mut temp_descriptor_writes = Vec::with_capacity(disk_scenery.buckets.len() * 2);
        for bucket_id in 0..disk_scenery.buckets.len() {
            let current_buffer_info = temp_buffer_infos.len();
            temp_buffer_infos.push(
                vk::DescriptorBufferInfo::builder()
                    .offset(0)
                    .range(vk::WHOLE_SIZE)
                    .buffer(buffers[disk_scenery.buckets[bucket_id].bounding_cone_buffer].0)
                    .build(),
            );
            temp_buffer_infos.push(
                vk::DescriptorBufferInfo::builder()
                    .offset(0)
                    .range(vk::WHOLE_SIZE)
                    .buffer(buffers[disk_scenery.buckets[bucket_id].draw_arguments_buffer].0)
                    .build(),
            );

            temp_descriptor_writes.push(
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[bucket_id])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&temp_buffer_infos[current_buffer_info..current_buffer_info + 1])
                    .build(),
            );
            temp_descriptor_writes.push(
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[bucket_id])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&temp_buffer_infos[current_buffer_info + 1..current_buffer_info + 2])
                    .build(),
            );
        }
        factory.update_descriptor_sets(&temp_descriptor_writes, &[]);

        let entry_point = std::ffi::CString::new("main").unwrap();
        let apex_culling_shader_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.apex_culling_compute_stage)
                .build(),
        );

        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&[descriptor_set_layout])
                .push_constant_ranges(&[vk::PushConstantRange::builder()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .offset(0)
                    .size(32)
                    .build()])
                .build(),
        );
        let pipeline = factory.create_compute_pipelines(
            pipeline_cache,
            &[vk::ComputePipelineCreateInfo::builder()
                .layout(pipeline_layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(apex_culling_shader_module)
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .build(),
                )
                .build()],
        )[0];

        let mut buckets = Vec::with_capacity(disk_scenery.buckets.len());
        for disk_bucket in &disk_scenery.buckets {
            buckets.push(ComputeBucket {
                // bounding_cone_buffer: disk_bucket.bounding_cone_buffer,
                draw_arguments_buffer: disk_bucket.draw_arguments_buffer,
                dispatch_size: ((disk_bucket.draw_arguments_count + 8) / 8) as _,
            });
        }

        Self {
            apex_culling_shader_module,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,

            pipeline_layout,
            pipeline,

            buckets,

            debug_parameters: Default::default(),
            debug_apex_culling_paused: false,
        }
    }

    pub fn dispatch(
        &self,
        buffers: &[HeapAllocatedResource<vk::Buffer>],
        command_buffer: &mut CommandBuffer,
        shared_frame_data: &SharedFrameData,
    ) {
        if !self.debug_apex_culling_paused {
            puffin::profile_function!();

            command_buffer.bind_pipeline(vk::PipelineBindPoint::COMPUTE, self.pipeline);
            command_buffer.push_constants(
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                shared_frame_data.get_view_position(),
            );
            command_buffer.push_constants(
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                16,
                &self.debug_parameters,
            );

            for (bucket_index, bucket) in self.buckets.iter().enumerate() {
                let draw_arguments_buffer = buffers[bucket.draw_arguments_buffer].0;

                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    None,
                    &[],
                    &[vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                        .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .src_queue_family_index(!0)
                        .dst_queue_family_index(!0)
                        .buffer(draw_arguments_buffer)
                        .offset(0)
                        .size(vk::WHOLE_SIZE)
                        .build()],
                    &[],
                );

                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    self.pipeline_layout,
                    0,
                    &[self.descriptor_sets[bucket_index]],
                    &[],
                );
                command_buffer.dispatch(bucket.dispatch_size, 1, 1);
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    None,
                    &[],
                    &[vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                        .src_queue_family_index(!0)
                        .dst_queue_family_index(!0)
                        .buffer(draw_arguments_buffer)
                        .offset(0)
                        .size(vk::WHOLE_SIZE)
                        .build()],
                    &[],
                );
            }
        }
    }

    pub fn debug_set_apex_culling_enabled(&mut self, enabled: bool) {
        self.debug_parameters[0] = !enabled as _;
    }

    pub fn debug_set_apex_culling_paused(&mut self, paused: bool) {
        self.debug_apex_culling_paused = paused;
    }
}
