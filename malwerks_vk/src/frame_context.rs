// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub const NUM_BUFFERED_GPU_FRAMES: usize = 3;

pub struct FrameContext {
    pub(crate) current_gpu_frame: usize,
}

impl FrameContext {
    pub(crate) fn new(current_gpu_frame: usize) -> Self {
        Self { current_gpu_frame }
    }

    pub fn current_gpu_frame(&self) -> usize {
        self.current_gpu_frame
    }
}
