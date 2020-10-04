// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::mesh_cluster_culling::*;
use crate::occluder_pass::*;
use crate::occluder_resolve::*;
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

    occluder_pipeline_layouts: Vec<vk::PipelineLayout>,
    occluder_pipelines: Vec<vk::Pipeline>,

    forward_pipeline_layouts: Vec<vk::PipelineLayout>,
    forward_pipelines: Vec<vk::Pipeline>,

    buckets: Vec<RenderBucket>,
    environment_probes: RenderEnvironmentProbes,

    render_transforms: RenderTransforms,
    cluster_culling: MeshClusterCulling,
    occluder_resolve: OccluderResolve,

    runtime_buffers: Vec<HeapAllocatedResource<vk::Buffer>>,

    debug_apex_culling_disabled: bool,
    debug_apex_culling_paused: bool,

    debug_occlusion_culling_disabled: bool,
    debug_occlusion_culling_paused: bool,
}

impl StaticScenery {
    pub fn from_disk<FT>(
        disk_scenery: &DiskStaticScenery,
        shared_frame_data: &SharedFrameData,
        occluder_pass: &OccluderPass,
        forward_pass: &FT,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self
    where
        FT: RenderPass,
    {
        let mut static_scenery = Self::default();
        static_scenery.initialize_buffers(&disk_scenery, command_buffer, factory, queue);
        static_scenery.initialize_images(&disk_scenery, command_buffer, factory, queue);
        static_scenery.initialize_environment_probes(&disk_scenery, factory);
        static_scenery.initialize_descriptor_pool(&disk_scenery, factory);
        static_scenery.initialize_buckets(&disk_scenery, factory);
        static_scenery.initialize_pipelines(&disk_scenery, factory, occluder_pass, forward_pass, shared_frame_data);
        static_scenery.initialize_mesh_cluster_culling(&disk_scenery, occluder_pass, factory);
        static_scenery
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.render_transforms.destroy(factory);
        self.occluder_resolve.destroy(factory);
        self.cluster_culling.destroy(factory);
        self.environment_probes.destroy(factory);
        for bucket in &mut self.buckets {
            bucket.destroy(factory);
        }
        for buffer in &self.buffers {
            factory.deallocate_buffer(buffer);
        }
        for buffer in &self.runtime_buffers {
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
        for occluder_pipeline in &self.occluder_pipelines {
            factory.destroy_pipeline(*occluder_pipeline);
        }
        for forward_pipeline in &self.forward_pipelines {
            factory.destroy_pipeline(*forward_pipeline);
        }
        for occluder_pipeline_layout in &self.occluder_pipeline_layouts {
            factory.destroy_pipeline_layout(*occluder_pipeline_layout);
        }
        for forward_pipeline_layout in &self.forward_pipeline_layouts {
            factory.destroy_pipeline_layout(*forward_pipeline_layout);
        }
        factory.destroy_pipeline_cache(self.pipeline_cache);
        for descriptor_set_layout in &self.descriptor_set_layouts {
            factory.destroy_descriptor_set_layout(*descriptor_set_layout);
        }
        factory.destroy_descriptor_pool(self.descriptor_pool);
    }

    pub fn dispatch_apex_culling(
        &self,
        command_buffer: &mut CommandBuffer,
        _frame_context: &FrameContext,
        shared_frame_data: &SharedFrameData,
    ) {
        if !self.debug_occlusion_culling_disabled && !self.debug_apex_culling_paused {
            puffin::profile_function!();

            self.cluster_culling
                .dispatch_apex_culling(command_buffer, shared_frame_data);
            self.cluster_culling
                .dispatch_count_to_occlusion_culling_arguments(command_buffer, shared_frame_data);
        }
    }

    pub fn dispatch_occlusion_culling(
        &self,
        command_buffer: &mut CommandBuffer,
        _frame_context: &FrameContext,
        shared_frame_data: &SharedFrameData,
    ) {
        if !self.debug_occlusion_culling_disabled && !self.debug_occlusion_culling_paused {
            puffin::profile_function!();

            command_buffer.pipeline_barrier(
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                None,
                &[],
                &[vk::BufferMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                    .src_queue_family_index(!0)
                    .dst_queue_family_index(!0)
                    .buffer(self.get_visibility_buffer())
                    .offset(0)
                    .size(vk::WHOLE_SIZE)
                    .build()],
                &[],
            );
            self.cluster_culling
                .dispatch_occlusion_culling(command_buffer, shared_frame_data);
        }
    }

    pub fn render_occluder(
        &self,
        command_buffer: &mut CommandBuffer,
        frame_context: &FrameContext,
        shared_frame_data: &SharedFrameData,
    ) {
        puffin::profile_function!();

        if !self.debug_occlusion_culling_disabled && !self.debug_occlusion_culling_paused {
            // TODO: only one environment probe is supported right now
            assert_eq!(self.environment_probes.descriptor_sets.len(), 1);

            let mut draw_index = 0usize;
            let mut instance_index = 0usize;
            for bucket in &self.buckets {
                let occluder_pipeline_layout = self.occluder_pipeline_layouts[bucket.material];

                command_buffer.bind_pipeline(
                    vk::PipelineBindPoint::GRAPHICS,
                    self.occluder_pipelines[bucket.material],
                );
                command_buffer.push_constants(
                    occluder_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    shared_frame_data.get_view_projection(),
                );

                for instance in &bucket.instances {
                    let occluder_arguments_buffer = if self.debug_apex_culling_disabled {
                        self.buffers[instance.occluder_arguments_buffer].0
                    } else {
                        instance.get_runtime_occluder_arguments_buffer()
                    };

                    let mesh = &self.meshes[instance.mesh];
                    let draw_count = mesh.mesh_cluster_count * instance.total_instance_count;

                    command_buffer.push_constants(
                        occluder_pipeline_layout,
                        vk::ShaderStageFlags::VERTEX,
                        64,
                        &[draw_index as u32, 0, 0, 0],
                    );
                    command_buffer.bind_descriptor_sets(
                        vk::PipelineBindPoint::GRAPHICS,
                        occluder_pipeline_layout,
                        0,
                        &[self.render_transforms.descriptor_sets[instance_index]],
                        &[],
                    );
                    command_buffer.bind_vertex_buffers(0, &[self.buffers[mesh.vertex_buffer].0], &[0]);
                    command_buffer.bind_index_buffer(
                        self.buffers[mesh.occluder_index_buffer].0,
                        0,
                        vk::IndexType::UINT16,
                    );

                    if self.debug_apex_culling_disabled {
                        command_buffer.draw_indexed_indirect(
                            occluder_arguments_buffer,
                            0,
                            instance.total_draw_count as _,
                            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as _,
                        );
                    } else {
                        command_buffer.draw_indexed_indirect_count(
                            occluder_arguments_buffer,
                            0,
                            instance.get_runtime_count_buffer(),
                            0,
                            instance.total_draw_count as _,
                            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as _,
                        );
                    }

                    draw_index += draw_count;
                    instance_index += 1;
                }
            }
        }

        command_buffer.next_subpass(vk::SubpassContents::INLINE);
        if !self.debug_occlusion_culling_disabled && !self.debug_occlusion_culling_paused {
            self.occluder_resolve.render(command_buffer, frame_context);
        }
    }

    pub fn render_forward(
        &self,
        command_buffer: &mut CommandBuffer,
        frame_context: &FrameContext,
        shared_frame_data: &SharedFrameData,
    ) {
        puffin::profile_function!();

        // TODO: only one environment probe is supported right now
        assert_eq!(self.environment_probes.descriptor_sets.len(), 1);

        let mut instance_index = 0usize;
        for bucket in &self.buckets {
            let forward_pipeline_layout = self.forward_pipeline_layouts[bucket.material];

            command_buffer.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, self.forward_pipelines[bucket.material]);
            command_buffer.push_constants(
                forward_pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                shared_frame_data.get_view_projection(),
            );

            for instance in &bucket.instances {
                #[allow(clippy::collapsible_if)] // This piece of code is much less readable when collapsed
                let (draw_arguments_buffer, count_offset) = if self.debug_occlusion_culling_disabled {
                    if self.debug_apex_culling_disabled {
                        (self.buffers[instance.draw_arguments_buffer].0, 0)
                    } else {
                        (instance.get_runtime_temp_draw_arguments_buffer(), 0)
                    }
                } else {
                    if self.debug_apex_culling_disabled {
                        (instance.get_runtime_temp_draw_arguments_buffer(), 0)
                    } else {
                        (instance.get_runtime_draw_arguments_buffer(), std::mem::size_of::<u32>())
                    }
                };

                command_buffer.push_constants(
                    forward_pipeline_layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    64,
                    &instance.material_data,
                );
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::GRAPHICS,
                    forward_pipeline_layout,
                    0,
                    &[
                        *shared_frame_data.get_frame_data_descriptor_set(frame_context),
                        self.render_transforms.descriptor_sets[instance_index],
                        self.descriptor_sets[instance.material_instance],
                        self.environment_probes.descriptor_sets[0],
                    ],
                    &[],
                );

                let mesh = &self.meshes[instance.mesh];
                command_buffer.bind_vertex_buffers(0, &[self.buffers[mesh.vertex_buffer].0], &[0]);
                command_buffer.bind_index_buffer(self.buffers[mesh.draw_index_buffer].0, 0, vk::IndexType::UINT16);
                if self.debug_occlusion_culling_disabled {
                    command_buffer.draw_indexed_indirect(
                        draw_arguments_buffer,
                        0,
                        (mesh.mesh_cluster_count * instance.total_instance_count) as _,
                        std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as _,
                    );
                } else {
                    command_buffer.draw_indexed_indirect_count(
                        draw_arguments_buffer,
                        0,
                        instance.get_runtime_count_buffer(),
                        count_offset as _,
                        (mesh.mesh_cluster_count * instance.total_instance_count) as _,
                        std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as _,
                    );
                }

                instance_index += 1;
            }
        }
    }

    pub fn get_visibility_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[0].0
    }

    pub fn get_image(&self, id: usize) -> vk::Image {
        self.images[id].0
    }

    pub fn get_image_view(&self, id: usize) -> vk::ImageView {
        self.image_views[id]
    }

    pub fn debug_set_apex_culling_enabled(&mut self, enabled: bool) {
        self.debug_apex_culling_disabled = !enabled;
        // self.cluster_culling.debug_set_apex_culling_enabled(enabled);
    }

    pub fn debug_set_apex_culling_paused(&mut self, paused: bool) {
        self.debug_apex_culling_paused = paused;
    }

    pub fn debug_set_occlusion_culling_enabled(&mut self, enabled: bool) {
        self.debug_occlusion_culling_disabled = !enabled;
        // self.cluster_culling.debug_set_occlusion_culling_enabled(enabled);
    }

    pub fn debug_set_occlusion_culling_paused(&mut self, paused: bool) {
        self.debug_occlusion_culling_paused = paused;
    }
}

struct RenderMesh {
    vertex_buffer: usize,
    draw_index_buffer: usize,
    occluder_index_buffer: usize,
    mesh_cluster_count: usize,
}

struct RenderInstance {
    mesh: usize,
    material_instance: usize,
    material_data: [u8; 64],

    bounding_cone_buffer: usize,
    occluder_arguments_buffer: usize,
    draw_arguments_buffer: usize,

    total_instance_count: usize,
    total_draw_count: usize,

    runtime_buffers: Vec<HeapAllocatedResource<vk::Buffer>>,
}

impl RenderInstance {
    pub fn get_runtime_occluder_arguments_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[0].0
    }

    pub fn get_runtime_temp_draw_arguments_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[1].0
    }

    pub fn get_runtime_draw_arguments_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[2].0
    }

    pub fn get_runtime_count_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[3].0
    }

    pub fn get_runtime_occlusion_culling_arguments_buffer(&self) -> vk::Buffer {
        self.runtime_buffers[4].0
    }
}

struct RenderBucket {
    material: usize,
    instances: Vec<RenderInstance>,
    instance_transform_buffer: usize,
}

impl RenderBucket {
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        for instance in &self.instances {
            for buffer in &instance.runtime_buffers {
                factory.deallocate_buffer(buffer);
            }
        }
    }
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
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_sampler(self.probe_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
    }
}

#[derive(Default)]
struct RenderTransforms {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,
}

impl RenderTransforms {
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
    }
}

impl StaticScenery {
    fn initialize_buffers(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
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
                    .usage(vk::BufferUsageFlags::from_raw(disk_buffer.usage_flags) | vk::BufferUsageFlags::TRANSFER_DST)
                    .build(),
                &vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::GpuOnly,
                    required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    ..Default::default()
                },
            );
            upload_batch.upload_buffer_memory(
                vk::PipelineStageFlags::ALL_COMMANDS,
                &buffer,
                &disk_buffer.data,
                0,
                factory,
            );
            self.buffers.push(buffer);
        }
        upload_batch.flush(factory, queue);

        for disk_mesh in &disk_scenery.meshes {
            let mesh = RenderMesh {
                vertex_buffer: disk_mesh.vertex_buffer,
                draw_index_buffer: disk_mesh.draw_index_buffer,
                occluder_index_buffer: disk_mesh.occluder_index_buffer,
                mesh_cluster_count: disk_mesh.mesh_clusters.len(),
            };
            self.meshes.push(mesh);
        }
    }

    fn initialize_images(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
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

    fn initialize_descriptor_pool(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut DeviceFactory) {
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

    fn initialize_pipelines<FT>(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        factory: &mut DeviceFactory,
        occluder_pass: &OccluderPass,
        forward_pass: &FT,
        shared_frame_data: &SharedFrameData,
    ) where
        FT: RenderPass,
    {
        self.shader_modules.reserve_exact(disk_scenery.materials.len() * 2);
        self.forward_pipeline_layouts
            .reserve_exact(disk_scenery.materials.len());
        self.forward_pipelines.reserve_exact(disk_scenery.materials.len());

        self.pipeline_cache = factory.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default());

        // TODO: ensure these never reallocate
        let mut temp_forward_stages = Vec::with_capacity(disk_scenery.materials.len() * 2);
        let mut temp_vertex_bindings = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_attributes = Vec::with_capacity(disk_scenery.materials.len() * 5);
        let mut temp_attachments = Vec::with_capacity(disk_scenery.materials.len() * 2);
        let mut temp_dynamic_state_values = Vec::with_capacity(disk_scenery.materials.len() * 2);

        let mut temp_occluder_vertex_input_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_forward_vertex_input_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_input_assembly_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_tessellation_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_viewport_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_forward_rasterization_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_multisample_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_depth_stencil_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_color_blend_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_dynamic_states = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_occluder_pipelines = Vec::with_capacity(disk_scenery.materials.len());
        let mut temp_forward_pipelines = Vec::with_capacity(disk_scenery.materials.len());

        let occluder_vertex_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.occluder_material_vertex_stage)
                .build(),
        );
        let occluder_fragment_module = factory.create_shader_module(
            &vk::ShaderModuleCreateInfo::builder()
                .code(&disk_scenery.global_resources.occluder_material_fragment_stage)
                .build(),
        );
        self.shader_modules.push(occluder_vertex_module);
        self.shader_modules.push(occluder_fragment_module);

        let entry_point = std::ffi::CString::new("main").unwrap();
        let temp_occluder_stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .name(&entry_point)
                .module(occluder_vertex_module)
                .stage(vk::ShaderStageFlags::VERTEX)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .name(&entry_point)
                .module(occluder_fragment_module)
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];

        let temp_occluder_rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .build();

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

            let temp_occluder_set_layouts = [self.render_transforms.descriptor_set_layout];
            let temp_forward_set_layouts = [
                shared_frame_data.get_frame_data_descriptor_set_layout(),
                self.render_transforms.descriptor_set_layout,
                self.descriptor_set_layouts[disk_material.material_layout],
                self.environment_probes.descriptor_set_layout,
            ];

            let temp_occluder_push_constant_ranges = [vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::VERTEX)
                .offset(0)
                .size(64 + 16)
                .build()];
            let temp_forward_push_constant_ranges = [
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
            ];

            let occluder_pipeline_layout = factory.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder()
                    .set_layouts(&temp_occluder_set_layouts)
                    .push_constant_ranges(&temp_occluder_push_constant_ranges),
            );
            let forward_pipeline_layout = factory.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder()
                    .set_layouts(&temp_forward_set_layouts)
                    .push_constant_ranges(&temp_forward_push_constant_ranges),
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
            // for matrix_attribute in 0..4 {
            //     temp_attributes.push(
            //         vk::VertexInputAttributeDescription::builder()
            //             .location(last_attribute_location + matrix_attribute + 1)
            //             .binding(1)
            //             .format(vk::Format::R32G32B32A32_SFLOAT)
            //             .offset(matrix_attribute * std::mem::size_of::<[f32; 4]>() as u32)
            //             .build(),
            //     );
            // }

            let shader_stages_start = temp_forward_stages.len();
            temp_forward_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .name(&entry_point)
                    .module(vertex_module)
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .build(),
            );
            temp_forward_stages.push(
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
            // temp_vertex_bindings.push(
            //     vk::VertexInputBindingDescription::builder()
            //         .binding(1)
            //         .stride(std::mem::size_of::<[f32; 16]>() as _)
            //         .input_rate(vk::VertexInputRate::INSTANCE)
            //         .build(),
            // );

            let states_start = temp_forward_vertex_input_states.len();
            temp_occluder_vertex_input_states.push(
                vk::PipelineVertexInputStateCreateInfo::builder()
                    .vertex_binding_descriptions(
                        &temp_vertex_bindings[vertex_bindings_start..temp_vertex_bindings.len()],
                    )
                    .vertex_attribute_descriptions(
                        &temp_attributes[vertex_attributes_start..vertex_attributes_start + 1],
                    )
                    .build(),
            );
            temp_forward_vertex_input_states.push(
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
            temp_forward_rasterization_states.push(
                vk::PipelineRasterizationStateCreateInfo::builder()
                    .line_width(1.0)
                    .cull_mode(vk::CullModeFlags::from_raw(disk_material.fragment_cull_flags))
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

            let occluder_pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
                .stages(&temp_occluder_stages)
                .vertex_input_state(&temp_occluder_vertex_input_states[states_start])
                .input_assembly_state(&temp_input_assembly_states[states_start])
                .tessellation_state(&temp_tessellation_states[states_start])
                .viewport_state(&temp_viewport_states[states_start])
                .rasterization_state(&temp_occluder_rasterization_state)
                .multisample_state(&temp_multisample_states[states_start])
                .depth_stencil_state(&temp_depth_stencil_states[states_start])
                .color_blend_state(&temp_color_blend_states[states_start])
                .dynamic_state(&temp_dynamic_states[states_start])
                .layout(occluder_pipeline_layout)
                .render_pass(occluder_pass.get_render_pass())
                .subpass(0)
                .base_pipeline_handle(vk::Pipeline::null())
                .base_pipeline_index(0)
                .build();

            let forward_pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
                .stages(&temp_forward_stages[shader_stages_start..temp_forward_stages.len()])
                .vertex_input_state(&temp_forward_vertex_input_states[states_start])
                .input_assembly_state(&temp_input_assembly_states[states_start])
                .tessellation_state(&temp_tessellation_states[states_start])
                .viewport_state(&temp_viewport_states[states_start])
                .rasterization_state(&temp_forward_rasterization_states[states_start])
                .multisample_state(&temp_multisample_states[states_start])
                .depth_stencil_state(&temp_depth_stencil_states[states_start])
                .color_blend_state(&temp_color_blend_states[states_start])
                .dynamic_state(&temp_dynamic_states[states_start])
                .layout(forward_pipeline_layout)
                .render_pass(forward_pass.get_render_pass())
                .subpass(0)
                .base_pipeline_handle(vk::Pipeline::null())
                .base_pipeline_index(0)
                .build();

            self.shader_modules.push(vertex_module);
            self.shader_modules.push(fragment_module);

            self.occluder_pipeline_layouts.push(occluder_pipeline_layout);
            temp_occluder_pipelines.push(occluder_pipeline_create_info);

            self.forward_pipeline_layouts.push(forward_pipeline_layout);
            temp_forward_pipelines.push(forward_pipeline_create_info);
        }

        log::info!("allocating {} occluder pipelines", temp_occluder_pipelines.len());
        self.occluder_pipelines = factory.create_graphics_pipelines(self.pipeline_cache, &temp_occluder_pipelines);

        log::info!("allocating {} forward pipelines", temp_forward_pipelines.len());
        self.forward_pipelines = factory.create_graphics_pipelines(self.pipeline_cache, &temp_forward_pipelines);
    }

    fn initialize_buckets(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut DeviceFactory) {
        log::info!("initializing {} buckets", disk_scenery.buckets.len());

        let mut render_instance_count = 0;
        let mut total_draw_count = 0;

        for disk_bucket in &disk_scenery.buckets {
            let material = disk_bucket.material;
            let mut instances = Vec::new();
            for disk_instance in &disk_bucket.instances {
                let mesh = disk_instance.mesh;
                let material_instance = disk_instance.material_instance;

                let mut material_data = [0u8; 64];
                {
                    let disk_data = &disk_scenery.material_instances[material_instance].material_data;
                    assert_eq!(disk_data.len(), 64);

                    material_data.copy_from_slice(disk_data);
                }

                let runtime_buffers = vec![
                    factory.allocate_buffer(
                        &vk::BufferCreateInfo::builder()
                            .size(
                                (disk_instance.total_draw_count * std::mem::size_of::<vk::DrawIndexedIndirectCommand>())
                                    as _,
                            )
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
                            .build(),
                        &vk_mem::AllocationCreateInfo {
                            usage: vk_mem::MemoryUsage::GpuOnly,
                            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                            ..Default::default()
                        },
                    ),
                    factory.allocate_buffer(
                        &vk::BufferCreateInfo::builder()
                            .size(
                                (disk_instance.total_draw_count * std::mem::size_of::<vk::DrawIndexedIndirectCommand>())
                                    as _,
                            )
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
                            .build(),
                        &vk_mem::AllocationCreateInfo {
                            usage: vk_mem::MemoryUsage::GpuOnly,
                            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                            ..Default::default()
                        },
                    ),
                    factory.allocate_buffer(
                        &vk::BufferCreateInfo::builder()
                            .size(
                                (disk_instance.total_draw_count * std::mem::size_of::<vk::DrawIndexedIndirectCommand>())
                                    as _,
                            )
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
                            .build(),
                        &vk_mem::AllocationCreateInfo {
                            usage: vk_mem::MemoryUsage::GpuOnly,
                            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                            ..Default::default()
                        },
                    ),
                    factory.allocate_buffer(
                        &vk::BufferCreateInfo::builder()
                            .size((2 * std::mem::size_of::<u32>()) as _)
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
                            .build(),
                        &vk_mem::AllocationCreateInfo {
                            usage: vk_mem::MemoryUsage::GpuOnly,
                            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                            ..Default::default()
                        },
                    ),
                    factory.allocate_buffer(
                        &vk::BufferCreateInfo::builder()
                            .size((std::mem::size_of::<vk::DispatchIndirectCommand>()) as _)
                            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
                            .build(),
                        &vk_mem::AllocationCreateInfo {
                            usage: vk_mem::MemoryUsage::GpuOnly,
                            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                            ..Default::default()
                        },
                    ),
                ];

                instances.push(RenderInstance {
                    mesh,
                    material_instance,
                    material_data,

                    bounding_cone_buffer: disk_instance.bounding_cone_buffer,
                    occluder_arguments_buffer: disk_instance.occluder_arguments_buffer,
                    draw_arguments_buffer: disk_instance.draw_arguments_buffer,

                    total_instance_count: disk_instance.total_instance_count,
                    total_draw_count: disk_instance.total_draw_count,

                    runtime_buffers,
                });
                render_instance_count += 1;
                total_draw_count += disk_instance.total_draw_count;
            }

            self.buckets.push(RenderBucket {
                material,
                instances,

                instance_transform_buffer: disk_bucket.instance_transform_buffer,
            });
        }

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(render_instance_count as _)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .build()])
                .build(),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&[vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX)
                    .build()])
                .build(),
        );
        let temp_per_descriptor_layouts = vec![descriptor_set_layout; render_instance_count];
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut temp_write_infos = Vec::with_capacity(render_instance_count);
        let mut descriptor_writes = Vec::with_capacity(render_instance_count);
        {
            let mut current_descriptor_set = 0;
            for bucket in &self.buckets {
                let mut current_offset = 0;
                for instance in &bucket.instances {
                    let range = instance.total_instance_count * std::mem::size_of::<[f32; 16]>();
                    let current_write_info = temp_write_infos.len();
                    temp_write_infos.push(
                        vk::DescriptorBufferInfo::builder()
                            .buffer(self.buffers[bucket.instance_transform_buffer].0)
                            .offset(current_offset as _)
                            .range(range as _)
                            .build(),
                    );

                    descriptor_writes.push(
                        vk::WriteDescriptorSet::builder()
                            .dst_set(descriptor_sets[current_descriptor_set])
                            .dst_binding(0)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .buffer_info(&temp_write_infos[current_write_info..current_write_info + 1])
                            .build(),
                    );
                    current_offset += range;
                    current_descriptor_set += 1;
                }
            }
        }
        factory.update_descriptor_sets(&descriptor_writes, &[]);

        self.render_transforms = RenderTransforms {
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            // transform_buffers,
        };

        self.runtime_buffers = vec![factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size((total_draw_count * std::mem::size_of::<u32>() * 4) as _)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            },
        )];
    }

    fn initialize_environment_probes(&mut self, disk_scenery: &DiskStaticScenery, factory: &mut DeviceFactory) {
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
                disk_scenery.global_resources.precomputed_brdf_image
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

    fn initialize_mesh_cluster_culling(
        &mut self,
        disk_scenery: &DiskStaticScenery,
        occluder_pass: &OccluderPass,
        factory: &mut DeviceFactory,
    ) {
        let mut culling_buckets = Vec::with_capacity(self.buckets.len());
        for bucket in &self.buckets {
            let mut instances = Vec::with_capacity(bucket.instances.len());
            for instance in &bucket.instances {
                instances.push(MeshCullingInstance {
                    input_bounding_cone_buffer: self.buffers[instance.bounding_cone_buffer].0,
                    input_occluder_arguments_buffer: self.buffers[instance.occluder_arguments_buffer].0,
                    input_draw_arguments_buffer: self.buffers[instance.draw_arguments_buffer].0,

                    count_buffer: instance.get_runtime_count_buffer(),
                    occlusion_culling_arguments_buffer: instance.get_runtime_occlusion_culling_arguments_buffer(),
                    occluder_arguments_buffer: instance.get_runtime_occluder_arguments_buffer(),
                    temp_draw_arguments_buffer: instance.get_runtime_temp_draw_arguments_buffer(),
                    draw_arguments_buffer: instance.get_runtime_draw_arguments_buffer(),

                    // instance_count: instance.total_instance_count,
                    draw_count: instance.total_draw_count,
                    dispatch_size: ((instance.total_draw_count + 8) / 8) as _,
                });
            }

            culling_buckets.push(MeshCullingBucket { instances });
        }

        self.cluster_culling = MeshClusterCulling::new(disk_scenery, culling_buckets, self.pipeline_cache, factory);
        self.cluster_culling.update_apex_culling_descriptor_sets(factory);
        self.cluster_culling
            .update_occlusion_culling_descriptor_sets(self.get_visibility_buffer(), factory);
        self.cluster_culling.update_count_to_dispatch_descriptor_sets(factory);
        self.occluder_resolve = OccluderResolve::new(
            disk_scenery,
            occluder_pass,
            self.get_visibility_buffer(),
            factory,
            self.pipeline_cache,
        );
    }
}
