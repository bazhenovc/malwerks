// Copyright (c) 2020 Kyrylo Bazhenov
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
    pub fragment_alpha_test: bool,
    pub fragment_cull_flags: u32, // vk::CullModeFlags pretending to be u32
}

#[derive(Serialize, Deserialize)]
pub struct DiskBuffer {
    pub stride: u64,
    pub usage_flags: u32, // vk::BufferUsageFlags pretending to be i32

    #[serde(with = "resource_compression")]
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskBoundingCone {
    pub cone_apex: [f32; 4],
    pub cone_axis: [f32; 4],
}

#[derive(Serialize, Deserialize)]
pub struct DiskMeshCluster {
    pub vertex_count: u16,
    pub index_count: u16,
}

#[derive(Serialize, Deserialize)]
pub struct DiskStaticMesh {
    pub vertex_buffer: usize,
    pub index_buffer: usize,
    pub mesh_clusters: Vec<DiskMeshCluster>,
    pub bounding_cones: Vec<DiskBoundingCone>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderInstance {
    pub mesh: usize,
    pub material_instance: usize,
    pub transforms: Vec<[f32; 16]>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskRenderBucket {
    pub material: usize,
    pub instances: Vec<DiskRenderInstance>,
    pub bounding_cone_buffer: usize,
    pub draw_arguments_buffer: usize,
    pub draw_arguments_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct DiskEnvironmentProbe {
    pub skybox_image: usize,
    pub iem_image: usize,
    pub pmrem_image: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DiskGlobalResources {
    pub precomputed_brdf_image: usize,

    pub apex_culling_compute_stage: Vec<u32>,

    pub skybox_vertex_stage: Vec<u32>,
    pub skybox_fragment_stage: Vec<u32>,

    pub postprocess_vertex_stage: Vec<u32>,
    pub postprocess_fragment_stage: Vec<u32>,

    pub imgui_vertex_stage: Vec<u32>,
    pub imgui_fragment_stage: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskStaticScenery {
    pub buffers: Vec<DiskBuffer>,
    pub meshes: Vec<DiskStaticMesh>,
    pub images: Vec<DiskImage>,
    pub samplers: Vec<DiskSampler>,
    pub material_layouts: Vec<DiskMaterialLayout>,
    pub material_instances: Vec<DiskMaterialInstance>,
    pub materials: Vec<DiskMaterial>,
    pub buckets: Vec<DiskRenderBucket>,
    pub environment_probes: Vec<DiskEnvironmentProbe>,
    pub global_resources: DiskGlobalResources,
}

impl DiskStaticScenery {
    pub fn serialize_into<W>(&self, writer: W, _compression_level: u32)
    where
        W: std::io::Write,
    {
        bincode::serialize_into(writer, self).expect("failed to serialize static scenery");
    }

    pub fn deserialize_from<R>(reader: R) -> DiskStaticScenery
    where
        R: std::io::Read,
    {
        bincode::deserialize_from(reader).expect("failed to deserialize static scenery")
    }
}
