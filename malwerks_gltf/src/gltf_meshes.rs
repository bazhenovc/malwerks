// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_external::*;
use malwerks_resources::*;

use ash::vk;

use crate::gltf_materials::*;
use crate::gltf_shared::*;

pub fn import_meshes(
    base_path: &std::path::Path,
    buffers: gltf::iter::Buffers,
    _views: gltf::iter::Views,
    meshes: gltf::iter::Meshes,
    materials: gltf::iter::Materials,
    material_layouts: &[DiskMaterialLayout],
) -> (
    Vec<DiskBuffer>,
    Vec<DiskRenderMesh>,
    Vec<DiskMaterial>,
    Vec<PrimitiveRemap>,
) {
    let mut out_buffers = Vec::with_capacity(meshes.len() * 2);
    let mut out_meshes = Vec::with_capacity(meshes.len());
    let mut out_materials = Vec::with_capacity(meshes.len()); // Allocate for the worst case

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

    let mut attribute_cache = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        log::info!(
            "loading and optimizing mesh {:?} with {:?} primitives",
            mesh.name().unwrap_or_default(),
            mesh.primitives().len()
        );

        let mut per_primitive_remap = Vec::new();
        for primitive in mesh.primitives() {
            let material_id = match primitive.material().index() {
                Some(index) => index,
                None => panic!("primitive material is not defined"),
            };

            let mut sorted_attributes: Vec<gltf::mesh::Attribute> = primitive.attributes().collect();
            let position_attribute = sorted_attributes
                .iter()
                .position(|attr| attr.0 == gltf::mesh::Semantic::Positions)
                .unwrap();
            if position_attribute != 0 {
                sorted_attributes.swap(0, position_attribute);
            }

            if let Some(normal_attribute) = sorted_attributes
                .iter()
                .position(|attr| attr.0 == gltf::mesh::Semantic::Normals)
            {
                sorted_attributes.swap(1, normal_attribute);
            }
            if let Some(tangent_attribute) = sorted_attributes
                .iter()
                .position(|attr| attr.0 == gltf::mesh::Semantic::Tangents)
            {
                sorted_attributes.swap(2, tangent_attribute);
            }

            let mut vertex_format = Vec::with_capacity(primitive.attributes().len());
            let mut attributes = Vec::with_capacity(primitive.attributes().len());
            let mut attribute_offset = 0;

            for attribute in sorted_attributes {
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

                        _ => unimplemented!("unsupported attribute semantic"),
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

            let real_mesh_id = out_meshes.len();
            let real_material_id = generate_material(
                material_id,
                vertex_stride,
                &attributes,
                materials.clone(),
                material_layouts,
                &mut attribute_cache,
                &mut out_materials,
            );

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
            let (vertex_buffer, index_buffer, index_format) = if let Some(indices) = primitive.indices() {
                let index_count = indices.count();
                let (index_stride, index_format) = match indices.data_type() {
                    gltf::accessor::DataType::U16 => (2, vk::IndexType::UINT16),
                    gltf::accessor::DataType::U32 => (4, vk::IndexType::UINT32),
                    _ => panic!("unsupported index format"),
                };

                let mut index_data = Vec::new();
                index_data.resize(index_count * index_stride, 0u8);

                let index_view = indices.view().expect("index buffer view undefined");
                let indices_start = index_view.offset();
                let indices_end = indices_start + index_view.length();

                let src_slice = &temp_buffers[index_view.buffer().index()][indices_start..indices_end];
                index_data.copy_from_slice(src_slice);

                let (vertex_buffer, index_buffer) = optimize_mesh(
                    &vertex_data,
                    vertex_stride,
                    vertex_count,
                    &index_data,
                    index_stride,
                    index_count,
                );

                (vertex_buffer, index_buffer, index_format)
            } else {
                todo!("Need to generate an index buffer that just directly follows the vertex buffer");
            };

            let index_count = index_buffer.data.len() / (index_buffer.stride as usize);
            log::info!(
                "mesh {:?} optimized: vertices: {} -> {}, indices: {}",
                mesh.name().unwrap_or_default(),
                vertex_count,
                vertex_buffer.data.len() / (vertex_buffer.stride as usize),
                index_count,
            );

            let vertex_buffer_id = out_buffers.len();
            out_buffers.push(vertex_buffer);
            out_buffers.push(index_buffer);

            let disk_mesh = DiskRenderMesh {
                vertex_buffer: vertex_buffer_id,
                index_buffer: (index_format.as_raw(), vertex_buffer_id + 1),
                index_count,
            };
            per_primitive_remap.push((real_mesh_id, real_material_id, material_id));
            out_meshes.push(disk_mesh);
        }
        primitive_remap_table.push(PrimitiveRemap {
            mesh_id: mesh.index(),
            primitives: per_primitive_remap,
        });
    }

    (out_buffers, out_meshes, out_materials, primitive_remap_table)
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
