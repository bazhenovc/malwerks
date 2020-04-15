pub const NUM_BUFFERED_GPU_FRAMES: usize = 3;

pub struct FrameContext {
    current_gpu_frame: usize,
}

impl FrameContext {
    pub(crate) fn new(current_gpu_frame: usize) -> Self {
        Self { current_gpu_frame }
    }

    pub fn current_gpu_frame(&self) -> usize {
        self.current_gpu_frame
    }
}
