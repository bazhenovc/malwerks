// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_gltf::*;
use malwerks_render::*;
use malwerks_vk::*;

use crate::camera::*;
use crate::forward_pass::*;
use crate::post_process::*;
use crate::shared_frame_data::*;
use crate::shared_resource_bundle::*;
use crate::sky_box::*;

pub struct GltfImportParameters {
    pub gltf_file: std::path::PathBuf,
    pub gltf_bundle_folder: std::path::PathBuf,
    pub gltf_temp_folder: std::path::PathBuf,
    pub gltf_force_import: bool,
    pub gltf_shaders_folder: std::path::PathBuf,
    pub gltf_bundle_compression_level: u32,
    pub gltf_queue_import: bool,
}

pub struct DemoPbrForwardLit {
    forward_pass: ForwardPass,
    shared_frame_data: SharedFrameData,
    sky_box: SkyBox,
    post_process: PostProcess,

    render_bundle: RenderBundle,
    render_stage_bundle: RenderStageBundle,
    render_state_bundle: RenderStateBundle,
}

impl DemoPbrForwardLit {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.forward_pass.destroy(factory);
        self.shared_frame_data.destroy(factory);
        self.sky_box.destroy(factory);
        self.post_process.destroy(factory);

        self.render_bundle.destroy(factory);
        self.render_stage_bundle.destroy(factory);
        self.render_state_bundle.destroy(factory);
    }

    pub fn new(
        gltf: &GltfImportParameters,
        shared_resources: &DiskSharedResources,
        render_shared_resources: &RenderSharedResources,
        render_size: (u32, u32),
        target_layer: &RenderLayer,
        command_buffer: &mut CommandBuffer,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let forward_pass = ForwardPass::new(render_size.0, render_size.1, device, factory);
        let shared_frame_data = SharedFrameData::new(factory);

        let sky_box = SkyBox::from_disk(
            shared_resources,
            render_shared_resources,
            &shared_frame_data,
            forward_pass.get_render_layer(),
            factory,
        );
        let post_process = PostProcess::new(
            shared_resources,
            forward_pass.get_render_layer(),
            0,
            target_layer,
            factory,
        );

        let (render_bundle, render_stage_bundle, render_state_bundle) = import_bundles(
            gltf,
            &shared_frame_data,
            render_shared_resources,
            forward_pass.get_render_layer(),
            command_buffer,
            device,
            factory,
            queue,
        );

        Self {
            forward_pass,
            shared_frame_data,
            sky_box,
            post_process,
            render_bundle,
            render_stage_bundle,
            render_state_bundle,
        }
    }

    pub fn import_bundles(
        &mut self,
        gltf: &GltfImportParameters,
        render_shared_resources: &RenderSharedResources,
        command_buffer: &mut CommandBuffer,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        queue.wait_idle();
        device.wait_idle();

        self.render_bundle.destroy(factory);
        self.render_stage_bundle.destroy(factory);
        self.render_state_bundle.destroy(factory);

        let (render_bundle, render_stage_bundle, render_state_bundle) = import_bundles(
            gltf,
            &self.shared_frame_data,
            render_shared_resources,
            self.forward_pass.get_render_layer(),
            command_buffer,
            device,
            factory,
            queue,
        );

        self.render_bundle = render_bundle;
        self.render_stage_bundle = render_stage_bundle;
        self.render_state_bundle = render_state_bundle;
    }

    pub fn render(
        &mut self,
        render_shared_resources: &RenderSharedResources,
        camera: &Camera,
        frame_context: &FrameContext,
        device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        puffin::profile_function!();

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

        let forward_color_image = self.forward_pass.get_color_image();
        let forward_layer = self.forward_pass.get_render_layer_mut();

        forward_layer.acquire_frame(frame_context, device, factory);
        forward_layer.begin_command_buffer(frame_context, screen_area);
        {
            let command_buffer = forward_layer.get_command_buffer(frame_context);
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

            render_bundle(
                render_shared_resources,
                &self.render_bundle,
                &self.render_state_bundle,
                command_buffer,
                frame_context,
                &self.shared_frame_data,
            );
            self.sky_box
                .render(command_buffer, frame_context, &self.shared_frame_data);
            forward_layer.end_command_buffer(frame_context);

            let command_buffer = forward_layer.get_command_buffer(frame_context);
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
                    .image(forward_color_image)
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
        }

        forward_layer.submit_commands(frame_context, queue);
    }

    pub fn post_process(&mut self, camera: &Camera, frame_context: &FrameContext, target_layer: &mut RenderLayer) {
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
        self.post_process.render(screen_area, frame_context, target_layer);
    }

    pub fn get_final_layer(&self) -> &RenderLayer {
        self.forward_pass.get_render_layer()
    }

    pub fn try_get_oldest_timestamps(
        &self,
        frame_context: &FrameContext,
        factory: &mut DeviceFactory,
    ) -> [(&'static str, [u64; 2]); 1] {
        let mut timestamps = [("ForwardPass", [0u64; 2])];
        if let Some(timestamp) = self.forward_pass.try_get_oldest_timestamp(frame_context, factory) {
            timestamps[0].1 = timestamp;
        }
        timestamps
    }
}

fn render_bundle(
    render_shared_resources: &RenderSharedResources,
    render_bundle: &RenderBundle,
    render_state_bundle: &RenderStateBundle,
    command_buffer: &mut CommandBuffer,
    frame_context: &FrameContext,
    shared_frame_data: &SharedFrameData,
) {
    puffin::profile_function!();

    let mut render_instance_id = 0;
    for bucket in &render_bundle.buckets {
        puffin::profile_scope!("render bucket");

        let pipeline_layout = render_state_bundle.pipeline_layouts[bucket.material];
        let pipeline = render_state_bundle.pipeline_states[bucket.material];

        command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, pipeline);
        command_buffer.push_constants(
            pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            shared_frame_data.get_view_projection(),
        );

        for instance in &bucket.instances {
            command_buffer.push_constants(
                pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                64,
                &instance.material_instance_data,
            );
            command_buffer.bind_descriptor_sets(
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                0,
                &[
                    render_bundle.descriptor_sets[instance.material_instance],
                    render_state_bundle.descriptor_sets[render_instance_id],
                    *shared_frame_data.get_frame_data_descriptor_set(frame_context),
                    render_shared_resources.descriptor_sets[0],
                ],
                &[],
            );

            let mesh = &render_bundle.meshes[instance.mesh];
            command_buffer.bind_vertex_buffers(0, &[render_bundle.buffers[mesh.vertex_buffer].0], &[0]);
            command_buffer.bind_index_buffer(render_bundle.buffers[mesh.index_buffer.1].0, 0, mesh.index_buffer.0);
            command_buffer.draw_indexed(mesh.index_count as _, instance.total_instance_count as _, 0, 0, 0);

            render_instance_id += 1;
        }
    }
}

fn import_bundles(
    gltf: &GltfImportParameters,
    shared_frame_data: &SharedFrameData,
    render_shared_resources: &RenderSharedResources,
    target_layer: &RenderLayer,
    command_buffer: &mut CommandBuffer,
    _device: &Device,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> (RenderBundle, RenderStageBundle, RenderStateBundle) {
    let temp_folder = gltf.gltf_temp_folder.join(&gltf.gltf_file.file_name().unwrap());

    let render_bundle_file = gltf
        .gltf_bundle_folder
        .join(gltf.gltf_file.with_extension("render_bundle").file_name().unwrap());
    let render_stage_bundle_file = gltf.gltf_bundle_folder.join(
        gltf.gltf_file
            .with_extension("render_stage_bundle")
            .file_name()
            .unwrap(),
    );

    let disk_render_bundle = if gltf.gltf_force_import || !render_bundle_file.exists() {
        let bundle = import_gltf_bundle(&gltf.gltf_file, &temp_folder);

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(render_bundle_file)
            .expect("failed to open bundle file for writing");
        bundle
            .serialize_into(file, gltf.gltf_bundle_compression_level)
            .expect("failed to serialize render bundle");
        bundle
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(render_bundle_file)
            .expect("failed to open bundle file for reading");
        DiskRenderBundle::deserialize_from(file).expect("failed to deserialize render bundle")
    };

    let disk_render_stage_bundle = if gltf.gltf_force_import || !render_stage_bundle_file.exists() {
        let shader_path = gltf.gltf_shaders_folder.join("gltf_pbr_material.glsl");
        let bundle = compile_gltf_shaders(&disk_render_bundle, &shader_path, &temp_folder);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(render_stage_bundle_file)
            .expect("failed to open stage bundle file for writing");
        bundle
            .serialize_into(file, gltf.gltf_bundle_compression_level)
            .expect("failed to serialize render stage bundle");
        bundle
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(render_stage_bundle_file)
            .expect("failed to open stage bundle file for reading");
        DiskShaderStageBundle::deserialize_from(file).expect("failed to deserialize render stage bundle")
    };

    let render_bundle = RenderBundle::from_disk(&disk_render_bundle, command_buffer, factory, queue);
    let render_stage_bundle = RenderStageBundle::new(&disk_render_stage_bundle, factory);
    let render_state_bundle = RenderStateBundle::new(
        &RenderStateBundleParameters {
            source_bundle: &disk_render_bundle,
            render_bundle: &render_bundle,
            render_stage_bundle: &render_stage_bundle,
            render_layer: target_layer,
            descriptor_set_layouts: &[
                shared_frame_data.descriptor_set_layout,
                render_shared_resources.descriptor_set_layout,
            ],
        },
        factory,
    );

    (render_bundle, render_stage_bundle, render_state_bundle)
}
