// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;
use malwerks_vk::*;

use crate::surface_winit::*;

pub struct SurfacePass {
    render_layer: RenderLayer,
    _images: Vec<vk::Image>,
    _image_views: Vec<vk::ImageView>,
    image_ready_semaphore: FrameLocal<vk::Semaphore>,
}

impl SurfacePass {
    pub fn new(surface: &SurfaceWinit, device: &Device, factory: &mut DeviceFactory) -> Self {
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
        let clear_values = vec![vk::ClearValue::default()];
        let image_ready_semaphore = FrameLocal::new(|_| factory.create_semaphore(&vk::SemaphoreCreateInfo::default()));

        Self {
            render_layer: RenderLayer::from_existing_render_pass(
                device,
                factory,
                render_pass,
                framebuffer,
                clear_values,
            ),
            _images: swapchain_images,
            _image_views: swapchain_image_views,
            image_ready_semaphore,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.image_ready_semaphore
            .destroy(|res| factory.destroy_semaphore(*res));
        self.render_layer.destroy(factory);
    }

    pub fn try_get_oldest_timestamp(
        &self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
    ) -> Option<[u64; 2]> {
        self.render_layer.try_get_oldest_timestamp(frame_context, factory)
    }

    pub fn get_image_ready_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
        *self.image_ready_semaphore.get(frame_context)
    }

    pub fn get_render_layer(&self) -> &RenderLayer {
        &self.render_layer
    }

    pub fn get_render_layer_mut(&mut self) -> &mut RenderLayer {
        &mut self.render_layer
    }
}
