// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

// use malwerks_dds::*;
use malwerks_render::*;
use malwerks_resources::*;

use crate::acceleration_structure::*;
use crate::shader_binding_table::*;

pub struct EnvironmentProbes {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,

    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,

    probe_images: Vec<HeapAllocatedResource<vk::Image>>,
    probe_image_views: Vec<vk::ImageView>,

    ray_gen_and_miss_modules: Vec<vk::ShaderModule>,
    closest_hit_modules: Vec<vk::ShaderModule>,
    ray_tracing_pipelines: Vec<vk::Pipeline>,

    shader_binding_table: ShaderBindingTable,
}

impl EnvironmentProbes {
    pub fn new(
        image_width: u32,
        image_height: u32,
        static_scenery: &DiskStaticScenery,
        ray_tracing_properties: &vk::PhysicalDeviceRayTracingPropertiesNV,
        acceleration_structure: &AccelerationStructure,
        factory: &mut DeviceFactory,
    ) -> Self {
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&[
                    vk::DescriptorSetLayoutBinding::builder()
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_NV)
                        .stage_flags(vk::ShaderStageFlags::RAYGEN_NV)
                        .binding(0)
                        .build(),
                    vk::DescriptorSetLayoutBinding::builder()
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .stage_flags(vk::ShaderStageFlags::RAYGEN_NV)
                        .binding(1)
                        .build(),
                    vk::DescriptorSetLayoutBinding::builder()
                        .descriptor_count(5)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .stage_flags(vk::ShaderStageFlags::CLOSEST_HIT_NV)
                        .binding(2)
                        .build(),
                ])
                .push_next(
                    &mut vk::DescriptorSetLayoutBindingFlagsCreateInfoEXT::builder()
                        .binding_flags(&[
                            vk::DescriptorBindingFlagsEXT::default(),
                            vk::DescriptorBindingFlagsEXT::default(),
                            vk::DescriptorBindingFlagsEXT::VARIABLE_DESCRIPTOR_COUNT,
                        ])
                        .build(),
                )
                .build(),
        );
        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&[descriptor_set_layout])
                .build(),
        );

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(static_scenery.environment_probes.len() as _)
                .pool_sizes(&[
                    vk::DescriptorPoolSize::builder()
                        .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_NV)
                        .descriptor_count(1)
                        .build(),
                    vk::DescriptorPoolSize::builder()
                        .ty(vk::DescriptorType::STORAGE_IMAGE)
                        .descriptor_count(1)
                        .build(),
                    vk::DescriptorPoolSize::builder()
                        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .descriptor_count(5)
                        .build(),
                ])
                .build(),
        );
        let temp_per_descriptor_layouts: Vec<vk::DescriptorSetLayout> = (0..static_scenery.environment_probes.len())
            .map(|_| descriptor_set_layout)
            .collect();
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut closest_hit_modules = Vec::with_capacity(static_scenery.materials.len());
        for disk_material in static_scenery.materials.iter() {
            closest_hit_modules.push(
                factory.create_shader_module(
                    &vk::ShaderModuleCreateInfo::builder()
                        .code(&disk_material.ray_closest_hit_stage)
                        .build(),
                ),
            );
        }
        let mut ray_gen_and_miss_modules = Vec::with_capacity(static_scenery.environment_probes.len() * 2);

        let mut shader_groups = Vec::with_capacity(2 + closest_hit_modules.len());
        for shader_id in 0..2 {
            shader_groups.push(
                vk::RayTracingShaderGroupCreateInfoNV::builder()
                    .ty(vk::RayTracingShaderGroupTypeNV::GENERAL)
                    .general_shader(shader_id as _)
                    .closest_hit_shader(vk::SHADER_UNUSED_NV)
                    .any_hit_shader(vk::SHADER_UNUSED_NV)
                    .intersection_shader(vk::SHADER_UNUSED_NV)
                    .build(),
            );
        }
        for shader_id in 0..static_scenery.materials.len() {
            shader_groups.push(
                vk::RayTracingShaderGroupCreateInfoNV::builder()
                    .ty(vk::RayTracingShaderGroupTypeNV::TRIANGLES_HIT_GROUP)
                    .general_shader(vk::SHADER_UNUSED_NV)
                    .closest_hit_shader((shader_id + 2) as _)
                    .any_hit_shader(vk::SHADER_UNUSED_NV)
                    .intersection_shader(vk::SHADER_UNUSED_NV)
                    .build(),
            );
        }

        log::info!("shader groups: {:?}", shader_groups.len());

        // let entry_point = std::ffi::CString::new("main").unwrap();
        let entry_point = std::ffi::CStr::from_bytes_with_nul(b"main\0").unwrap();
        let mut shader_stages = Vec::with_capacity(static_scenery.environment_probes.len() * shader_groups.len());

        // TODO: ensure these never reallocate
        let mut temp_writes = Vec::with_capacity(descriptor_sets.len());
        let mut temp_acceleration_structures = Vec::with_capacity(descriptor_sets.len());
        let mut temp_acceleration_structure_infos = Vec::with_capacity(descriptor_sets.len());
        let mut temp_image_infos = Vec::with_capacity(descriptor_sets.len() * 6);
        let mut temp_ray_tracing_pipelines = Vec::with_capacity(static_scenery.environment_probes.len());

        let mut probe_images = Vec::with_capacity(static_scenery.environment_probes.len());
        let mut probe_image_views = Vec::with_capacity(static_scenery.environment_probes.len());
        for (probe_id, disk_probe) in static_scenery.environment_probes.iter().enumerate() {
            let probe_images_start = probe_images.len();
            probe_images.push(
                factory.allocate_image(
                    &vk::ImageCreateInfo::builder()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::R32G32B32A32_SFLOAT)
                        .extent(vk::Extent3D {
                            width: image_width,
                            height: image_height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::STORAGE)
                        .initial_layout(vk::ImageLayout::UNDEFINED)
                        .build(),
                    &vk_mem::AllocationCreateInfo {
                        usage: vk_mem::MemoryUsage::GpuOnly,
                        required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                        ..Default::default()
                    },
                ),
            );
            probe_image_views.push(
                factory.create_image_view(
                    &vk::ImageViewCreateInfo::builder()
                        .image(probe_images[probe_images_start].0)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R32G32B32A32_SFLOAT)
                        .components(vk::ComponentMapping::default())
                        .subresource_range(
                            vk::ImageSubresourceRange::builder()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1)
                                .build(),
                        ),
                ),
            );

            let acceleration_structure_start = temp_acceleration_structures.len();
            temp_acceleration_structures.push(acceleration_structure.get_top_level_acceleration_structure());
            temp_acceleration_structure_infos.push(
                vk::WriteDescriptorSetAccelerationStructureNV::builder()
                    .acceleration_structures(
                        &temp_acceleration_structures[acceleration_structure_start..temp_acceleration_structures.len()],
                    )
                    .build(),
            );
            let writes_start = temp_writes.len();
            temp_writes.push(
                vk::WriteDescriptorSet::builder()
                    .dst_binding(0)
                    .dst_set(descriptor_sets[probe_id])
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_NV)
                    .push_next(&mut temp_acceleration_structure_infos[acceleration_structure_start])
                    .build(),
            );
            temp_writes[writes_start].descriptor_count = 1;

            let images_start = temp_image_infos.len();
            temp_image_infos.push(
                vk::DescriptorImageInfo::builder()
                    .image_view(probe_image_views[probe_images_start])
                    .image_layout(vk::ImageLayout::GENERAL)
                    .build(),
            );
            temp_writes.push(
                vk::WriteDescriptorSet::builder()
                    .dst_binding(1)
                    .dst_set(descriptor_sets[probe_id])
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&temp_image_infos[images_start..temp_image_infos.len()])
                    .build(),
            );

            let ray_gen_and_miss_start = ray_gen_and_miss_modules.len();
            ray_gen_and_miss_modules.push(
                factory.create_shader_module(
                    &vk::ShaderModuleCreateInfo::builder()
                        .code(&disk_probe.ray_gen_stage)
                        .build(),
                ),
            );
            ray_gen_and_miss_modules.push(
                factory.create_shader_module(
                    &vk::ShaderModuleCreateInfo::builder()
                        .code(&disk_probe.ray_miss_stage)
                        .build(),
                ),
            );

            let pipeline_shaders_start = shader_stages.len();
            shader_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::RAYGEN_NV)
                    .module(ray_gen_and_miss_modules[ray_gen_and_miss_start])
                    .name(&entry_point)
                    .build(),
            );
            shader_stages.push(
                vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::MISS_NV)
                    .module(ray_gen_and_miss_modules[ray_gen_and_miss_start + 1])
                    .name(&entry_point)
                    .build(),
            );
            for closest_hit_module in closest_hit_modules.iter() {
                shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::CLOSEST_HIT_NV)
                        .module(*closest_hit_module)
                        .name(&entry_point)
                        .build(),
                );
            }

            temp_ray_tracing_pipelines.push(
                vk::RayTracingPipelineCreateInfoNV::builder()
                    .stages(&shader_stages[pipeline_shaders_start..shader_stages.len()])
                    .groups(&shader_groups)
                    .max_recursion_depth(1)
                    .layout(pipeline_layout)
                    .build(),
            );
        }

        //log::info!("{:?}", &ray_gen_and_miss_modules);
        //log::info!("{:?}", &closest_hit_modules);
        //log::info!("{:?}", &shader_stages);
        // log::info!("{:?}", &temp_ray_tracing_pipelines);

        log::info!("allocating {} ray tracing pipelines", temp_ray_tracing_pipelines.len());
        let ray_tracing_pipelines =
            factory.create_ray_tracing_pipelines_nv(vk::PipelineCache::null(), &temp_ray_tracing_pipelines);
        let shader_binding_table = ShaderBindingTable::new(shader_stages.len(), ray_tracing_properties, factory);

        factory.update_descriptor_sets(&temp_writes, &[]);

        Self {
            descriptor_set_layout,
            pipeline_layout,
            descriptor_pool,
            descriptor_sets,
            probe_images,
            probe_image_views,
            ray_gen_and_miss_modules,
            closest_hit_modules,
            ray_tracing_pipelines,
            shader_binding_table,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        self.shader_binding_table.destroy(factory);
        for pipeline in self.ray_tracing_pipelines.iter() {
            factory.destroy_pipeline(*pipeline);
        }
        for shader_module in self.closest_hit_modules.iter() {
            factory.destroy_shader_module(*shader_module);
        }
        for shader_module in self.ray_gen_and_miss_modules.iter() {
            factory.destroy_shader_module(*shader_module);
        }
        for image_view in self.probe_image_views.iter() {
            factory.destroy_image_view(*image_view);
        }
        for image in self.probe_images.iter() {
            factory.deallocate_image(image);
        }
        factory.destroy_pipeline_layout(self.pipeline_layout);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
        factory.destroy_descriptor_pool(self.descriptor_pool);
    }

    pub fn build(&mut self, command_buffer: &mut CommandBuffer, factory: &mut DeviceFactory, queue: &mut DeviceQueue) {
        self.shader_binding_table
            .build(&self.ray_tracing_pipelines, command_buffer, factory, queue);
    }

    pub fn bake_environment_probes(
        &mut self,
        image_width: u32,
        image_height: u32,
        command_buffer: &mut CommandBuffer,
        device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) {
        let temp_buffer = factory.allocate_buffer(
            &vk::BufferCreateInfo::builder()
                .size(self.probe_images[0].1.get_size() as _)
                .usage(vk::BufferUsageFlags::TRANSFER_DST)
                .build(),
            &vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::CpuOnly,
                required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
                ..Default::default()
            },
        );

        for (pipeline_id, pipeline) in self.ray_tracing_pipelines.iter().enumerate() {
            let table_stride = self.shader_binding_table.get_table_stride();

            let ray_gen_buffer = self.shader_binding_table.get_table_buffer().0;
            let ray_gen_offset = 0;

            let ray_miss_buffer = self.shader_binding_table.get_table_buffer().0;
            let ray_miss_offset = table_stride;
            let ray_miss_stride = table_stride;

            let ray_hit_buffer = self.shader_binding_table.get_table_buffer().0;
            let ray_hit_offset = 2 * table_stride;
            let ray_hit_stride = table_stride;

            let ray_call_buffer = vk::Buffer::null();
            let ray_call_offset = 0;
            let ray_call_stride = 0;

            command_buffer.reset();
            command_buffer.begin(
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                    .build(),
            );

            command_buffer.bind_pipeline(vk::PipelineBindPoint::RAY_TRACING_NV, *pipeline);
            command_buffer.bind_descriptor_sets(
                vk::PipelineBindPoint::RAY_TRACING_NV,
                self.pipeline_layout,
                0,
                &[self.descriptor_sets[pipeline_id]],
                &[],
            );

            command_buffer.pipeline_barrier(
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::RAY_TRACING_SHADER_NV,
                None,
                &[],
                &[],
                &[vk::ImageMemoryBarrier::builder()
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(!0)
                    .dst_queue_family_index(!0)
                    .image(self.probe_images[pipeline_id].0)
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

            command_buffer.trace_rays_nv(
                ray_gen_buffer,
                ray_gen_offset as _,
                ray_miss_buffer,
                ray_miss_offset as _,
                ray_miss_stride as _,
                ray_hit_buffer,
                ray_hit_offset as _,
                ray_hit_stride as _,
                ray_call_buffer,
                ray_call_offset as _,
                ray_call_stride as _,
                image_width,
                image_height,
                1,
            );

            command_buffer.pipeline_barrier(
                vk::PipelineStageFlags::RAY_TRACING_SHADER_NV,
                vk::PipelineStageFlags::TRANSFER,
                None,
                &[],
                &[],
                &[vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(!0)
                    .dst_queue_family_index(!0)
                    .image(self.probe_images[pipeline_id].0)
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
            command_buffer.copy_image_to_buffer(
                self.probe_images[pipeline_id].0,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                temp_buffer.0,
                &[vk::BufferImageCopy::builder()
                    .image_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(0)
                            .base_array_layer(0)
                            .layer_count(1)
                            .build(),
                    )
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: image_width,
                        height: image_height,
                        depth: 1,
                    })
                    .buffer_offset(0)
                    .build()],
            );
            command_buffer.pipeline_barrier(
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::RAY_TRACING_SHADER_NV,
                None,
                &[],
                &[],
                &[vk::ImageMemoryBarrier::builder()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(!0)
                    .dst_queue_family_index(!0)
                    .image(self.probe_images[pipeline_id].0)
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

            command_buffer.end();
            queue.submit(
                &[vk::SubmitInfo::builder()
                    .command_buffers(&[command_buffer.clone().into()])
                    .build()],
                vk::Fence::null(),
            );

            queue.wait_idle();
            device.wait_idle();

            // let mut scratch_image = ScratchImage::new(
            //     image_width,
            //     image_height,
            //     1,
            //     1,
            //     1,
            //     DXGI_FORMAT_R32G32B32A32_FLOAT,
            //     false,
            // );
            // let temp_memory = factory.map_allocation_memory(&temp_buffer);
            // unsafe {
            //     assert_eq!(scratch_image.as_slice().len(), temp_buffer.1.get_size());
            //     let dst_slice = scratch_image.as_slice_mut();
            //     std::ptr::copy_nonoverlapping(temp_memory, dst_slice.as_mut_ptr(), dst_slice.len());
            // }
            // factory.unmap_allocation_memory(&temp_buffer);
            // scratch_image.save_to_file(std::path::Path::new(&format!("light_probe_{}.dds", pipeline_id)));
        }

        factory.deallocate_buffer(&temp_buffer);
    }
}
