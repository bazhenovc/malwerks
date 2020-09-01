// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

// This mod here contains a few important macros, so it has to be first in the import list
// As of today, macros in rust can only be used after they're defined, so we have to make sure
// that module import order stays consistent, because `cargo fmt` rearranges imported modules.
#[macro_use]
mod render_pass;

mod camera;
mod forward_pass;
mod gpu_profiler;
mod post_process;
mod render_world;
mod sky_box;
mod static_scenery;
mod upload_batch;

mod mesh_cluster_culling;
mod occluder_pass;
mod occluder_resolve;
mod shared_frame_data;

pub use camera::*;
pub use forward_pass::*;
pub use gpu_profiler::*;
pub use post_process::*;
pub use render_pass::*;
pub use render_world::*;
pub use sky_box::*;
pub use static_scenery::*;
pub use upload_batch::*;

pub use malwerks_resources::DiskGlobalResources;
pub use malwerks_vk::*;

pub use puffin;
pub use ultraviolet as utv;

#[cfg(test)]
mod test_render_passes;
