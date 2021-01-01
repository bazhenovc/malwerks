// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

pub struct RenderImageParameters {
    pub image_format: vk::Format,
    pub image_usage: vk::ImageUsageFlags,
    pub image_clear_value: vk::ClearValue,
}

pub struct RenderPassParameters<'a> {
    pub flags: vk::SubpassDescriptionFlags,
    pub pipeline_bind_point: vk::PipelineBindPoint,
    pub input_attachments: Option<&'a [vk::AttachmentReference]>,
    pub color_attachments: Option<&'a [vk::AttachmentReference]>,
    pub resolve_attachments: Option<&'a [vk::AttachmentReference]>,
    pub depth_stencil_attachment: Option<&'a vk::AttachmentReference>,
    pub preserve_attachments: Option<&'a [u32]>,
}

pub struct RenderLayerParameters<'a> {
    pub render_image_parameters: &'a [RenderImageParameters],
    pub depth_image_parameters: Option<RenderImageParameters>,
    pub render_pass_parameters: &'a [RenderPassParameters<'a>],
    pub render_pass_dependencies: Option<&'a [vk::SubpassDependency]>,
}

pub struct RenderLayer {
    render_pass: vk::RenderPass,
    framebuffer: FrameLocal<vk::Framebuffer>,
    command_pool: FrameLocal<vk::CommandPool>,
    command_buffer: FrameLocal<CommandBuffer>,
    signal_semaphore: FrameLocal<vk::Semaphore>,
    signal_fence: FrameLocal<vk::Fence>,
    wait_semaphores: Vec<vk::Semaphore>,
    wait_stage_mask: Vec<vk::PipelineStageFlags>,
    timestamp_query_pool: vk::QueryPool,
    render_images: Vec<RenderImage>,
    depth_image: Option<RenderImage>,
    clear_values: Vec<vk::ClearValue>,
}

impl RenderLayer {
    pub fn new<'a>(
        device: &Device,
        factory: &mut DeviceFactory,
        width: u32,
        height: u32,
        layer_parameters: &RenderLayerParameters<'a>,
    ) -> Self {
        let command_pool = FrameLocal::new(|_| {
            factory.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(device.get_graphics_queue_index())
                    .build(),
            )
        });
        let command_buffer = FrameLocal::new(|f| {
            factory.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(*command_pool.get_frame(f))
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build(),
            )[0]
        });
        let signal_semaphore = FrameLocal::new(|_| factory.create_semaphore(&vk::SemaphoreCreateInfo::default()));
        let signal_fence = FrameLocal::new(|_| {
            factory.create_fence(
                &vk::FenceCreateInfo::builder()
                    .flags(vk::FenceCreateFlags::SIGNALED)
                    .build(),
            )
        });

        let timestamp_query_pool = factory.create_query_pool(
            &vk::QueryPoolCreateInfo::builder()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count((2 * NUM_BUFFERED_GPU_FRAMES) as _)
                .build(),
        );

        let mut clear_values = Vec::with_capacity(
            layer_parameters.render_image_parameters.len()
                + (layer_parameters.depth_image_parameters.is_some() as usize),
        );
        let mut all_image_views = Vec::with_capacity(clear_values.len());

        let mut render_images = Vec::with_capacity(layer_parameters.render_image_parameters.len());
        for parameters in layer_parameters.render_image_parameters {
            let (image, image_view) =
                allocate_render_image(device, factory, width, height, parameters, vk::ImageAspectFlags::COLOR);

            clear_values.push(parameters.image_clear_value);
            all_image_views.push(image_view);
            render_images.push(RenderImage { image, image_view });
        }

        let depth_image = if let Some(depth_image_parameters) = layer_parameters.depth_image_parameters.as_ref() {
            let (image, image_view) = allocate_render_image(
                device,
                factory,
                width,
                height,
                &depth_image_parameters,
                vk::ImageAspectFlags::DEPTH,
            );
            clear_values.push(depth_image_parameters.image_clear_value);
            all_image_views.push(image_view);

            Some(RenderImage { image, image_view })
        } else {
            None
        };

        let render_pass = {
            let mut attachments = Vec::with_capacity(render_images.len() + (depth_image.is_some() as usize));
            let mut color_attachments = Vec::with_capacity(render_images.len());

            for image in layer_parameters.render_image_parameters {
                color_attachments.push(
                    vk::AttachmentReference::builder()
                        .attachment(attachments.len() as _)
                        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .build(),
                );
                attachments.push(
                    vk::AttachmentDescription::builder()
                        .flags(Default::default())
                        .format(image.image_format)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .build(),
                );
            }
            if let Some(depth_image_parameters) = layer_parameters.depth_image_parameters.as_ref() {
                attachments.push(
                    vk::AttachmentDescription::builder()
                        .flags(Default::default())
                        .format(depth_image_parameters.image_format)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .build(),
                );
            }

            let mut subpasses = Vec::with_capacity(layer_parameters.render_pass_parameters.len());
            for render_pass_parameter in layer_parameters.render_pass_parameters {
                let mut subpass_builder = vk::SubpassDescription::builder()
                    .flags(render_pass_parameter.flags)
                    .pipeline_bind_point(render_pass_parameter.pipeline_bind_point);

                if let Some(input_attachments) = render_pass_parameter.input_attachments {
                    subpass_builder = subpass_builder.input_attachments(input_attachments);
                }
                if let Some(color_attachments) = render_pass_parameter.color_attachments {
                    subpass_builder = subpass_builder.color_attachments(color_attachments);
                }
                if let Some(resolve_attachments) = render_pass_parameter.resolve_attachments {
                    subpass_builder = subpass_builder.resolve_attachments(resolve_attachments);
                }
                if let Some(depth_stencil_attachment) = render_pass_parameter.depth_stencil_attachment {
                    subpass_builder = subpass_builder.depth_stencil_attachment(depth_stencil_attachment);
                }
                if let Some(preserve_attachments) = render_pass_parameter.preserve_attachments {
                    subpass_builder = subpass_builder.preserve_attachments(preserve_attachments);
                }

                subpasses.push(subpass_builder.build());
            }

            let mut render_pass_builder = vk::RenderPassCreateInfo::builder()
                .flags(Default::default())
                .attachments(&attachments)
                .subpasses(&subpasses);
            if let Some(dependencies) = layer_parameters.render_pass_dependencies {
                render_pass_builder = render_pass_builder.dependencies(dependencies)
            }

            factory.create_render_pass(&render_pass_builder.build())
        };

        let framebuffer = FrameLocal::new(|_| {
            factory.create_framebuffer(
                &vk::FramebufferCreateInfo::builder()
                    .flags(Default::default())
                    .render_pass(render_pass)
                    .attachments(&all_image_views)
                    .width(width)
                    .height(height)
                    .layers(1)
                    .build(),
            )
        });

        Self {
            render_pass,
            framebuffer,
            command_pool,
            command_buffer,
            signal_semaphore,
            signal_fence,
            wait_semaphores: Vec::new(),
            wait_stage_mask: Vec::new(),
            timestamp_query_pool,
            render_images,
            depth_image,
            clear_values,
        }
    }

    pub fn from_existing_render_pass(
        device: &Device,
        factory: &mut DeviceFactory,
        render_pass: vk::RenderPass,
        framebuffer: FrameLocal<vk::Framebuffer>,
        clear_values: Vec<vk::ClearValue>,
    ) -> Self {
        let command_pool = FrameLocal::new(|_| {
            factory.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(device.get_graphics_queue_index())
                    .build(),
            )
        });
        let command_buffer = FrameLocal::new(|f| {
            factory.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(*command_pool.get_frame(f))
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build(),
            )[0]
        });
        let signal_semaphore = FrameLocal::new(|_| factory.create_semaphore(&vk::SemaphoreCreateInfo::default()));
        let signal_fence = FrameLocal::new(|_| {
            factory.create_fence(
                &vk::FenceCreateInfo::builder()
                    .flags(vk::FenceCreateFlags::SIGNALED)
                    .build(),
            )
        });

        let timestamp_query_pool = factory.create_query_pool(
            &vk::QueryPoolCreateInfo::builder()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count((2 * NUM_BUFFERED_GPU_FRAMES) as _)
                .build(),
        );

        Self {
            render_pass,
            framebuffer,
            command_pool,
            command_buffer,
            signal_semaphore,
            signal_fence,
            wait_semaphores: Vec::new(),
            wait_stage_mask: Vec::new(),
            timestamp_query_pool,
            render_images: Vec::new(),
            depth_image: None,
            clear_values,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_render_pass(self.render_pass);
        self.framebuffer.destroy(|res| factory.destroy_framebuffer(*res));
        self.command_pool.destroy(|res| factory.destroy_command_pool(*res));
        self.signal_semaphore.destroy(|res| factory.destroy_semaphore(*res));
        self.signal_fence.destroy(|res| factory.destroy_fence(*res));
        factory.destroy_query_pool(self.timestamp_query_pool);
        for image in &self.render_images {
            factory.deallocate_image(&image.image);
            factory.destroy_image_view(image.image_view);
        }
        if let Some(depth_image) = self.depth_image.as_ref() {
            factory.deallocate_image(&depth_image.image);
            factory.destroy_image_view(depth_image.image_view);
        }
    }

    pub fn get_render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    pub fn get_render_image(&self, index: usize) -> (vk::Image, vk::ImageView) {
        let image = &self.render_images[index];
        (image.image.0, image.image_view)
    }

    pub fn get_depth_image(&self) -> Option<(vk::Image, vk::ImageView)> {
        match &self.depth_image {
            Some(depth_image) => Some((depth_image.image.0, depth_image.image_view)),
            None => None,
        }
    }

    pub fn get_image_resource(&self, index: usize) -> &HeapAllocatedResource<vk::Image> {
        &self.render_images[index].image
    }

    pub fn get_depth_resource(&self) -> Option<&HeapAllocatedResource<vk::Image>> {
        match &self.depth_image {
            Some(depth_image) => Some(&depth_image.image),
            None => None,
        }
    }
}

impl RenderLayer {
    pub fn add_dependency(
        &mut self,
        frame_context: &FrameContext,
        layer: &RenderLayer,
        stage_mask: vk::PipelineStageFlags,
    ) {
        self.wait_semaphores.push(*layer.signal_semaphore.get(frame_context));
        self.wait_stage_mask.push(stage_mask);
    }

    pub fn add_wait_condition(&mut self, semaphore: vk::Semaphore, stage_mask: vk::PipelineStageFlags) {
        self.wait_semaphores.push(semaphore);
        self.wait_stage_mask.push(stage_mask);
    }

    pub fn acquire_frame(&mut self, frame_context: &FrameContext, device: &mut Device, factory: &mut DeviceFactory) {
        let signal_fence = self.signal_fence.get(frame_context);
        device.reset_fences(&[*signal_fence]);

        let command_pool = self.command_pool.get(frame_context);
        factory.reset_command_pool(*command_pool);

        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.begin(
            &vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                //.inheritance_info(...)
                .build(),
        );
        let start_pass_query = frame_context.current_gpu_frame() * 2;
        command_buffer.reset_query_pool(self.timestamp_query_pool, start_pass_query as _, 2);

        command_buffer.write_timestamp(
            vk::PipelineStageFlags::ALL_GRAPHICS,
            self.timestamp_query_pool,
            start_pass_query as _,
        );
    }

    pub fn begin_render_pass(&mut self, frame_context: &FrameContext, render_area: vk::Rect2D) {
        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.begin_render_pass(
            &vk::RenderPassBeginInfo::builder()
                .render_pass(self.render_pass)
                .framebuffer(*self.framebuffer.get(frame_context))
                .render_area(render_area)
                .clear_values(&self.clear_values)
                .build(),
            vk::SubpassContents::INLINE,
        );
    }

    pub fn end_render_pass(&mut self, frame_context: &FrameContext) {
        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.end_render_pass();

        let end_pass_query = frame_context.current_gpu_frame() * 2 + 1;
        command_buffer.write_timestamp(
            vk::PipelineStageFlags::ALL_GRAPHICS,
            self.timestamp_query_pool,
            end_pass_query as _,
        );
    }

    pub fn submit_commands(&mut self, frame_context: &FrameContext, queue: &mut DeviceQueue) {
        let signal_semaphore = self.signal_semaphore.get(frame_context);
        let signal_fence = self.signal_fence.get(frame_context);

        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.end();

        queue.submit(
            &[vk::SubmitInfo::builder()
                .wait_semaphores(&self.wait_semaphores)
                .wait_dst_stage_mask(&self.wait_stage_mask)
                .signal_semaphores(&[*signal_semaphore])
                .command_buffers(&[command_buffer.clone().into()])
                .build()],
            *signal_fence,
        );

        self.wait_semaphores.clear();
        self.wait_stage_mask.clear();
    }
}

impl RenderLayer {
    pub fn get_signal_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
        *self.signal_semaphore.get(frame_context)
    }

    pub fn get_signal_fence(&self, frame_context: &FrameContext) -> vk::Fence {
        *self.signal_fence.get(frame_context)
    }

    pub fn get_framebuffer(&self, frame_context: &FrameContext) -> vk::Framebuffer {
        *self.framebuffer.get(frame_context)
    }

    pub fn get_command_buffer(&mut self, frame_context: &FrameContext) -> &mut CommandBuffer {
        self.command_buffer.get_mut(frame_context)
    }

    pub fn try_get_oldest_timestamp(
        &self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
    ) -> Option<[u64; 2]> {
        let oldest_frame = (frame_context.current_gpu_frame() + NUM_BUFFERED_GPU_FRAMES) % NUM_BUFFERED_GPU_FRAMES;
        let mut data = [0u64; 2];
        let result = factory.get_query_pool_results(
            self.timestamp_query_pool,
            (oldest_frame * 2) as _,
            2,
            &mut data,
            vk::QueryResultFlags::TYPE_64,
        );
        match result {
            Ok(_) => Some(data),
            Err(code) => match code {
                vk::Result::NOT_READY => None,
                _ => panic!("try_get_oldest_timestamp(): Internal GPU error: {}", code),
            },
        }
    }
}

struct RenderImage {
    image: HeapAllocatedResource<vk::Image>,
    image_view: vk::ImageView,
}

fn allocate_render_image(
    device: &Device,
    factory: &mut DeviceFactory,
    width: u32,
    height: u32,
    parameters: &RenderImageParameters,
    aspect_mask: vk::ImageAspectFlags,
) -> (HeapAllocatedResource<vk::Image>, vk::ImageView) {
    let extra_image_usage_flags = if device.get_device_options().enable_render_target_export {
        vk::ImageUsageFlags::TRANSFER_SRC
    } else {
        vk::ImageUsageFlags::default()
    };

    let image_extent = vk::Extent3D {
        width,
        height,
        depth: 1,
    };

    let image = factory.allocate_image(
        &vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(parameters.image_format)
            .extent(image_extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(parameters.image_usage | extra_image_usage_flags)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .build(),
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        },
    );
    let image_view = factory.create_image_view(
        &vk::ImageViewCreateInfo::builder()
            .image(image.0)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(parameters.image_format)
            .components(vk::ComponentMapping::default())
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(aspect_mask)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .build(),
    );

    (image, image_view)
}
