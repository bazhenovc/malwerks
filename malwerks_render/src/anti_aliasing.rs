// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_core::*;
use malwerks_vk::*;

use crate::common_shaders::*;
use crate::shared_frame_data::*;

pub struct AntiAliasing {
    render_layers: [RenderLayer; 2],

    point_sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,

    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,

    pipeline_layout: vk::PipelineLayout,
    pipelines: Vec<vk::Pipeline>,

    previous_layer: usize,
    current_layer: usize,
}

impl AntiAliasing {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for render_layer in &mut self.render_layers {
            render_layer.destroy(factory);
        }
        factory.destroy_sampler(self.point_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        factory.destroy_shader_module(self.vert_module);
        factory.destroy_shader_module(self.frag_module);
        factory.destroy_pipeline_layout(self.pipeline_layout);
        for pipeline in &self.pipelines {
            factory.destroy_pipeline(*pipeline);
        }
    }
    pub fn new(
        common_shaders: &DiskCommonShaders,
        shared_frame_data: &SharedFrameData,
        source_layer: &RenderLayer,
        source_color_image: usize,
        image_format: vk::Format,
        image_width: u32,
        image_height: u32,
        device: &Device,
        factory: &mut DeviceFactory,
    ) -> Self {
        let color_attachments = [vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build()];
        let render_layer_parameters = RenderLayerParameters {
            render_image_parameters: &[RenderImageParameters {
                image_format,
                image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                image_clear_value: vk::ClearValue::default(),
            }],
            depth_image_parameters: None,
            render_pass_parameters: &[RenderPassParameters {
                flags: vk::SubpassDescriptionFlags::default(),
                pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
                input_attachments: None,
                color_attachments: Some(&color_attachments),
                resolve_attachments: None,
                depth_stencil_attachment: None,
                preserve_attachments: None,
            }],
            render_pass_dependencies: None,
        };

        let render_layers = [
            RenderLayer::new(device, factory, image_width, image_height, &render_layer_parameters),
            RenderLayer::new(device, factory, image_width, image_height, &render_layer_parameters),
        ];

        let vert_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&common_shaders.anti_aliasing_vertex_stage)
                .build(),
        );
        let frag_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&common_shaders.anti_aliasing_fragment_stage)
                .build(),
        );

        let entry_name = std::ffi::CString::new("main").expect("failed to allocate entry name");
        let vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(vert_module)
            .stage(vk::ShaderStageFlags::VERTEX);
        let fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(frag_module)
            .stage(vk::ShaderStageFlags::FRAGMENT);

        let point_sampler = factory.create_sampler(
            &vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .min_lod(0.0)
                .max_lod(std::f32::MAX)
                .build(),
        );

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder().max_sets(2).pool_sizes(&[
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(2)
                    .build(),
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(3)
                    .build(),
            ]),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&[
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ]),
        );
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&[descriptor_set_layout, descriptor_set_layout])
                .build(),
        );

        let source_color_image = source_layer.get_render_image(source_color_image).1;
        let source_depth_image = source_layer
            .get_depth_image()
            .expect("Depth image is required for anti aliasing")
            .1;
        factory.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[0])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&[vk::DescriptorImageInfo::builder().sampler(point_sampler).build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[0])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(source_color_image)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[0])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(source_depth_image)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[0])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(render_layers[1].get_render_image(0).1)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[1])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&[vk::DescriptorImageInfo::builder().sampler(point_sampler).build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[1])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(source_color_image)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[1])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(source_depth_image)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[1])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(render_layers[0].get_render_image(0).1)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
            ],
            &[],
        );

        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&[descriptor_set_layout, shared_frame_data.descriptor_set_layout])
                .build(),
        );
        let mut pipeline_create_infos = [vk::GraphicsPipelineCreateInfo::builder()
            .stages(&[vertex_stage.build(), fragment_stage.build()])
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
            .depth_stencil_state(
                &vk::PipelineDepthStencilStateCreateInfo::builder()
                    .flags(Default::default())
                    .depth_test_enable(true)
                    .depth_write_enable(false)
                    .depth_compare_op(vk::CompareOp::EQUAL)
                    .stencil_test_enable(false)
                    .build(),
            )
            .color_blend_state(
                &vk::PipelineColorBlendStateCreateInfo::builder().attachments(&[
                    vk::PipelineColorBlendAttachmentState::builder()
                        .blend_enable(false)
                        .color_write_mask(
                            vk::ColorComponentFlags::R
                                | vk::ColorComponentFlags::G
                                | vk::ColorComponentFlags::B
                                | vk::ColorComponentFlags::A,
                        )
                        .build(),
                ]),
            )
            .dynamic_state(
                &vk::PipelineDynamicStateCreateInfo::builder()
                    .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR])
                    .build(),
            )
            .layout(pipeline_layout)
            .subpass(0)
            .base_pipeline_handle(vk::Pipeline::null())
            .base_pipeline_index(0)
            .build(); 2];
        pipeline_create_infos[0].render_pass = render_layers[0].get_render_pass();
        pipeline_create_infos[1].render_pass = render_layers[1].get_render_pass();

        let pipelines = factory.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_create_infos);

        Self {
            render_layers,
            point_sampler,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            vert_module,
            frag_module,
            pipeline_layout,
            pipelines,
            previous_layer: 1,
            current_layer: 0,
        }
    }

    pub fn render(
        &mut self,
        screen_area: vk::Rect2D,
        shared_frame_data: &SharedFrameData,
        frame_context: &FrameContext,
        device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        let previous_image = self.render_layers[self.previous_layer].get_render_image(0).0;
        let current_image = self.render_layers[self.current_layer].get_render_image(0).0;

        let current_layer = &mut self.render_layers[self.current_layer];
        current_layer.acquire_frame(frame_context, device, factory);

        let command_buffer = current_layer.get_command_buffer(frame_context);
        command_buffer.pipeline_barrier(
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            None,
            &[],
            &[],
            &[vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::MEMORY_READ)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(!0)
                .dst_queue_family_index(!0)
                .image(current_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build()],
        );
        command_buffer.pipeline_barrier(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            None,
            &[],
            &[],
            &[vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(!0)
                .dst_queue_family_index(!0)
                .image(previous_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build()],
        );

        current_layer.begin_render_pass(frame_context, screen_area);

        let command_buffer = current_layer.get_command_buffer(frame_context);
        command_buffer.set_viewport(
            0,
            &[vk::Viewport {
                x: screen_area.offset.x as _,
                y: screen_area.offset.y as _,
                width: screen_area.extent.width as _,
                height: screen_area.extent.height as _,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        command_buffer.set_scissor(0, &[screen_area]);

        command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, self.pipelines[self.current_layer]);
        command_buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &[
                self.descriptor_sets[self.current_layer],
                *shared_frame_data.get_frame_data_descriptor_set(frame_context),
            ],
            &[],
        );
        command_buffer.draw(3, 1, 0, 0);

        current_layer.end_render_pass(frame_context);

        let command_buffer = current_layer.get_command_buffer(frame_context);
        command_buffer.pipeline_barrier(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            None,
            &[],
            &[],
            &[vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(!0)
                .dst_queue_family_index(!0)
                .image(current_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build()],
        );
        current_layer.submit_commands(frame_context, queue);

        self.previous_layer = self.current_layer;
        self.current_layer = (self.current_layer + 1) % 2;
    }

    pub fn get_previous_render_layer(&self) -> &RenderLayer {
        &self.render_layers[self.previous_layer]
    }

    pub fn get_current_render_layer(&self) -> &RenderLayer {
        &self.render_layers[self.current_layer]
    }

    pub fn get_current_render_layer_mut(&mut self) -> &mut RenderLayer {
        &mut self.render_layers[self.current_layer]
    }
}
