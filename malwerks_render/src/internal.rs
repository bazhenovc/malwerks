// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

pub(crate) fn copy_to_mapped_memory<T>(data: &[T], memory: *mut u8) {
    // validate alignment, should be aligned to 8 and match alignment of T
    assert_eq!((memory as usize) & ((1 << (8 - 1)) - 1), 0);
    assert_eq!((memory as usize) & ((1 << (std::mem::align_of::<T>() - 1)) - 1), 0);

    unsafe {
        #[allow(clippy::cast_ptr_alignment)] // alignment is validated above
        std::ptr::copy_nonoverlapping(data.as_ptr(), memory as _, data.len());
    }
}

pub(crate) struct UploadBatch<'a> {
    //factory: &'a mut GraphicsFactory,
    //queue: &'a mut DeviceQueue,
    command_buffer: &'a mut CommandBuffer,
    temporary_buffers: Vec<HeapAllocatedResource<vk::Buffer>>,
}

impl<'a> Drop for UploadBatch<'a> {
    fn drop(&mut self) {
        assert!(self.temporary_buffers.is_empty());
    }
}

impl<'a> UploadBatch<'a> {
    pub fn new(command_buffer: &'a mut CommandBuffer) -> Self {
        command_buffer.reset();
        command_buffer.begin(
            &vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                .build(),
        );

        Self {
            //factory,
            //queue,
            command_buffer,
            temporary_buffers: Vec::new(),
        }
    }

    pub fn upload_image_memory(
        &mut self,
        image: &HeapAllocatedResource<vk::Image>,
        image_size: (u32, u32, u32),
        image_params: (usize, usize, usize),
        image_memory: &[u8],
        factory: &mut GraphicsFactory,
    ) {
        let temp_buffer = upload_image_memory(
            image,
            image_size,
            image_params,
            image_memory,
            factory,
            self.command_buffer,
        );
        self.temporary_buffers.push(temp_buffer);
    }

    pub fn upload_buffer_memory(
        &mut self,
        buffer: &HeapAllocatedResource<vk::Buffer>,
        buffer_memory: &[u8],
        factory: &mut GraphicsFactory,
    ) {
        let temp_buffer = upload_buffer_memory(buffer, buffer_memory, factory, self.command_buffer);
        self.temporary_buffers.push(temp_buffer);
    }

    pub fn flush(&mut self, factory: &mut GraphicsFactory, queue: &mut DeviceQueue) {
        if !self.temporary_buffers.is_empty() {
            self.command_buffer.end();
            queue.submit(
                &[vk::SubmitInfo::builder()
                    .command_buffers(&[self.command_buffer.clone().into()])
                    .build()],
                vk::Fence::null(),
            );
            queue.wait_idle();
            for temp_buffer in &self.temporary_buffers {
                factory.deallocate_buffer(&temp_buffer);
            }
            self.temporary_buffers.clear();
        }
    }
}

// TODO: make crate-local
pub fn upload_image_memory(
    image: &HeapAllocatedResource<vk::Image>,
    image_size: (u32, u32, u32),
    image_params: (usize, usize, usize),
    image_memory: &[u8],
    factory: &mut GraphicsFactory,
    command_buffer: &mut CommandBuffer,
) -> HeapAllocatedResource<vk::Buffer> {
    let (image_block_size, num_mip_levels, num_array_layers) = image_params;
    let temp_buffer = allocate_temporary_buffer(image_memory, factory);

    let mut mip_offset = 0;
    let mut buffer_copies = Vec::with_capacity(num_mip_levels);
    for layer in 0..num_array_layers {
        for mip in 0..num_mip_levels {
            let mip_width = (image_size.0 >> mip).max(1) as usize;
            let mip_height = (image_size.1 >> mip).max(1) as usize;
            let mip_depth = (image_size.2 >> mip).max(1) as usize;

            let row_pitch = image_block_size * ((mip_width + 3) / 4).max(1);
            let mip_size = row_pitch * ((mip_height + 3) / 4).max(1);

            buffer_copies.push(
                vk::BufferImageCopy::builder()
                    .image_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(mip as _)
                            .base_array_layer(layer as _)
                            .layer_count(1)
                            .build(),
                    )
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: mip_width as _,
                        height: mip_height as _,
                        depth: mip_depth as _,
                    })
                    //.buffer_image_height(mip_height as _)
                    //.buffer_row_length(row_pitch as _)
                    .buffer_offset(mip_offset as _)
                    .build(),
            );

            mip_offset += mip_depth * mip_size;
        }
    }

    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::HOST,
        vk::PipelineStageFlags::TRANSFER,
        None,
        &[],
        &[],
        &[vk::ImageMemoryBarrier::builder()
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .image(image.0)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(num_mip_levels as _)
                    .base_array_layer(0)
                    .layer_count(num_array_layers as _)
                    .build(),
            )
            .build()],
    );
    command_buffer.copy_buffer_to_image(
        temp_buffer.0,
        image.0,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &buffer_copies,
    );
    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        None,
        &[],
        &[],
        &[vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .image(image.0)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(num_mip_levels as _)
                    .base_array_layer(0)
                    .layer_count(num_array_layers as _)
                    .build(),
            )
            .build()],
    );

    temp_buffer
}

fn upload_buffer_memory(
    buffer: &HeapAllocatedResource<vk::Buffer>,
    buffer_memory: &[u8],
    factory: &mut GraphicsFactory,
    command_buffer: &mut CommandBuffer,
) -> HeapAllocatedResource<vk::Buffer> {
    let temp_buffer = allocate_temporary_buffer(buffer_memory, factory);

    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::HOST,
        vk::PipelineStageFlags::TRANSFER,
        None,
        &[],
        &[vk::BufferMemoryBarrier::builder()
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .buffer(buffer.0)
            .offset(0)
            .size(buffer_memory.len() as _)
            .build()],
        &[],
    );
    command_buffer.copy_buffer(
        temp_buffer.0,
        buffer.0,
        &[vk::BufferCopy::builder()
            .src_offset(0)
            .dst_offset(0)
            .size(buffer_memory.len() as _)
            .build()],
    );
    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::VERTEX_SHADER,
        None,
        &[],
        &[vk::BufferMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .buffer(buffer.0)
            .offset(0)
            .size(buffer_memory.len() as _)
            .build()],
        &[],
    );

    temp_buffer
}

fn allocate_temporary_buffer(memory: &[u8], factory: &mut GraphicsFactory) -> HeapAllocatedResource<vk::Buffer> {
    let temp_buffer = factory.allocate_buffer(
        &vk::BufferCreateInfo::builder()
            .size(memory.len() as _)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .build(),
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuOnly,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        },
    );
    let temp_memory = factory.map_allocation_memory(&temp_buffer);
    unsafe {
        //assert_eq!(memory.len(), temp_buffer.1.get_size());
        assert!(memory.len() <= temp_buffer.1.get_size());
        std::ptr::copy_nonoverlapping(memory.as_ptr(), temp_memory as _, memory.len());
    }
    factory.unmap_allocation_memory(&temp_buffer);
    temp_buffer
}
