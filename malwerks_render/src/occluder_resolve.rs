// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::occluder_pass::*;
use crate::render_pass::*;

#[derive(Default)]
pub struct OccluderResolve {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,

    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    // visibility_flags_buffer: HeapAllocatedResource<vk::Buffer>,
}

impl OccluderResolve {
    pub fn new(
        disk_scenery: &DiskStaticScenery,
        source_pass: &OccluderPass, // TODO: make it a generic render pass
        output_visibility_buffer: vk::Buffer,
        factory: &mut DeviceFactory,
        pipeline_cache: vk::PipelineCache,
    ) -> Self {
        log::info!("initializing occluder resolve");
        let vert_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.occluder_resolve_vertex_stage)
                .build(),
        );
        let frag_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.occluder_resolve_fragment_stage)
                .build(),
        );

        let entry_name = std::ffi::CString::new("main").expect("failed to allocate entry name");
        let resolve_vert = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(vert_module)
            .stage(vk::ShaderStageFlags::VERTEX);
        let resolve_frag = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(frag_module)
            .stage(vk::ShaderStageFlags::FRAGMENT);

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder().max_sets(1).pool_sizes(&[
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::INPUT_ATTACHMENT)
                    .descriptor_count(1)
                    .build(),
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .build(),
            ]),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&[
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ]),
        );

        let temp_per_descriptor_layouts = [descriptor_set_layout; 1];
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut temp_buffer_infos = [Default::default(); 1];
        let mut temp_descriptor_writes = [Default::default(); 2];
        {
            temp_buffer_infos[0] = vk::DescriptorBufferInfo::builder()
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .buffer(output_visibility_buffer)
                .build();

            temp_descriptor_writes[0] = vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_sets[0])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .image_info(&[vk::DescriptorImageInfo::builder()
                    .image_view(source_pass.get_occluder_data_image_view())
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .build()])
                .build();
            temp_descriptor_writes[1] = vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_sets[0])
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&temp_buffer_infos)
                .build();
        }
        factory.update_descriptor_sets(&temp_descriptor_writes, &[]);

        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&[descriptor_set_layout])
                .build(),
        );
        let pipeline = factory.create_graphics_pipelines(
            pipeline_cache,
            &[vk::GraphicsPipelineCreateInfo::builder()
                .stages(&[resolve_vert.build(), resolve_frag.build()])
                .vertex_input_state(
                    &vk::PipelineVertexInputStateCreateInfo::builder()
                        .vertex_binding_descriptions(&[])
                        .build(),
                )
                .input_assembly_state(
                    &vk::PipelineInputAssemblyStateCreateInfo::builder()
                        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                        .primitive_restart_enable(false)
                        .build(),
                )
                .tessellation_state(&Default::default())
                .viewport_state(
                    &vk::PipelineViewportStateCreateInfo::builder()
                        .viewport_count(1)
                        .scissor_count(1)
                        .build(),
                )
                .rasterization_state(
                    &vk::PipelineRasterizationStateCreateInfo::builder()
                        .line_width(1.0)
                        .build(),
                )
                .multisample_state(
                    &vk::PipelineMultisampleStateCreateInfo::builder()
                        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                        .build(),
                )
                .depth_stencil_state(&Default::default())
                .color_blend_state(
                    &vk::PipelineColorBlendStateCreateInfo::builder().attachments(&[
                        vk::PipelineColorBlendAttachmentState::builder()
                            .blend_enable(false)
                            .color_write_mask(vk::ColorComponentFlags::default())
                            .build(),
                    ]),
                )
                .dynamic_state(
                    &vk::PipelineDynamicStateCreateInfo::builder()
                        .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
                        .build(),
                )
                .layout(pipeline_layout)
                .render_pass(source_pass.get_render_pass())
                .subpass(1)
                .base_pipeline_handle(vk::Pipeline::null())
                .base_pipeline_index(0)
                .build()],
        )[0];

        Self {
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            vert_module,
            frag_module,
            pipeline_layout,
            pipeline,
            // visibility_flags_buffer,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        factory.destroy_shader_module(self.vert_module);
        factory.destroy_shader_module(self.frag_module);
        factory.destroy_pipeline_layout(self.pipeline_layout);
        factory.destroy_pipeline(self.pipeline);
        // factory.deallocate_buffer(&self.visibility_flags_buffer);
    }

    pub fn render(&self, command_buffer: &mut CommandBuffer, _frame_context: &FrameContext) {
        command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, self.pipeline);
        command_buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[0]],
            &[],
        );
        command_buffer.draw(3, 1, 0, 0);
        // command_buffer.pipeline_barrier(
        //     vk::PipelineStageFlags::FRAGMENT_SHADER,
        //     vk::PipelineStageFlags::DRAW_INDIRECT,
        //     None,
        //     &[],
        //     &[vk::BufferMemoryBarrier::builder()
        //         .src_access_mask(vk::AccessFlags::SHADER_WRITE)
        //         .dst_access_mask(vk::AccessFlags::INDIRECT_COMMAND_READ)
        //         .src_queue_family_index(!0)
        //         .dst_queue_family_index(!0)
        //         .buffer(target_buffer)
        //         .offset(0)
        //         .size(vk::WHOLE_SIZE)
        //         .build()],
        //     &[],
        // );
    }
}
