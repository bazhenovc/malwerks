// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DiskSampler {
    pub mag_filter: i32,     // vk::Filter pretending to be i32
    pub min_filter: i32,     // vk::Filter pretending to be i32
    pub mipmap_mode: i32,    // vk::SamplerMipmapMode pretending to be i32
    pub address_mode_u: i32, // vk::SamplerAddressMode pretending to be i32
    pub address_mode_v: i32, // vk::SamplerAddressMode pretending to be i32
    pub address_mode_w: i32, // vk::SamplerAddressMode pretending to be i32
}

#[derive(Serialize, Deserialize)]
pub struct DiskImage {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub block_size: usize,
    pub mipmap_count: usize,
    pub layer_count: usize,
    pub image_type: i32, // vk::ImageType pretending to be i32
    pub view_type: i32,  // vk::ImageViewType pretending to be i32
    pub format: i32,     // vk::Format pretending to be i32
    pub pixels: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskShader {
    pub name: String,
    pub bytecode: Vec<u8>,
    pub enabled_features: String, // Semicolon-separated list of values
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterialLayout {
    pub image_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterialInstance {
    pub material_layout: usize,
    pub material_data: Vec<u8>,      // arbitrary material data that goes into push constants
    pub images: Vec<(usize, usize)>, // (texture_id, sampler_id)
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterial {
    pub material_layout: usize,
    pub vertex_stride: u64,
    pub vertex_format: Vec<(i32, u32, usize)>, // vk::Format pretending to be i32, location, offset
    pub vertex_stage: Vec<u32>,
    pub fragment_stage: Vec<u32>,
    //pub fragment_alpha_discard: bool,
    pub ray_closest_hit_stage: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskBuffer {
    pub data: Vec<u8>,
    pub stride: u64,
    pub usage_flags: u32, // vk::BufferUsageFlags pretending to be i32
}

#[derive(Serialize, Deserialize)]
pub struct DiskMesh {
    pub vertex_buffer: usize,
    pub vertex_count: u32,
    pub vertex_stride: u64,

    pub index_buffer: Option<(usize, i32)>, // buffer_id, vk::IndexType pretending to be i32
    pub index_count: u32,

    pub bounding_box: ([f32; 3], [f32; 3]),
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderInstance {
    pub mesh: usize,
    pub material_instance: usize,
    pub transforms: Vec<[f32; 16]>,
    pub bounding_boxes: Vec<([f32; 3], [f32; 3])>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderBucket {
    pub material: usize,
    pub instances: Vec<DiskRenderInstance>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskEnvironmentProbe {
    pub skybox_image: usize,
    pub skybox_vertex_stage: Vec<u32>,
    pub skybox_fragment_stage: Vec<u32>,

    pub iem_image: usize,
    pub pmrem_image: usize,
    pub precomputed_brdf_image: usize,

    pub ray_gen_stage: Vec<u32>,
    pub ray_miss_stage: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskStaticScenery {
    pub buffers: Vec<DiskBuffer>,
    pub meshes: Vec<DiskMesh>,
    pub images: Vec<DiskImage>,
    pub samplers: Vec<DiskSampler>,
    pub material_layouts: Vec<DiskMaterialLayout>,
    pub material_instances: Vec<DiskMaterialInstance>,
    pub materials: Vec<DiskMaterial>,
    pub buckets: Vec<DiskRenderBucket>,

    // TODO: make this less specific
    pub environment_probes: Vec<DiskEnvironmentProbe>,
}
