// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::internal::*;

use ash::vk;

#[repr(transparent)]
#[derive(Copy, Clone)]
#[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/VkCommandBuffer.html"]
pub struct CommandBuffer(vk::CommandBuffer);

impl vk::Handle for CommandBuffer {
    const TYPE: vk::ObjectType = vk::CommandBuffer::TYPE;

    fn as_raw(self) -> u64 {
        self.0.as_raw()
    }

    fn from_raw(raw: u64) -> Self {
        CommandBuffer(vk::CommandBuffer::from_raw(raw))
    }
}

impl From<CommandBuffer> for vk::CommandBuffer {
    fn from(item: CommandBuffer) -> vk::CommandBuffer {
        item.0
    }
}

impl CommandBuffer {
    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetCommandBuffer.html"]
    pub fn reset(&mut self) {
        unsafe {
            let error_code = ash_static().fp_10.reset_command_buffer(self.0, std::mem::transmute(0));
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("reset_command_buffer() failed: {:?}", error_code),
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetCommandBuffer.html"]
    pub fn reset_with_flags(&mut self, flags: vk::CommandBufferResetFlags) {
        unsafe {
            let error_code = ash_static().fp_10.reset_command_buffer(self.0, flags);
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("reset_command_buffer() failed: {:?}", error_code),
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkBeginCommandBuffer.html"]
    pub fn begin(&mut self, begin_info: &vk::CommandBufferBeginInfo) {
        unsafe {
            let error_code = ash_static().fp_10.begin_command_buffer(self.0, begin_info);
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("begin_command_buffer() failed: {:?}", error_code),
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkEndCommandBuffer.html"]
    pub fn end(&mut self) {
        unsafe {
            let error_code = ash_static().fp_10.end_command_buffer(self.0);
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("end_command_buffer() failed: {:?}", error_code),
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetEvent.html"]
    pub fn set_event(&mut self, event: vk::Event, stage_mask: vk::PipelineStageFlags) {
        unsafe {
            ash_static().fp_10.cmd_set_event(self.0, event, stage_mask);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdWaitEvents.html"]
    pub fn wait_events(
        &mut self,
        events: &[vk::Event],
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
        memory_barriers: &[vk::MemoryBarrier],
        buffer_memory_barriers: &[vk::BufferMemoryBarrier],
        image_memory_barriers: &[vk::ImageMemoryBarrier],
    ) {
        unsafe {
            ash_static().fp_10.cmd_wait_events(
                self.0,
                events.len() as _,
                events.as_ptr(),
                src_stage_mask,
                dst_stage_mask,
                memory_barriers.len() as _,
                memory_barriers.as_ptr(),
                buffer_memory_barriers.len() as _,
                buffer_memory_barriers.as_ptr(),
                image_memory_barriers.len() as _,
                image_memory_barriers.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBindIndexBuffer.html"]
    pub fn bind_index_buffer(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize, index_type: vk::IndexType) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_bind_index_buffer(self.0, buffer, offset, index_type);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdClearColorImage.html"]
    pub fn clear_color_image(
        &mut self,
        image: vk::Image,
        image_layout: vk::ImageLayout,
        clear_color_value: &vk::ClearColorValue,
        ranges: &[vk::ImageSubresourceRange],
    ) {
        unsafe {
            ash_static().fp_10.cmd_clear_color_image(
                self.0,
                image,
                image_layout,
                clear_color_value,
                ranges.len() as _,
                ranges.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdClearDepthStencilImage.html"]
    pub fn clear_depth_stencil_image(
        &mut self,
        image: vk::Image,
        image_layout: vk::ImageLayout,
        clear_depth_stencil_value: vk::ClearDepthStencilValue,
        ranges: &[vk::ImageSubresourceRange],
    ) {
        unsafe {
            ash_static().fp_10.cmd_clear_depth_stencil_image(
                self.0,
                image,
                image_layout,
                &clear_depth_stencil_value,
                ranges.len() as _,
                ranges.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdClearAttachments.html"]
    pub fn clear_attachments(&mut self, attachments: &[vk::ClearAttachment], rects: &[vk::ClearRect]) {
        unsafe {
            ash_static().fp_10.cmd_clear_attachments(
                self.0,
                attachments.len() as _,
                attachments.as_ptr(),
                rects.len() as _,
                rects.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDrawIndexed.html"]
    pub fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            ash_static().fp_10.cmd_draw_indexed(
                self.0,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDrawIndexedIndirect.html"]
    pub fn draw_indexed_indirect(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize, draw_count: u32, stride: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_draw_indexed_indirect(self.0, buffer, offset, draw_count, stride);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdExecuteCommands.html"]
    pub fn execute_commands(&mut self, secondary_command_buffers: &[CommandBuffer]) {
        unsafe {
            ash_static().fp_10.cmd_execute_commands(
                self.0,
                secondary_command_buffers.len() as _,
                secondary_command_buffers.as_ptr() as _,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBindDescriptorSets.html"]
    pub fn bind_descriptor_sets(
        &mut self,
        pipeline_bind_point: vk::PipelineBindPoint,
        layout: vk::PipelineLayout,
        first_set: u32,
        descriptor_sets: &[vk::DescriptorSet],
        dynamic_offsets: &[u32],
    ) {
        unsafe {
            ash_static().fp_10.cmd_bind_descriptor_sets(
                self.0,
                pipeline_bind_point,
                layout,
                first_set,
                descriptor_sets.len() as _,
                descriptor_sets.as_ptr(),
                dynamic_offsets.len() as _,
                dynamic_offsets.as_ptr(),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyQueryPoolResults.html"]
    pub fn copy_query_pool_results(
        &mut self,
        query_pool: vk::QueryPool,
        first_query: u32,
        query_count: u32,
        dst_buffer: vk::Buffer,
        dst_offset: vk::DeviceSize,
        stride: vk::DeviceSize,
        flags: vk::QueryResultFlags,
    ) {
        unsafe {
            ash_static().fp_10.cmd_copy_query_pool_results(
                self.0,
                query_pool,
                first_query,
                query_count,
                dst_buffer,
                dst_offset,
                stride,
                flags,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdPushConstants.html"]
    pub fn push_constants<T>(
        &mut self,
        layout: vk::PipelineLayout,
        stage_flags: vk::ShaderStageFlags,
        offset: u32,
        constants: &[T],
    ) {
        unsafe {
            ash_static().fp_10.cmd_push_constants(
                self.0,
                layout,
                stage_flags,
                offset,
                (constants.len() * std::mem::size_of::<T>()) as _,
                constants.as_ptr() as _,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBeginRenderPass.html"]
    pub fn begin_render_pass(&mut self, begin_info: &vk::RenderPassBeginInfo, contents: vk::SubpassContents) {
        unsafe {
            ash_static().fp_10.cmd_begin_render_pass(self.0, begin_info, contents);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdNextSubpass.html"]
    pub fn next_subpass(&mut self, contents: vk::SubpassContents) {
        unsafe {
            ash_static().fp_10.cmd_next_subpass(self.0, contents);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBindPipeline.html"]
    pub fn bind_pipeline(&mut self, pipeline_bind_point: vk::PipelineBindPoint, pipeline: vk::Pipeline) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_bind_pipeline(self.0, pipeline_bind_point, pipeline);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetScissor.html"]
    pub fn set_scissor(&mut self, first_scissor: u32, scissors: &[vk::Rect2D]) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_scissor(self.0, first_scissor, scissors.len() as _, scissors.as_ptr());
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetLineWidth.html"]
    pub fn set_line_width(&mut self, line_width: f32) {
        unsafe {
            ash_static().fp_10.cmd_set_line_width(self.0, line_width);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBindVertexBuffers.html"]
    pub fn bind_vertex_buffers(&mut self, first_binding: u32, buffers: &[vk::Buffer], offsets: &[vk::DeviceSize]) {
        unsafe {
            ash_static().fp_10.cmd_bind_vertex_buffers(
                self.0,
                first_binding,
                buffers.len() as _,
                buffers.as_ptr(),
                offsets.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdEndRenderPass.html"]
    pub fn end_render_pass(&mut self) {
        unsafe {
            ash_static().fp_10.cmd_end_render_pass(self.0);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDraw.html"]
    pub fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_draw(self.0, vertex_count, instance_count, first_vertex, first_instance);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDrawIndirect.html"]
    pub fn draw_indirect(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize, draw_count: u32, stride: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_draw_indirect(self.0, buffer, offset, draw_count, stride);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDispatch.html"]
    pub fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_dispatch(self.0, group_count_x, group_count_y, group_count_z);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdDispatchIndirect.html"]
    pub fn dispatch_indirect(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize) {
        unsafe {
            ash_static().fp_10.cmd_dispatch_indirect(self.0, buffer, offset);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetViewport.html"]
    pub fn set_viewport(&mut self, first_viewport: u32, viewports: &[vk::Viewport]) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_viewport(self.0, first_viewport, viewports.len() as _, viewports.as_ptr());
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetDepthBias.html"]
    pub fn set_depth_bias(&mut self, constant_factor: f32, clamp: f32, slope_factor: f32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_depth_bias(self.0, constant_factor, clamp, slope_factor);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetBlendConstants.html"]
    pub fn set_blend_constants(&mut self, blend_constants: &[f32; 4]) {
        unsafe {
            ash_static().fp_10.cmd_set_blend_constants(self.0, blend_constants);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetDepthBounds.html"]
    pub fn set_depth_bounds(&mut self, min_depth_bounds: f32, max_depth_bounds: f32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_depth_bounds(self.0, min_depth_bounds, max_depth_bounds);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetStencilCompareMask.html"]
    pub fn set_stencil_compare_mask(&mut self, face_mask: vk::StencilFaceFlags, compare_mask: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_stencil_compare_mask(self.0, face_mask, compare_mask);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetStencilWriteMask.html"]
    pub fn set_stencil_write_mask(&mut self, face_mask: vk::StencilFaceFlags, write_mask: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_stencil_write_mask(self.0, face_mask, write_mask);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdSetStencilReference.html"]
    pub fn set_stencil_reference(&mut self, face_mask: vk::StencilFaceFlags, reference: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_set_stencil_reference(self.0, face_mask, reference);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBeginQuery.html"]
    pub fn begin_query(&mut self, query_pool: vk::QueryPool, query: u32, flags: vk::QueryControlFlags) {
        unsafe {
            ash_static().fp_10.cmd_begin_query(self.0, query_pool, query, flags);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdEndQuery.html"]
    pub fn end_query(&mut self, query_pool: vk::QueryPool, query: u32) {
        unsafe {
            ash_static().fp_10.cmd_end_query(self.0, query_pool, query);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdResetQueryPool.html"]
    pub fn reset_query_pool(&mut self, pool: vk::QueryPool, first_query: u32, query_count: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_reset_query_pool(self.0, pool, first_query, query_count);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdWriteTimestamp.html"]
    pub fn write_timestamp(&mut self, pipeline_stage: vk::PipelineStageFlags, query_pool: vk::QueryPool, query: u32) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_write_timestamp(self.0, pipeline_stage, query_pool, query);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdPipelineBarrier.html"]
    pub fn pipeline_barrier(
        &mut self,
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
        dependency_flags: Option<vk::DependencyFlags>,
        memory_barriers: &[vk::MemoryBarrier],
        buffer_memory_barriers: &[vk::BufferMemoryBarrier],
        image_memory_barriers: &[vk::ImageMemoryBarrier],
    ) {
        unsafe {
            ash_static().fp_10.cmd_pipeline_barrier(
                self.0,
                src_stage_mask,
                dst_stage_mask,
                dependency_flags.unwrap_or_else(|| std::mem::transmute(0)),
                memory_barriers.len() as _,
                memory_barriers.as_ptr(),
                buffer_memory_barriers.len() as _,
                buffer_memory_barriers.as_ptr(),
                image_memory_barriers.len() as _,
                image_memory_barriers.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBlitImage.html"]
    pub fn blit_image(
        &mut self,
        src_image: vk::Image,
        src_image_layout: vk::ImageLayout,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        regions: &[vk::ImageBlit],
        filter: vk::Filter,
    ) {
        unsafe {
            ash_static().fp_10.cmd_blit_image(
                self.0,
                src_image,
                src_image_layout,
                dst_image,
                dst_image_layout,
                regions.len() as _,
                regions.as_ptr(),
                filter,
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdResolveImage.html"]
    pub fn resolve_image(
        &mut self,
        src_image: vk::Image,
        src_image_layout: vk::ImageLayout,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        regions: &[vk::ImageResolve],
    ) {
        unsafe {
            ash_static().fp_10.cmd_resolve_image(
                self.0,
                src_image,
                src_image_layout,
                dst_image,
                dst_image_layout,
                regions.len() as _,
                regions.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdFillBuffer.html"]
    pub fn fill_buffer(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize, size: vk::DeviceSize, data: u32) {
        unsafe {
            ash_static().fp_10.cmd_fill_buffer(self.0, buffer, offset, size, data);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdUpdateBuffer.html"]
    pub fn update_buffer(&mut self, buffer: vk::Buffer, offset: vk::DeviceSize, data: &[u8]) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_update_buffer(self.0, buffer, offset, data.len() as _, data.as_ptr() as _);
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyBuffer.html"]
    pub fn copy_buffer(&mut self, src_buffer: vk::Buffer, dst_buffer: vk::Buffer, regions: &[vk::BufferCopy]) {
        unsafe {
            ash_static()
                .fp_10
                .cmd_copy_buffer(self.0, src_buffer, dst_buffer, regions.len() as _, regions.as_ptr());
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyImageToBuffer.html"]
    pub fn copy_image_to_buffer(
        &mut self,
        src_image: vk::Image,
        src_image_layout: vk::ImageLayout,
        dst_buffer: vk::Buffer,
        regions: &[vk::BufferImageCopy],
    ) {
        unsafe {
            ash_static().fp_10.cmd_copy_image_to_buffer(
                self.0,
                src_image,
                src_image_layout,
                dst_buffer,
                regions.len() as _,
                regions.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyBufferToImage.html"]
    pub fn copy_buffer_to_image(
        &mut self,
        src_buffer: vk::Buffer,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        regions: &[vk::BufferImageCopy],
    ) {
        unsafe {
            ash_static().fp_10.cmd_copy_buffer_to_image(
                self.0,
                src_buffer,
                dst_image,
                dst_image_layout,
                regions.len() as _,
                regions.as_ptr(),
            );
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyImage.html"]
    pub fn copy_image(
        &mut self,
        src_image: vk::Image,
        src_image_layout: vk::ImageLayout,
        dst_image: vk::Image,
        dst_image_layout: vk::ImageLayout,
        regions: &[vk::ImageCopy],
    ) {
        unsafe {
            ash_static().fp_10.cmd_copy_image(
                self.0,
                src_image,
                src_image_layout,
                dst_image,
                dst_image_layout,
                regions.len() as _,
                regions.as_ptr(),
            );
        }
    }
}

// ray tracing nv

impl CommandBuffer {
    #[allow(clippy::too_many_arguments)]
    #[doc = "<https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdBuildAccelerationStructureNV.html>"]
    pub fn build_acceleration_structure_nv(
        &mut self,
        info: &vk::AccelerationStructureInfoNV,
        instance_data: vk::Buffer,
        instance_offset: vk::DeviceSize,
        update: bool,
        dst: vk::AccelerationStructureNV,
        src: vk::AccelerationStructureNV,
        scratch: vk::Buffer,
        scratch_offset: vk::DeviceSize,
    ) {
        unsafe {
            ash_static().ray_tracing_nv.cmd_build_acceleration_structure_nv(
                self.0,
                info,
                instance_data,
                instance_offset,
                if update { vk::TRUE } else { vk::FALSE },
                dst,
                src,
                scratch,
                scratch_offset,
            );
        }
    }

    #[doc = "<https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdCopyAccelerationStructureNV.html>"]
    pub fn copy_acceleration_structure_nv(
        &mut self,
        dst: vk::AccelerationStructureNV,
        src: vk::AccelerationStructureNV,
        mode: vk::CopyAccelerationStructureModeNV,
    ) {
        unsafe {
            ash_static()
                .ray_tracing_nv
                .cmd_copy_acceleration_structure_nv(self.0, dst, src, mode);
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[doc = "<https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdTraceRaysNV.html>"]
    pub fn trace_rays_nv(
        &mut self,
        raygen_shader_binding_table_buffer: vk::Buffer,
        raygen_shader_binding_offset: vk::DeviceSize,
        miss_shader_binding_table_buffer: vk::Buffer,
        miss_shader_binding_offset: vk::DeviceSize,
        miss_shader_binding_stride: vk::DeviceSize,
        hit_shader_binding_table_buffer: vk::Buffer,
        hit_shader_binding_offset: vk::DeviceSize,
        hit_shader_binding_stride: vk::DeviceSize,
        callable_shader_binding_table_buffer: vk::Buffer,
        callable_shader_binding_offset: vk::DeviceSize,
        callable_shader_binding_stride: vk::DeviceSize,
        width: u32,
        height: u32,
        depth: u32,
    ) {
        unsafe {
            ash_static().ray_tracing_nv.cmd_trace_rays_nv(
                self.0,
                raygen_shader_binding_table_buffer,
                raygen_shader_binding_offset,
                miss_shader_binding_table_buffer,
                miss_shader_binding_offset,
                miss_shader_binding_stride,
                hit_shader_binding_table_buffer,
                hit_shader_binding_offset,
                hit_shader_binding_stride,
                callable_shader_binding_table_buffer,
                callable_shader_binding_offset,
                callable_shader_binding_stride,
                width,
                height,
                depth,
            );
        }
    }

    #[doc = "<https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkCmdWriteAccelerationStructuresPropertiesNV.html>"]
    pub fn write_acceleration_structures_properties_nv(
        &mut self,
        structures: &[vk::AccelerationStructureNV],
        query_type: vk::QueryType,
        query_pool: vk::QueryPool,
        first_query: u32,
    ) {
        unsafe {
            ash_static()
                .ray_tracing_nv
                .cmd_write_acceleration_structures_properties_nv(
                    self.0,
                    structures.len() as u32,
                    structures.as_ptr(),
                    query_type,
                    query_pool,
                    first_query,
                );
        }
    }
}
