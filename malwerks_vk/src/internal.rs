// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ash::vk;

pub(crate) struct AshStatic {
    pub fp_10: vk::DeviceFnV1_0,
    pub fp_11: vk::DeviceFnV1_1,
    pub draw_indirect_count: vk::KhrDrawIndirectCountFn,
    pub ray_tracing_nv: vk::NvRayTracingFn,
}

static mut ASH_STATIC: Option<AshStatic> = None;

#[inline]
pub(crate) unsafe fn ash_static() -> &'static AshStatic {
    ASH_STATIC.as_ref().unwrap()
}

pub(crate) unsafe fn ash_static_init(
    fp_10: vk::DeviceFnV1_0,
    fp_11: vk::DeviceFnV1_1,
    draw_indirect_count: vk::KhrDrawIndirectCountFn,
    ray_tracing_nv: vk::NvRayTracingFn,
) {
    match ASH_STATIC {
        None => {
            ASH_STATIC = Some(AshStatic {
                fp_10,
                fp_11,
                draw_indirect_count,
                ray_tracing_nv,
            });
        }
        Some(_) => panic!("ash static data initialized twice"),
    }
}
