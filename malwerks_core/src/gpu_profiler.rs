// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use puffin::*;

#[derive(Default)]
pub struct GpuProfiler {
    stream: Stream,
    depth: usize,
    gpu_time_offset: i64,
}

impl GpuProfiler {
    pub fn begin_scope(&mut self, name: &'static str, timestamp_start: u64) -> usize {
        if self.gpu_time_offset == 0 {
            self.gpu_time_offset = (timestamp_start as i64) - puffin::now_ns();
        }
        self.depth += 1;
        self.stream
            .begin_scope((timestamp_start as i64) - self.gpu_time_offset, name, "", "")
    }

    pub fn end_scope(&mut self, start_offset: usize, timestamp_end: u64) {
        if self.depth > 0 {
            self.depth -= 1;
        } else {
            panic!("Mismatched scope begin/end calls");
        }
        self.stream
            .end_scope(start_offset, (timestamp_end as i64) - self.gpu_time_offset);
    }

    pub fn report_frame(&mut self) {
        assert_eq!(self.depth, 0, "Mismatched scope begin/end calls");
        self.gpu_time_offset = 0;
        let info = ThreadInfo {
            start_time_ns: None,
            name: "GPU".to_owned(),
        };
        let stream = std::mem::take(&mut self.stream);
        GlobalProfiler::lock().report(info, stream)
    }
}
