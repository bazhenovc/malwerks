// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::frame_context::*;

pub struct FrameLocal<T> {
    frame_resources: [T; NUM_BUFFERED_GPU_FRAMES],
}

impl<T> FrameLocal<T> {
    pub fn new<F>(mut closure: F) -> Self
    where
        F: FnMut(usize) -> T,
    {
        // TODO: this is stupid, make a proper macro or smth
        assert_eq!(NUM_BUFFERED_GPU_FRAMES, 3);
        Self {
            frame_resources: [closure(0), closure(1), closure(2)],
        }
    }

    pub fn destroy<F>(&mut self, mut closure: F)
    where
        F: FnMut(&T),
    {
        for resource in &mut self.frame_resources {
            closure(resource);
        }
    }

    pub fn get_frame(&self, frame: usize) -> &T {
        &self.frame_resources[frame]
    }

    pub fn get(&self, frame_context: &FrameContext) -> &T {
        &self.frame_resources[frame_context.current_gpu_frame()]
    }

    pub fn get_mut(&mut self, frame_context: &FrameContext) -> &mut T {
        &mut self.frame_resources[frame_context.current_gpu_frame()]
    }
}
