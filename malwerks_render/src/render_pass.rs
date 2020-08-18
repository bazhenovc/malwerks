// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

pub trait RenderPass {
    fn begin(
        &mut self,
        frame_context: &FrameContext,
        device: &mut Device,
        factory: &mut DeviceFactory,
        render_area: vk::Rect2D,
    );
    fn end(&mut self, frame_context: &FrameContext);

    fn submit_commands(&mut self, frame_context: &FrameContext, queue: &mut DeviceQueue);

    fn get_signal_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore;
    fn get_signal_fence(&self, frame_context: &FrameContext) -> vk::Fence;

    fn get_render_pass(&self) -> vk::RenderPass;
    fn get_framebuffer(&self, frame_context: &FrameContext) -> vk::Framebuffer;
    fn get_command_buffer(&mut self, frame_context: &FrameContext) -> &mut CommandBuffer;

    fn add_wait_condition(&mut self, semaphore: vk::Semaphore, stage_mask: vk::PipelineStageFlags);

    fn try_get_oldest_timestamp(&self, frame_context: &FrameContext, factory: &mut DeviceFactory) -> Option<[u64; 2]>;

    fn destroy(&mut self, factory: &mut DeviceFactory);

    fn add_dependency<T>(&mut self, frame_context: &FrameContext, pass: &T, stage_mask: vk::PipelineStageFlags)
    where
        T: RenderPass,
    {
        self.add_wait_condition(pass.get_signal_semaphore(frame_context), stage_mask);
    }
}

#[macro_export]
macro_rules! default_render_pass_impl {
    ($pass_type: ty, $proxy_member: ident) => {
        impl RenderPass for $pass_type {
            fn begin(
                &mut self,
                frame_context: &FrameContext,
                device: &mut Device,
                factory: &mut DeviceFactory,
                render_area: vk::Rect2D,
            ) {
                self.$proxy_member
                    .begin(frame_context, device, factory, render_area);
            }

            fn end(&mut self, frame_context: &FrameContext) {
                self.$proxy_member.end(frame_context);
            }

            fn submit_commands(&mut self, frame_context: &FrameContext, queue: &mut DeviceQueue) {
                self.$proxy_member.submit_commands(frame_context, queue);
            }

            fn get_signal_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
                self.$proxy_member.get_signal_semaphore(frame_context)
            }

            fn get_signal_fence(&self, frame_context: &FrameContext) -> vk::Fence {
                self.$proxy_member.get_signal_fence(frame_context)
            }

            fn get_render_pass(&self) -> vk::RenderPass {
                self.$proxy_member.get_render_pass()
            }

            fn get_framebuffer(&self, frame_context: &FrameContext) -> vk::Framebuffer {
                self.$proxy_member.get_framebuffer(frame_context)
            }

            fn get_command_buffer(&mut self, frame_context: &FrameContext) -> &mut CommandBuffer {
                self.$proxy_member.get_command_buffer(frame_context)
            }

            fn add_wait_condition(&mut self, semaphore: vk::Semaphore, stage_mask: vk::PipelineStageFlags) {
                self.$proxy_member.add_wait_condition(semaphore, stage_mask);
            }

            fn try_get_oldest_timestamp(
                &self,
                frame_context: &FrameContext,
                factory: &mut DeviceFactory,
            ) -> Option<[u64; 2]> {
                self.$proxy_member
                    .try_get_oldest_timestamp(frame_context, factory)
            }

            fn destroy(&mut self, factory: &mut DeviceFactory) {
                self.$proxy_member.destroy(factory);
                self.destroy_internal(factory);
            }
        }
    };
}

pub struct BaseRenderPass {
    render_pass: vk::RenderPass,
    framebuffer: FrameLocal<vk::Framebuffer>,
    command_pool: FrameLocal<vk::CommandPool>,
    command_buffer: FrameLocal<CommandBuffer>,
    signal_semaphore: FrameLocal<vk::Semaphore>,
    signal_fence: FrameLocal<vk::Fence>,
    wait_semaphores: Vec<vk::Semaphore>,
    wait_stage_mask: Vec<vk::PipelineStageFlags>,
    clear_values: Vec<vk::ClearValue>,
    timestamp_query_pool: vk::QueryPool,
}

impl BaseRenderPass {
    pub fn new(
        device: &Device,
        factory: &mut DeviceFactory,
        render_pass: vk::RenderPass,
        framebuffer: FrameLocal<vk::Framebuffer>,
        clear_values: Vec<vk::ClearValue>,
    ) -> Self {
        let command_pool = FrameLocal::new(|_| {
            factory.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(device.get_graphics_queue_index())
                    .build(),
            )
        });
        let command_buffer = FrameLocal::new(|f| {
            factory.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(*command_pool.get_frame(f))
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build(),
            )[0]
        });
        let signal_semaphore = FrameLocal::new(|_| factory.create_semaphore(&vk::SemaphoreCreateInfo::default()));
        let signal_fence = FrameLocal::new(|_| {
            factory.create_fence(
                &vk::FenceCreateInfo::builder()
                    .flags(vk::FenceCreateFlags::SIGNALED)
                    .build(),
            )
        });

        let timestamp_query_pool = factory.create_query_pool(
            &vk::QueryPoolCreateInfo::builder()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count((2 * NUM_BUFFERED_GPU_FRAMES) as _)
                .build(),
        );

        Self {
            render_pass,
            framebuffer,
            command_pool,
            command_buffer,
            signal_semaphore,
            signal_fence,
            wait_semaphores: Vec::new(),
            wait_stage_mask: Vec::new(),
            clear_values,
            timestamp_query_pool,
        }
    }
}

impl RenderPass for BaseRenderPass {
    fn begin(
        &mut self,
        frame_context: &FrameContext,
        device: &mut Device,
        factory: &mut DeviceFactory,
        render_area: vk::Rect2D,
    ) {
        let signal_fence = self.get_signal_fence(frame_context);
        device.reset_fences(&[signal_fence]);

        let command_pool = self.command_pool.get(frame_context);
        factory.reset_command_pool(*command_pool);

        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.begin(
            &vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                //.inheritance_info(...)
                .build(),
        );
        let start_pass_query = frame_context.current_gpu_frame() * 2;
        command_buffer.reset_query_pool(self.timestamp_query_pool, start_pass_query as _, 2);

        command_buffer.write_timestamp(
            vk::PipelineStageFlags::ALL_GRAPHICS,
            self.timestamp_query_pool,
            start_pass_query as _,
        );
        command_buffer.begin_render_pass(
            &vk::RenderPassBeginInfo::builder()
                .render_pass(self.render_pass)
                .framebuffer(*self.framebuffer.get(frame_context))
                .render_area(render_area)
                .clear_values(&self.clear_values)
                .build(),
            vk::SubpassContents::INLINE,
        );
    }

    fn end(&mut self, frame_context: &FrameContext) {
        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.end_render_pass();

        let end_pass_query = frame_context.current_gpu_frame() * 2 + 1;
        command_buffer.write_timestamp(
            vk::PipelineStageFlags::ALL_GRAPHICS,
            self.timestamp_query_pool,
            end_pass_query as _,
        );
    }

    fn submit_commands(&mut self, frame_context: &FrameContext, queue: &mut DeviceQueue) {
        let signal_semaphore = self.get_signal_semaphore(frame_context);
        let signal_fence = self.get_signal_fence(frame_context);

        let command_buffer = self.command_buffer.get_mut(frame_context);
        command_buffer.end();

        queue.submit(
            &[vk::SubmitInfo::builder()
                .wait_semaphores(&self.wait_semaphores)
                .wait_dst_stage_mask(&self.wait_stage_mask)
                .signal_semaphores(&[signal_semaphore])
                .command_buffers(&[command_buffer.clone().into()])
                .build()],
            signal_fence,
        );

        self.wait_semaphores.clear();
        self.wait_stage_mask.clear();
    }

    fn get_signal_semaphore(&self, frame_context: &FrameContext) -> vk::Semaphore {
        *self.signal_semaphore.get(frame_context)
    }

    fn get_signal_fence(&self, frame_context: &FrameContext) -> vk::Fence {
        *self.signal_fence.get(frame_context)
    }

    fn get_render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    fn get_framebuffer(&self, frame_context: &FrameContext) -> vk::Framebuffer {
        *self.framebuffer.get(frame_context)
    }

    fn get_command_buffer(&mut self, frame_context: &FrameContext) -> &mut CommandBuffer {
        self.command_buffer.get_mut(frame_context)
    }

    fn add_wait_condition(&mut self, semaphore: vk::Semaphore, stage_mask: vk::PipelineStageFlags) {
        self.wait_semaphores.push(semaphore);
        self.wait_stage_mask.push(stage_mask);
    }

    fn try_get_oldest_timestamp(&self, frame_context: &FrameContext, factory: &mut DeviceFactory) -> Option<[u64; 2]> {
        let oldest_frame = (frame_context.current_gpu_frame() + NUM_BUFFERED_GPU_FRAMES) % NUM_BUFFERED_GPU_FRAMES;
        let mut data = [0u64; 2];
        let result = factory.get_query_pool_results(
            self.timestamp_query_pool,
            (oldest_frame * 2) as _,
            2,
            &mut data,
            vk::QueryResultFlags::TYPE_64,
        );
        match result {
            Ok(_) => Some(data),
            Err(code) => match code {
                vk::Result::NOT_READY => None,
                _ => panic!("try_get_oldest_timestamp(): Internal GPU error: {}", code),
            },
        }
    }

    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_render_pass(self.render_pass);
        self.framebuffer.destroy(|res| factory.destroy_framebuffer(*res));
        self.command_pool.destroy(|res| factory.destroy_command_pool(*res));
        self.signal_semaphore.destroy(|res| factory.destroy_semaphore(*res));
        self.signal_fence.destroy(|res| factory.destroy_fence(*res));
    }
}
