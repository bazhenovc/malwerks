// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ash::vk;
use malwerks_resources::*;

mod dds;
mod texconv;

use texconv::*;

fn import_gltf(file_name: &str) -> DiskStaticScenery {
    let mut static_scenery = DiskStaticScenery {
        images: Vec::new(),
        buffers: Vec::new(),
        meshes: Vec::new(),
        samplers: Vec::new(),
        material_layouts: Vec::new(),
        material_instances: Vec::new(),
        materials: Vec::new(),
        buckets: Vec::new(),
        environment_probes: Vec::new(),
    };

    let gltf = gltf::Gltf::open(file_name).expect("failed to open gltf");
    let base_path = std::path::Path::new(file_name)
        .parent()
        .expect("failed to get file base path");

    import_material_instances(&mut static_scenery, gltf.materials());
    let primitive_remap_table = import_meshes(
        &mut static_scenery,
        &base_path,
        gltf.buffers(),
        gltf.views(),
        gltf.meshes(),
        gltf.materials(),
    );
    import_nodes(&mut static_scenery, primitive_remap_table, gltf.nodes());
    import_images(&mut static_scenery, &base_path, gltf.materials(), gltf.images());
    import_samplers(&mut static_scenery, gltf.samplers());
    import_probes(&mut static_scenery, &base_path);
    static_scenery
}

struct PrimitiveRemap {
    mesh_id: usize,
    primitives: Vec<(usize, usize, usize)>, // mesh_index, material_id, material_instance_id
}

fn import_meshes(
    static_scenery: &mut DiskStaticScenery,
    base_path: &std::path::Path,
    buffers: gltf::iter::Buffers,
    _views: gltf::iter::Views,
    meshes: gltf::iter::Meshes,
    materials: gltf::iter::Materials,
) -> Vec<PrimitiveRemap> {
    static_scenery.buffers.reserve_exact(meshes.len() * 2);
    static_scenery.meshes.reserve_exact(meshes.len());
    static_scenery.materials.reserve_exact(meshes.len());

    let mut primitive_remap_table = Vec::with_capacity(meshes.len());

    let mut temp_buffers = Vec::with_capacity(buffers.len());
    for buffer in buffers {
        match buffer.source() {
            gltf::buffer::Source::Bin => panic!("bin section is not supported"),
            gltf::buffer::Source::Uri(path) => {
                use std::io::Read;

                let file_path = base_path.join(path);
                log::info!("loading buffer: {:?}", &file_path);

                let mut buffer_data = Vec::new();
                buffer_data.resize(buffer.length(), 0u8);

                let mut file = std::fs::File::open(file_path).expect("failed to open buffer file");
                file.read_exact(buffer_data.as_mut_slice())
                    .expect("failed to read buffer file");

                temp_buffers.push(buffer_data);
            }
        }
    }

    let mut temp_preludes = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        log::info!(
            "loading mesh {:?} with {:?} primitives",
            mesh.name().unwrap_or_default(),
            mesh.primitives().len()
        );

        let mut per_primitive_remap = Vec::new();
        for primitive in mesh.primitives() {
            let material_id = match primitive.material().index() {
                Some(index) => index,
                None => panic!("primitive material is not defined"),
            };

            //let mut sorted_attributes: Vec<gltf::mesh::Attribute> = primitive.attributes().collect();
            //sorted_attributes.sort_by(|a, b| {
            //    if a.0 == gltf::mesh::Semantic::Positions {
            //        std::cmp::Ordering::Less
            //    } else if b.0 == gltf::mesh::Semantic::Positions {
            //        std::cmp::Ordering::Greater
            //    } else {
            //        std::cmp::Ordering::Equal
            //    }
            //});

            let mut vertex_format = Vec::with_capacity(primitive.attributes().len());
            let mut attributes = Vec::with_capacity(primitive.attributes().len());
            let mut attribute_offset = 0;
            for attribute in primitive.attributes() {
                let accessor: gltf::accessor::Accessor = attribute.1;
                let view = accessor.view().expect("no buffer view for attribute");
                let offset = view.offset();
                let length = view.length();
                let location = attributes.len();

                let data = &temp_buffers[view.buffer().index()][offset..offset + length];
                let (stride, format, type_name) = convert_to_format(&accessor);

                attributes.push(Attribute {
                    semantic: attribute.0.clone(),
                    semantic_name: match attribute.0 {
                        gltf::mesh::Semantic::Positions => String::from("position"),
                        gltf::mesh::Semantic::Normals => String::from("normal"),
                        gltf::mesh::Semantic::Tangents => String::from("tangent"),
                        gltf::mesh::Semantic::TexCoords(idx) => format!("uv{}", idx),

                        _ => panic!("attribute semantic is not supported"),
                    },
                    location,
                    format,
                    type_name,
                    //data_type: accessor.data_type(),
                    //dimensions: accessor.dimensions(),
                    count: accessor.count(),
                    stride,
                    offset: attribute_offset,
                    data,
                });

                attribute_offset += stride;
                vertex_format.push(format.as_raw());
            }

            let vertex_count = attributes[0].count;
            let mut vertex_stride = 0;
            for attribute in &attributes {
                vertex_stride += attribute.stride;
            }

            let real_mesh_id = static_scenery.meshes.len();
            let real_material_id = generate_material(
                static_scenery,
                base_path,
                material_id,
                vertex_stride,
                &mut temp_preludes,
                &attributes,
                materials.clone(),
            );
            per_primitive_remap.push((real_mesh_id, real_material_id, material_id));

            let disk_mesh = DiskMesh {
                vertex_buffer: static_scenery.buffers.len(),
                index_buffer: match primitive.indices() {
                    Some(indices) => Some((
                        static_scenery.buffers.len() + 1,
                        match indices.data_type() {
                            gltf::accessor::DataType::U16 => vk::IndexType::UINT16.as_raw(),
                            gltf::accessor::DataType::U32 => vk::IndexType::UINT32.as_raw(),
                            _ => panic!("unsupported index data type"),
                        },
                    )),
                    None => None,
                },
                draw_count: match primitive.indices() {
                    Some(indices) => indices.count() as _,
                    None => vertex_count as _,
                },
                //vertex_format,
                //material_id: real_material_id,
            };
            static_scenery.meshes.push(disk_mesh);

            let mut vertex_data = Vec::new();
            vertex_data.resize(vertex_count * vertex_stride, 0u8);

            for vertex_id in 0..vertex_count {
                let mut vertex_offset = vertex_id * vertex_stride;
                for attribute in &attributes {
                    assert_eq!(attribute.count, vertex_count);
                    let attribute_offset = vertex_id * attribute.stride;

                    let src_slice = &attribute.data[attribute_offset..attribute_offset + attribute.stride];
                    let dst_slice = &mut vertex_data[vertex_offset..vertex_offset + attribute.stride];
                    dst_slice.copy_from_slice(src_slice);

                    vertex_offset += attribute.stride;
                }
            }

            // TODO: Detect and merge identical buffers
            static_scenery.buffers.push(DiskBuffer {
                data: vertex_data,
                stride: vertex_stride,
                usage_flags: (vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST).as_raw(),
            });

            if let Some(indices) = primitive.indices() {
                let index_count = indices.count();
                let index_stride = match indices.data_type() {
                    gltf::accessor::DataType::U16 => 2,
                    gltf::accessor::DataType::U32 => 4,
                    _ => panic!("unsupported index format"),
                };

                let mut index_data = Vec::new();
                index_data.resize(index_count * index_stride, 0u8);

                let index_view = indices.view().expect("index buffer view undefined");
                let indices_start = index_view.offset();
                let indices_end = indices_start + index_view.length();

                let src_slice = &temp_buffers[index_view.buffer().index()][indices_start..indices_end];
                index_data.copy_from_slice(src_slice);

                static_scenery.buffers.push(DiskBuffer {
                    data: index_data,
                    stride: index_stride,
                    usage_flags: (vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST).as_raw(),
                });
            }
        }
        primitive_remap_table.push(PrimitiveRemap {
            mesh_id: mesh.index(),
            primitives: per_primitive_remap,
        });
    }
    primitive_remap_table
}

struct Attribute<'a> {
    semantic: gltf::mesh::Semantic,
    semantic_name: String,
    location: usize,
    format: vk::Format,
    //data_type: gltf::accessor::DataType,
    //dimensions: gltf::accessor::Dimensions,
    type_name: &'a str,
    count: usize,
    stride: usize,
    offset: usize,
    data: &'a [u8],
}

fn generate_material<'a>(
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
    shader_prelude.push_str(&format!(
        "layout (location = {}) in mat4 WorldTransform;\n",
        last_attribute_location + 1
    ));
    //shader_prelude.push_str(&format!(
    //    "layout (location = {}) in mat3 NormalTransform;\n",
    //    last_attribute_location + 5,
    //));
    shader_prelude.push_str("vec4 generated_vertex_shader() {\n");
    shader_prelude.push_str("    mat4 normal_transform = transpose(inverse(WorldTransform));\n");
    for attribute in attributes {
        match attribute.semantic {
            gltf::mesh::Semantic::Positions => shader_prelude.push_str(&format!(
                "    VS_{0} = (WorldTransform * vec4(IN_{0}.xyz, 1.0)).xyz;\n",
                attribute.semantic_name
            )),

            gltf::mesh::Semantic::Normals => shader_prelude.push_str(&format!(
                //"    VS_{0} = normalize(mat3(WorldTransform) * IN_{0});\n",
                "    VS_{0} = normalize(mat3(normal_transform) * IN_{0});\n",
                attribute.semantic_name
            )),

            gltf::mesh::Semantic::Tangents => shader_prelude.push_str(&format!(
                "    VS_{0} = vec4(normalize(mat3(WorldTransform) * IN_{0}.xyz), IN_{0}.w);\n",
                //"    VS_{0} = WorldTransform * IN_{0};\n",
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
                    "layout (set = 1, binding = {}) uniform sampler2D {};\n",
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

    //let has_alpha_test = match material.alpha_mode() {
    //    gltf::json::material::AlphaMode::Opaque => false,
    //    gltf::json::material::AlphaMode::Mask => {
    //        shader_prelude.push_str("#define HAS_AlphaDiscard\n");
    //        true
    //    }
    //    gltf::json::material::AlphaMode::Blend => false,
    //};

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

        let pbr_material_glsl = std::fs::read_to_string(
            base_path
                .join("..")
                .join("..")
                .join("malwerks_shaders")
                .join("pbr_material.glsl"),
        )
        .expect("failed to open pbr_material.glsl");

        let id = temp_preludes.len();
        temp_preludes.push(shader_prelude.clone());

        let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
        compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
        compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
        compile_options.set_warnings_as_errors();
        compile_options.set_include_callback(
            |requested_source_path, _directive_type, _contained_within_path, _recursion_depth| {
                if requested_source_path == "shader_prelude.glsl" {
                    Ok(shaderc::ResolvedInclude {
                        resolved_name: String::from("shader_prelude.glsl"),
                        content: shader_prelude.clone(),
                    })
                } else {
                    Err(format!("failed to find include file: {}", requested_source_path))
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
            //fragment_alpha_discard: has_alpha_test,
        });

        id
    }
}

fn import_nodes(
    static_scenery: &mut DiskStaticScenery,
    primitive_remap: Vec<PrimitiveRemap>,
    nodes: gltf::iter::Nodes,
) {
    use std::collections::HashMap;

    let mut buckets = HashMap::<usize, HashMap<(usize, usize), Vec<[f32; 16]>>>::new();
    for node in nodes {
        if let Some(mesh) = node.mesh() {
            log::info!("importing node {:?}", node.name().unwrap_or("<unnamed>"));
            let remap = &primitive_remap[mesh.index()];
            assert_eq!(remap.mesh_id, mesh.index());

            for (mesh_index, material_id, material_instance_id) in &remap.primitives {
                let instance_transform = {
                    let node_transform = node.transform().matrix();
                    let mut transform: [f32; 16] = [0.0; 16];
                    (&mut transform[0..4]).copy_from_slice(&node_transform[0]);
                    (&mut transform[4..8]).copy_from_slice(&node_transform[1]);
                    (&mut transform[8..12]).copy_from_slice(&node_transform[2]);
                    (&mut transform[12..16]).copy_from_slice(&node_transform[3]);
                    transform
                };

                match buckets.get_mut(&material_id) {
                    Some(bucket) => match bucket.get_mut(&(*mesh_index, *material_instance_id)) {
                        Some(instance) => {
                            instance.push(instance_transform);
                        }
                        None => {
                            bucket.insert((*mesh_index, *material_instance_id), vec![instance_transform]);
                        }
                    },
                    None => {
                        let mut new_value = HashMap::new();
                        new_value.insert((*mesh_index, *material_instance_id), vec![instance_transform]);
                        buckets.insert(*material_id, new_value);
                    }
                }
            }
        }
    }

    static_scenery.buckets = buckets
        .into_iter()
        .map(|(material, instances)| DiskRenderBucket {
            material,
            instances: instances
                .into_iter()
                .map(|((mesh, material_instance), transforms)| DiskRenderInstance {
                    mesh,
                    material_instance,
                    transforms,
                })
                .collect(),
        })
        .collect();
}

fn import_images(
    static_scenery: &mut DiskStaticScenery,
    base_path: &std::path::Path,
    materials: gltf::iter::Materials,
    images: gltf::iter::Images,
) {
    macro_rules! update_image_usage {
        ($image_usage: ident, $texture: expr, $usage: expr) => {
            if let Some(info) = $texture {
                if let Some(old_usage) = $image_usage[info.texture().index()] {
                    assert_eq!(old_usage, $usage);
                } else {
                    $image_usage[info.texture().index()] = Some($usage);
                }
            }
        };
    }

    let mut images_usage = Vec::with_capacity(images.len());
    images_usage.resize(images.len(), None);

    for material in materials {
        let pbr_metallic_roughness = material.pbr_metallic_roughness();
        update_image_usage!(
            images_usage,
            pbr_metallic_roughness.base_color_texture(),
            ImageUsage::SrgbColor
        );
        update_image_usage!(
            images_usage,
            pbr_metallic_roughness.metallic_roughness_texture(),
            ImageUsage::MetallicRoughnessMap
        );
        update_image_usage!(images_usage, material.normal_texture(), ImageUsage::NormalMap);
        update_image_usage!(
            images_usage,
            material.occlusion_texture(),
            ImageUsage::AmbientOcclusionMap
        );
        update_image_usage!(images_usage, material.emissive_texture(), ImageUsage::SrgbColor);
    }

    static_scenery.images.reserve_exact(images.len());
    for image in images {
        let image_path = match image.source() {
            gltf::image::Source::View { .. } => panic!("buffer image views are not supported right now"),
            gltf::image::Source::Uri { uri, .. } => base_path.join(uri),
        };
        let image_index = static_scenery.images.len();
        let image_usage = match images_usage[image_index] {
            Some(usage) => usage,
            None => {
                log::warn!("unused texture: {:?}", image.source());
                ImageUsage::SrgbColor
            }
        };

        log::info!("importing image: {:?} as {:?}", &image_path, image_usage);
        static_scenery
            .images
            .push(compress_image(image_usage, base_path, &image_path));
    }
}

// TODO: make this less specific
fn import_probes(static_scenery: &mut DiskStaticScenery, base_path: &std::path::Path) {
    let probe_image_id = static_scenery.images.len();
    let probe_path = base_path.join("probe");
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentSkybox,
        base_path,
        &probe_path.join("output_skybox.dds"),
    ));
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentIem,
        base_path,
        &probe_path.join("output_iem.dds"),
    ));
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentPmrem,
        base_path,
        &probe_path.join("output_pmrem.dds"),
    ));
    static_scenery.images.push(compress_image(
        ImageUsage::EnvironmentBrdf,
        base_path,
        &probe_path.join("brdf.dds"),
    ));

    let skybox_glsl = std::fs::read_to_string(
        base_path
            .join("..")
            .join("..")
            .join("malwerks_shaders")
            .join("sky_box.glsl"),
    )
    .expect("failed to open sky_box.glsl");

    let mut compile_options = shaderc::CompileOptions::new().expect("failed to initialize GLSL compiler options");
    compile_options.set_source_language(shaderc::SourceLanguage::GLSL);
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    compile_options.set_warnings_as_errors();

    let mut vertex_stage_options = compile_options.clone().expect("failed to clone vertex options");
    vertex_stage_options.add_macro_definition("VERTEX_STAGE", None);
    let mut fragment_stage_options = compile_options.clone().expect("failed to clone fragment options");
    fragment_stage_options.add_macro_definition("FRAGMENT_STAGE", None);

    let mut compiler = shaderc::Compiler::new().expect("failed to initialize GLSL compiler");
    let skybox_vertex_stage = Vec::from(
        compiler
            .compile_into_spirv(
                &skybox_glsl,
                shaderc::ShaderKind::Vertex,
                "sky_box.glsl",
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
                "sky_box.glsl",
                "main",
                Some(&fragment_stage_options),
            )
            .expect("failed to compile fragment shader")
            .as_binary(),
    );

    static_scenery.environment_probes.reserve_exact(1);
    static_scenery.environment_probes.push(DiskEnvironmentProbe {
        skybox_image: probe_image_id,
        skybox_vertex_stage,
        skybox_fragment_stage,
        iem_image: probe_image_id + 1,
        pmrem_image: probe_image_id + 2,
        precomputed_brdf_image: probe_image_id + 3,
    });
}

fn import_samplers(static_scenery: &mut DiskStaticScenery, samplers: gltf::iter::Samplers) {
    if samplers.len() == 0 {
        static_scenery.samplers.reserve_exact(1);
        static_scenery.samplers.push(DiskSampler {
            mag_filter: vk::Filter::LINEAR.as_raw(),
            min_filter: vk::Filter::LINEAR.as_raw(),
            mipmap_mode: vk::SamplerMipmapMode::LINEAR.as_raw(),
            address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE.as_raw(),
            address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE.as_raw(),
            address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE.as_raw(),
        });
    } else {
        static_scenery.samplers.reserve_exact(samplers.len());
        for sampler in samplers {
            let disk_sampler = DiskSampler {
                mag_filter: match sampler.mag_filter() {
                    Some(filter) => match filter {
                        gltf::texture::MagFilter::Nearest => vk::Filter::NEAREST,
                        gltf::texture::MagFilter::Linear => vk::Filter::LINEAR,
                    }
                    .as_raw(),
                    None => vk::Filter::LINEAR.as_raw(),
                },
                min_filter: match sampler.min_filter() {
                    Some(filter) => {
                        match filter {
                            gltf::texture::MinFilter::Nearest => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::Linear => vk::Filter::LINEAR,

                            // These filters are used in combination with mipmap mode below
                            gltf::texture::MinFilter::NearestMipmapNearest => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::NearestMipmapLinear => vk::Filter::NEAREST,
                            gltf::texture::MinFilter::LinearMipmapNearest => vk::Filter::LINEAR,
                            gltf::texture::MinFilter::LinearMipmapLinear => vk::Filter::LINEAR,
                        }
                        .as_raw()
                    }
                    None => vk::Filter::LINEAR.as_raw(),
                },
                mipmap_mode: match sampler.min_filter() {
                    Some(filter) => {
                        match filter {
                            // These filters are used in combination with min filter above
                            gltf::texture::MinFilter::NearestMipmapNearest => vk::SamplerMipmapMode::NEAREST,
                            gltf::texture::MinFilter::LinearMipmapNearest => vk::SamplerMipmapMode::NEAREST,
                            gltf::texture::MinFilter::NearestMipmapLinear => vk::SamplerMipmapMode::LINEAR,
                            gltf::texture::MinFilter::LinearMipmapLinear => vk::SamplerMipmapMode::LINEAR,

                            _ => vk::SamplerMipmapMode::LINEAR,
                        }
                        .as_raw()
                    }
                    None => vk::SamplerMipmapMode::LINEAR.as_raw(),
                },
                address_mode_u: convert_wrap_mode(sampler.wrap_s()).as_raw(),
                address_mode_v: convert_wrap_mode(sampler.wrap_t()).as_raw(),
                address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE.as_raw(),
            };
            static_scenery.samplers.push(disk_sampler);
        }
    }
}

fn import_material_instances(static_scenery: &mut DiskStaticScenery, materials: gltf::iter::Materials) {
    static_scenery.material_layouts.reserve_exact(materials.len());
    static_scenery.material_instances.reserve_exact(materials.len());

    for material in materials {
        let mut images = Vec::with_capacity(5);
        macro_rules! instance_texture {
            ($images: ident, $texture: expr) => {
                if let Some(image) = $texture {
                    $images.push((
                        image.texture().index(),
                        image.texture().sampler().index().unwrap_or(0),
                    ));
                }
            };
        }

        let pbr_metallic_roughness = material.pbr_metallic_roughness();
        instance_texture!(images, pbr_metallic_roughness.base_color_texture());
        instance_texture!(images, pbr_metallic_roughness.metallic_roughness_texture());
        instance_texture!(images, material.normal_texture());
        instance_texture!(images, material.occlusion_texture());
        instance_texture!(images, material.emissive_texture());

        let material_layout = match static_scenery
            .material_layouts
            .iter()
            .position(|item| item.image_count == images.len())
        {
            Some(id) => id,
            None => {
                let new_id = static_scenery.material_layouts.len();
                static_scenery.material_layouts.push(DiskMaterialLayout {
                    image_count: images.len(),
                });
                new_id
            }
        };

        #[repr(C)]
        #[derive(serde::Serialize)]
        struct PackedMaterialData {
            base_color_factor: [f32; 4],
            metallic_roughness_discard_unused: [f32; 4],
            emissive_rgb_unused: [f32; 4],
            unused: [f32; 4],
        };
        assert_eq!(std::mem::size_of::<PackedMaterialData>(), 64);

        let packed_data = PackedMaterialData {
            base_color_factor: pbr_metallic_roughness.base_color_factor(),
            metallic_roughness_discard_unused: [
                pbr_metallic_roughness.metallic_factor(),
                pbr_metallic_roughness.roughness_factor(),
                material.alpha_cutoff(),
                0.0,
            ],
            emissive_rgb_unused: [
                material.emissive_factor()[0],
                material.emissive_factor()[0],
                material.emissive_factor()[0],
                0.0,
            ],
            unused: [0.0f32; 4],
        };
        let material_data = bincode::serialize(&packed_data).expect("failed to serialize material instance data");
        assert_eq!(material_data.len(), 64);

        static_scenery.material_instances.push(DiskMaterialInstance {
            material_layout,
            material_data,
            images,
        });
    }
}

fn convert_wrap_mode(mode: gltf::texture::WrappingMode) -> vk::SamplerAddressMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        gltf::texture::WrappingMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        gltf::texture::WrappingMode::Repeat => vk::SamplerAddressMode::REPEAT,
    }
}

fn convert_to_format(accessor: &gltf::accessor::Accessor) -> (usize, vk::Format, &'static str) {
    match accessor.dimensions() {
        gltf::accessor::Dimensions::Scalar => match accessor.data_type() {
            gltf::accessor::DataType::U8 => (1, vk::Format::R8_UINT, "uint8_t"),
            gltf::accessor::DataType::U16 => (2, vk::Format::R16_UINT, "uint16_t"),
            gltf::accessor::DataType::U32 => (4, vk::Format::R32_UINT, "uint"),
            gltf::accessor::DataType::I8 => (1, vk::Format::R8_SINT, "int8_t"),
            gltf::accessor::DataType::I16 => (2, vk::Format::R16_SINT, "int16_t"),
            //gltf::accessor::DataType::I32 => (4, vk::Format::R32_SINT),
            gltf::accessor::DataType::F32 => (4, vk::Format::R32_SFLOAT, "float"),
        },

        gltf::accessor::Dimensions::Vec2 => match accessor.data_type() {
            gltf::accessor::DataType::U8 => (2, vk::Format::R8G8_UINT, "u8vec2"),
            gltf::accessor::DataType::U16 => (4, vk::Format::R16G16_UINT, "u16vec2"),
            gltf::accessor::DataType::U32 => (8, vk::Format::R32G32_UINT, "uvec2"),
            gltf::accessor::DataType::I8 => (2, vk::Format::R8G8_SINT, "i8vec2"),
            gltf::accessor::DataType::I16 => (4, vk::Format::R16G16_SINT, "i16vec2"),
            //gltf::accessor::DataType::I32 => (8, vk::Format::R32G32_SINT),
            gltf::accessor::DataType::F32 => (8, vk::Format::R32G32_SFLOAT, "vec2"),
        },

        gltf::accessor::Dimensions::Vec3 => match accessor.data_type() {
            gltf::accessor::DataType::U8 => (3, vk::Format::R8G8B8_UINT, "u8vec3"),
            gltf::accessor::DataType::U16 => (6, vk::Format::R16G16B16_UINT, "u16vec3"),
            gltf::accessor::DataType::U32 => (12, vk::Format::R32G32B32_UINT, "uvec3"),
            gltf::accessor::DataType::I8 => (3, vk::Format::R8G8B8_SINT, "i8vec3"),
            gltf::accessor::DataType::I16 => (6, vk::Format::R16G16B16_SINT, "i16vec3"),
            //gltf::accessor::DataType::I32 => (12, vk::Format::R32G32B32_SINT),
            gltf::accessor::DataType::F32 => (12, vk::Format::R32G32B32_SFLOAT, "vec3"),
        },

        gltf::accessor::Dimensions::Vec4 => match accessor.data_type() {
            gltf::accessor::DataType::U8 => (4, vk::Format::R8G8B8A8_UINT, "u8vec4"),
            gltf::accessor::DataType::U16 => (8, vk::Format::R16G16B16A16_UINT, "u16vec4"),
            gltf::accessor::DataType::U32 => (16, vk::Format::R32G32B32A32_UINT, "uvec4"),
            gltf::accessor::DataType::I8 => (4, vk::Format::R8G8B8A8_SINT, "i8vec4"),
            gltf::accessor::DataType::I16 => (8, vk::Format::R16G16B16A16_SINT, "i16vec4"),
            //gltf::accessor::DataType::I32 => (16, vk::Format::R32G32B32A32_SINT),
            gltf::accessor::DataType::F32 => (16, vk::Format::R32G32B32A32_SFLOAT, "vec4"),
        },

        _ => panic!("unsupported vertex element type"),
        //gltf::accessor::Dimensions::Mat2 => base_size * 2 * 2,
        //gltf::accessor::Dimensions::Mat3 => base_size * 3 * 3,
        //gltf::accessor::Dimensions::Mat4 => base_size * 4 * 4,
    }
}

fn main() {
    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let static_scenery = import_gltf(&args[1]);

    let out_path = std::path::Path::new(&args[1]).with_extension("world");
    log::info!(
        "saving {} buffers, {} meshes, {} images, {} samplers, {} layouts, {} instances, {} materials, {} buckets to {:?}",
        static_scenery.buffers.len(),
        static_scenery.meshes.len(),
        static_scenery.images.len(),
        static_scenery.samplers.len(),
        static_scenery.material_layouts.len(),
        static_scenery.material_instances.len(),
        static_scenery.materials.len(),
        static_scenery.buckets.len(),
        &out_path,
    );

    let encoded = bincode::serialize(&static_scenery).expect("failed to serialize static scenery");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(out_path)
            .expect("failed to open output file");
        file.write_all(&encoded[..]).expect("failed to write serialized data");
    }
}
