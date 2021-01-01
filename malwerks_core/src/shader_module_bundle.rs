// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_bundles::*;
use malwerks_vk::*;

pub struct MaterialShaderModules {
    pub vertex_stage: vk::ShaderModule,
    pub geometry_stage: vk::ShaderModule,
    pub tessellation_control_stage: vk::ShaderModule,
    pub tessellation_evaluation_stage: vk::ShaderModule,
    pub fragment_stage: vk::ShaderModule,
}

pub struct RayTracingShaderModules {
    pub ray_generation_stage: vk::ShaderModule,
    pub ray_closest_hit_stage: vk::ShaderModule,
    pub ray_any_hit_stage: vk::ShaderModule,
    pub ray_miss_stage: vk::ShaderModule,
    pub intersection_stage: vk::ShaderModule,
}

pub enum ShaderModules {
    Material(MaterialShaderModules),
    RayTracing(RayTracingShaderModules),
    Compute(vk::ShaderModule),
}

pub struct ShaderModuleBundle {
    pub shader_stages: Vec<ShaderModules>,
}

impl ShaderModuleBundle {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        macro_rules! destroy_shader_stage {
            ($stage: expr) => {
                if $stage != vk::ShaderModule::null() {
                    factory.destroy_shader_module($stage);
                }
            };
        }
        for stage in &self.shader_stages {
            match stage {
                ShaderModules::Material(material_stage) => {
                    destroy_shader_stage!(material_stage.vertex_stage);
                    destroy_shader_stage!(material_stage.geometry_stage);
                    destroy_shader_stage!(material_stage.tessellation_control_stage);
                    destroy_shader_stage!(material_stage.tessellation_evaluation_stage);
                    destroy_shader_stage!(material_stage.fragment_stage);
                }

                ShaderModules::RayTracing(ray_tracing_stage) => {
                    destroy_shader_stage!(ray_tracing_stage.ray_generation_stage);
                    destroy_shader_stage!(ray_tracing_stage.ray_closest_hit_stage);
                    destroy_shader_stage!(ray_tracing_stage.ray_any_hit_stage);
                    destroy_shader_stage!(ray_tracing_stage.ray_miss_stage);
                    destroy_shader_stage!(ray_tracing_stage.intersection_stage);
                }

                ShaderModules::Compute(compute_stage) => {
                    destroy_shader_stage!(*compute_stage);
                }
            }
        }
    }

    pub fn new(disk_stages: &DiskShaderStageBundle, factory: &mut DeviceFactory) -> Self {
        macro_rules! create_shader_stage {
            ($code: expr) => {
                if $code.is_empty() {
                    vk::ShaderModule::null()
                } else {
                    factory.create_shader_module(&vk::ShaderModuleCreateInfo::builder().code(&$code).build())
                }
            };
        }
        let mut shader_stages = Vec::with_capacity(disk_stages.shader_stages.len());
        for disk_stage in &disk_stages.shader_stages {
            shader_stages.push(match disk_stage {
                DiskShaderStages::Material(material_stage) => {
                    let vertex_stage = create_shader_stage!(material_stage.vertex_stage);
                    let geometry_stage = create_shader_stage!(material_stage.geometry_stage);
                    let tessellation_control_stage = create_shader_stage!(material_stage.tessellation_control_stage);
                    let tessellation_evaluation_stage =
                        create_shader_stage!(material_stage.tessellation_evaluation_stage);
                    let fragment_stage = create_shader_stage!(material_stage.fragment_stage);

                    ShaderModules::Material(MaterialShaderModules {
                        vertex_stage,
                        geometry_stage,
                        tessellation_control_stage,
                        tessellation_evaluation_stage,
                        fragment_stage,
                    })
                }

                DiskShaderStages::RayTracing(ray_tracing) => {
                    let ray_generation_stage = create_shader_stage!(ray_tracing.ray_generation_stage);
                    let ray_closest_hit_stage = create_shader_stage!(ray_tracing.ray_closest_hit_stage);
                    let ray_any_hit_stage = create_shader_stage!(ray_tracing.ray_any_hit_stage);
                    let ray_miss_stage = create_shader_stage!(ray_tracing.ray_miss_stage);
                    let intersection_stage = create_shader_stage!(ray_tracing.intersection_stage);

                    ShaderModules::RayTracing(RayTracingShaderModules {
                        ray_generation_stage,
                        ray_closest_hit_stage,
                        ray_any_hit_stage,
                        ray_miss_stage,
                        intersection_stage,
                    })
                }

                DiskShaderStages::Compute(compute) => {
                    let compute_stage = create_shader_stage!(compute);

                    ShaderModules::Compute(compute_stage)
                }
            });
        }

        ShaderModuleBundle { shader_stages }
    }
}
