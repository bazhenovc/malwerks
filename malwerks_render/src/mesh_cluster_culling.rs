// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::shared_frame_data::*;

pub(crate) struct MeshCullingInstance {
    pub(crate) input_bounding_cone_buffer: vk::Buffer,
    pub(crate) input_occluder_arguments_buffer: vk::Buffer,
    pub(crate) input_draw_arguments_buffer: vk::Buffer,

    pub(crate) count_buffer: vk::Buffer,
    pub(crate) occlusion_culling_arguments_buffer: vk::Buffer,
    pub(crate) occluder_arguments_buffer: vk::Buffer,
    pub(crate) temp_draw_arguments_buffer: vk::Buffer,
    pub(crate) draw_arguments_buffer: vk::Buffer,

    // pub(crate) instance_count: usize,
    pub(crate) draw_count: usize,
    pub(crate) dispatch_size: u32,
}

pub(crate) struct MeshCullingBucket {
    pub(crate) instances: Vec<MeshCullingInstance>,
}

#[derive(Default)]
pub(crate) struct MeshClusterCulling {
    apex_culling_shader_module: vk::ShaderModule,
    occlusion_culling_shader_module: vk::ShaderModule,
    occlusion_culling_arguments_shader_module: vk::ShaderModule,

    apex_culling_descriptor_pool: vk::DescriptorPool,
    apex_culling_descriptor_set_layout: vk::DescriptorSetLayout,
    apex_culling_descriptor_sets: Vec<vk::DescriptorSet>,
    apex_culling_pipeline_layout: vk::PipelineLayout,

    occlusion_culling_descriptor_pool: vk::DescriptorPool,
    occlusion_culling_descriptor_set_layout: vk::DescriptorSetLayout,
    occlusion_culling_descriptor_sets: Vec<vk::DescriptorSet>,
    occlusion_culling_pipeline_layout: vk::PipelineLayout,

    count_to_dispatch_descriptor_pool: vk::DescriptorPool,
    count_to_dispatch_descriptor_set_layout: vk::DescriptorSetLayout,
    count_to_dispatch_descriptor_sets: Vec<vk::DescriptorSet>,
    count_to_dispatch_pipeline_layout: vk::PipelineLayout,

    compute_pipelines: Vec<vk::Pipeline>,

    buckets: Vec<MeshCullingBucket>,
}

impl MeshClusterCulling {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for pipeline in &self.compute_pipelines {
            factory.destroy_pipeline(*pipeline);
        }
        factory.destroy_shader_module(self.apex_culling_shader_module);
        factory.destroy_shader_module(self.occlusion_culling_shader_module);
        factory.destroy_shader_module(self.occlusion_culling_arguments_shader_module);

        factory.destroy_descriptor_pool(self.apex_culling_descriptor_pool);
        factory.destroy_descriptor_set_layout(self.apex_culling_descriptor_set_layout);
        factory.destroy_pipeline_layout(self.apex_culling_pipeline_layout);

        factory.destroy_descriptor_pool(self.occlusion_culling_descriptor_pool);
        factory.destroy_descriptor_set_layout(self.occlusion_culling_descriptor_set_layout);
        factory.destroy_pipeline_layout(self.occlusion_culling_pipeline_layout);

        factory.destroy_descriptor_pool(self.count_to_dispatch_descriptor_pool);
        factory.destroy_descriptor_set_layout(self.count_to_dispatch_descriptor_set_layout);
        factory.destroy_pipeline_layout(self.count_to_dispatch_pipeline_layout);
    }

    pub fn new(
        disk_scenery: &DiskStaticScenery,
        buckets: Vec<MeshCullingBucket>,
        pipeline_cache: vk::PipelineCache,
        factory: &mut DeviceFactory,
    ) -> Self {
        log::info!("initializing mesh cluster culling");

        let entry_point = std::ffi::CString::new("main").unwrap();
        let apex_culling_shader_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.apex_culling_compute_stage)
                .build(),
        );
        let occlusion_culling_shader_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.occlusion_culling_compute_stage)
                .build(),
        );
        let occlusion_culling_arguments_shader_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.count_to_dispatch_compute_stage)
                .build(),
        );

        let mut descriptor_count = 0;
        for bucket in &buckets {
            descriptor_count += bucket.instances.len();
        }

        let (
            apex_culling_descriptor_pool,
            apex_culling_descriptor_set_layout,
            apex_culling_descriptor_sets,
            apex_culling_pipeline_layout,
        ) = create_apex_culling_layout(descriptor_count, factory);

        let (
            occlusion_culling_descriptor_pool,
            occlusion_culling_descriptor_set_layout,
            occlusion_culling_descriptor_sets,
            occlusion_culling_pipeline_layout,
        ) = create_occlusion_culling_layout(descriptor_count, factory);

        let (
            count_to_dispatch_descriptor_pool,
            count_to_dispatch_descriptor_set_layout,
            count_to_dispatch_descriptor_sets,
            count_to_dispatch_pipeline_layout,
        ) = create_count_to_dispatch_layout(descriptor_count, factory);

        let compute_pipelines = factory.create_compute_pipelines(
            pipeline_cache,
            &[
                vk::ComputePipelineCreateInfo::builder()
                    .layout(apex_culling_pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::builder()
                            .name(&entry_point)
                            .module(apex_culling_shader_module)
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .build(),
                    )
                    .build(),
                vk::ComputePipelineCreateInfo::builder()
                    .layout(occlusion_culling_pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::builder()
                            .name(&entry_point)
                            .module(occlusion_culling_shader_module)
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .build(),
                    )
                    .build(),
                vk::ComputePipelineCreateInfo::builder()
                    .layout(count_to_dispatch_pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::builder()
                            .name(&entry_point)
                            .module(occlusion_culling_arguments_shader_module)
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .build(),
                    )
                    .build(),
            ],
        );

        Self {
            apex_culling_shader_module,
            occlusion_culling_shader_module,
            occlusion_culling_arguments_shader_module,

            apex_culling_descriptor_pool,
            apex_culling_descriptor_set_layout,
            apex_culling_descriptor_sets,
            apex_culling_pipeline_layout,

            occlusion_culling_descriptor_pool,
            occlusion_culling_descriptor_set_layout,
            occlusion_culling_descriptor_sets,
            occlusion_culling_pipeline_layout,

            count_to_dispatch_descriptor_pool,
            count_to_dispatch_descriptor_set_layout,
            count_to_dispatch_descriptor_sets,
            count_to_dispatch_pipeline_layout,

            compute_pipelines,

            buckets,
        }
    }

    pub fn dispatch_apex_culling(&self, command_buffer: &mut CommandBuffer, shared_frame_data: &SharedFrameData) {
        puffin::profile_function!();

        command_buffer.bind_pipeline(vk::PipelineBindPoint::COMPUTE, self.compute_pipelines[0]);
        command_buffer.push_constants(
            self.apex_culling_pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            shared_frame_data.get_view_position(),
        );

        let mut current_instance = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    None,
                    &[],
                    &[
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.count_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.occluder_arguments_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        // vk::BufferMemoryBarrier::builder()
                        //     .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                        //     .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                        //     .src_queue_family_index(!0)
                        //     .dst_queue_family_index(!0)
                        //     .buffer(instance.occlusion_culling_arguments_buffer)
                        //     .offset(0)
                        //     .size(vk::WHOLE_SIZE)
                        //     .build(),
                    ],
                    &[],
                );
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    self.apex_culling_pipeline_layout,
                    0,
                    &[self.apex_culling_descriptor_sets[current_instance]],
                    &[],
                );
                command_buffer.dispatch(instance.dispatch_size, 1, 1);
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    None,
                    &[],
                    &[
                        // vk::BufferMemoryBarrier::builder()
                        //     .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        //     .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                        //     .src_queue_family_index(!0)
                        //     .dst_queue_family_index(!0)
                        //     .buffer(instance.count_buffer)
                        //     .offset(0)
                        //     .size(vk::WHOLE_SIZE)
                        //     .build(),
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.occluder_arguments_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        // vk::BufferMemoryBarrier::builder()
                        //     .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        //     .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                        //     .src_queue_family_index(!0)
                        //     .dst_queue_family_index(!0)
                        //     .buffer(instance.occlusion_culling_arguments_buffer)
                        //     .offset(0)
                        //     .size(vk::WHOLE_SIZE)
                        //     .build(),
                    ],
                    &[],
                );

                current_instance += 1;
            }
        }
    }

    pub fn dispatch_count_to_occlusion_culling_arguments(
        &self,
        command_buffer: &mut CommandBuffer,
        _shared_frame_data: &SharedFrameData,
    ) {
        puffin::profile_function!();

        command_buffer.bind_pipeline(vk::PipelineBindPoint::COMPUTE, self.compute_pipelines[2]);
        let mut current_instance = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    None,
                    &[],
                    &[vk::BufferMemoryBarrier::builder()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .src_queue_family_index(!0)
                        .dst_queue_family_index(!0)
                        .buffer(instance.count_buffer)
                        .offset(0)
                        .size(vk::WHOLE_SIZE)
                        .build()],
                    &[],
                );
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
                        .buffer(instance.occlusion_culling_arguments_buffer)
                        .offset(0)
                        .size(vk::WHOLE_SIZE)
                        .build()],
                    &[],
                );
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    self.count_to_dispatch_pipeline_layout,
                    0,
                    &[self.count_to_dispatch_descriptor_sets[current_instance]],
                    &[],
                );
                command_buffer.dispatch(1, 1, 1);
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    None,
                    &[],
                    &[
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::SHADER_READ)
                            .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.count_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.occlusion_culling_arguments_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                    ],
                    &[],
                );

                current_instance += 1;
            }
        }
    }

    pub fn dispatch_occlusion_culling(&self, command_buffer: &mut CommandBuffer, _shared_frame_data: &SharedFrameData) {
        puffin::profile_function!();

        command_buffer.bind_pipeline(vk::PipelineBindPoint::COMPUTE, self.compute_pipelines[1]);
        let mut current_instance = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    None,
                    &[],
                    &[
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.count_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.draw_arguments_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                    ],
                    &[],
                );
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    self.occlusion_culling_pipeline_layout,
                    0,
                    &[self.occlusion_culling_descriptor_sets[current_instance]],
                    &[],
                );
                // command_buffer.dispatch(instance.dispatch_size, 1, 1);
                command_buffer.dispatch_indirect(instance.occlusion_culling_arguments_buffer, 0);
                command_buffer.pipeline_barrier(
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::DRAW_INDIRECT,
                    None,
                    &[],
                    &[
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.count_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                        vk::BufferMemoryBarrier::builder()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
                            .src_queue_family_index(!0)
                            .dst_queue_family_index(!0)
                            .buffer(instance.draw_arguments_buffer)
                            .offset(0)
                            .size(vk::WHOLE_SIZE)
                            .build(),
                    ],
                    &[],
                );

                current_instance += 1;
            }
        }
    }

    // pub fn debug_set_apex_culling_enabled(&mut self, enabled: bool) {
    //     self.shared_parameters[1] = !enabled as _;
    // }

    //pub fn debug_set_occlusion_culling_enabled(&mut self, enabled: bool) {
    //    self.shared_parameters[2] = !enabled as _;
    //}

    pub fn update_apex_culling_descriptor_sets(&mut self, factory: &mut DeviceFactory) {
        log::info!("updating apex culling descriptor sets");
        let mut temp_buffer_infos = Vec::with_capacity(self.apex_culling_descriptor_sets.len() * 6);
        let mut temp_descriptor_writes = Vec::with_capacity(self.apex_culling_descriptor_sets.len() * 6);

        let mut instance_id = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                let current_buffer_info = temp_buffer_infos.len();
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.input_bounding_cone_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.input_occluder_arguments_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.input_draw_arguments_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.count_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.occluder_arguments_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.temp_draw_arguments_buffer)
                        .build(),
                );
                // temp_buffer_infos.push(
                //     vk::DescriptorBufferInfo::builder()
                //         .offset(0)
                //         .range(vk::WHOLE_SIZE)
                //         .buffer(instance.occlusion_culling_arguments_buffer)
                //         .build(),
                // );

                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info..current_buffer_info + 1])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 1..current_buffer_info + 2])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 2..current_buffer_info + 3])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 3..current_buffer_info + 4])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(4)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 4..current_buffer_info + 5])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.apex_culling_descriptor_sets[instance_id])
                        .dst_binding(5)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 5..current_buffer_info + 6])
                        .build(),
                );
                // temp_descriptor_writes.push(
                //     vk::WriteDescriptorSet::builder()
                //         .dst_set(self.apex_culling_descriptor_sets[instance_id])
                //         .dst_binding(6)
                //         .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                //         .buffer_info(&temp_buffer_infos[current_buffer_info + 6..current_buffer_info + 7])
                //         .build(),
                // );

                instance_id += 1;
            }
        }
        factory.update_descriptor_sets(&temp_descriptor_writes, &[]);
    }

    pub fn update_occlusion_culling_descriptor_sets(
        &mut self,
        visibility_buffer: vk::Buffer,
        factory: &mut DeviceFactory,
    ) {
        log::info!("updating occlusion culling descriptor sets");
        let mut temp_buffer_infos = Vec::with_capacity(self.occlusion_culling_descriptor_sets.len() * 4);
        let mut temp_descriptor_writes = Vec::with_capacity(self.occlusion_culling_descriptor_sets.len() * 4);

        let mut instance_id = 0;
        let mut visibility_offset = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                let current_buffer_info = temp_buffer_infos.len();
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.temp_draw_arguments_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(visibility_offset as _)
                        .range((std::mem::size_of::<u32>() * 4 * instance.draw_count) as _)
                        .buffer(visibility_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.count_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.draw_arguments_buffer)
                        .build(),
                );

                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.occlusion_culling_descriptor_sets[instance_id])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info..current_buffer_info + 1])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.occlusion_culling_descriptor_sets[instance_id])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 1..current_buffer_info + 2])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.occlusion_culling_descriptor_sets[instance_id])
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 2..current_buffer_info + 3])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.occlusion_culling_descriptor_sets[instance_id])
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 3..current_buffer_info + 4])
                        .build(),
                );

                visibility_offset += instance.draw_count * std::mem::size_of::<u32>() * 4;
                instance_id += 1;
            }
        }
        factory.update_descriptor_sets(&temp_descriptor_writes, &[]);
    }

    pub fn update_count_to_dispatch_descriptor_sets(&mut self, factory: &mut DeviceFactory) {
        log::info!("updating dispatch command descriptor sets");
        let mut temp_buffer_infos = Vec::with_capacity(self.count_to_dispatch_descriptor_sets.len() * 2);
        let mut temp_descriptor_writes = Vec::with_capacity(self.count_to_dispatch_descriptor_sets.len() * 2);

        let mut instance_id = 0;
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                let current_buffer_info = temp_buffer_infos.len();
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.count_buffer)
                        .build(),
                );
                temp_buffer_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .offset(0)
                        .range(vk::WHOLE_SIZE)
                        .buffer(instance.occlusion_culling_arguments_buffer)
                        .build(),
                );

                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.count_to_dispatch_descriptor_sets[instance_id])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info..current_buffer_info + 1])
                        .build(),
                );
                temp_descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(self.count_to_dispatch_descriptor_sets[instance_id])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_buffer_infos[current_buffer_info + 1..current_buffer_info + 2])
                        .build(),
                );

                instance_id += 1;
            }
        }
        factory.update_descriptor_sets(&temp_descriptor_writes, &[]);
    }
}

fn create_apex_culling_layout(
    max_sets: usize,
    factory: &mut DeviceFactory,
) -> (
    vk::DescriptorPool,
    vk::DescriptorSetLayout,
    Vec<vk::DescriptorSet>,
    vk::PipelineLayout,
) {
    let descriptor_pool = factory.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::builder()
            .max_sets(max_sets as _)
            .pool_sizes(&[vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(6)
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
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(5)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                // vk::DescriptorSetLayoutBinding::builder()
                //     .binding(6)
                //     .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                //     .descriptor_count(1)
                //     .stage_flags(vk::ShaderStageFlags::COMPUTE)
                //     .build(),
            ])
            .build(),
    );

    let temp_per_descriptor_layouts = vec![descriptor_set_layout; max_sets];
    let descriptor_sets = factory.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&temp_per_descriptor_layouts)
            .build(),
    );

    let pipeline_layout = factory.create_pipeline_layout(
        &vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&[descriptor_set_layout])
            .push_constant_ranges(&[vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .offset(0)
                .size(64)
                .build()])
            .build(),
    );

    (descriptor_pool, descriptor_set_layout, descriptor_sets, pipeline_layout)
}

fn create_occlusion_culling_layout(
    max_sets: usize,
    factory: &mut DeviceFactory,
) -> (
    vk::DescriptorPool,
    vk::DescriptorSetLayout,
    Vec<vk::DescriptorSet>,
    vk::PipelineLayout,
) {
    let descriptor_pool = factory.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::builder()
            .max_sets(max_sets as _)
            .pool_sizes(&[vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(4)
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
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .build(),
            ])
            .build(),
    );

    let temp_per_descriptor_layouts = vec![descriptor_set_layout; max_sets];
    let descriptor_sets = factory.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&temp_per_descriptor_layouts)
            .build(),
    );

    let pipeline_layout = factory.create_pipeline_layout(
        &vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&[descriptor_set_layout])
            .build(),
    );

    (descriptor_pool, descriptor_set_layout, descriptor_sets, pipeline_layout)
}

fn create_count_to_dispatch_layout(
    max_sets: usize,
    factory: &mut DeviceFactory,
) -> (
    vk::DescriptorPool,
    vk::DescriptorSetLayout,
    Vec<vk::DescriptorSet>,
    vk::PipelineLayout,
) {
    let descriptor_pool = factory.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::builder()
            .max_sets(max_sets as _)
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

    let temp_per_descriptor_layouts = vec![descriptor_set_layout; max_sets];
    let descriptor_sets = factory.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&temp_per_descriptor_layouts)
            .build(),
    );

    let pipeline_layout = factory.create_pipeline_layout(
        &vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&[descriptor_set_layout])
            .build(),
    );

    (descriptor_pool, descriptor_set_layout, descriptor_sets, pipeline_layout)
}
