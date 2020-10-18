// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;

pub fn compile_gltf_shaders(
    source_bundle: &DiskRenderBundle,
    shader_path: &std::path::Path,
    temp_folder: &std::path::Path,
) -> DiskShaderStageBundle {
    std::fs::create_dir_all(temp_folder).expect("failed to create temp folder for shaders");
    log::info!(
        "compiling {} \"{}\" shaders",
        source_bundle.materials.len(),
        shader_path.to_str().expect("failed to convert shader path to str")
    );

    let shader_code = std::fs::read_to_string(shader_path).expect("failed to open shader file");

    let mut compiler = shaderc::Compiler::new().expect("failed to initialize GLSL compiler");
    let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
    compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    compile_options.set_warnings_as_errors();

    let mut shader_stages = Vec::with_capacity(source_bundle.materials.len());
    for (material_id, material) in source_bundle.materials.iter().enumerate() {
        let attribute_fetch_code = generate_attribute_fetch_code(&material.vertex_format);
        let image_mapping_code = generate_image_mapping_code(&material.shader_image_mapping);

        std::fs::write(
            temp_folder.join(&format!("attribute_fetch_{}.glsl", material_id)),
            &attribute_fetch_code,
        )
        .expect("failed to write generated attribute fetch shader");
        std::fs::write(
            temp_folder.join(&format!("image_mapping{}.glsl", material_id)),
            &image_mapping_code,
        )
        .expect("failed to write generated image mapping shader");

        compile_options.set_include_callback(
            move |requested_source_path, _directive_type, _contained_within_path, _recursion_depth| {
                if requested_source_path == "generated://attribute_fetch.glsl" {
                    Ok(shaderc::ResolvedInclude {
                        resolved_name: String::from("attribute_fetch.glsl"),
                        content: attribute_fetch_code.clone(),
                    })
                } else if requested_source_path == "generated://image_mapping.glsl" {
                    Ok(shaderc::ResolvedInclude {
                        resolved_name: String::from("image_mapping.glsl"),
                        content: image_mapping_code.clone(),
                    })
                } else {
                    match std::fs::read_to_string(&requested_source_path) {
                        Ok(included_source) => Ok(shaderc::ResolvedInclude {
                            resolved_name: String::from(requested_source_path),
                            content: included_source,
                        }),

                        Err(e) => Err(format!(
                            "failed to open GLSL include file {}: {}",
                            &requested_source_path, e
                        )),
                    }
                }
            },
        );

        let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
        vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);
        let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
        fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

        let vertex_stage = compiler
            .compile_into_spirv(
                &shader_code,
                shaderc::ShaderKind::Vertex,
                shader_path.to_str().expect("failed to convert shader path to str"),
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader");
        let fragment_stage = compiler
            .compile_into_spirv(
                &shader_code,
                shaderc::ShaderKind::Fragment,
                shader_path.to_str().expect("failed to convert shader path to str"),
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader");

        shader_stages.push(DiskShaderStages::Material(DiskMaterialStages {
            vertex_stage: vertex_stage.as_binary().into(),
            geometry_stage: Vec::new(),
            tessellation_control_stage: Vec::new(),
            tessellation_evaluation_stage: Vec::new(),
            fragment_stage: fragment_stage.as_binary().into(),
        }));
    }

    DiskShaderStageBundle { shader_stages }
}

fn generate_attribute_fetch_code(vertex_format: &[DiskVertexAttribute]) -> String {
    let mut shader_code = String::from("// Autogenerated vertex attribute fetch code\n");
    for attribute in vertex_format {
        shader_code.push_str(&format!("#define HAS_VS_{0} 1\n", attribute.attribute_name));
    }

    shader_code.push_str("#ifdef VERTEX_STAGE\n");
    for attribute in vertex_format {
        let type_name = get_attribute_type_name(attribute.attribute_format);
        shader_code.push_str(&format!(
            "layout (location = {0}) in {1} IN_{2};\nlayout (location = {0}) out {1} VS_{2};\n",
            attribute.attribute_location, type_name, attribute.attribute_name,
        ));
    }
    shader_code.push_str("layout (std430, set = 1, binding = 0) restrict readonly buffer InstanceDataBuffer {\n");
    shader_code.push_str("    mat4 WorldTransforms[];\n");
    shader_code.push_str("};\n");
    shader_code.push_str("vec3 transform_direction(vec3 v, mat3 m)\n");
    shader_code.push_str("{ return normalize(m * (v / vec3(dot(m[0], m[0]), dot(m[1], m[1]), dot(m[2], m[2])))); }\n");
    shader_code.push_str("vec4 fetch_vertex_attributes() {\n");
    shader_code.push_str("    mat4 world_transform = WorldTransforms[gl_InstanceIndex];\n");
    for attribute in vertex_format {
        match attribute.attribute_semantic {
            DiskVertexSemantic::Position => shader_code.push_str(&format!(
                "    VS_{0} = (world_transform * vec4(IN_{0}.xyz, 1.0)).xyz;\n",
                attribute.attribute_name
            )),

            DiskVertexSemantic::Normal => shader_code.push_str(&format!(
                "    VS_{0} = transform_direction(IN_{0}, mat3(world_transform));\n",
                attribute.attribute_name
            )),

            DiskVertexSemantic::Tangent => shader_code.push_str(&format!(
                "    VS_{0} = vec4(normalize(mat3(world_transform) * IN_{0}.xyz), IN_{0}.w);\n",
                attribute.attribute_name
            )),

            DiskVertexSemantic::Interpolated => {
                shader_code.push_str(&format!("    VS_{0} = IN_{0};\n", attribute.attribute_name))
            }
        }
    }
    shader_code.push_str("    return vec4(VS_position.xyz, 1.0);\n");
    shader_code.push_str("}\n");
    shader_code.push_str("#endif\n");

    shader_code.push_str("#ifdef FRAGMENT_STAGE\n");
    for attribute in vertex_format {
        let type_name = get_attribute_type_name(attribute.attribute_format);
        shader_code.push_str(&format!(
            "layout (location = {0}) in {1} VS_{2};\n",
            attribute.attribute_location, type_name, attribute.attribute_name,
        ));
    }
    shader_code.push_str("#endif\n");

    shader_code
}

fn generate_image_mapping_code(images: &[(String, String)]) -> String {
    let mut shader_code = String::from("// Autogenerated shader image mapping code\n");

    shader_code.push_str("#ifdef FRAGMENT_STAGE\n");
    for (binding, image) in images.iter().enumerate() {
        shader_code.push_str(&format!(
            "layout (set = 0, binding = {}) uniform sampler2D {};\n",
            binding, image.0
        ));
        shader_code.push_str(&format!("#define HAS_{} 1\n", image.0));
        shader_code.push_str(&format!("#define {}_UV {}\n", image.0, image.1));
    }
    shader_code.push_str("#endif\n");

    shader_code
}

fn get_attribute_type_name(attribute_format: i32) -> &'static str {
    match vk::Format::from_raw(attribute_format) {
        vk::Format::R32_SINT => "int",
        vk::Format::R32G32_SINT => "ivec2",
        vk::Format::R32G32B32_SINT => "ivec3",
        vk::Format::R32G32B32A32_SINT => "ivec4",

        vk::Format::R32_UINT => "uint",
        vk::Format::R32G32_UINT => "uvec2",
        vk::Format::R32G32B32_UINT => "uvec3",
        vk::Format::R32G32B32A32_UINT => "uvec4",

        vk::Format::R32_SFLOAT => "float",
        vk::Format::R32G32_SFLOAT => "vec2",
        vk::Format::R32G32B32_SFLOAT => "vec3",
        vk::Format::R32G32B32A32_SFLOAT => "vec4",

        _ => unimplemented!(),
    }
}
