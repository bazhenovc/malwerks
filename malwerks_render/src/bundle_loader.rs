// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_core::*;
use malwerks_vk::*;

use malwerks_bundles::*;

use malwerks_external::*;
use malwerks_gltf::*;

use crate::common_shaders::*;
use crate::material_shaders::*;
use crate::pbr_resource_bundle::*;

use crate::imgui_renderer::*;

pub type BundleId = usize;

pub struct BundleLoaderParameters<'a> {
    pub bundle_compression_level: u32,
    pub temporary_folder: &'a std::path::Path,
    pub base_path: &'a std::path::Path,
    pub shader_bundle_path: &'a std::path::Path,
    pub pbr_resource_folder: &'a std::path::Path,
}

pub struct BundleLoader {
    command_pool: vk::CommandPool,
    command_buffers: Vec<CommandBuffer>,

    common_shaders: DiskCommonShaders,
    pbr_resource_bundle: PbrResourceBundle,
    resource_bundles: Vec<LoadedBundle>,

    base_path: std::path::PathBuf,
    temporary_folder: std::path::PathBuf,
    compression_level: u32,
}

impl BundleLoader {
    pub fn new<'a>(
        parameters: &BundleLoaderParameters<'a>,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Self {
        let command_pool = factory.create_command_pool(
            &vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(device.get_graphics_queue_index())
                .build(),
        );
        let mut command_buffers = factory.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build(),
        );

        let common_shaders = import_common_shaders(
            parameters.base_path,
            parameters.shader_bundle_path,
            parameters.bundle_compression_level,
        );
        let pbr_resource_bundle = import_pbr_resource_bundle(
            &parameters.temporary_folder.join("pbr_resource_bundle"),
            parameters.pbr_resource_folder,
            parameters.bundle_compression_level,
            &mut command_buffers[0],
            device,
            factory,
            queue,
        );
        let resource_bundles = Vec::new();

        let base_path = parameters.base_path.to_path_buf();
        let temporary_folder = parameters.temporary_folder.to_path_buf();
        let compression_level = parameters.bundle_compression_level;

        Self {
            command_pool,
            command_buffers,
            common_shaders,
            pbr_resource_bundle,
            resource_bundles,
            base_path,
            temporary_folder,
            compression_level,
        }
    }

    pub fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_command_pool(self.command_pool);
        self.pbr_resource_bundle.destroy(factory);
        for loaded_bundle in &mut self.resource_bundles {
            assert_eq!(
                loaded_bundle.use_count, 0,
                "destroying bundle that is still in use: {:?}",
                loaded_bundle.bundle_file
            );
            // resource_bundle.destroy(factory);
        }
    }

    pub fn get_base_path(&self) -> &std::path::Path {
        &self.base_path
    }

    pub fn get_command_buffer_mut(&mut self) -> &mut CommandBuffer {
        &mut self.command_buffers[0]
    }

    pub fn get_common_shaders(&self) -> &DiskCommonShaders {
        &self.common_shaders
    }

    pub fn get_pbr_resource_bundle(&self) -> &PbrResourceBundle {
        &self.pbr_resource_bundle
    }
}

impl BundleLoader {
    pub fn request_import_bundle(
        &mut self,
        gltf_file: &std::path::Path,
        bundle_file: &std::path::Path,
        device: &Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> BundleId {
        log::info!("bundle import requested: {:?} -> {:?}", gltf_file, bundle_file);
        if let Some(bundle_index) = self
            .resource_bundles
            .iter()
            .position(|item| item.bundle_file == bundle_file)
        {
            if self.resource_bundles[bundle_index].use_count == 0 {
                self.resource_bundles[bundle_index].use_count = 1;
                self.resource_bundles[bundle_index].bundle = import_bundle(
                    &self.temporary_folder.join(bundle_file),
                    gltf_file,
                    bundle_file,
                    self.compression_level,
                    &mut self.command_buffers[0],
                    device,
                    factory,
                    queue,
                );
            } else {
                self.resource_bundles[bundle_index].use_count += 1;
            }

            bundle_index
        } else {
            let bundle_index = self.resource_bundles.len();
            self.resource_bundles.push(LoadedBundle {
                bundle_file: bundle_file.to_path_buf(),
                bundle: import_bundle(
                    &self.temporary_folder.join(bundle_file),
                    gltf_file,
                    bundle_file,
                    self.compression_level,
                    &mut self.command_buffers[0],
                    device,
                    factory,
                    queue,
                ),
                use_count: 1,
            });
            bundle_index
        }
    }

    pub fn release_bundle(&mut self, bundle_id: BundleId, factory: &mut DeviceFactory) {
        let bundle = &mut self.resource_bundles[bundle_id];
        assert_ne!(bundle.use_count, 0, "trying to destroy already destroyed bundle");

        bundle.use_count -= 1;
        if bundle.use_count == 0 {
            bundle.bundle.destroy(factory);
        }
    }

    pub fn resolve_resource_bundle(&self, bundle_id: BundleId) -> &ResourceBundle {
        &self.resource_bundles[bundle_id].bundle
    }

    pub fn compile_shader_module_bundle(
        &self,
        resource_bundle_id: BundleId,
        bundle_file: &std::path::Path,
        shader_file: &std::path::Path,
        factory: &mut DeviceFactory,
    ) -> ShaderModuleBundle {
        let resource_bundle = &self.resource_bundles[resource_bundle_id];
        assert_ne!(
            resource_bundle.use_count, 0,
            "trying to compile shader modules for destroyed bundle {:?}",
            resource_bundle.bundle_file
        );

        let disk_shader_stage = if !bundle_file.exists() {
            let bundle = compile_material_shaders(
                &resource_bundle.bundle,
                shader_file,
                &self.temporary_folder.join(shader_file.file_name().unwrap()),
            );
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(bundle_file)
                .expect("failed to open shader stage bundle file for writing");
            bundle
                .serialize_into(file, self.compression_level)
                .expect("failed to serialize shader bundle");
            bundle
        } else {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .open(bundle_file)
                .expect("failed to open shader stage bundle for reading");
            DiskShaderStageBundle::deserialize_from(file).expect("failed to deserialize shader stage bundle")
        };

        ShaderModuleBundle::new(&disk_shader_stage, factory)
    }

    pub fn create_pipeline_bundle<F>(&self, resource_bundle_id: BundleId, mut func: F) -> PipelineBundle
    where
        F: FnMut(&PbrResourceBundle, &ResourceBundle) -> PipelineBundle,
    {
        let resource_bundle = &self.resource_bundles[resource_bundle_id];
        assert_ne!(
            resource_bundle.use_count, 0,
            "trying to compile shader modules for destroyed bundle {:?}",
            resource_bundle.bundle_file
        );
        func(&self.pbr_resource_bundle, &resource_bundle.bundle)
    }
}

impl BundleLoader {
    pub fn create_imgui_renderer(
        &mut self,
        imgui: &mut imgui::Context,
        target_layer: &RenderLayer,
        device: &mut Device,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> ImguiRenderer {
        ImguiRenderer::new(
            imgui,
            &self.common_shaders,
            target_layer,
            &mut self.command_buffers[0],
            device,
            factory,
            queue,
        )
    }
}

struct LoadedBundle {
    bundle_file: std::path::PathBuf,
    bundle: ResourceBundle,
    use_count: isize,
}

fn import_pbr_resource_bundle(
    temporary_path: &std::path::Path,
    input_path: &std::path::Path,
    compression_level: u32,
    command_buffer: &mut CommandBuffer,
    _device: &Device,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> PbrResourceBundle {
    let bundle_file = input_path.with_extension("bundle");
    let disk_bundle = if !bundle_file.exists() {
        let precomputed_brdf_image = compress_image(
            ImageUsage::EnvironmentBrdf,
            temporary_path,
            &input_path.join("brdf.dds"),
        );

        let probe_image = compress_image(
            ImageUsage::EnvironmentSkybox,
            temporary_path,
            &input_path.join("probe_image.dds"),
        );
        let iem_image = compress_image(
            ImageUsage::EnvironmentIem,
            temporary_path,
            &input_path.join("probe_iem.dds"),
        );
        let pmrem_image = compress_image(
            ImageUsage::EnvironmentPmrem,
            temporary_path,
            &input_path.join("probe_pmrem.dds"),
        );

        let bundle = DiskPbrResourceBundle {
            precomputed_brdf_image,
            environment_probe: DiskEnvironmentProbe {
                probe_image,
                iem_image,
                pmrem_image,
            },
        };

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(bundle_file)
            .expect("failed to open shared bundle file for writing");
        bundle
            .serialize_into(file, compression_level)
            .expect("failed to serialize bundle");

        bundle
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(bundle_file)
            .expect("failed to open shared bundle file for reading");
        DiskPbrResourceBundle::deserialize_from(file).expect("failed to deserialize bundle")
    };

    PbrResourceBundle::new(&disk_bundle, command_buffer, factory, queue)
}

fn import_bundle(
    temporary_path: &std::path::Path,
    gltf_file: &std::path::Path,
    bundle_file: &std::path::Path,
    compression_level: u32,
    command_buffer: &mut CommandBuffer,
    _device: &Device,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> ResourceBundle {
    let disk_resource_bundle = if !bundle_file.exists() {
        let bundle = import_gltf_bundle(gltf_file, &temporary_path.join(gltf_file));

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(bundle_file)
            .expect("failed to open bundle file for writing");
        bundle
            .serialize_into(file, compression_level)
            .expect("failed to serialize resource bundle");
        bundle
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(bundle_file)
            .expect("failed to open resource bundle file for reading");
        DiskResourceBundle::deserialize_from(file).expect("failed to deserialize resource bundle")
    };

    ResourceBundle::from_disk(&disk_resource_bundle, command_buffer, factory, queue)
}

fn import_common_shaders(
    base_path: &std::path::Path,
    shader_bundle_path: &std::path::Path,
    compression_level: u32,
) -> DiskCommonShaders {
    let disk_common_shaders = if !shader_bundle_path.exists() {
        let bundle = compile_common_shaders(base_path);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(shader_bundle_path)
            .expect("failed to open common shader bundle file for writing");
        bundle
            .serialize_into(file, compression_level)
            .expect("failed to serialize common shader bundle");
        bundle
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(shader_bundle_path)
            .expect("failed to open common shader bundle file for reading");
        DiskCommonShaders::deserialize_from(file).expect("failed to deserialize common shader bundle")
    };
    disk_common_shaders
}

fn compile_common_shaders(base_path: &std::path::Path) -> DiskCommonShaders {
    let base_shader_path = base_path.join("malwerks_shaders");

    let apex_culling_glsl =
        std::fs::read_to_string(base_shader_path.join("apex_culling.glsl")).expect("failed to open apex_culling.glsl");
    let occlusion_culling_glsl = std::fs::read_to_string(base_shader_path.join("occlusion_culling.glsl"))
        .expect("failed to open occlusion_culling.glsl");
    let count_to_dispatch_glsl = std::fs::read_to_string(base_shader_path.join("count_to_dispatch.glsl"))
        .expect("failed to open count_to_dispatch.glsl");

    let empty_fragment_glsl = "#version 460 core\nvoid main() {}\n";

    let occluder_material_glsl = std::fs::read_to_string(base_shader_path.join("occluder_material.glsl"))
        .expect("failed to open occluder_material.glsl");

    let occluder_resolve_glsl = std::fs::read_to_string(base_shader_path.join("occluder_resolve.glsl"))
        .expect("failed to open occluder_resolve.glsl");

    let tone_map_glsl =
        std::fs::read_to_string(base_shader_path.join("tone_map.glsl")).expect("failed to open tone_map.glsl");

    let imgui_glsl = std::fs::read_to_string(base_shader_path.join("imgui.glsl")).expect("failed to open imgui.glsl");

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

    let tone_map_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &tone_map_glsl,
                shaderc::ShaderKind::Vertex,
                "tone_map.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader")
            .as_binary(),
    );
    let tone_map_fragment_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &tone_map_glsl,
                shaderc::ShaderKind::Fragment,
                "tone_map.glsl",
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

    let (skybox_vertex_stage, skybox_fragment_stage) = compile_environment_probe_shaders(base_path);
    DiskCommonShaders {
        apex_culling_compute_stage,
        occlusion_culling_compute_stage,
        count_to_dispatch_compute_stage,
        empty_fragment_stage,
        occluder_material_vertex_stage,
        occluder_material_fragment_stage,
        occluder_resolve_vertex_stage,
        occluder_resolve_fragment_stage,
        skybox_vertex_stage,
        skybox_fragment_stage,
        tone_map_vertex_stage,
        tone_map_fragment_stage,
        imgui_vertex_stage,
        imgui_fragment_stage,
    }
}

fn compile_environment_probe_shaders(base_path: &std::path::Path) -> (Vec<u32>, Vec<u32>) {
    let skybox_glsl = std::fs::read_to_string(base_path.join("malwerks_shaders").join("environment_probe.glsl"))
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
