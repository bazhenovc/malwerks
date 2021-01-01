// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod resource_compression;

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

    #[serde(with = "resource_compression")]
    pub pixels: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterialLayout {
    pub image_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterialInstance {
    pub material_layout: usize,
    pub material_instance_data: Vec<u8>, // arbitrary material data that goes into push constants
    pub images: Vec<(usize, usize)>,     // (texture_id, sampler_id)
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub enum DiskVertexSemantic {
    Position,
    Normal,
    Tangent,
    Interpolated,
}

#[derive(Serialize, Deserialize)]
pub struct DiskVertexAttribute {
    pub attribute_name: String,
    pub attribute_semantic: DiskVertexSemantic,
    pub attribute_format: i32, // vk::Format pretending to be i32
    pub attribute_location: u32,
    pub attribute_offset: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterial {
    pub material_layout: usize,

    pub vertex_stride: u64,
    pub vertex_format: Vec<DiskVertexAttribute>,

    pub fragment_alpha_test: bool,
    pub fragment_cull_flags: u32, // vk::CullModeFlags pretending to be u32

    pub shader_image_mapping: Vec<(String, String)>, // image_name, uv_channel_name
    pub shader_macro_definitions: Vec<(String, String)>, // name, value
}

#[derive(Serialize, Deserialize)]
pub struct DiskBuffer {
    pub stride: u64,
    pub usage_flags: u32, // vk::BufferUsageFlags pretending to be u32

    #[serde(with = "resource_compression")]
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderMesh {
    pub vertex_buffer: usize,
    pub index_buffer: (i32, usize), // vk::IndexType pretending to be i32, buffer_id
    pub index_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderInstance {
    pub mesh: usize,
    pub material_instance: usize,

    pub total_instance_count: usize,
    pub total_draw_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderBucket {
    pub material: usize,
    pub instances: Vec<DiskRenderInstance>,
    pub instance_transform_buffer: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskResourceBundle {
    pub buffers: Vec<DiskBuffer>,
    pub meshes: Vec<DiskRenderMesh>,
    pub images: Vec<DiskImage>,
    pub samplers: Vec<DiskSampler>,
    pub material_layouts: Vec<DiskMaterialLayout>,
    pub material_instances: Vec<DiskMaterialInstance>,
    pub materials: Vec<DiskMaterial>,
    pub buckets: Vec<DiskRenderBucket>,
}

impl DiskResourceBundle {
    pub fn serialize_into<W>(&self, writer: W, _compression_level: u32) -> Result<(), ()>
    where
        W: std::io::Write,
    {
        match bincode::serialize_into(writer, self) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn deserialize_from<R>(reader: R) -> Result<Self, ()>
    where
        R: std::io::Read,
    {
        match bincode::deserialize_from(reader) {
            Ok(bundle) => Ok(bundle),
            Err(_) => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DiskMaterialStages {
    pub vertex_stage: Vec<u32>,
    pub geometry_stage: Vec<u32>,
    pub tessellation_control_stage: Vec<u32>,
    pub tessellation_evaluation_stage: Vec<u32>,
    pub fragment_stage: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRayTracingStages {
    pub ray_generation_stage: Vec<u32>,
    pub ray_closest_hit_stage: Vec<u32>,
    pub ray_any_hit_stage: Vec<u32>,
    pub ray_miss_stage: Vec<u32>,
    pub intersection_stage: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub enum DiskShaderStages {
    Material(DiskMaterialStages),
    RayTracing(DiskRayTracingStages),
    Compute(Vec<u32>),
}

#[derive(Serialize, Deserialize)]
pub struct DiskShaderStageBundle {
    pub shader_stages: Vec<DiskShaderStages>,
}

impl DiskShaderStageBundle {
    pub fn serialize_into<W>(&self, writer: W, _compression_level: u32) -> Result<(), ()>
    where
        W: std::io::Write,
    {
        match bincode::serialize_into(writer, self) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn deserialize_from<R>(reader: R) -> Result<Self, ()>
    where
        R: std::io::Read,
    {
        match bincode::deserialize_from(reader) {
            Ok(bundle) => Ok(bundle),
            Err(_) => Err(()),
        }
    }
}
