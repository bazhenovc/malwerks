// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_bundles::*;
use malwerks_core::*;
use malwerks_vk::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskEnvironmentProbe {
    pub probe_image: DiskImage,
    pub iem_image: DiskImage,
    pub pmrem_image: DiskImage,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskPbrResourceBundle {
    pub precomputed_brdf_image: DiskImage,
    pub environment_probe: DiskEnvironmentProbe,
}

impl DiskPbrResourceBundle {
    pub fn serialize_into<W>(&self, writer: W, _compression_level: u32) -> Result<(), ()>
    where
        W: std::io::Write,
    {
        match bincode::serialize_into(writer, self) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn deserialize_from<R>(reader: R) -> Result<Self, ()>
    where
        R: std::io::Read,
    {
        match bincode::deserialize_from(reader) {
            Ok(bundle) => Ok(bundle),
            Err(_) => Err(()),
        }
    }
}

pub struct PbrResourceBundle {
    pub images: Vec<HeapAllocatedResource<vk::Image>>,
    pub image_views: Vec<vk::ImageView>,

    pub linear_sampler: vk::Sampler,

    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: Vec<vk::DescriptorSet>,
}

impl PbrResourceBundle {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for image in &self.images {
            factory.deallocate_image(image);
        }
        for image_view in &self.image_views {
            factory.destroy_image_view(*image_view);
        }
        factory.destroy_sampler(self.linear_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
    }

    pub fn new(
        disk_resources: &DiskPbrResourceBundle,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let mut images = Vec::with_capacity(4);
        let mut image_views = Vec::with_capacity(4);

        let mut upload_batch = UploadBatch::new(command_buffer);
        for disk_image in &[
            &disk_resources.precomputed_brdf_image,
            &disk_resources.environment_probe.probe_image,
            &disk_resources.environment_probe.iem_image,
            &disk_resources.environment_probe.pmrem_image,
        ] {
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

        let linear_sampler = factory.create_sampler(
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
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(4)
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
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ]),
        );

        let temp_per_descriptor_layouts = [descriptor_set_layout; 1];
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut temp_writes = [vk::WriteDescriptorSet::default(); 4];
        let mut temp_image_infos = [vk::DescriptorImageInfo::default(); 4];
        for (image_id, image_view) in image_views.iter().enumerate() {
            temp_image_infos[image_id] = vk::DescriptorImageInfo::builder()
                .image_view(*image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .sampler(linear_sampler)
                .build();
            temp_writes[image_id] = vk::WriteDescriptorSet::builder()
                .dst_binding(image_id as _)
                .dst_set(descriptor_sets[0])
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&temp_image_infos[image_id..image_id + 1])
                .build();
        }
        factory.update_descriptor_sets(&temp_writes, &[]);

        Self {
            images,
            image_views,
            linear_sampler,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
        }
    }

    pub fn get_probe_image_view(&self) -> vk::ImageView {
        self.image_views[1]
    }
}
