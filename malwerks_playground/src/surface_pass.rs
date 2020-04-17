// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use crate::surface_winit::*;

pub struct SurfacePass {
    base_pass: BaseRenderPass,
    _images: Vec<vk::Image>,
    _image_views: Vec<vk::ImageView>,
    image_ready_semaphore: FrameLocal<vk::Semaphore>,
}

impl SurfacePass {
    pub fn new(surface: &SurfaceWinit, device: &GraphicsDevice, factory: &mut GraphicsFactory) -> Self {
        let swapchain_images = unsafe {
            surface
                .get_swapchain_loader()
                .get_swapchain_images(surface.get_swapchain())
                .unwrap()
        };
        let swapchain_image_views: Vec<vk::ImageView> = swapchain_images
            .iter()
            .map(|&image| {
                factory.create_image_view(
                    &vk::ImageViewCreateInfo::builder()
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(surface.get_surface_format())
                        .components(Default::default())
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .image(image)
                        .build(),
                )
            })
            .collect();

        let render_pass = factory.create_render_pass(
            &vk::RenderPassCreateInfo::builder()
                .flags(Default::default())
                .attachments(&[vk::AttachmentDescription::builder()
                    .flags(Default::default())
                    .format(surface.get_surface_format())
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .build()])
                .subpasses(&[vk::SubpassDescription::builder()
                    .flags(Default::default())
                    .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                    //.input_attachments(...)
                    //.resolve_attachments(...)
                    //.preserve_attachments(...)
                    //.depth_stencil_attachment(...)
                    .color_attachments(&[vk::AttachmentReference::builder()
                        .attachment(0)
                        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .build()])
                    .build()])
                .dependencies(&[vk::SubpassDependency::builder()
                    .src_subpass(vk::SUBPASS_EXTERNAL)
                    .dst_subpass(0)
                    .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                    .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                    .src_access_mask(Default::default())
                    .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dependency_flags(vk::DependencyFlags::BY_REGION)
                    .build()])
                .build(),
        );

        let framebuffer = FrameLocal::new(|frame_index| {
            factory.create_framebuffer(
                &vk::FramebufferCreateInfo::builder()
                    .flags(Default::default())
                    .render_pass(render_pass)
                    .attachments(&swapchain_image_views[frame_index..=frame_index])
                    .width(surface.get_surface_extent().width)
                    .height(surface.get_surface_extent().height)
                    .layers(1)
                    .build(),
            )
        });
        let image_ready_semaphore = FrameLocal::new(|_| factory.create_semaphore(&vk::SemaphoreCreateInfo::default()));

        Self {
            base_pass: BaseRenderPass::new(
                device,
                factory,
                render_pass,
                framebuffer,
                vec![vk::ClearValue::default()],
            ),
            _images: swapchain_images,
            _image_views: swapchain_image_views,
            image_ready_semaphore,
        }
    }

    //pub fn get_image(&self) -> vk::Image {
    //    self.images[self.base_pass.get_current_frame()]
    //}

    //pub fn get_image_view(&self) -> vk::ImageView {
    //    self.image_views[self.base_pass.get_current_frame()]
    //}

    pub fn get_image_ready_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
        *self.image_ready_semaphore.get(frame_context)
    }

    //pub fn get_base_pass(&self) -> &BaseRenderPass {
    //    &self.base_pass
    //}
}

impl RenderPass for SurfacePass {
    fn begin(
        &mut self,
        frame_context: &FrameContext,
        device: &mut GraphicsDevice,
        factory: &mut GraphicsFactory,
        render_area: vk::Rect2D,
    ) {
        self.add_wait_condition(
            self.get_image_ready_semaphore(frame_context),
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        );
        self.base_pass.begin(frame_context, device, factory, render_area);
    }

    fn end(&mut self, frame_context: &FrameContext) {
        self.base_pass.end(frame_context);
    }

    fn submit_commands(&mut self, frame_context: &FrameContext, graphics_queue: &mut DeviceQueue) {
        self.base_pass.submit_commands(frame_context, graphics_queue);
    }

    fn get_signal_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
        self.base_pass.get_signal_semaphore(frame_context)
    }

    fn get_signal_fence(&self, frame_context: &FrameContext) -> vk::Fence {
        self.base_pass.get_signal_fence(frame_context)
    }

    fn get_render_pass(&self) -> vk::RenderPass {
        self.base_pass.get_render_pass()
    }

    fn get_framebuffer(&self, frame_context: &FrameContext) -> vk::Framebuffer {
        self.base_pass.get_framebuffer(frame_context)
    }

    fn get_command_buffer(&mut self, frame_context: &FrameContext) -> &mut CommandBuffer {
        self.base_pass.get_command_buffer(frame_context)
    }

    fn add_wait_condition(&mut self, semaphore: vk::Semaphore, stage_mask: vk::PipelineStageFlags) {
        self.base_pass.add_wait_condition(semaphore, stage_mask);
    }

    fn destroy(&mut self, factory: &mut GraphicsFactory) {
        self.image_ready_semaphore
            .destroy(|res| factory.destroy_semaphore(*res));
        self.base_pass.destroy(factory);
    }
}
