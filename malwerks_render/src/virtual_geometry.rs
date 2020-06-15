// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet as utv;

#[repr(C)]
pub struct Patch {
    vertices: [u16; 4],
    child_patches: [u16; 4],
}

pub struct Tile {
    patches: Vec<Patch>, // up to 16384 patches, 65536 vertices
    aabb_min: utv::vec::Vec3,
    aabb_max: utv::vec::Vec3,
}

pub struct VirtualGeometry {
    tiles: Vec<Tile>,

    patch_cache: std::collections::VecDeque<usize>,
    index_buffer_cache: Vec<u16>,
}

impl VirtualGeometry {
    pub fn update_patch_cache(&mut self) {
        //
    }

    pub fn tessellate_visible_patches(&mut self) {
        //
    }
}
