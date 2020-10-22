// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub use malwerks_bundles::*;
pub use malwerks_core::*;
pub use malwerks_external::*;
pub use malwerks_vk::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskEnvironmentProbe {
    pub skybox_image: DiskImage,
    pub iem_image: DiskImage,
    pub pmrem_image: DiskImage,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskSharedResources {
    pub precomputed_brdf_image: DiskImage,
    pub environment_probe: DiskEnvironmentProbe,

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

    pub postprocess_vertex_stage: Vec<u32>,
    pub postprocess_fragment_stage: Vec<u32>,

    pub imgui_vertex_stage: Vec<u32>,
    pub imgui_fragment_stage: Vec<u32>,
}

impl DiskSharedResources {
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

pub struct RenderSharedResources {
    pub images: Vec<HeapAllocatedResource<vk::Image>>,
    pub image_views: Vec<vk::ImageView>,

    pub linear_sampler: vk::Sampler,

    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: Vec<vk::DescriptorSet>,
}

impl RenderSharedResources {
    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        for image in &self.images {
            factory.deallocate_image(image);
        }
        for image_view in &self.image_views {
            factory.destroy_image_view(*image_view);
        }
        factory.destroy_sampler(self.linear_sampler);
        factory.destroy_descriptor_pool(self.descriptor_pool);
        factory.destroy_descriptor_set_layout(self.descriptor_set_layout);
    }

    pub fn new(
        disk_resources: &DiskSharedResources,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let mut images = Vec::with_capacity(4);
        let mut image_views = Vec::with_capacity(4);

        let mut upload_batch = UploadBatch::new(command_buffer);
        for disk_image in &[
            &disk_resources.precomputed_brdf_image,
            &disk_resources.environment_probe.iem_image,
            &disk_resources.environment_probe.pmrem_image,
            &disk_resources.environment_probe.skybox_image,
        ] {
            let image_view_type = vk::ImageViewType::from_raw(disk_image.view_type);
            let image_flags = match image_view_type {
                vk::ImageViewType::CUBE => vk::ImageCreateFlags::CUBE_COMPATIBLE,
                vk::ImageViewType::CUBE_ARRAY => vk::ImageCreateFlags::CUBE_COMPATIBLE,

                vk::ImageViewType::TYPE_2D_ARRAY => vk::ImageCreateFlags::TYPE_2D_ARRAY_COMPATIBLE,

                _ => vk::ImageCreateFlags::default(),
            };

            let allocated_image = factory.allocate_image(
                &vk::ImageCreateInfo::builder()
                    .flags(image_flags)
                    .image_type(vk::ImageType::from_raw(disk_image.image_type))
                    .format(vk::Format::from_raw(disk_image.format))
                    .extent(vk::Extent3D {
                        width: disk_image.width,
                        height: disk_image.height,
                        depth: disk_image.depth,
                    })
                    .mip_levels(disk_image.mipmap_count as _)
                    .array_layers(disk_image.layer_count as _)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .build(),
                &vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::GpuOnly,
                    required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    ..Default::default()
                },
            );

            upload_batch.upload_image_memory(
                &allocated_image,
                (disk_image.width, disk_image.height, disk_image.depth),
                (disk_image.block_size, disk_image.mipmap_count, disk_image.layer_count),
                &disk_image.pixels,
                factory,
            );

            image_views.push(
                factory.create_image_view(
                    &vk::ImageViewCreateInfo::builder()
                        .image(allocated_image.0)
                        .view_type(image_view_type)
                        .format(vk::Format::from_raw(disk_image.format))
                        .components(vk::ComponentMapping::default())
                        .subresource_range(
                            vk::ImageSubresourceRange::builder()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(disk_image.mipmap_count as _)
                                .base_array_layer(0)
                                .layer_count(disk_image.layer_count as _)
                                .build(),
                        )
                        .build(),
                ),
            );
            images.push(allocated_image);
        }
        upload_batch.flush(factory, queue);

        let linear_sampler = factory.create_sampler(
            &vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .min_lod(0.0)
                .max_lod(std::f32::MAX)
                .build(),
        );

        let descriptor_pool = factory.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::builder()
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize::builder()
                    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(3)
                    .build()])
                .build(),
        );
        let descriptor_set_layout = factory.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&[
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
                vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .build(),
            ]),
        );

        let temp_per_descriptor_layouts = [descriptor_set_layout; 1];
        let descriptor_sets = factory.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&temp_per_descriptor_layouts)
                .build(),
        );

        let mut temp_writes = [vk::WriteDescriptorSet::default(); 3];
        let mut temp_image_infos = [vk::DescriptorImageInfo::default(); 3];
        for (image_id, image_view) in [image_views[0], image_views[1], image_views[2]].iter().enumerate() {
            temp_image_infos[image_id] = vk::DescriptorImageInfo::builder()
                .image_view(*image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .sampler(linear_sampler)
                .build();
            temp_writes[image_id] = vk::WriteDescriptorSet::builder()
                .dst_binding(image_id as _)
                .dst_set(descriptor_sets[0])
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&temp_image_infos[image_id..image_id + 1])
                .build();
        }
        factory.update_descriptor_sets(&temp_writes, &[]);

        Self {
            images,
            image_views,
            linear_sampler,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
        }
    }

    // pub fn get_precomputed_brdf_image_view(&self) -> vk::ImageView {
    //     self.image_views[0]
    // }

    // pub fn get_iem_image_view(&self) -> vk::ImageView {
    //     self.image_views[1]
    // }

    // pub fn get_pmrem_image_view(&self) -> vk::ImageView {
    //     self.image_views[2]
    // }

    pub fn get_skybox_image_view(&self) -> vk::ImageView {
        self.image_views[3]
    }
}

pub fn import_shared_resources(base_path: &std::path::Path, temp_file_path: &std::path::Path) -> DiskSharedResources {
    let (precomputed_brdf_image, environment_probe) = import_global_images(base_path, temp_file_path);
    let (skybox_vertex_stage, skybox_fragment_stage) = import_environment_probe_shaders(base_path);
    let (
        apex_culling_compute_stage,
        occlusion_culling_compute_stage,
        count_to_dispatch_compute_stage,
        empty_fragment_stage,
        occluder_material_vertex_stage,
        occluder_material_fragment_stage,
        occluder_resolve_vertex_stage,
        occluder_resolve_fragment_stage,
        postprocess_vertex_stage,
        postprocess_fragment_stage,
        imgui_vertex_stage,
        imgui_fragment_stage,
    ) = import_global_shaders(base_path);

    DiskSharedResources {
        precomputed_brdf_image,
        environment_probe,

        skybox_vertex_stage,
        skybox_fragment_stage,

        apex_culling_compute_stage,
        occlusion_culling_compute_stage,
        count_to_dispatch_compute_stage,
        empty_fragment_stage,
        occluder_material_vertex_stage,
        occluder_material_fragment_stage,
        occluder_resolve_vertex_stage,
        occluder_resolve_fragment_stage,
        postprocess_vertex_stage,
        postprocess_fragment_stage,
        imgui_vertex_stage,
        imgui_fragment_stage,
    }
}

fn import_global_images(
    base_path: &std::path::Path,
    temp_file_path: &std::path::Path,
) -> (DiskImage, DiskEnvironmentProbe) {
    let precomputed_brdf_image =
        compress_image(ImageUsage::EnvironmentBrdf, temp_file_path, &base_path.join("brdf.dds"));

    let skybox_image = compress_image(
        ImageUsage::EnvironmentSkybox,
        temp_file_path,
        &base_path.join("probe_skybox.dds"),
    );
    let iem_image = compress_image(
        ImageUsage::EnvironmentIem,
        temp_file_path,
        &base_path.join("probe_iem.dds"),
    );
    let pmrem_image = compress_image(
        ImageUsage::EnvironmentPmrem,
        temp_file_path,
        &base_path.join("probe_pmrem.dds"),
    );

    (
        precomputed_brdf_image,
        DiskEnvironmentProbe {
            skybox_image,
            iem_image,
            pmrem_image,
        },
    )
}

fn import_environment_probe_shaders(base_path: &std::path::Path) -> (Vec<u32>, Vec<u32>) {
    let skybox_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("environment_probe.glsl"),
    )
    .expect("failed to open environment_probe.glsl");

    let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
    compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    compile_options.set_warnings_as_errors();

    let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
    vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);
    let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
    fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

    let mut ray_tracing_options = compile_options.clone().expect("failed to clone ray tracing options");
    ray_tracing_options.add_macro_definition("RAY_TRACING", None);
    let mut ray_gen_options = ray_tracing_options.clone().expect("failed to clone ray gen options");
    ray_gen_options.add_macro_definition("RAY_GEN_STAGE", None);
    let mut ray_miss_options = ray_tracing_options.clone().expect("failed to clone ray miss options");
    ray_miss_options.add_macro_definition("RAY_MISS_STAGE", None);

    let mut compiler = shaderc::Compiler::new().expect("failed to initialize GLSL compiler");
    let skybox_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &skybox_glsl,
                shaderc::ShaderKind::Vertex,
                "environment_probe.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let skybox_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &skybox_glsl,
                shaderc::ShaderKind::Fragment,
                "environment_probe.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    (skybox_vertex_stage, skybox_fragment_stage)
}

fn import_global_shaders(
    base_path: &std::path::Path,
) -> (
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
) {
    let apex_culling_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("apex_culling.glsl"),
    )
    .expect("failed to open apex_culling.glsl");
    let occlusion_culling_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("occlusion_culling.glsl"),
    )
    .expect("failed to open occlusion_culling.glsl");
    let count_to_dispatch_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("count_to_dispatch.glsl"),
    )
    .expect("failed to open count_to_dispatch.glsl");

    let empty_fragment_glsl = "#version 460 core\nvoid main() {}\n";

    let occluder_material_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("occluder_material.glsl"),
    )
    .expect("failed to open occluder_material.glsl");

    let occluder_resolve_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("occluder_resolve.glsl"),
    )
    .expect("failed to open occluder_resolve.glsl");

    let postprocess_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("postprocess.glsl"),
    )
    .expect("failed to open postprocess.glsl");

    let imgui_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("imgui.glsl"),
    )
    .expect("failed to open imgui.glsl");

    let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
    compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    compile_options.set_warnings_as_errors();

    let mut compute_stage_options = compile_options.clone().expect("failed to clone compute options");
    compute_stage_options.add_macro_definition("COMPUTE_STAGE", None);

    let mut compiler = shaderc::Compiler::new().expect("failed to initialize GLSL compiler");
    let apex_culling_compute_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &apex_culling_glsl,
                shaderc::ShaderKind::Compute,
                "apex_culling.glsl",
                "main",
                Some(&compute_stage_options),
            )
            .expect("failed to compile compute shader")
            .as_binary(),
    );
    let occlusion_culling_compute_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &occlusion_culling_glsl,
                shaderc::ShaderKind::Compute,
                "occlusion_culling.glsl",
                "main",
                Some(&compute_stage_options),
            )
            .expect("failed to compile compute shader")
            .as_binary(),
    );
    let count_to_dispatch_compute_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &count_to_dispatch_glsl,
                shaderc::ShaderKind::Compute,
                "count_to_dispatch.glsl",
                "main",
                Some(&compute_stage_options),
            )
            .expect("failed to compile compute shader")
            .as_binary(),
    );

    let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
    vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);

    let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
    fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

    let empty_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &empty_fragment_glsl,
                shaderc::ShaderKind::Fragment,
                "empty_fragment.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile empty fragment stage")
            .as_binary(),
    );

    let occluder_material_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &occluder_material_glsl,
                shaderc::ShaderKind::Vertex,
                "occluder_material.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let occluder_material_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &occluder_material_glsl,
                shaderc::ShaderKind::Fragment,
                "occluder_material.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    let occluder_resolve_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &occluder_resolve_glsl,
                shaderc::ShaderKind::Vertex,
                "occluder_resolve.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let occluder_resolve_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &occluder_resolve_glsl,
                shaderc::ShaderKind::Fragment,
                "occluder_resolve.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    let postprocess_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &postprocess_glsl,
                shaderc::ShaderKind::Vertex,
                "postprocess.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let postprocess_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &postprocess_glsl,
                shaderc::ShaderKind::Fragment,
                "postprocess.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    let imgui_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &imgui_glsl,
                shaderc::ShaderKind::Vertex,
                "imgui.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let imgui_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &imgui_glsl,
                shaderc::ShaderKind::Fragment,
                "imgui.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    (
        apex_culling_compute_stage,
        occlusion_culling_compute_stage,
        count_to_dispatch_compute_stage,
        empty_fragment_stage,
        occluder_material_vertex_stage,
        occluder_material_fragment_stage,
        occluder_resolve_vertex_stage,
        occluder_resolve_fragment_stage,
        postprocess_vertex_stage,
        postprocess_fragment_stage,
        imgui_vertex_stage,
        imgui_fragment_stage,
    )
}
