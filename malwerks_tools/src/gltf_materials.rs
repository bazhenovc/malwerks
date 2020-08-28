// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;

pub struct Attribute<'a> {
    pub semantic: gltf::mesh::Semantic,
    pub semantic_name: String,
    pub location: usize,
    pub format: vk::Format,
    //pub data_type: gltf::accessor::DataType,
    //pub dimensions: gltf::accessor::Dimensions,
    pub type_name: &'a str,
    pub count: usize,
    pub stride: usize,
    pub offset: usize,
    pub data: &'a [u8],
}

pub fn generate_material<'a>(
    static_scenery: &mut DiskStaticScenery,
    base_path: &std::path::Path,
    material_id: usize,
    vertex_stride: usize,
    temp_preludes: &mut Vec<String>,
    attributes: &[Attribute<'a>],
    materials: gltf::iter::Materials,
) -> usize {
    let mut last_attribute_location = 0;
    let mut shader_prelude = String::from("#ifdef VERTEX_STAGE\n");
    for attribute in attributes {
        shader_prelude.push_str(&format!(
            "layout (location = {0}) in {1} IN_{2};\nlayout (location = {0}) out {1} VS_{2};\n",
            attribute.location, attribute.type_name, attribute.semantic_name,
        ));
        last_attribute_location = last_attribute_location.max(attribute.location);
    }
    shader_prelude.push_str("layout (std430, set = 1, binding = 0) readonly buffer InstanceDataBuffer {\n");
    shader_prelude.push_str("    mat4 WorldTransforms[];\n");
    shader_prelude.push_str("};\n");
    shader_prelude.push_str("vec3 transform_direction(vec3 v, mat3 m)\n");
    shader_prelude
        .push_str("{ return normalize(m * (v / vec3(dot(m[0], m[0]), dot(m[1], m[1]), dot(m[2], m[2])))); }\n");
    shader_prelude.push_str("vec4 generated_vertex_shader() {\n");
    shader_prelude.push_str("    mat4 world_transform = WorldTransforms[gl_InstanceIndex];\n");
    for attribute in attributes {
        match attribute.semantic {
            gltf::mesh::Semantic::Positions => shader_prelude.push_str(&format!(
                "    VS_{0} = (world_transform * vec4(IN_{0}.xyz, 1.0)).xyz;\n",
                attribute.semantic_name
            )),

            gltf::mesh::Semantic::Normals => shader_prelude.push_str(&format!(
                "    VS_{0} = transform_direction(IN_{0}, mat3(world_transform));\n",
                attribute.semantic_name
            )),

            gltf::mesh::Semantic::Tangents => shader_prelude.push_str(&format!(
                "    VS_{0} = vec4(normalize(mat3(world_transform) * IN_{0}.xyz), IN_{0}.w);\n",
                attribute.semantic_name
            )),

            _ => shader_prelude.push_str(&format!("    VS_{0} = IN_{0};\n", attribute.semantic_name)),
        }
    }
    shader_prelude.push_str("    return vec4(VS_position.xyz, 1.0);\n");
    shader_prelude.push_str("}\n");
    shader_prelude.push_str("#endif\n");

    macro_rules! texture_prelude {
        ($prelude: ident, $images: ident, $texture: expr, $texture_name: expr) => {
            if let Some(image) = $texture {
                let binding = $images.len();
                $images.push((
                    image.texture().index(),
                    image.texture().sampler().index().unwrap_or(0),
                ));
                $prelude.push_str(&format!(
                    "layout (set = 2, binding = {}) uniform sampler2D {};\n",
                    binding, $texture_name
                ));
                $prelude.push_str(&format!("#define HAS_{}\n", $texture_name));
                $prelude.push_str(&format!(
                    "#define {}_UV VS_uv{}\n",
                    $texture_name,
                    image.tex_coord()
                ));
            }
        };
    }

    let mut images = Vec::with_capacity(5);
    let material = materials.clone().nth(material_id).expect("failed to find material id");
    let pbr_metallic_roughness = material.pbr_metallic_roughness();

    shader_prelude.push_str("#ifdef FRAGMENT_STAGE\n");
    for attribute in attributes {
        shader_prelude.push_str(&format!(
            "layout (location = {0}) in {1} VS_{2};\n",
            attribute.location, attribute.type_name, attribute.semantic_name,
        ));
        shader_prelude.push_str(&format!("#define HAS_VS_{0}\n", &attribute.semantic_name));
    }

    texture_prelude!(
        shader_prelude,
        images,
        pbr_metallic_roughness.base_color_texture(),
        "BaseColorTexture"
    );
    texture_prelude!(
        shader_prelude,
        images,
        pbr_metallic_roughness.metallic_roughness_texture(),
        "MetallicRoughnessTexture"
    );
    texture_prelude!(shader_prelude, images, material.normal_texture(), "NormalTexture");
    texture_prelude!(shader_prelude, images, material.occlusion_texture(), "OcclusionTexture");
    texture_prelude!(shader_prelude, images, material.emissive_texture(), "EmissiveTexture");

    let fragment_alpha_test = match material.alpha_mode() {
        gltf::json::material::AlphaMode::Opaque => false,
        gltf::json::material::AlphaMode::Mask => {
            shader_prelude.push_str("#define HAS_AlphaDiscard\n");
            true
        }
        gltf::json::material::AlphaMode::Blend => false,
    };
    let fragment_cull_flags = if material.double_sided() {
        vk::CullModeFlags::NONE.as_raw()
    } else {
        vk::CullModeFlags::BACK.as_raw()
    };

    shader_prelude.push_str("#endif\n");

    let existing_id = temp_preludes.iter().position(|s| *s == shader_prelude);
    if let Some(existing_id) = existing_id {
        existing_id
    } else {
        std::fs::write(
            base_path.join(format!("shader_prelude_{}.glsl", images.len())),
            &shader_prelude,
        )
        .expect("failed to write shader prelude file");

        let shaders_path = base_path.join("..").join("..").join("malwerks_shaders");
        let pbr_material_glsl =
            std::fs::read_to_string(shaders_path.join("pbr_material.glsl")).expect("failed to open pbr_material.glsl");

        let id = temp_preludes.len();
        temp_preludes.push(shader_prelude.clone());

        let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
        compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
        compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
        compile_options.set_warnings_as_errors();
        compile_options.set_include_callback(
            |requested_source_path, _directive_type, _contained_within_path, _recursion_depth| {
                if requested_source_path == "generated://shader_prelude.glsl" {
                    Ok(shaderc::ResolvedInclude {
                        resolved_name: String::from("generated://shader_prelude.glsl"),
                        content: shader_prelude.clone(),
                    })
                } else {
                    match std::fs::read_to_string(shaders_path.join(&requested_source_path)) {
                        Ok(included_source) => Ok(shaderc::ResolvedInclude {
                            resolved_name: String::from(requested_source_path),
                            content: included_source,
                        }),

                        Err(e) => Err(format!("failed to open {}: {}", &requested_source_path, e)),
                    }
                }
            },
        );
        let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
        vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);
        let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
        fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

        let mut compiler = shaderc::Compiler::new().expect("failed to initialize GLSL compiler");
        let vertex_stage = compiler
            .compile_into_spirv(
                &pbr_material_glsl,
                shaderc::ShaderKind::Vertex,
                "pbr_material.glsl",
                "main",
                Some(&vertex_stage_options),
            )
            .expect("failed to compile vertex shader");
        let fragment_stage = compiler
            .compile_into_spirv(
                &pbr_material_glsl,
                shaderc::ShaderKind::Fragment,
                "pbr_material.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader");

        static_scenery.materials.push(DiskMaterial {
            material_layout: static_scenery
                .material_layouts
                .iter()
                .position(|item| item.image_count == images.len())
                .expect("failed to find material layout"),
            vertex_stride: vertex_stride as _,
            vertex_format: attributes
                .iter()
                .map(|a| (a.format.as_raw(), a.location as _, a.offset as _))
                .collect(),
            vertex_stage: Vec::from(vertex_stage.as_binary()),
            fragment_stage: Vec::from(fragment_stage.as_binary()),
            fragment_alpha_test,
            fragment_cull_flags,
        });

        id
    }
}
