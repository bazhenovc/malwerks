// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

// This mod here contains a few important macros, so it has to be first in the import list
// As of today, macros in rust can only be used after they're defined, so we have to make sure
// that module import order stays consistent, because `cargo fmt` rearranges imported modules.
#[macro_use]
mod render_pass;

mod backbuffer_pass;
mod camera;
mod forward_pass;
mod post_process;
mod render_world;
mod shared_frame_data;
mod sky_box;
mod static_scenery;

pub use backbuffer_pass::*;
pub use camera::*;
pub use forward_pass::*;
pub use post_process::*;
pub use render_pass::*;
pub use render_world::*;
pub use shared_frame_data::*;
pub use sky_box::*;
pub use static_scenery::*;

pub use malwerks_vk::*;

pub use microprofile;
pub use ultraviolet as utv;

// TODO: make crate-local
mod internal;
pub use internal::upload_image_memory;

#[macro_export]
macro_rules! include_spirv {
    ($path: expr) => {
        unsafe {
            let bytes = include_bytes!(concat!(env!("OUT_DIR"), $path));
            std::slice::from_raw_parts(bytes.as_ptr() as *const u32, bytes.len() / 4)
        }
    };
}
