// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::render_pass::*;
use crate::shared_frame_data::*;
use crate::upload_batch::*;

#[derive(Default)]
pub struct StaticScenery {
    buffers: Vec<HeapAllocatedResource<vk::Buffer>>,
    meshes: Vec<RenderMesh>,

    images: Vec<HeapAllocatedResource<vk::Image>>,
    image_views: Vec<vk::ImageView>,
    samplers: Vec<vk::Sampler>,

    shader_modules: Vec<vk::ShaderModule>,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    descriptor_sets: Vec<vk::DescriptorSet>,

    pipeline_cache: vk::PipelineCache,
    pipeline_layouts: Vec<vk::PipelineLayout>,
    pipelines: Vec<vk::Pipeline>,

    buckets: Vec<RenderBucket>,
    environment_probes: RenderEnvironmentProbes,
}

impl StaticScenery {
    pub fn from_disk<T>(
        disk_scenery: &DiskStaticScenery,
        shared_frame_data: &SharedFrameData,
        render_pass: &T,
        command_buffer: &mut CommandBuffer,
        factory: &mut GraphicsFactory,
        queue: &mut DeviceQueue,
    ) -> Self
    where
        T: RenderPass,
    {
        let mut static_scenery = Self::default();
        static_scenery.initialize_buffers(&disk_scenery, factory, command_buffer, queue);
        static_scenery.initialize_images(&disk_scenery, factory, command_buffer, queue);
        static_scenery.initialize_environment_probes(&disk_scenery, factory);
        static_scenery.initialize_descriptor_pool(&disk_scenery, factory);
        static_scenery.initialize_pipelines(&disk_scenery, factory, render_pass, shared_frame_data);
        static_scenery.initialize_buckets(&disk_scenery, factory);
        static_scenery
    }

    pub fn destroy(&mut self, factory: &mut GraphicsFactory) {
        self.environment_probes.destroy(factory);
        for bucket in &self.buckets {
            for instance in &bucket.instances {
                factory.deallocate_buffer(&instance.transform_buffer);
            }
        }
        for buffer in &self.buffers {
            factory.deallocate_buffer(buffer);
        }
        for module in &self.shader_modules {
            factory.destroy_shader_module(*module);
        }
        for sampler in &self.samplers {
            factory.destroy_sampler(*sampler);
        }
        for image_view in &self.image_views {
            factory.destroy_image_view(*image_view);
        }
        for image in &self.images {
            factory.deallocate_image(image);
        }
        for pipeline in &self.pipelines {
            factory.destroy_pipeline(*pipeline);
        }
        for pipeline_layout in &self.pipeline_layouts {
            factory.destroy_pipeline_layout(*pipeline_layout);
        }
        factory.destroy_pipeline_cache(self.pipeline_cache);
        for descriptor_set_layout in &self.descriptor_set_layouts {
            factory.destroy_descriptor_set_layout(*descriptor_set_layout);
        }
        factory.destroy_descriptor_pool(self.descriptor_pool);
    }

    pub fn render(
        &self,
        command_buffer: &mut CommandBuffer,
        frame_context: &FrameContext,
        shared_frame_data: &SharedFrameData,
    ) {
        // TODO: only one environment probe is supported right now
        assert_eq!(self.environment_probes.descriptor_sets.len(), 1);

        for bucket in &self.buckets {
            let pipeline_layout = self.pipeline_layouts[bucket.material];
            let pipeline = self.pipelines[bucket.material];

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
                    &instance.material_data,
                );
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline_layout,
                    0,
                    &[
                        *shared_frame_data.get_frame_data_descriptor_set(frame_context),
                        self.descriptor_sets[instance.material_instance],
                        self.environment_probes.descriptor_sets[0],
                    ],
                    &[],
                );

                let mesh = &self.meshes[instance.mesh];
                let vertex_buffer = mesh.vertex_buffer;
                let index_buffer = mesh.index_buffer;

                command_buffer.bind_vertex_buffers(
                    0,
                    &[self.buffers[vertex_buffer].0, instance.transform_buffer.0],
                    &[0, 0],
                );
                match index_buffer {
                    Some(index_buffer) => {
                        command_buffer.bind_index_buffer(self.buffers[index_buffer.0].0, 0, index_buffer.1);
                        command_buffer.draw_indexed(mesh.draw_count, instance.transform_data.len() as _, 0, 0, 0);
                    }
                    None => {
                        command_buffer.draw(mesh.draw_count, instance.transform_data.len() as _, 0, 0);
                    }
                }
            }
        }
    }
}

struct RenderMesh {
    vertex_buffer: usize,
    index_buffer: Option<(usize, vk::IndexType)>,
    draw_count: u32,
}

struct RenderInstance {
    mesh: usize,
    material_instance: usize,
    material_data: [u8; 64],
    transform_data: Vec<[f32; 16]>,
    transform_buffer: HeapAllocatedResource<vk::Buffer>,
}

struct RenderBucket {
    material: usize,
    instances: Vec<RenderInstance>,
}

#[derive(Default)]
struct RenderEnvironmentProbes {
    //radiance_image: usize,
    //irradiance_image: usize,
    probe_sampler: vk::Sampler,

    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,
}

impl RenderEnvironmentProbes {
    fn destroy(&mut self, factory: &mut GraphicsFactory) {
        factory.destroy_sampler(self.probe_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
    }
}

impl StaticScenery {
    pub fn get_image(&self, id: usize) -> vk::Image {
        self.images[id].0
    }

    pub fn get_image_view(&self, id: usize) -> vk::ImageView {
        self.image_views[id]
    }

    fn initialize_buffers(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        factory: &mut GraphicsFactory,
        command_buffer: &mut CommandBuffer,
        queue: &mut DeviceQueue,
    ) {
        log::info!(
            "initializing {} meshes and {} buffers",
            disk_scenery.meshes.len(),
            disk_scenery.buffers.len()
        );

        self.buffers.reserve_exact(disk_scenery.buffers.len());
        self.meshes.reserve_exact(disk_scenery.meshes.len());

        let mut upload_batch = UploadBatch::new(command_buffer);
        for disk_buffer in &disk_scenery.buffers {
            let buffer = factory.allocate_buffer(
                &vk::BufferCreateInfo::builder()
                    .size(disk_buffer.data.len() as _)
                    .usage(vk::BufferUsageFlags::from_raw(disk_buffer.usage_flags))
                    .build(),
                &vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::GpuOnly,
                    required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    ..Default::default()
                },
            );
            upload_batch.upload_buffer_memory(&buffer, &disk_buffer.data, factory);
            self.buffers.push(buffer);
        }
        upload_batch.flush(factory, queue);

        for disk_mesh in &disk_scenery.meshes {
            let mesh = RenderMesh {
                vertex_buffer: disk_mesh.vertex_buffer,
                index_buffer: match disk_mesh.index_buffer {
                    Some(index_buffer) => Some((index_buffer.0, vk::IndexType::from_raw(index_buffer.1))),
                    None => None,
                },
                draw_count: disk_mesh.draw_count,
            };
            self.meshes.push(mesh);
        }
    }

    fn initialize_images(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        factory: &mut GraphicsFactory,
        command_buffer: &mut CommandBuffer,
        queue: &mut DeviceQueue,
    ) {
        log::info!(
            "initializing {} images and {} samplers",
            disk_scenery.images.len(),
            disk_scenery.samplers.len()
        );

        self.images.reserve_exact(disk_scenery.images.len());
        self.image_views.reserve_exact(disk_scenery.images.len());

        let mut upload_batch = UploadBatch::new(command_buffer);
        for disk_image in &disk_scenery.images {
            let image_view_type = vk::ImageViewType::from_raw(disk_image.view_type);
            let image_flags = match image_view_type {
                vk::ImageViewType::CUBE => vk::ImageCreateFlags::CUBE_COMPATIBLE,
                vk::ImageViewType::CUBE_ARRAY => vk::ImageCreateFlags::CUBE_COMPATIBLE,

                vk::ImageViewType::TYPE_2D_ARRAY => vk::ImageCreateFlags::TYPE_2D_ARRAY_COMPATIBLE,

                _ => vk::ImageCreateFlags::default(),
            };

            let allocated_image = factory.allocate_image(
                &vk::ImageCreateInfo::builder()
                    .flags(image_flags)
                    .image_type(vk::ImageType::from_raw(disk_image.image_type))
                    .format(vk::Format::from_raw(disk_image.format))
                    .extent(vk::Extent3D {
                        width: disk_image.width,
                        height: disk_image.height,
                        depth: disk_image.depth,
                    })
                    .mip_levels(disk_image.mipmap_count as _)
                    .array_layers(disk_image.layer_count as _)
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

            upload_batch.upload_image_memory(
                &allocated_image,
                (disk_image.width, disk_image.height, disk_image.depth),
                (disk_image.block_size, disk_image.mipmap_count, disk_image.layer_count),
                &disk_image.pixels,
                factory,
            );

            self.image_views.push(
                factory.create_image_view(
                    &vk::ImageViewCreateInfo::builder()
                        .image(allocated_image.0)
                        .view_type(image_view_type)
                        .format(vk::Format::from_raw(disk_image.format))
                        .components(vk::ComponentMapping::default())
                        .subresource_range(
                            vk::ImageSubresourceRange::builder()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(disk_image.mipmap_count as _)
                                .base_array_layer(0)
                                .layer_count(disk_image.layer_count as _)
                                .build(),
                        )
                        .build(),
                ),
            );
            self.images.push(allocated_image);
        }
        upload_batch.flush(factory, queue);

        for disk_sampler in &disk_scenery.samplers {
            self.samplers.push(
                factory.create_sampler(
                    &vk::SamplerCreateInfo::builder()
                        .address_mode_u(vk::SamplerAddressMode::from_raw(disk_sampler.address_mode_u))
                        .address_mode_v(vk::SamplerAddressMode::from_raw(disk_sampler.address_mode_v))
                        .address_mode_w(vk::SamplerAddressMode::from_raw(disk_sampler.address_mode_w))
                        .mag_filter(vk::Filter::from_raw(disk_sampler.mag_filter))
                        .min_filter(vk::Filter::from_raw(disk_sampler.min_filter))
                        .mipmap_mode(vk::SamplerMipmapMode::from_raw(disk_sampler.mipmap_mode))
                        .min_lod(0.0)
                        .max_lod(std::f32::MAX)
                        .build(),
                ),
            );
        }
    }

    fn initialize_descriptor_pool(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut GraphicsFactory) {
        // TODO: ensure these never reallocate
        let mut temp_bindings = Vec::with_capacity(5);

        self.descriptor_set_layouts
            .reserve_exact(disk_scenery.material_layouts.len());
        for disk_material_layout in &disk_scenery.material_layouts {
            for binding_id in 0..disk_material_layout.image_count {
                temp_bindings.push(
                    vk::DescriptorSetLayoutBinding::builder()
                        .binding(binding_id as _)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                        .build(),
                );
            }
            let layout = factory.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder()
                    .bindings(&temp_bindings)
                    .build(),
            );
            self.descriptor_set_layouts.push(layout);
            temp_bindings.clear();
        }

        // TODO: ensure these never reallocate
        let mut temp_writes = Vec::with_capacity(disk_scenery.material_instances.len() * 5);
        let mut temp_write_ids = Vec::with_capacity(disk_scenery.material_instances.len() * 5);
        let mut temp_image_infos = Vec::with_capacity(disk_scenery.material_instances.len() * 5);
        let mut temp_per_descriptor_layouts = Vec::with_capacity(disk_scenery.material_instances.len());

        for disk_material_instance in &disk_scenery.material_instances {
            let layout = self.descriptor_set_layouts[disk_material_instance.material_layout];

            let descriptor_id = temp_per_descriptor_layouts.len();
            temp_per_descriptor_layouts.push(layout);

            for (binding_id, image) in disk_material_instance.images.iter().enumerate() {
                let image_info_index = temp_image_infos.len();
                temp_image_infos.push(
                    vk::DescriptorImageInfo::builder()
                        .image_view(self.image_views[image.0])
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .sampler(self.samplers[image.1])
                        .build(),
                );
                temp_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_binding(binding_id as _)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&temp_image_infos[image_info_index..temp_image_infos.len()])
                        .build(),
                );
                temp_write_ids.push(descriptor_id);
            }
        }

        log::info!(
            "allocating {} set layouts, {} descriptors and {} bindings",
            self.descriptor_set_layouts.len(),
            temp_per_descriptor_layouts.len(),
            temp_writes.len()
        );

        self.descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(temp_per_descriptor_layouts.len() as _)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(temp_writes.len() as _)
                    .build()])
                .build(),
        );
        self.descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        for i in 0..temp_writes.len() {
            temp_writes[i].dst_set = self.descriptor_sets[temp_write_ids[i]];
        }
        factory.update_descriptor_sets(&temp_writes, &[]);
    }

    fn initialize_pipelines<T>(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        factory: &mut GraphicsFactory,
        render_pass: &T,
        shared_frame_data: &SharedFrameData,
    ) where
        T: RenderPass,
    {
        self.shader_modules.reserve_exact(disk_scenery.materials.len() * 2);
        self.pipeline_layouts.reserve_exact(disk_scenery.materials.len());
        self.pipelines.reserve_exact(disk_scenery.materials.len());

        self.pipeline_cache = factory.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default());

        // TODO: ensure these never reallocate
        let mut temp_stages = Vec::with_capacity(disk_scenery.materials.len() * 2);
        let mut temp_vertex_bindings = Vec::with_capacity(disk_scenery.materials.len() * 2);
        let mut temp_attributes = Vec::with_capacity(disk_scenery.materials.len() * (5 + 4));
        let mut temp_attachments = Vec::with_capacity(disk_scenery.materials.len() * 2);
        let mut temp_dynamic_state_values = Vec::with_capacity(disk_scenery.materials.len() * 2);

        let mut temp_vertex_input_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_input_assembly_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_tessellation_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_viewport_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_rasterization_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_multisample_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_depth_stencil_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_color_blend_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_dynamic_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_pipelines = Vec::with_capacity(disk_scenery.materials.len());

        let entry_point = std::ffi::CString::new("main").unwrap();
        for disk_material in &disk_scenery.materials {
            let vertex_module = factory.create_shader_module(
                &vk::ShaderModuleCreateInfo::builder()
                    .code(&disk_material.vertex_stage)
                    .build(),
            );
            let fragment_module = factory.create_shader_module(
                &vk::ShaderModuleCreateInfo::builder()
                    .code(&disk_material.fragment_stage)
                    .build(),
            );

            let pipeline_layout = factory.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder()
                    .set_layouts(&[
                        shared_frame_data.get_frame_data_descriptor_set_layout(),
                        self.descriptor_set_layouts[disk_material.material_layout],
                        self.environment_probes.descriptor_set_layout,
                    ])
                    .push_constant_ranges(&[
                        vk::PushConstantRange::builder()
                            .stage_flags(vk::ShaderStageFlags::VERTEX)
                            .offset(0)
                            .size(64)
                            .build(),
                        vk::PushConstantRange::builder()
                            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                            .offset(64)
                            .size(64)
                            .build(),
                    ]),
            );

            let vertex_attributes_start = temp_attributes.len();
            let mut last_attribute_location = 0;
            for format in &disk_material.vertex_format {
                temp_attributes.push(
                    vk::VertexInputAttributeDescription::builder()
                        .location(format.1)
                        .binding(0)
                        .format(vk::Format::from_raw(format.0))
                        .offset(format.2 as _)
                        .build(),
                );
                last_attribute_location = last_attribute_location.max(format.1);
            }
            for matrix_attribute in 0..4 {
                temp_attributes.push(
                    vk::VertexInputAttributeDescription::builder()
                        .location(last_attribute_location + matrix_attribute + 1)
                        .binding(1)
                        .format(vk::Format::R32G32B32A32_SFLOAT)
                        .offset(matrix_attribute * std::mem::size_of::<[f32; 4]>() as u32)
                        .build(),
                );
            }

            let shader_stages_start = temp_stages.len();
            temp_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .name(&entry_point)
                    .module(vertex_module)
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .build(),
            );
            temp_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .name(&entry_point)
                    .module(fragment_module)
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            );

            let vertex_bindings_start = temp_vertex_bindings.len();
            temp_vertex_bindings.push(
                vk::VertexInputBindingDescription::builder()
                    .binding(0)
                    .stride(disk_material.vertex_stride as _)
                    .input_rate(vk::VertexInputRate::VERTEX)
                    .build(),
            );
            temp_vertex_bindings.push(
                vk::VertexInputBindingDescription::builder()
                    .binding(1)
                    .stride(std::mem::size_of::<[f32; 16]>() as _)
                    .input_rate(vk::VertexInputRate::INSTANCE)
                    .build(),
            );

            let states_start = temp_vertex_input_states.len();
            temp_vertex_input_states.push(
                vk::PipelineVertexInputStateCreateInfo::builder()
                    .vertex_binding_descriptions(
                        &temp_vertex_bindings[vertex_bindings_start..temp_vertex_bindings.len()],
                    )
                    .vertex_attribute_descriptions(&temp_attributes[vertex_attributes_start..temp_attributes.len()])
                    .build(),
            );
            temp_input_assembly_states.push(
                vk::PipelineInputAssemblyStateCreateInfo::builder()
                    .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                    .primitive_restart_enable(false)
                    .build(),
            );
            temp_tessellation_states.push(vk::PipelineTessellationStateCreateInfo::default());
            temp_viewport_states.push(
                vk::PipelineViewportStateCreateInfo::builder()
                    .viewport_count(1)
                    .scissor_count(1)
                    .build(),
            );
            temp_rasterization_states.push(
                vk::PipelineRasterizationStateCreateInfo::builder()
                    .line_width(1.0)
                    .cull_mode(vk::CullModeFlags::BACK)
                    //.rasterizer_discard_enable(disk_material.fragment_alpha_discard)
                    .build(),
            );
            temp_multisample_states.push(
                vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                    .build(),
            );
            temp_depth_stencil_states.push(
                vk::PipelineDepthStencilStateCreateInfo::builder()
                    .flags(Default::default())
                    .depth_test_enable(true)
                    .depth_write_enable(true)
                    .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
                    .stencil_test_enable(false)
                    .build(),
            );

            let attachments_start = temp_attachments.len();
            temp_attachments.push(
                vk::PipelineColorBlendAttachmentState::builder()
                    .blend_enable(false)
                    .color_write_mask(
                        vk::ColorComponentFlags::R
                            | vk::ColorComponentFlags::G
                            | vk::ColorComponentFlags::B
                            | vk::ColorComponentFlags::A,
                    )
                    .build(),
            );
            temp_color_blend_states.push(
                vk::PipelineColorBlendStateCreateInfo::builder()
                    .attachments(&temp_attachments[attachments_start..temp_attachments.len()])
                    .build(),
            );

            let dynamic_states_start = temp_dynamic_state_values.len();
            temp_dynamic_state_values.push(vk::DynamicState::VIEWPORT);
            temp_dynamic_state_values.push(vk::DynamicState::SCISSOR);
            temp_dynamic_states.push(
                vk::PipelineDynamicStateCreateInfo::builder()
                    .dynamic_states(&temp_dynamic_state_values[dynamic_states_start..temp_dynamic_state_values.len()])
                    .build(),
            );

            let pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
                .stages(&temp_stages[shader_stages_start..temp_stages.len()])
                .vertex_input_state(&temp_vertex_input_states[states_start])
                .input_assembly_state(&temp_input_assembly_states[states_start])
                .tessellation_state(&temp_tessellation_states[states_start])
                .viewport_state(&temp_viewport_states[states_start])
                .rasterization_state(&temp_rasterization_states[states_start])
                .multisample_state(&temp_multisample_states[states_start])
                .depth_stencil_state(&temp_depth_stencil_states[states_start])
                .color_blend_state(&temp_color_blend_states[states_start])
                .dynamic_state(&temp_dynamic_states[states_start])
                .layout(pipeline_layout)
                .render_pass(render_pass.get_render_pass())
                .subpass(0)
                .base_pipeline_handle(vk::Pipeline::null())
                .base_pipeline_index(0)
                .build();

            self.shader_modules.push(vertex_module);
            self.shader_modules.push(fragment_module);
            self.pipeline_layouts.push(pipeline_layout);
            temp_pipelines.push(pipeline_create_info);
        }

        log::info!("allocating {} graphics pipelines", temp_pipelines.len());
        self.pipelines = factory.create_graphics_pipelines(self.pipeline_cache, &temp_pipelines);
    }

    fn initialize_buckets(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut GraphicsFactory) {
        for disk_bucket in &disk_scenery.buckets {
            let material = disk_bucket.material;
            let mut instances = Vec::new();
            for disk_instance in &disk_bucket.instances {
                let mesh = disk_instance.mesh;
                let material_instance = disk_instance.material_instance;

                let transform_data = disk_instance.transforms.clone();
                let transform_buffer = factory.allocate_buffer(
                    &vk::BufferCreateInfo::builder()
                        .size((transform_data.len() * std::mem::size_of::<f32>() * 16) as _)
                        .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
                        .build(),
                    &vk_mem::AllocationCreateInfo {
                        usage: vk_mem::MemoryUsage::CpuToGpu,
                        ..Default::default()
                    },
                );

                let transform_memory = factory.map_allocation_memory(&transform_buffer);
                copy_to_mapped_memory(&transform_data, transform_memory);
                factory.unmap_allocation_memory(&transform_buffer);

                let mut material_data = [0u8; 64];
                {
                    let disk_data = &disk_scenery.material_instances[material_instance].material_data;
                    assert_eq!(disk_data.len(), 64);

                    material_data.copy_from_slice(disk_data);
                }

                instances.push(RenderInstance {
                    mesh,
                    material_instance,
                    material_data,
                    transform_data,
                    transform_buffer,
                });
            }

            self.buckets.push(RenderBucket { material, instances });
        }
    }

    fn initialize_environment_probes(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut GraphicsFactory) {
        let probe_sampler = factory.create_sampler(
            &vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .min_lod(0.0)
                .max_lod(std::f32::MAX)
                .build(),
        );

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets((disk_scenery.environment_probes.len() * 2) as _)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(3)
                    .build()])
                .build(),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&[
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ]),
        );

        let temp_per_descriptor_layouts: Vec<vk::DescriptorSetLayout> = (0..disk_scenery.environment_probes.len())
            .map(|_| descriptor_set_layout)
            .collect();
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        // TODO: ensure these never reallocate
        let mut temp_writes = Vec::with_capacity(descriptor_sets.len() * 3);
        let mut temp_image_infos = Vec::with_capacity(descriptor_sets.len() * 3);

        for (probe_id, disk_probe) in disk_scenery.environment_probes.iter().enumerate() {
            macro_rules! write_image {
                ($temp_image_infos: ident, $temp_writes: ident, $binding: expr, $probe_id: expr, $image: expr) => {{
                    let image_info_index = $temp_image_infos.len();
                    $temp_image_infos.push(
                        vk::DescriptorImageInfo::builder()
                            .image_view(self.image_views[$image])
                            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .sampler(probe_sampler)
                            .build(),
                    );
                    $temp_writes.push(
                        vk::WriteDescriptorSet::builder()
                            .dst_binding($binding)
                            .dst_set(descriptor_sets[$probe_id])
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .image_info(&temp_image_infos[image_info_index..temp_image_infos.len()])
                            .build(),
                    );
                }};
            }

            write_image!(temp_image_infos, temp_writes, 0, probe_id, disk_probe.iem_image);
            write_image!(temp_image_infos, temp_writes, 1, probe_id, disk_probe.pmrem_image);
            write_image!(
                temp_image_infos,
                temp_writes,
                2,
                probe_id,
                disk_probe.precomputed_brdf_image
            );
        }

        factory.update_descriptor_sets(&temp_writes, &[]);

        self.environment_probes = RenderEnvironmentProbes {
            probe_sampler,

            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
        };
    }
}
