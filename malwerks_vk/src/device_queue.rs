// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::internal::*;

use ash::vk;

#[repr(transparent)]
#[derive(Copy, Clone)]
#[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/VkQueue.html"]
pub struct DeviceQueue(pub(crate) vk::Queue);

impl From<DeviceQueue> for vk::Queue {
    fn from(item: DeviceQueue) -> vk::Queue {
        item.0
    }
}

impl DeviceQueue {
    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkQueueWaitIdle.html"]
    pub fn wait_idle(&mut self) {
        unsafe {
            let error_code = ash_static().fp_10.queue_wait_idle(self.0);
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("queue_submit() failed"),
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkQueueSubmit.html"]
    pub fn submit(&mut self, submits: &[vk::SubmitInfo], fence: vk::Fence) {
        unsafe {
            let error_code = ash_static()
                .fp_10
                .queue_submit(self.0, submits.len() as _, submits.as_ptr(), fence);
            match error_code {
                vk::Result::SUCCESS => {}
                _ => panic!("queue_submit() failed"),
            }
        }
    }
}
