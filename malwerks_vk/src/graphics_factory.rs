use ash::version::*;
use ash::vk;

use crate::command_buffer::*;

pub struct GraphicsFactory {
    device: ash::Device,
    allocator: vk_mem::Allocator,
}

impl GraphicsFactory {
    pub(crate) fn new(device: ash::Device, instance: ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        Self {
            device: device.clone(),
            allocator: vk_mem::Allocator::new(&vk_mem::AllocatorCreateInfo {
                physical_device,
                device,
                instance,
                flags: vk_mem::AllocatorCreateFlags::NONE,
                preferred_large_heap_block_size: 0,
                frame_in_use_count: 0,
                heap_size_limits: None,
            })
            .expect("failed to create VMA allocator"),
        }
    }
}

#[derive(Clone)]
pub struct HeapAllocatedResource<T>(pub T, pub vk_mem::AllocationInfo, vk_mem::Allocation);

impl GraphicsFactory {
    pub fn allocate_buffer(
        &mut self,
        create_info: &vk::BufferCreateInfo,
        allocate_info: &vk_mem::AllocationCreateInfo,
    ) -> HeapAllocatedResource<vk::Buffer> {
        let (buffer, alloc, info) = self
            .allocator
            .create_buffer(create_info, allocate_info)
            .expect("allocate_buffer() failed");

        HeapAllocatedResource(buffer, info, alloc)
    }

    pub fn deallocate_buffer(&mut self, buffer: &HeapAllocatedResource<vk::Buffer>) {
        self.allocator
            .destroy_buffer(buffer.0, &buffer.2)
            .expect("deallocate_buffer() failed");
    }

    pub fn allocate_image(
        &mut self,
        create_info: &vk::ImageCreateInfo,
        allocate_info: &vk_mem::AllocationCreateInfo,
    ) -> HeapAllocatedResource<vk::Image> {
        let (image, alloc, info) = self
            .allocator
            .create_image(create_info, allocate_info)
            .expect("allocate_image() failed");

        HeapAllocatedResource(image, info, alloc)
    }

    pub fn deallocate_image(&mut self, image: &HeapAllocatedResource<vk::Image>) {
        self.allocator
            .destroy_image(image.0, &image.2)
            .expect("deallocate_image() failed");
    }

    pub fn map_allocation_memory<T>(&mut self, item: &HeapAllocatedResource<T>) -> *mut u8 {
        self.allocator.map_memory(&item.2).expect("map_memory() failed")
    }

    pub fn unmap_allocation_memory<T>(&mut self, item: &HeapAllocatedResource<T>) {
        self.allocator.unmap_memory(&item.2).expect("unmap_memory() failed");
    }
}

impl GraphicsFactory {
    // samplers

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateSampler.html"]
    pub fn create_sampler(&mut self, create_info: &vk::SamplerCreateInfo) -> vk::Sampler {
        unsafe {
            self.device
                .create_sampler(create_info, None)
                .expect("create_sampler() failed")
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroySampler.html"]
    pub fn destroy_sampler(&mut self, sampler: vk::Sampler) {
        unsafe {
            self.device.destroy_sampler(sampler, None);
        }
    }

    // memory

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkAllocateMemory.html"]
    pub fn allocate_memory(&mut self, create_info: &vk::MemoryAllocateInfo) -> vk::DeviceMemory {
        unsafe { self.device.allocate_memory(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkFreeMemory.html"]
    pub fn free_memory(&mut self, memory: vk::DeviceMemory) {
        unsafe {
            self.device.free_memory(memory, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkMapMemory.html"]
    pub fn map_memory(
        &mut self,
        memory: vk::DeviceMemory,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
        flags: Option<vk::MemoryMapFlags>,
    ) -> *mut std::os::raw::c_void {
        unsafe {
            self.device
                .map_memory(memory, offset, size, flags.unwrap_or_else(|| std::mem::transmute(0)))
                .unwrap()
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkUnmapMemory.html"]
    pub fn unmap_memory(&mut self, memory: vk::DeviceMemory) {
        unsafe {
            self.device.unmap_memory(memory);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkInvalidateMappedMemoryRanges.html"]
    pub fn invalidate_mapped_memory_ranges(&mut self, ranges: &[vk::MappedMemoryRange]) {
        unsafe {
            self.device.invalidate_mapped_memory_ranges(ranges).unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkFlushMappedMemoryRanges.html"]
    pub fn flush_mapped_memory_ranges(&mut self, ranges: &[vk::MappedMemoryRange]) {
        unsafe {
            self.device.flush_mapped_memory_ranges(ranges).unwrap();
        }
    }

    // command buffers and pools

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkAllocateCommandBuffers.html"]
    pub fn allocate_command_buffers(&mut self, create_info: &vk::CommandBufferAllocateInfo) -> Vec<CommandBuffer> {
        unsafe {
            let mut command_buffers = Vec::with_capacity(create_info.command_buffer_count as _);
            command_buffers.set_len(command_buffers.capacity());

            let error_code = self.device.fp_v1_0().allocate_command_buffers(
                self.device.handle(),
                create_info,
                command_buffers.as_ptr() as _,
            );
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("allocate_command_buffers() failed"),
            }

            command_buffers
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkAllocateCommandBuffers.html"]
    pub fn free_command_buffers(&mut self, command_pool: vk::CommandPool, command_buffers: &[CommandBuffer]) {
        unsafe {
            self.device.fp_v1_0().free_command_buffers(
                self.device.handle(),
                command_pool,
                command_buffers.len() as _,
                command_buffers.as_ptr() as _,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateCommandPool.html"]
    pub fn create_command_pool(&mut self, create_info: &vk::CommandPoolCreateInfo) -> vk::CommandPool {
        unsafe { self.device.create_command_pool(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyCommandPool.html"]
    pub fn destroy_command_pool(&mut self, pool: vk::CommandPool) {
        unsafe {
            self.device.destroy_command_pool(pool, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetCommandPool.html"]
    pub fn reset_command_pool(&mut self, command_pool: vk::CommandPool) {
        unsafe {
            self.device
                .reset_command_pool(command_pool, std::mem::transmute(0))
                .unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetCommandPool.html"]
    pub fn reset_command_pool_with_flags(&mut self, command_pool: vk::CommandPool, flags: vk::CommandPoolResetFlags) {
        unsafe {
            self.device.reset_command_pool(command_pool, flags).unwrap();
        }
    }

    // events

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateEvent.html"]
    pub fn create_event(&mut self, create_info: &vk::EventCreateInfo) -> vk::Event {
        unsafe { self.device.create_event(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyEvent.html"]
    pub fn destroy_event(&mut self, event: vk::Event) {
        unsafe {
            self.device.destroy_event(event, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetEventStatus.html"]
    pub fn get_event_status(&mut self, event: vk::Event) -> bool {
        unsafe { self.device.get_event_status(event).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkSetEvent.html"]
    pub fn set_event(&mut self, event: vk::Event) {
        unsafe { self.device.set_event(event).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetEvent.html"]
    pub fn reset_event(&mut self, event: vk::Event) {
        unsafe {
            self.device.reset_event(event).unwrap();
        }
    }

    // queries

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateQueryPool.html"]
    pub fn create_query_pool(&mut self, create_info: &vk::QueryPoolCreateInfo) -> vk::QueryPool {
        unsafe { self.device.create_query_pool(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetQueryPoolResults.html"]
    pub fn get_query_pool_results<T>(
        &mut self,
        query_pool: vk::QueryPool,
        first_query: u32,
        query_count: u32,
        data: &mut [T],
        flags: vk::QueryResultFlags,
    ) {
        unsafe {
            self.device
                .get_query_pool_results(query_pool, first_query, query_count, data, flags)
                .unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyQueryPool.html"]
    pub fn destroy_query_pool(&mut self, pool: vk::QueryPool) {
        unsafe {
            self.device.destroy_query_pool(pool, None);
        }
    }

    // fences and semaphors

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateFence.html"]
    pub fn create_fence(&mut self, create_info: &vk::FenceCreateInfo) -> vk::Fence {
        unsafe { self.device.create_fence(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyFence.html"]
    pub fn destroy_fence(&mut self, fence: vk::Fence) {
        unsafe {
            self.device.destroy_fence(fence, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateSemaphore.html"]
    pub fn create_semaphore(&mut self, create_info: &vk::SemaphoreCreateInfo) -> vk::Semaphore {
        unsafe { self.device.create_semaphore(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroySemaphore.html"]
    pub fn destroy_semaphore(&mut self, semaphore: vk::Semaphore) {
        unsafe {
            self.device.destroy_semaphore(semaphore, None);
        }
    }

    // images and image views

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateImage.html"]
    pub fn create_image(&mut self, create_info: &vk::ImageCreateInfo) -> vk::Image {
        unsafe { self.device.create_image(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyImage.html"]
    pub fn destroy_image(&mut self, image: vk::Image) {
        unsafe {
            self.device.destroy_image(image, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkBindImageMemory.html"]
    pub fn bind_image_memory(&mut self, image: vk::Image, device_memory: vk::DeviceMemory, offset: vk::DeviceSize) {
        unsafe {
            self.device.bind_image_memory(image, device_memory, offset).unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetImageSubresourceLayout.html"]
    pub fn get_image_subresource_layout(
        &mut self,
        image: vk::Image,
        subresource: vk::ImageSubresource,
    ) -> vk::SubresourceLayout {
        unsafe { self.device.get_image_subresource_layout(image, subresource) }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetImageMemoryRequirements.html"]
    pub fn get_image_memory_requirements(&mut self, image: vk::Image) -> vk::MemoryRequirements {
        unsafe { self.device.get_image_memory_requirements(image) }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateImageView.html"]
    pub fn create_image_view(&mut self, create_info: &vk::ImageViewCreateInfo) -> vk::ImageView {
        unsafe { self.device.create_image_view(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyImageView.html"]
    pub fn destroy_image_view(&mut self, image_view: vk::ImageView) {
        unsafe {
            self.device.destroy_image_view(image_view, None);
        }
    }

    // buffers and buffer views

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateBuffer.html"]
    pub fn create_buffer(&mut self, create_info: &vk::BufferCreateInfo) -> vk::Buffer {
        unsafe { self.device.create_buffer(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyBuffer.html"]
    pub fn destroy_buffer(&mut self, buffer: vk::Buffer) {
        unsafe {
            self.device.destroy_buffer(buffer, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateBufferView.html"]
    pub fn create_buffer_view(&mut self, create_info: &vk::BufferViewCreateInfo) -> vk::BufferView {
        unsafe { self.device.create_buffer_view(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyBufferView.html"]
    pub fn destroy_buffer_view(&mut self, buffer_view: vk::BufferView) {
        unsafe {
            self.device.destroy_buffer_view(buffer_view, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetBufferMemoryRequirements.html"]
    pub fn get_buffer_memory_requirements(&mut self, buffer: vk::Buffer) -> vk::MemoryRequirements {
        unsafe { self.device.get_buffer_memory_requirements(buffer) }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkBindBufferMemory.html"]
    pub fn bind_buffer_memory(&mut self, buffer: vk::Buffer, device_memory: vk::DeviceMemory, offset: vk::DeviceSize) {
        unsafe {
            self.device.bind_buffer_memory(buffer, device_memory, offset).unwrap();
        }
    }

    // render passes and frame buffers

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateRenderPass.html"]
    pub fn create_render_pass(&mut self, create_info: &vk::RenderPassCreateInfo) -> vk::RenderPass {
        unsafe { self.device.create_render_pass(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyRenderPass.html"]
    pub fn destroy_render_pass(&mut self, renderpass: vk::RenderPass) {
        unsafe {
            self.device.destroy_render_pass(renderpass, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateFramebuffer.html"]
    pub fn create_framebuffer(&mut self, create_info: &vk::FramebufferCreateInfo) -> vk::Framebuffer {
        unsafe { self.device.create_framebuffer(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyFramebuffer.html"]
    pub fn destroy_framebuffer(&mut self, framebuffer: vk::Framebuffer) {
        unsafe {
            self.device.destroy_framebuffer(framebuffer, None);
        }
    }

    // pipelines

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreatePipelineLayout.html"]
    pub fn create_pipeline_layout(&mut self, create_info: &vk::PipelineLayoutCreateInfo) -> vk::PipelineLayout {
        unsafe { self.device.create_pipeline_layout(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyPipelineLayout.html"]
    pub fn destroy_pipeline_layout(&mut self, pipeline_layout: vk::PipelineLayout) {
        unsafe {
            self.device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreatePipelineCache.html"]
    pub fn create_pipeline_cache(&mut self, create_info: &vk::PipelineCacheCreateInfo) -> vk::PipelineCache {
        unsafe { self.device.create_pipeline_cache(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyPipelineCache.html"]
    pub fn destroy_pipeline_cache(&mut self, pipeline_cache: vk::PipelineCache) {
        unsafe {
            self.device.destroy_pipeline_cache(pipeline_cache, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetPipelineCacheData.html"]
    pub fn get_pipeline_cache_data(&mut self, pipeline_cache: vk::PipelineCache) -> Vec<u8> {
        unsafe { self.device.get_pipeline_cache_data(pipeline_cache).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateGraphicsPipelines.html"]
    pub fn create_graphics_pipelines(
        &mut self,
        pipeline_cache: vk::PipelineCache,
        create_infos: &[vk::GraphicsPipelineCreateInfo],
    ) -> Vec<vk::Pipeline> {
        unsafe {
            self.device
                .create_graphics_pipelines(pipeline_cache, create_infos, None)
                .unwrap()
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateComputePipelines.html"]
    pub fn create_compute_pipelines(
        &mut self,
        pipeline_cache: vk::PipelineCache,
        create_infos: &[vk::ComputePipelineCreateInfo],
    ) -> Vec<vk::Pipeline> {
        unsafe {
            self.device
                .create_compute_pipelines(pipeline_cache, create_infos, None)
                .unwrap()
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyPipeline.html"]
    pub fn destroy_pipeline(&mut self, pipeline: vk::Pipeline) {
        unsafe {
            self.device.destroy_pipeline(pipeline, None);
        }
    }

    // shader modules

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateShaderModule.html"]
    pub fn create_shader_module(&mut self, create_info: &vk::ShaderModuleCreateInfo) -> vk::ShaderModule {
        unsafe { self.device.create_shader_module(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyShaderModule.html"]
    pub fn destroy_shader_module(&mut self, shader: vk::ShaderModule) {
        unsafe {
            self.device.destroy_shader_module(shader, None);
        }
    }

    // descriptors and descriptor sets

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateDescriptorPool.html"]
    pub fn create_descriptor_pool(&mut self, create_info: &vk::DescriptorPoolCreateInfo) -> vk::DescriptorPool {
        unsafe { self.device.create_descriptor_pool(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyDescriptorPool.html"]
    pub fn destroy_descriptor_pool(&mut self, pool: vk::DescriptorPool) {
        unsafe {
            self.device.destroy_descriptor_pool(pool, None);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetDescriptorPool.html"]
    pub fn reset_descriptor_pool(&mut self, pool: vk::DescriptorPool) {
        unsafe {
            self.device.reset_descriptor_pool(pool, std::mem::transmute(0)).unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetDescriptorPool.html"]
    pub fn reset_descriptor_pool_with_flags(&mut self, pool: vk::DescriptorPool, flags: vk::DescriptorPoolResetFlags) {
        unsafe {
            self.device.reset_descriptor_pool(pool, flags).unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkAllocateDescriptorSets.html"]
    pub fn allocate_descriptor_sets(&mut self, create_info: &vk::DescriptorSetAllocateInfo) -> Vec<vk::DescriptorSet> {
        unsafe { self.device.allocate_descriptor_sets(create_info).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkFreeDescriptorSets.html"]
    pub fn free_descriptor_sets(&mut self, pool: vk::DescriptorPool, descriptor_sets: &[vk::DescriptorSet]) {
        unsafe {
            self.device.free_descriptor_sets(pool, descriptor_sets);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkUpdateDescriptorSets.html"]
    pub fn update_descriptor_sets(
        &mut self,
        descriptor_writes: &[vk::WriteDescriptorSet],
        descriptor_copies: &[vk::CopyDescriptorSet],
    ) {
        unsafe {
            self.device.update_descriptor_sets(descriptor_writes, descriptor_copies);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCreateDescriptorSetLayout.html"]
    pub fn create_descriptor_set_layout(
        &mut self,
        create_info: &vk::DescriptorSetLayoutCreateInfo,
    ) -> vk::DescriptorSetLayout {
        unsafe { self.device.create_descriptor_set_layout(create_info, None).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDestroyDescriptorSetLayout.html"]
    pub fn destroy_descriptor_set_layout(&mut self, layout: vk::DescriptorSetLayout) {
        unsafe {
            self.device.destroy_descriptor_set_layout(layout, None);
        }
    }
}
