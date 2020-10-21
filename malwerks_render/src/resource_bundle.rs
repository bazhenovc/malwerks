// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::upload_batch::*;

pub struct RenderMesh {
    pub vertex_buffer: usize,
    pub index_buffer: (vk::IndexType, usize),
    pub index_count: usize,
}

pub struct RenderInstance {
    pub mesh: usize,
    pub material_instance: usize,
    pub material_instance_data: [u8; 64],

    pub total_instance_count: usize,
    pub total_draw_count: usize,
}

pub struct RenderBucket {
    pub material: usize,
    pub instances: Vec<RenderInstance>,
    pub instance_transform_buffer: usize,
}

pub struct RenderMaterial {
    pub material_layout: usize,

    pub vertex_stride: u32,
    pub vertex_format: Vec<(vk::Format, u32, u32)>, // format, location, offset

    pub fragment_alpha_test: bool,
    pub fragment_cull_flags: vk::CullModeFlags,
}

pub struct ResourceBundle {
    pub buffers: Vec<HeapAllocatedResource<vk::Buffer>>,
    pub meshes: Vec<RenderMesh>,
    pub images: Vec<HeapAllocatedResource<vk::Image>>,
    pub image_views: Vec<vk::ImageView>,
    pub samplers: Vec<vk::Sampler>,
    pub buckets: Vec<RenderBucket>,

    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_layouts: Vec<vk::DescriptorSetLayout>, // directly maps to `material_layouts`
    pub descriptor_sets: Vec<vk::DescriptorSet>,          // directly maps to `material_instances`

    pub materials: Vec<RenderMaterial>,
}

impl ResourceBundle {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for buffer in &self.buffers {
            factory.deallocate_buffer(buffer);
        }
        for image in &self.images {
            factory.deallocate_image(image);
        }
        for image_view in &self.image_views {
            factory.destroy_image_view(*image_view);
        }
        for sampler in &self.samplers {
            factory.destroy_sampler(*sampler);
        }
        factory.destroy_descriptor_pool(self.descriptor_pool);
        for descriptor_layout in &self.descriptor_layouts {
            factory.destroy_descriptor_set_layout(*descriptor_layout);
        }
    }

    pub fn from_disk(
        disk_render_bundle: &DiskRenderBundle,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let buffers = initialize_buffers(&disk_render_bundle, command_buffer, factory, queue);
        let meshes = initialize_meshes(&disk_render_bundle);
        let (images, image_views, samplers) = initialize_images(&disk_render_bundle, command_buffer, factory, queue);
        let (descriptor_pool, descriptor_layouts, descriptor_sets) =
            initialize_descriptor_pool(&disk_render_bundle, &image_views, &samplers, factory);
        let buckets = initialize_buckets(&disk_render_bundle, command_buffer, factory, queue);
        let materials = initialize_materials(&disk_render_bundle);

        Self {
            buffers,
            meshes,
            images,
            image_views,
            samplers,
            buckets,

            descriptor_pool,
            descriptor_layouts,
            descriptor_sets,

            materials,
        }
    }
}

fn initialize_buffers(
    disk_render_bundle: &DiskRenderBundle,
    command_buffer: &mut CommandBuffer,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> Vec<HeapAllocatedResource<vk::Buffer>> {
    log::info!("initializing {} buffers", disk_render_bundle.buffers.len());

    let mut buffers = Vec::with_capacity(disk_render_bundle.buffers.len());

    let mut upload_batch = UploadBatch::new(command_buffer);
    for disk_buffer in &disk_render_bundle.buffers {
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
        buffers.push(buffer);
    }
    upload_batch.flush(factory, queue);
    buffers
}

fn initialize_meshes(disk_render_bundle: &DiskRenderBundle) -> Vec<RenderMesh> {
    let mut meshes = Vec::with_capacity(disk_render_bundle.meshes.len());
    for disk_mesh in &disk_render_bundle.meshes {
        meshes.push(RenderMesh {
            vertex_buffer: disk_mesh.vertex_buffer,
            index_buffer: (
                vk::IndexType::from_raw(disk_mesh.index_buffer.0),
                disk_mesh.index_buffer.1,
            ),
            index_count: disk_mesh.index_count,
            // indirect_draw_buffer: disk_mesh.indirect_draw_buffer,
            // indirect_draw_count: disk_mesh.indirect_draw_count,
        });
    }
    meshes
}

fn initialize_images(
    disk_render_bundle: &DiskRenderBundle,
    command_buffer: &mut CommandBuffer,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> (
    Vec<HeapAllocatedResource<vk::Image>>,
    Vec<vk::ImageView>,
    Vec<vk::Sampler>,
) {
    log::info!(
        "initializing {} images and {} samplers",
        disk_render_bundle.images.len(),
        disk_render_bundle.samplers.len()
    );

    let mut images = Vec::with_capacity(disk_render_bundle.images.len());
    let mut image_views = Vec::with_capacity(disk_render_bundle.images.len());

    let mut upload_batch = UploadBatch::new(command_buffer);
    for disk_image in &disk_render_bundle.images {
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

        image_views.push(
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
        images.push(allocated_image);
    }
    upload_batch.flush(factory, queue);

    let mut samplers = Vec::with_capacity(disk_render_bundle.samplers.len());
    for disk_sampler in &disk_render_bundle.samplers {
        samplers.push(
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

    (images, image_views, samplers)
}

fn initialize_descriptor_pool(
    disk_render_bundle: &DiskRenderBundle,
    image_views: &[vk::ImageView],
    samplers: &[vk::Sampler],
    factory: &mut DeviceFactory,
) -> (vk::DescriptorPool, Vec<vk::DescriptorSetLayout>, Vec<vk::DescriptorSet>) {
    let mut max_descriptor_image_count = 0;
    for disk_material_layout in &disk_render_bundle.material_layouts {
        max_descriptor_image_count = max_descriptor_image_count.max(disk_material_layout.image_count);
    }

    let mut temp_bindings = Vec::with_capacity(max_descriptor_image_count);
    let mut descriptor_set_layouts = Vec::with_capacity(disk_render_bundle.material_layouts.len());

    for disk_material_layout in &disk_render_bundle.material_layouts {
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
        descriptor_set_layouts.push(layout);
        temp_bindings.clear();
    }

    let max_descriptor_count = disk_render_bundle.material_instances.len() * max_descriptor_image_count;
    let mut temp_writes = Vec::with_capacity(max_descriptor_count);
    let mut temp_write_ids = Vec::with_capacity(max_descriptor_count);
    let mut temp_image_infos = Vec::with_capacity(max_descriptor_count);
    let mut temp_per_descriptor_layouts = Vec::with_capacity(disk_render_bundle.material_instances.len());

    for disk_material_instance in &disk_render_bundle.material_instances {
        let layout = descriptor_set_layouts[disk_material_instance.material_layout];

        let descriptor_id = temp_per_descriptor_layouts.len();
        temp_per_descriptor_layouts.push(layout);

        for (binding_id, image) in disk_material_instance.images.iter().enumerate() {
            let image_info_index = temp_image_infos.len();
            temp_image_infos.push(
                vk::DescriptorImageInfo::builder()
                    .image_view(image_views[image.0])
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .sampler(samplers[image.1])
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
        descriptor_set_layouts.len(),
        temp_per_descriptor_layouts.len(),
        temp_writes.len()
    );

    let descriptor_pool = factory.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::builder()
            .max_sets(temp_per_descriptor_layouts.len() as _)
            .pool_sizes(&[vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(temp_writes.len() as _)
                .build()])
            .build(),
    );
    let descriptor_sets = factory.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&temp_per_descriptor_layouts)
            .build(),
    );

    for i in 0..temp_writes.len() {
        temp_writes[i].dst_set = descriptor_sets[temp_write_ids[i]];
    }
    factory.update_descriptor_sets(&temp_writes, &[]);

    (descriptor_pool, descriptor_set_layouts, descriptor_sets)
}

fn initialize_buckets(
    disk_render_bundle: &DiskRenderBundle,
    _command_buffer: &mut CommandBuffer,
    _factory: &mut DeviceFactory,
    _queue: &mut DeviceQueue,
) -> Vec<RenderBucket> {
    let mut buckets = Vec::with_capacity(disk_render_bundle.buckets.len());

    for disk_bucket in &disk_render_bundle.buckets {
        let material = disk_bucket.material;
        let mut instances = Vec::with_capacity(disk_bucket.instances.len());

        for disk_instance in &disk_bucket.instances {
            let mesh = disk_instance.mesh;
            let material_instance = disk_instance.material_instance;

            let mut material_instance_data = [0u8; 64];
            {
                let disk_data = &disk_render_bundle.material_instances[material_instance].material_instance_data;
                assert_eq!(disk_data.len(), 64);

                material_instance_data.copy_from_slice(disk_data);
            }

            let total_instance_count = disk_instance.total_instance_count;
            let total_draw_count = disk_instance.total_draw_count;

            instances.push(RenderInstance {
                mesh,
                material_instance,
                material_instance_data,

                total_instance_count,
                total_draw_count,
            });
        }

        buckets.push(RenderBucket {
            material,
            instances,
            instance_transform_buffer: disk_bucket.instance_transform_buffer,
        });
    }

    buckets
}

fn initialize_materials(disk_render_bundle: &DiskRenderBundle) -> Vec<RenderMaterial> {
    let mut materials = Vec::with_capacity(disk_render_bundle.materials.len());
    for disk_material in &disk_render_bundle.materials {
        let material_layout = disk_material.material_layout;

        let vertex_stride = disk_material.vertex_stride as u32;
        let mut vertex_format = Vec::with_capacity(disk_material.vertex_format.len());
        for vertex_attribute in &disk_material.vertex_format {
            vertex_format.push((
                vk::Format::from_raw(vertex_attribute.attribute_format),
                vertex_attribute.attribute_location,
                vertex_attribute.attribute_offset as u32,
            ));
        }

        let fragment_alpha_test = disk_material.fragment_alpha_test;
        let fragment_cull_flags = vk::CullModeFlags::from_raw(disk_material.fragment_cull_flags);

        materials.push(RenderMaterial {
            material_layout,
            vertex_stride,
            vertex_format,
            fragment_alpha_test,
            fragment_cull_flags,
        });
    }
    materials
}
