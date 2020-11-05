// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_core::*;
use malwerks_vk::*;

use crate::bundle_loader::*;
use crate::camera::*;
use crate::shared_frame_data::*;
use crate::sky_box::*;
use crate::tone_map::*;

pub struct PbrForwardLitParameters<'a> {
    pub render_width: u32,
    pub render_height: u32,
    pub target_layer: Option<&'a RenderLayer>,
    pub bundle_loader: &'a BundleLoader,
}

pub struct PbrForwardLit {
    render_layer: RenderLayer,
    render_bundles: Vec<(String, ResourceBundleReference, ShaderModuleBundle, PipelineBundle)>,
    pbr_resource_bundle: PbrResourceBundleReference,

    sky_box: SkyBox,
    shared_frame_data: SharedFrameData,

    tone_map: Option<ToneMap>,
}

impl PbrForwardLit {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for (_, _, shader_module_bundle, pipeline_bundle) in &mut self.render_bundles {
            pipeline_bundle.destroy(factory);
            shader_module_bundle.destroy(factory);
        }

        self.render_layer.destroy(factory);
        self.shared_frame_data.destroy(factory);
        self.sky_box.destroy(factory);

        if let Some(tone_map) = &mut self.tone_map {
            tone_map.destroy(factory);
        }
    }

    pub fn new(parameters: &PbrForwardLitParameters, device: &Device, factory: &mut DeviceFactory) -> Self {
        let render_layer = RenderLayer::new(
            device,
            factory,
            parameters.render_width,
            parameters.render_height,
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
        let render_bundles = Vec::new();
        let pbr_resource_bundle = parameters.bundle_loader.get_pbr_resource_bundle();

        let shared_frame_data = SharedFrameData::new(factory);
        let sky_box = SkyBox::from_disk(
            parameters.bundle_loader.get_common_shaders(),
            &pbr_resource_bundle.borrow(),
            &shared_frame_data,
            &render_layer,
            factory,
        );

        let tone_map = if let Some(target_layer) = parameters.target_layer {
            Some(ToneMap::new(
                parameters.bundle_loader.get_common_shaders(),
                &render_layer,
                0,
                target_layer,
                factory,
            ))
        } else {
            None
        };

        Self {
            render_layer,
            render_bundles,
            pbr_resource_bundle,

            shared_frame_data,
            sky_box,
            tone_map,
        }
    }

    pub fn render(
        &mut self,
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

        let forward_color_image = self.render_layer.get_render_image(0);
        self.render_layer.acquire_frame(frame_context, device, factory);
        self.render_layer.begin_command_buffer(frame_context, screen_area);
        {
            let command_buffer = self.render_layer.get_command_buffer(frame_context);
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

            let pbr_resource_bundle = self.pbr_resource_bundle.borrow();
            for (_, resource_bundle, _, pipeline_bundle) in &self.render_bundles {
                let resource_bundle = resource_bundle.borrow();

                let mut render_instance_id = 0;
                for bucket in &resource_bundle.buckets {
                    puffin::profile_scope!("render bucket");

                    let pipeline_layout = pipeline_bundle.pipeline_layouts[bucket.material];
                    let pipeline = pipeline_bundle.pipelines[bucket.material];

                    command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, pipeline);
                    command_buffer.push_constants(
                        pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        self.shared_frame_data.get_view_projection(),
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
                                resource_bundle.descriptor_sets[instance.material_instance],
                                pipeline_bundle.descriptor_sets[render_instance_id],
                                *self.shared_frame_data.get_frame_data_descriptor_set(frame_context),
                                pbr_resource_bundle.descriptor_sets[0],
                            ],
                            &[],
                        );

                        let mesh = &resource_bundle.meshes[instance.mesh];
                        command_buffer.bind_vertex_buffers(0, &[resource_bundle.buffers[mesh.vertex_buffer].0], &[0]);
                        command_buffer.bind_index_buffer(
                            resource_bundle.buffers[mesh.index_buffer.1].0,
                            0,
                            mesh.index_buffer.0,
                        );
                        command_buffer.draw_indexed(mesh.index_count as _, instance.total_instance_count as _, 0, 0, 0);

                        render_instance_id += 1;
                    }
                }
            }

            self.sky_box
                .render(command_buffer, frame_context, &self.shared_frame_data);
            self.render_layer.end_command_buffer(frame_context);

            let command_buffer = self.render_layer.get_command_buffer(frame_context);
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
                    .image(forward_color_image.0)
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

        self.render_layer.submit_commands(frame_context, queue);
    }

    pub fn post_process(&mut self, camera: &Camera, frame_context: &FrameContext, target_layer: &mut RenderLayer) {
        if let Some(tone_map) = &self.tone_map {
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
            tone_map.render(screen_area, frame_context, target_layer);
        }
    }
}

impl PbrForwardLit {
    pub fn add_render_bundle(
        &mut self,
        bundle_name: &str,
        bundle_loader: &mut BundleLoader,
        gltf_file: &std::path::Path,
        bundle_file: &std::path::Path,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        log::info!("adding render bundle \"{}\"", bundle_name);
        let shader_path = bundle_loader
            .get_base_path()
            .join("malwerks_shaders/gltf_pbr_material.glsl");

        let resource_bundle = bundle_loader.request_bundle(gltf_file, bundle_file, device, factory, queue);
        let shader_module_bundle = bundle_loader.compile_shader_module_bundle(
            &resource_bundle,
            &bundle_file.with_extension("pbr_forward_lit"),
            &shader_path,
            factory,
        );
        let pipeline_bundle =
            bundle_loader.create_pipeline_bundle(&resource_bundle, |pbr_resource_bundle, resource_bundle| {
                PipelineBundle::new(
                    &PipelineBundleParameters {
                        resource_bundle,
                        shader_module_bundle: &shader_module_bundle,
                        render_layer: &self.render_layer,
                        descriptor_set_layouts: &[
                            self.shared_frame_data.descriptor_set_layout,
                            pbr_resource_bundle.descriptor_set_layout,
                        ],
                    },
                    factory,
                )
            });

        self.render_bundles.push((
            bundle_name.to_string(),
            resource_bundle,
            shader_module_bundle,
            pipeline_bundle,
        ));
    }

    // TODO: Implement queued remove
    pub fn remove_render_bundle(
        &mut self,
        bundle_name: &str,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        queue.wait_idle();
        device.wait_idle();

        let mut index = 0;
        while index != self.render_bundles.len() {
            if self.render_bundles[index].0 == bundle_name {
                log::info!("removing render bundle \"{}\"", bundle_name);
                let (_, _, mut shader_module_bundle, mut pipeline_bundle) = self.render_bundles.swap_remove(index);

                pipeline_bundle.destroy(factory);
                shader_module_bundle.destroy(factory);
            } else {
                index += 1;
            }
        }
    }

    pub fn get_render_bundles(&self) -> &[(String, ResourceBundleReference, ShaderModuleBundle, PipelineBundle)] {
        &self.render_bundles
    }
}

impl PbrForwardLit {
    pub fn try_get_oldest_timestamps(
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
}
