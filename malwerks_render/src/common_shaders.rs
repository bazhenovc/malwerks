// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskCommonShaders {
    pub apex_culling_compute_stage: Vec<u32>,
    pub occlusion_culling_compute_stage: Vec<u32>,
    pub count_to_dispatch_compute_stage: Vec<u32>,

    pub empty_fragment_stage: Vec<u32>,

    pub occluder_material_vertex_stage: Vec<u32>,
    pub occluder_material_fragment_stage: Vec<u32>,

    pub occluder_resolve_vertex_stage: Vec<u32>,
    pub occluder_resolve_fragment_stage: Vec<u32>,

    pub skybox_vertex_stage: Vec<u32>,
    pub skybox_fragment_stage: Vec<u32>,

    pub tone_map_vertex_stage: Vec<u32>,
    pub tone_map_fragment_stage: Vec<u32>,

    pub imgui_vertex_stage: Vec<u32>,
    pub imgui_fragment_stage: Vec<u32>,
}

impl DiskCommonShaders {
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
