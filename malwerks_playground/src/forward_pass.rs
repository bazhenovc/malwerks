// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;
use malwerks_vk::*;

pub struct ForwardPass {
    render_layer: RenderLayer,
    // extent: vk::Extent3D,
}

impl ForwardPass {
    pub fn new(width: u32, height: u32, device: &Device, factory: &mut DeviceFactory) -> Self {
        let render_layer = RenderLayer::new(
            device,
            factory,
            width,
            height,
            &RenderLayerParameters {
                render_image_parameters: &[RenderImageParameters {
                    image_format: vk::Format::B10G11R11_UFLOAT_PACK32,
                    image_usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    image_clear_value: vk::ClearValue::default(),
                }],
                depth_image_parameters: Some(RenderImageParameters {
                    image_format: vk::Format::D32_SFLOAT,
                    image_usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                    image_clear_value: vk::ClearValue::default(),
                }),
                render_pass_parameters: &[RenderPassParameters {
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
                }],
                render_pass_dependencies: None,
            },
        );

        Self {
            render_layer,
            // extent: vk::Extent3D {
            //     width,
            //     height,
            //     depth: 1,
            // },
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.render_layer.destroy(factory);
    }

    // pub fn get_render_pass(&self) -> vk::RenderPass {
    //     self.render_layer.get_render_pass()
    // }

    // pub fn get_depth_image(&self) -> vk::Image {
    //     self.render_layer.get_depth_image().unwrap().0
    // }

    // pub fn get_depth_image_view(&self) -> vk::ImageView {
    //     self.render_layer.get_depth_image().unwrap().1
    // }

    pub fn get_color_image(&self) -> vk::Image {
        self.render_layer.get_render_image(0).0
    }

    // pub fn get_color_image_view(&self) -> vk::ImageView {
    //     self.render_layer.get_render_image(0).1
    // }

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

    // pub fn get_extent(&self) -> vk::Extent3D {
    //     self.extent
    // }
}
