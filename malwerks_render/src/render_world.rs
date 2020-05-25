// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::camera::*;
use crate::forward_pass::*;
use crate::render_pass::*;
use crate::shared_frame_data::*;
use crate::sky_box::*;
use crate::static_scenery::*;

pub struct RenderWorld {
    forward_pass: ForwardPass,
    static_scenery: StaticScenery,
    sky_box: SkyBox,

    shared_frame_data: SharedFrameData,
}

impl RenderWorld {
    pub fn from_file(
        world_path: &std::path::Path,
        render_size: (u32, u32),
        command_buffer: &mut CommandBuffer,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        log::info!("loading world: {:?}", &world_path);

        let encoded = {
            use std::io::Read;
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .open(world_path)
                .expect("failed to open world file");

            let mut encoded = Vec::new();
            file.read_to_end(&mut encoded).expect("failed to read world file");
            encoded
        };
        let static_scenery = bincode::deserialize(&encoded).expect("failed to deserialize world file");

        Self::from_disk(&static_scenery, render_size, command_buffer, device, factory, queue)
    }

    pub fn from_disk(
        disk_scenery: &DiskStaticScenery,
        render_size: (u32, u32),
        command_buffer: &mut CommandBuffer,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let forward_pass = ForwardPass::new(render_size.0, render_size.1, device, factory);
        let shared_frame_data = SharedFrameData::new(factory);
        let static_scenery = StaticScenery::from_disk(
            disk_scenery,
            &shared_frame_data,
            &forward_pass,
            command_buffer,
            factory,
            queue,
        );
        let sky_box = SkyBox::from_disk(
            disk_scenery,
            &static_scenery,
            &shared_frame_data,
            &forward_pass,
            factory,
        );
        Self {
            forward_pass,
            static_scenery,
            sky_box,
            shared_frame_data,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.shared_frame_data.destroy(factory);
        self.static_scenery.destroy(factory);
        self.forward_pass.destroy(factory);
    }

    pub fn get_render_pass(&self) -> &ForwardPass {
        &self.forward_pass
    }

    pub fn render(
        &mut self,
        camera: &Camera,
        frame_context: &FrameContext,
        device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        let viewport = camera.get_viewport();
        let screen_area = vk::Rect2D {
            offset: vk::Offset2D {
                x: viewport.x,
                y: viewport.y,
            },
            extent: vk::Extent2D {
                width: viewport.width,
                height: viewport.height,
            },
        };
        self.shared_frame_data.update(frame_context, camera, factory);
        self.forward_pass.begin(frame_context, device, factory, screen_area);

        // let depth_image = self.forward_pass.get_depth_image();
        let color_image = self.forward_pass.get_color_image();
        let command_buffer = self.forward_pass.get_command_buffer(frame_context);
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

        self.static_scenery
            .render(command_buffer, frame_context, &self.shared_frame_data);
        self.sky_box
            .render(command_buffer, frame_context, &self.shared_frame_data);
        self.forward_pass.end(frame_context);

        let command_buffer = self.forward_pass.get_command_buffer(frame_context);
        command_buffer.pipeline_barrier(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            None,
            &[],
            &[],
            &[
                // vk::ImageMemoryBarrier::builder()
                //     .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                //     .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                //     .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                //     .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                //     .src_queue_family_index(!0)
                //     .dst_queue_family_index(!0)
                //     .image(depth_image)
                //     .subresource_range(
                //         vk::ImageSubresourceRange::builder()
                //             .aspect_mask(vk::ImageAspectFlags::DEPTH)
                //             .base_mip_level(0)
                //             .level_count(1)
                //             .base_array_layer(0)
                //             .layer_count(1)
                //             .build(),
                //     )
                //     .build(),
                vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(!0)
                    .dst_queue_family_index(!0)
                    .image(color_image)
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
            ],
        );
        self.forward_pass.submit_commands(frame_context, queue);
    }

    pub fn get_instance_count(&self) -> usize {
        self.static_scenery.get_instance_count()
    }
}

impl RenderWorld {
    pub fn create_instances_nv(
        &self,
        instance_mask: u32,
        shader_binding_table_offset: u32,
        flags: vk::GeometryInstanceFlagsNV,
        acceleration_structure_reference: u64,
    ) -> Vec<vk::AccelerationStructureInstanceNV> {
        self.static_scenery.create_instances_nv(
            instance_mask,
            shader_binding_table_offset,
            flags,
            acceleration_structure_reference,
        )
    }

    pub fn create_aabbs_nv(
        &self,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> (Vec<vk::GeometryNV>, HeapAllocatedResource<vk::Buffer>) {
        self.static_scenery.create_aabbs_nv(command_buffer, factory, queue)
    }

    pub fn create_geometries_nv(&self) -> Vec<vk::GeometryNV> {
        self.static_scenery.create_geometries_nv()
    }
}
