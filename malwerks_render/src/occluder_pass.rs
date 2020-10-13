// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

use crate::render_layer::*;

pub struct OccluderPass {
    render_layer: RenderLayer,
    extent: vk::Extent3D,
}

impl OccluderPass {
    pub fn new(width: u32, height: u32, device: &Device, factory: &mut DeviceFactory) -> Self {
        let width = width / 2;
        let height = height / 2;

        let render_layer = RenderLayer::new(
            device,
            factory,
            width,
            height,
            &RenderLayerParameters {
                render_image_parameters: &[RenderImageParameters {
                    image_format: vk::Format::R32_UINT,
                    image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                    image_clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            uint32: [!0, !0, !0, !0],
                        },
                    },
                }],
                depth_image_parameters: Some(RenderImageParameters {
                    image_format: vk::Format::D32_SFLOAT,
                    image_usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                    image_clear_value: vk::ClearValue::default(),
                }),
                render_pass_parameters: &[
                    RenderPassParameters {
                        flags: vk::SubpassDescriptionFlags::default(),
                        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
                        input_attachments: None,
                        color_attachments: Some(&[vk::AttachmentReference::builder()
                            .attachment(0)
                            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                            .build()]),
                        resolve_attachments: None,
                        depth_stencil_attachment: Some(
                            &vk::AttachmentReference::builder()
                                .attachment(1)
                                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                                .build(),
                        ),
                        preserve_attachments: None,
                    },
                    RenderPassParameters {
                        flags: vk::SubpassDescriptionFlags::default(),
                        pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
                        input_attachments: Some(&[vk::AttachmentReference::builder()
                            .attachment(0)
                            .layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .build()]),
                        color_attachments: None,
                        resolve_attachments: None,
                        depth_stencil_attachment: None,
                        preserve_attachments: None,
                    },
                ],
                render_pass_dependencies: Some(&[
                    vk::SubpassDependency::builder()
                        .src_subpass(vk::SUBPASS_EXTERNAL)
                        .dst_subpass(0)
                        .src_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                        .src_access_mask(vk::AccessFlags::MEMORY_READ)
                        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dependency_flags(vk::DependencyFlags::BY_REGION)
                        .build(),
                    vk::SubpassDependency::builder()
                        .src_subpass(0)
                        .dst_subpass(1)
                        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                        .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                        .dependency_flags(vk::DependencyFlags::BY_REGION)
                        .build(),
                    vk::SubpassDependency::builder()
                        .src_subpass(0)
                        .dst_subpass(vk::SUBPASS_EXTERNAL)
                        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                        .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                        .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                        .dependency_flags(vk::DependencyFlags::BY_REGION)
                        .build(),
                ]),
            },
        );

        Self {
            render_layer,
            extent: vk::Extent3D {
                width,
                height,
                depth: 1,
            },
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.render_layer.destroy(factory);
    }

    pub fn get_render_pass(&self) -> vk::RenderPass {
        self.render_layer.get_render_pass()
    }

    pub fn get_occluder_data_image(&self) -> vk::Image {
        self.render_layer.get_render_image(0).0
    }

    pub fn get_occluder_data_image_view(&self) -> vk::ImageView {
        self.render_layer.get_render_image(0).1
    }

    pub fn try_get_oldest_timestamp(
        &self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
    ) -> Option<[u64; 2]> {
        self.render_layer.try_get_oldest_timestamp(frame_context, factory)
    }

    pub fn get_render_layer(&self) -> &RenderLayer {
        &self.render_layer
    }

    pub fn get_render_layer_mut(&mut self) -> &mut RenderLayer {
        &mut self.render_layer
    }

    pub fn get_extent(&self) -> vk::Extent3D {
        self.extent
    }
}
