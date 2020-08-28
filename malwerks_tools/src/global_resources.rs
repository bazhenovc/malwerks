// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use crate::texconv::*;

pub fn import_global_resources(static_scenery: &mut DiskStaticScenery, base_path: &std::path::Path) {
    import_global_images(static_scenery, base_path);
    import_environment_probe_shaders(&mut static_scenery.global_resources, base_path);
    import_global_shaders(&mut static_scenery.global_resources, base_path);
}

fn import_global_images(static_scenery: &mut DiskStaticScenery, base_path: &std::path::Path) {
    let brdf_image_id = static_scenery.images.len();
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentBrdf,
        base_path,
        &base_path.join("global").join("brdf.dds"),
    ));
    static_scenery.global_resources.precomputed_brdf_image = brdf_image_id;
}

fn import_environment_probe_shaders(global_resources: &mut DiskGlobalResources, base_path: &std::path::Path) {
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

    global_resources.skybox_vertex_stage = skybox_vertex_stage;
    global_resources.skybox_fragment_stage = skybox_fragment_stage;
}

fn import_global_shaders(global_resources: &mut DiskGlobalResources, base_path: &std::path::Path) {
    let apex_culling_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("apex_culling.glsl"),
    )
    .expect("failed to open environment_probe.glsl");

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

    let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
    vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);

    let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
    fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

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

    global_resources.apex_culling_compute_stage = apex_culling_compute_stage;
    global_resources.postprocess_vertex_stage = postprocess_vertex_stage;
    global_resources.postprocess_fragment_stage = postprocess_fragment_stage;
    global_resources.imgui_vertex_stage = imgui_vertex_stage;
    global_resources.imgui_fragment_stage = imgui_fragment_stage;
}
