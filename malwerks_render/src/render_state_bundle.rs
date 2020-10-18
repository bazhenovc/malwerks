// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;
use malwerks_vk::*;

use crate::render_bundle::*;
use crate::render_layer::*;
use crate::render_stage_bundle::*;

pub struct RenderStateBundleParameters<'a> {
    pub source_bundle: &'a DiskRenderBundle,
    pub render_bundle: &'a RenderBundle,
    pub render_stage_bundle: &'a RenderStageBundle,
    pub render_layer: &'a RenderLayer,

    pub descriptor_set_layouts: &'a [vk::DescriptorSetLayout],
}

pub struct RenderStateBundle {
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: Vec<vk::DescriptorSet>,

    pub pipeline_cache: vk::PipelineCache,
    pub pipeline_layouts: Vec<vk::PipelineLayout>, // directly maps to `materials` in the render bundle
    pub pipeline_states: Vec<vk::Pipeline>,        // directly maps to `materials` in the render bundle
}

impl RenderStateBundle {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_layout);
        factory.destroy_pipeline_cache(self.pipeline_cache);
        for pipeline_layout in &self.pipeline_layouts {
            factory.destroy_pipeline_layout(*pipeline_layout);
        }
        for pipeline in &self.pipeline_states {
            factory.destroy_pipeline(*pipeline);
        }
    }

    pub fn new<'a>(parameters: &RenderStateBundleParameters<'a>, factory: &mut DeviceFactory) -> Self {
        let (descriptor_pool, descriptor_layout, descriptor_sets) =
            initialize_descriptor_pool(parameters.render_bundle, factory);
        let (pipeline_cache, pipeline_layouts, pipeline_states) = initialize_pipelines(
            parameters.source_bundle,
            parameters.render_bundle,
            parameters.render_stage_bundle,
            parameters.render_layer,
            descriptor_layout,
            parameters.descriptor_set_layouts,
            factory,
        );

        Self {
            descriptor_pool,
            descriptor_layout,
            descriptor_sets,

            pipeline_cache,
            pipeline_layouts,
            pipeline_states,
        }
    }
}

fn initialize_descriptor_pool(
    render_bundle: &RenderBundle,
    factory: &mut DeviceFactory,
) -> (vk::DescriptorPool, vk::DescriptorSetLayout, Vec<vk::DescriptorSet>) {
    let mut render_instance_count = 0;
    for bucket in &render_bundle.buckets {
        render_instance_count += bucket.instances.len();
    }

    let descriptor_pool = factory.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::builder()
            .max_sets(render_instance_count as _)
            .pool_sizes(&[vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .build()])
            .build(),
    );
    let descriptor_layout = factory.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&[vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX)
                .build()])
            .build(),
    );

    let temp_per_descriptor_layouts = vec![descriptor_layout; render_instance_count];
    let descriptor_sets = factory.allocate_descriptor_sets(
        &vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&temp_per_descriptor_layouts)
            .build(),
    );

    let mut temp_write_infos = Vec::with_capacity(render_instance_count);
    let mut descriptor_writes = Vec::with_capacity(render_instance_count);
    {
        let mut current_descriptor_set = 0;
        for bucket in &render_bundle.buckets {
            let mut current_offset = 0;
            for instance in &bucket.instances {
                let range = instance.total_instance_count * std::mem::size_of::<[f32; 16]>();
                let current_write_info = temp_write_infos.len();
                temp_write_infos.push(
                    vk::DescriptorBufferInfo::builder()
                        .buffer(render_bundle.buffers[bucket.instance_transform_buffer].0)
                        .offset(current_offset as _)
                        .range(range as _)
                        .build(),
                );

                descriptor_writes.push(
                    vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_sets[current_descriptor_set])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&temp_write_infos[current_write_info..current_write_info + 1])
                        .build(),
                );
                current_offset += range;
                current_descriptor_set += 1;
            }
        }
    }
    factory.update_descriptor_sets(&descriptor_writes, &[]);

    (descriptor_pool, descriptor_layout, descriptor_sets)
}

fn initialize_pipelines(
    source_bundle: &DiskRenderBundle,
    render_bundle: &RenderBundle,
    render_stage_bundle: &RenderStageBundle,
    render_layer: &RenderLayer,
    descriptor_layout: vk::DescriptorSetLayout,
    extra_descriptor_layouts: &[vk::DescriptorSetLayout],
    factory: &mut DeviceFactory,
) -> (vk::PipelineCache, Vec<vk::PipelineLayout>, Vec<vk::Pipeline>) {
    assert!(
        render_stage_bundle.shader_stages.len() == source_bundle.materials.len(),
        "incompatible stage bundle, shader stages are not directly mapped to bundle materials"
    );
    let mut max_vertex_attributes = 0;
    for material in &source_bundle.materials {
        max_vertex_attributes = max_vertex_attributes.max(material.vertex_format.len());
    }

    let mut max_shader_stages = 0;
    for stages in &render_stage_bundle.shader_stages {
        match stages {
            RenderShaderStages::Material(material_stage) => {
                let mut stage_count = 0;
                stage_count += (material_stage.vertex_stage != vk::ShaderModule::null()) as usize;
                stage_count += (material_stage.geometry_stage != vk::ShaderModule::null()) as usize;
                stage_count += (material_stage.tessellation_control_stage != vk::ShaderModule::null()) as usize;
                stage_count += (material_stage.tessellation_evaluation_stage != vk::ShaderModule::null()) as usize;
                stage_count += (material_stage.fragment_stage != vk::ShaderModule::null()) as usize;

                max_shader_stages = max_shader_stages.max(stage_count);
            }

            _ => panic!("incompatible stage bundle, non-material shader stages found"),
        }
    }

    let mut temp_shader_stages = Vec::with_capacity(source_bundle.materials.len() * max_shader_stages);
    let mut temp_vertex_bindings = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_attributes = Vec::with_capacity(source_bundle.materials.len() * max_vertex_attributes);
    let mut temp_attachments = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_dynamic_state_values = Vec::with_capacity(source_bundle.materials.len() * 2);

    let mut temp_vertex_input_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_input_assembly_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_tessellation_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_viewport_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_rasterization_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_multisample_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_depth_stencil_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_color_blend_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_dynamic_states = Vec::with_capacity(source_bundle.materials.len());
    let mut temp_pipelines = Vec::with_capacity(source_bundle.materials.len());

    let mut temp_descriptor_layouts = vec![vk::DescriptorSetLayout::null(); 2 + extra_descriptor_layouts.len()];
    for (layout_id, layout) in extra_descriptor_layouts.iter().enumerate() {
        temp_descriptor_layouts[2 + layout_id] = *layout;
    }

    let entry_point = std::ffi::CString::new("main").unwrap();
    let mut pipeline_layouts = Vec::with_capacity(source_bundle.materials.len());
    for (material_id, disk_material) in source_bundle.materials.iter().enumerate() {
        temp_descriptor_layouts[0] = render_bundle.descriptor_layouts[disk_material.material_layout];
        temp_descriptor_layouts[1] = descriptor_layout;

        let temp_push_constant_ranges = [
            vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::VERTEX)
                .offset(0)
                .size(64)
                .build(),
            vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .offset(64)
                .size(64)
                .build(),
        ];

        let pipeline_layout = factory.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&temp_descriptor_layouts)
                .push_constant_ranges(&temp_push_constant_ranges)
                .build(),
        );

        let vertex_attributes_start = temp_attributes.len();
        for attribute in &disk_material.vertex_format {
            temp_attributes.push(
                vk::VertexInputAttributeDescription::builder()
                    .location(attribute.attribute_location)
                    .binding(0)
                    .format(vk::Format::from_raw(attribute.attribute_format))
                    .offset(attribute.attribute_offset as _)
                    .build(),
            );
        }

        let shader_stages_start = temp_shader_stages.len();
        if let RenderShaderStages::Material(shader_modules) = &render_stage_bundle.shader_stages[material_id] {
            if shader_modules.vertex_stage != vk::ShaderModule::null() {
                temp_shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(shader_modules.vertex_stage)
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .build(),
                );
            }

            if shader_modules.geometry_stage != vk::ShaderModule::null() {
                temp_shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(shader_modules.geometry_stage)
                        .stage(vk::ShaderStageFlags::GEOMETRY)
                        .build(),
                );
            }

            if shader_modules.tessellation_control_stage != vk::ShaderModule::null() {
                temp_shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(shader_modules.tessellation_control_stage)
                        .stage(vk::ShaderStageFlags::TESSELLATION_CONTROL)
                        .build(),
                );
            }

            if shader_modules.tessellation_evaluation_stage != vk::ShaderModule::null() {
                temp_shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(shader_modules.tessellation_evaluation_stage)
                        .stage(vk::ShaderStageFlags::TESSELLATION_EVALUATION)
                        .build(),
                );
            }

            if shader_modules.fragment_stage != vk::ShaderModule::null() {
                temp_shader_stages.push(
                    vk::PipelineShaderStageCreateInfo::builder()
                        .name(&entry_point)
                        .module(shader_modules.fragment_stage)
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .build(),
                );
            }
        }

        let vertex_bindings_start = temp_vertex_bindings.len();
        temp_vertex_bindings.push(
            vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(disk_material.vertex_stride as _)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build(),
        );

        let states_start = temp_vertex_input_states.len();
        temp_vertex_input_states.push(
            vk::PipelineVertexInputStateCreateInfo::builder()
                .vertex_binding_descriptions(&temp_vertex_bindings[vertex_bindings_start..temp_vertex_bindings.len()])
                .vertex_attribute_descriptions(&temp_attributes[vertex_attributes_start..temp_attributes.len()])
                .build(),
        );
        temp_input_assembly_states.push(
            vk::PipelineInputAssemblyStateCreateInfo::builder()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false)
                .build(),
        );
        temp_tessellation_states.push(vk::PipelineTessellationStateCreateInfo::default());
        temp_viewport_states.push(
            vk::PipelineViewportStateCreateInfo::builder()
                .viewport_count(1)
                .scissor_count(1)
                .build(),
        );
        temp_rasterization_states.push(
            vk::PipelineRasterizationStateCreateInfo::builder()
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::from_raw(disk_material.fragment_cull_flags))
                .build(),
        );
        temp_multisample_states.push(
            vk::PipelineMultisampleStateCreateInfo::builder()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                .build(),
        );
        temp_depth_stencil_states.push(
            vk::PipelineDepthStencilStateCreateInfo::builder()
                .flags(Default::default())
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
                .stencil_test_enable(false)
                .build(),
        );

        let attachments_start = temp_attachments.len();
        temp_attachments.push(
            vk::PipelineColorBlendAttachmentState::builder()
                .blend_enable(false)
                .color_write_mask(
                    vk::ColorComponentFlags::R
                        | vk::ColorComponentFlags::G
                        | vk::ColorComponentFlags::B
                        | vk::ColorComponentFlags::A,
                )
                .build(),
        );
        temp_color_blend_states.push(
            vk::PipelineColorBlendStateCreateInfo::builder()
                .attachments(&temp_attachments[attachments_start..temp_attachments.len()])
                .build(),
        );

        let dynamic_states_start = temp_dynamic_state_values.len();
        temp_dynamic_state_values.push(vk::DynamicState::VIEWPORT);
        temp_dynamic_state_values.push(vk::DynamicState::SCISSOR);
        temp_dynamic_states.push(
            vk::PipelineDynamicStateCreateInfo::builder()
                .dynamic_states(&temp_dynamic_state_values[dynamic_states_start..temp_dynamic_state_values.len()])
                .build(),
        );

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&temp_shader_stages[shader_stages_start..temp_shader_stages.len()])
            .vertex_input_state(&temp_vertex_input_states[states_start])
            .input_assembly_state(&temp_input_assembly_states[states_start])
            .tessellation_state(&temp_tessellation_states[states_start])
            .viewport_state(&temp_viewport_states[states_start])
            .rasterization_state(&temp_rasterization_states[states_start])
            .multisample_state(&temp_multisample_states[states_start])
            .depth_stencil_state(&temp_depth_stencil_states[states_start])
            .color_blend_state(&temp_color_blend_states[states_start])
            .dynamic_state(&temp_dynamic_states[states_start])
            .layout(pipeline_layout)
            .render_pass(render_layer.get_render_pass())
            .subpass(0)
            .base_pipeline_handle(vk::Pipeline::null())
            .base_pipeline_index(0)
            .build();

        pipeline_layouts.push(pipeline_layout);
        temp_pipelines.push(pipeline_create_info);
    }

    log::info!("allocating {} graphics pipelines", temp_pipelines.len());

    let pipeline_cache = factory.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default());
    let pipelines = factory.create_graphics_pipelines(pipeline_cache, &temp_pipelines);

    (pipeline_cache, pipeline_layouts, pipelines)
}
