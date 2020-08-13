// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

use ash::vk;
use ultraviolet as utv;

use crate::gltf_materials::*;
use crate::gltf_shared::*;

use crate::meshopt::*;

pub fn import_meshes(
    static_scenery: &mut DiskStaticScenery,
    base_path: &std::path::Path,
    buffers: gltf::iter::Buffers,
    _views: gltf::iter::Views,
    meshes: gltf::iter::Meshes,
    materials: gltf::iter::Materials,
    optimize_geometry: bool,
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
            "{} {:?} with {:?} primitives",
            if optimize_geometry {
                "loading and optimizing mesh"
            } else {
                "loading mesh"
            },
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

            let mut bounding_box = BoundingBox::new_empty();

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

                    if attribute.semantic == gltf::mesh::Semantic::Positions {
                        let f32_slice = unsafe {
                            assert!(src_slice.len() <= std::mem::size_of::<[f32; 3]>());

                            #[allow(clippy::cast_ptr_alignment)]
                            std::ptr::read_unaligned(src_slice.as_ptr() as *const [f32; 3])
                        };
                        bounding_box.insert_point(utv::vec::Vec3::new(f32_slice[0], f32_slice[1], f32_slice[2]));
                    }
                }
            }

            // TODO: Detect and merge identical buffers
            let vertex_buffer = static_scenery.buffers.len();
            static_scenery.buffers.push(DiskBuffer {
                data: vertex_data,
                stride: vertex_stride as _,
                usage_flags: (vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST).as_raw(),
            });

            let (index_buffer, index_count) = if let Some(indices) = primitive.indices() {
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

                let index_buffer = static_scenery.buffers.len();
                static_scenery.buffers.push(DiskBuffer {
                    data: index_data,
                    stride: index_stride as _,
                    usage_flags: (vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST).as_raw(),
                });

                let (index_type, index_count) = if optimize_geometry {
                    optimize_mesh(&mut static_scenery.buffers, vertex_buffer, (index_buffer, index_stride))
                } else {
                    (
                        match indices.data_type() {
                            gltf::accessor::DataType::U16 => vk::IndexType::UINT16,
                            gltf::accessor::DataType::U32 => vk::IndexType::UINT32,
                            _ => panic!("unsupported index data type"),
                        },
                        indices.count() as _,
                    )
                };

                (Some((index_buffer, index_type.as_raw())), index_count)
            } else {
                if optimize_geometry {
                    let index_buffer = static_scenery.buffers.len();
                    static_scenery.buffers.push(DiskBuffer {
                        data: vec![],
                        stride: 0,
                        usage_flags: (vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST).as_raw(),
                    });

                    let (index_type, index_count) =
                        optimize_mesh(&mut static_scenery.buffers, vertex_buffer, (index_buffer, 0));

                    (Some((index_buffer, index_type.as_raw())), index_count as _)
                } else {
                    (None, 0)
                }
            };

            let vertex_count = vertex_count as _;
            let vertex_stride = vertex_stride as _;
            let disk_mesh = DiskMesh {
                vertex_buffer,
                vertex_count,
                vertex_stride,
                index_buffer,
                index_count,

                bounding_box: {
                    let mut min = [0.0; 3];
                    min.copy_from_slice(bounding_box.min.as_slice());

                    let mut max = [0.0; 3];
                    max.copy_from_slice(bounding_box.max.as_slice());

                    (min, max)
                },
            };
            static_scenery.meshes.push(disk_mesh);
            per_primitive_remap.push((real_mesh_id, real_material_id, material_id, bounding_box));
        }
        primitive_remap_table.push(PrimitiveRemap {
            mesh_id: mesh.index(),
            primitives: per_primitive_remap,
        });
    }
    primitive_remap_table
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
