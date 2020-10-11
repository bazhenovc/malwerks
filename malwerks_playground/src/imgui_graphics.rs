// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use crate::surface_pass::*;

#[allow(dead_code)]
pub struct ImguiGraphics {
    font_image: HeapAllocatedResource<vk::Image>,
    font_view: vk::ImageView,
    font_sampler: vk::Sampler,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,

    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    buffer_set: BufferSet,
}

impl ImguiGraphics {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.deallocate_image(&self.font_image);
        factory.destroy_image_view(self.font_view);
        factory.destroy_sampler(self.font_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        factory.destroy_shader_module(self.vert_module);
        factory.destroy_shader_module(self.frag_module);
        factory.destroy_pipeline_layout(self.pipeline_layout);
        factory.destroy_pipeline(self.pipeline);
        self.buffer_set.destroy(factory);
    }

    pub fn new(
        imgui: &mut imgui::Context,
        global_resources: &DiskGlobalResources,
        pass: &SurfacePass,
        command_buffer: &mut CommandBuffer,
        _device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let vert_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&global_resources.imgui_vertex_stage)
                .build(),
        );
        let frag_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&global_resources.imgui_fragment_stage)
                .build(),
        );

        let entry_name = std::ffi::CString::new("main").unwrap();
        let imgui_vert = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(vert_module)
            .stage(vk::ShaderStageFlags::VERTEX);
        let imgui_frag = vk::PipelineShaderStageCreateInfo::builder()
            .name(&entry_name)
            .module(frag_module)
            .stage(vk::ShaderStageFlags::FRAGMENT);

        let font_image = Self::create_font_texture(imgui, factory, command_buffer, queue);
        let font_sampler = factory.create_sampler(
            &vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .build(),
        );
        let font_view = factory.create_image_view(
            &vk::ImageViewCreateInfo::builder()
                .image(font_image.0)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                ),
        );

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder().max_sets(1).pool_sizes(&[
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .build(),
                vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
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
            ]),
        );
        let descriptor_set = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&[descriptor_set_layout])
                .build(),
        )[0];

        factory.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&[vk::DescriptorImageInfo::builder().sampler(font_sampler).build()])
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::builder()
                        .image_view(font_view)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .build()])
                    .build(),
            ],
            &[],
        );

        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&[descriptor_set_layout])
                .push_constant_ranges(&[vk::PushConstantRange::builder()
                    .stage_flags(vk::ShaderStageFlags::VERTEX)
                    .offset(0)
                    .size(64)
                    .build()])
                .build(),
        );
        let pipeline = factory.create_graphics_pipelines(
            vk::PipelineCache::null(),
            &[vk::GraphicsPipelineCreateInfo::builder()
                .stages(&[imgui_vert.build(), imgui_frag.build()])
                .vertex_input_state(
                    &vk::PipelineVertexInputStateCreateInfo::builder()
                        .vertex_binding_descriptions(&[vk::VertexInputBindingDescription::builder()
                            .binding(0)
                            .stride(std::mem::size_of::<imgui::DrawVert>() as _)
                            .input_rate(vk::VertexInputRate::VERTEX)
                            .build()])
                        .vertex_attribute_descriptions(&[
                            vk::VertexInputAttributeDescription::builder()
                                .location(0)
                                .binding(0)
                                .format(vk::Format::R32G32_SFLOAT)
                                .offset(0)
                                .build(),
                            vk::VertexInputAttributeDescription::builder()
                                .location(1)
                                .binding(0)
                                .format(vk::Format::R32G32_SFLOAT)
                                .offset(8)
                                .build(),
                            vk::VertexInputAttributeDescription::builder()
                                .location(2)
                                .binding(0)
                                .format(vk::Format::R32_UINT)
                                .offset(16)
                                .build(),
                        ]),
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
                        .depth_test_enable(false)
                        .depth_write_enable(false)
                        .stencil_test_enable(false)
                        .build(),
                )
                .color_blend_state(
                    &vk::PipelineColorBlendStateCreateInfo::builder().attachments(&[
                        vk::PipelineColorBlendAttachmentState::builder()
                            .blend_enable(true)
                            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                            .color_blend_op(vk::BlendOp::ADD)
                            .src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                            .alpha_blend_op(vk::BlendOp::ADD)
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
                .render_pass(pass.get_render_layer().get_render_pass())
                .subpass(0)
                .base_pipeline_handle(vk::Pipeline::null())
                .base_pipeline_index(0)
                .build()],
        )[0];

        Self {
            font_image,
            font_view,
            font_sampler,

            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,

            vert_module,
            frag_module,

            pipeline_layout,
            pipeline,

            buffer_set: BufferSet::new(),
        }
    }

    pub fn draw(
        &mut self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
        command_buffer: &mut CommandBuffer,
        draw_data: &imgui::DrawData,
    ) {
        puffin::profile_function!();

        let width = draw_data.display_size[0];
        let height = draw_data.display_size[1];

        let clip_offset = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;

        #[rustfmt::skip]
        let matrix = [
            2.0 / width, 0.0, 0.0, 0.0,
            0.0, 2.0 / height, 0.0, 0.0,
            0.0, 0.0, -1.0, 0.0,
            -1.0, -1.0, 0.0, 1.0,
        ];

        command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, self.pipeline);
        command_buffer.push_constants(self.pipeline_layout, vk::ShaderStageFlags::VERTEX, 0, &matrix);
        command_buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &[self.descriptor_set],
            &[],
        );
        command_buffer.set_viewport(
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width,
                height,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );

        self.buffer_set.acquire_frame(frame_context, factory);
        for draw_list in draw_data.draw_lists() {
            puffin::profile_scope!("imgui_draw_list");

            let vertex_buffer = self.buffer_set.create_buffer(
                frame_context,
                factory,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                draw_list.vtx_buffer(),
            );
            let index_buffer = self.buffer_set.create_buffer(
                frame_context,
                factory,
                vk::BufferUsageFlags::INDEX_BUFFER,
                draw_list.idx_buffer(),
            );

            command_buffer.bind_vertex_buffers(0, &[vertex_buffer], &[0]);
            command_buffer.bind_index_buffer(index_buffer, 0, vk::IndexType::UINT16);
            for cmd in draw_list.commands() {
                match cmd {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        let clip_rect = [
                            (cmd_params.clip_rect[0] - clip_offset[0]) * clip_scale[0],
                            (cmd_params.clip_rect[1] - clip_offset[1]) * clip_scale[1],
                            (cmd_params.clip_rect[2] - clip_offset[0]) * clip_scale[0],
                            (cmd_params.clip_rect[3] - clip_offset[1]) * clip_scale[1],
                        ];

                        //let texture_id = cmd_params.texture_id.into();
                        //let tex = self
                        //    .textures
                        //    .get(texture_id)
                        //    .ok_or_else(|| RendererError::BadTexture(texture_id))?;
                        //render_pass.set_bind_group(1, &tex.bind_group, &[]);

                        let scissors = vk::Rect2D {
                            offset: vk::Offset2D {
                                x: clip_rect[0].max(0.0).floor() as _,
                                y: clip_rect[1].max(0.0).floor() as _,
                            },
                            extent: vk::Extent2D {
                                width: (clip_rect[2] - clip_rect[0]).abs().ceil() as _,
                                height: (clip_rect[3] - clip_rect[1]).abs().ceil() as _,
                            },
                        };

                        command_buffer.set_scissor(0, &[scissors]);
                        command_buffer.draw_indexed(
                            count as _,
                            1,
                            cmd_params.idx_offset as _,
                            cmd_params.vtx_offset as _,
                            0,
                        );
                    }
                    imgui::DrawCmd::ResetRenderState => {}
                    //imgui::DrawCmd::RawCallback => {},
                    _ => {}
                }
            }
        }
    }

    fn create_font_texture(
        imgui: &mut imgui::Context,
        factory: &mut DeviceFactory,
        command_buffer: &mut CommandBuffer,
        queue: &mut DeviceQueue,
    ) -> HeapAllocatedResource<vk::Image> {
        let mut atlas = imgui.fonts();
        let texture_handle = atlas.build_rgba32_texture();

        let image = factory.allocate_image(
            &vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width: texture_handle.width,
                    height: texture_handle.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        );

        let mut upload_batch = UploadBatch::new(command_buffer);
        upload_batch.upload_image_memory(
            &image,
            (texture_handle.width, texture_handle.height, 1),
            (0, 1, 1),
            texture_handle.data,
            factory,
        );
        upload_batch.flush(factory, queue);
        image
    }
}

struct BufferSet {
    vertex_buffers: FrameLocal<Vec<HeapAllocatedResource<vk::Buffer>>>,
    index_buffers: FrameLocal<Vec<HeapAllocatedResource<vk::Buffer>>>,
}

impl BufferSet {
    pub fn new() -> Self {
        Self {
            vertex_buffers: FrameLocal::new(|_| Vec::new()),
            index_buffers: FrameLocal::new(|_| Vec::new()),
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.vertex_buffers.destroy(|res| {
            for buffer in res.iter() {
                factory.deallocate_buffer(buffer);
            }
        });
        self.index_buffers.destroy(|res| {
            for buffer in res.iter() {
                factory.deallocate_buffer(buffer);
            }
        });
    }

    pub fn acquire_frame(&mut self, frame_context: &FrameContext, factory: &mut DeviceFactory) {
        puffin::profile_function!();

        let vertex_buffers = self.vertex_buffers.get_mut(frame_context);
        for buffer in vertex_buffers.iter() {
            factory.deallocate_buffer(buffer);
        }
        vertex_buffers.clear();

        let index_buffers = self.index_buffers.get_mut(frame_context);
        for buffer in index_buffers.iter() {
            factory.deallocate_buffer(buffer);
        }
        index_buffers.clear();
    }

    pub fn create_buffer<T>(
        &mut self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
        usage: vk::BufferUsageFlags,
        data: &[T],
    ) -> vk::Buffer {
        puffin::profile_function!();

        let buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size((data.len() * std::mem::size_of::<T>()) as _)
                .usage(usage)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::CpuToGpu,
                required_flags: vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
                ..Default::default()
            },
        );

        let memory = factory.map_allocation_memory(&buffer);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), memory as _, data.len());
        }
        factory.unmap_allocation_memory(&buffer);

        let returned_buffer = buffer.0;
        match usage {
            vk::BufferUsageFlags::VERTEX_BUFFER => self.vertex_buffers.get_mut(frame_context).push(buffer),
            vk::BufferUsageFlags::INDEX_BUFFER => self.index_buffers.get_mut(frame_context).push(buffer),
            _ => panic!("not supported"),
        }
        returned_buffer
    }
}
