// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

use crate::render_pass::*;

pub struct ForwardPass {
    base_pass: BaseRenderPass,

    depth_image: HeapAllocatedResource<vk::Image>,
    depth_image_view: vk::ImageView,

    color_image: HeapAllocatedResource<vk::Image>,
    color_image_view: vk::ImageView,
}

impl ForwardPass {
    pub fn new(width: u32, height: u32, device: &GraphicsDevice, factory: &mut GraphicsFactory) -> Self {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };
        let depth_image = factory.allocate_image(
            &vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::D32_SFLOAT)
                .extent(extent)
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        );
        let depth_image_view = factory.create_image_view(
            &vk::ImageViewCreateInfo::builder()
                .image(depth_image.0)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::D32_SFLOAT)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build(),
        );

        let color_image = factory.allocate_image(
            &vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::B10G11R11_UFLOAT_PACK32)
                .extent(extent)
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        );
        let color_image_view = factory.create_image_view(
            &vk::ImageViewCreateInfo::builder()
                .image(color_image.0)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::B10G11R11_UFLOAT_PACK32)
                .components(vk::ComponentMapping::default())
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build(),
        );

        let render_pass = factory.create_render_pass(
            &vk::RenderPassCreateInfo::builder()
                .flags(Default::default())
                .attachments(&[
                    vk::AttachmentDescription::builder()
                        .flags(Default::default())
                        .format(vk::Format::B10G11R11_UFLOAT_PACK32)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .build(),
                    vk::AttachmentDescription::builder()
                        .flags(Default::default())
                        .format(vk::Format::D32_SFLOAT)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                        .build(),
                ])
                .subpasses(&[vk::SubpassDescription::builder()
                    .flags(Default::default())
                    .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                    //.input_attachments(&[])
                    //.resolve_attachments(&[])
                    //.preserve_attachments(&[])
                    .color_attachments(&[vk::AttachmentReference::builder()
                        .attachment(0)
                        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .build()])
                    .depth_stencil_attachment(
                        &vk::AttachmentReference::builder()
                            .attachment(1)
                            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                            .build(),
                    )
                    .build()])
                .dependencies(&[
                    vk::SubpassDependency::builder()
                        .src_subpass(vk::SUBPASS_EXTERNAL)
                        .dst_subpass(0)
                        .src_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                        .src_access_mask(vk::AccessFlags::MEMORY_READ)
                        .dst_access_mask(
                            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        )
                        .dependency_flags(vk::DependencyFlags::BY_REGION)
                        .build(),
                    vk::SubpassDependency::builder()
                        .src_subpass(0)
                        .dst_subpass(vk::SUBPASS_EXTERNAL)
                        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                        .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                        .src_access_mask(
                            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        )
                        .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                        .dependency_flags(vk::DependencyFlags::BY_REGION)
                        .build(),
                ])
                .build(),
        );

        // TODO: Create 1 framebuffer instead of per-frame
        let framebuffer = FrameLocal::new(|_| {
            factory.create_framebuffer(
                &vk::FramebufferCreateInfo::builder()
                    .flags(Default::default())
                    .render_pass(render_pass)
                    .attachments(&[color_image_view, depth_image_view])
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1)
                    .build(),
            )
        });

        Self {
            base_pass: BaseRenderPass::new(
                device,
                factory,
                render_pass,
                framebuffer,
                vec![vk::ClearValue::default(), vk::ClearValue::default()],
            ),

            depth_image,
            depth_image_view,
            color_image,
            color_image_view,
        }
    }

    //fn get_depth_image_view(&self) -> vk::ImageView {
    //    self.depth_image_view
    //}

    pub fn get_color_image(&self) -> vk::Image {
        self.color_image.0
    }

    pub fn get_color_image_view(&self) -> vk::ImageView {
        self.color_image_view
    }
}

impl ForwardPass {
    fn destroy_internal(&mut self, graphics_factory: &mut GraphicsFactory) {
        graphics_factory.deallocate_image(&self.depth_image);
        graphics_factory.deallocate_image(&self.color_image);
        graphics_factory.destroy_image_view(self.depth_image_view);
        graphics_factory.destroy_image_view(self.color_image_view);
    }
}

default_render_pass_impl!(ForwardPass, base_pass);
