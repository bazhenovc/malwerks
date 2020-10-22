// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_bundles::*;

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
    material_id: usize,
    vertex_stride: usize,
    attributes: &[Attribute<'a>],
    materials: gltf::iter::Materials,
    material_layouts: &[DiskMaterialLayout],
    in_attribute_cache: &mut Vec<&'a [Attribute<'a>]>,
    in_materials: &mut Vec<DiskMaterial>,
) -> usize {
    macro_rules! texture_prelude {
        ($images: ident, $texture: expr, $texture_name: expr) => {
            if let Some(image) = $texture {
                $images.push((String::from($texture_name), format!("VS_uv{}", image.tex_coord())));
            }
        };
    }

    let mut images = Vec::with_capacity(5);
    let material = materials.clone().nth(material_id).expect("failed to find material id");
    let pbr_metallic_roughness = material.pbr_metallic_roughness();

    texture_prelude!(images, pbr_metallic_roughness.base_color_texture(), "BaseColorTexture");
    texture_prelude!(
        images,
        pbr_metallic_roughness.metallic_roughness_texture(),
        "MetallicRoughnessTexture"
    );
    texture_prelude!(images, material.normal_texture(), "NormalTexture");
    texture_prelude!(images, material.occlusion_texture(), "OcclusionTexture");
    texture_prelude!(images, material.emissive_texture(), "EmissiveTexture");

    let fragment_alpha_test = match material.alpha_mode() {
        gltf::json::material::AlphaMode::Opaque => false,
        gltf::json::material::AlphaMode::Mask => true,
        gltf::json::material::AlphaMode::Blend => false,
    };
    let fragment_cull_flags = if material.double_sided() {
        vk::CullModeFlags::NONE.as_raw()
    } else {
        vk::CullModeFlags::BACK.as_raw()
    };

    let existing_id = in_attribute_cache.iter().position(|cached_attributes| {
        if cached_attributes.len() != attributes.len() {
            false
        } else {
            for i in 0..cached_attributes.len() {
                if cached_attributes[i].semantic != attributes[i].semantic
                    || cached_attributes[i].semantic_name != attributes[i].semantic_name
                    || cached_attributes[i].location != attributes[i].location
                    || cached_attributes[i].format != attributes[i].format
                {
                    return false;
                }
            }

            true
        }
    });
    if let Some(existing_id) = existing_id {
        existing_id
    } else {
        let id = in_materials.len();
        in_materials.push(DiskMaterial {
            material_layout: material_layouts
                .iter()
                .position(|item| item.image_count == images.len())
                .expect("failed to find material layout"),
            vertex_stride: vertex_stride as _,
            vertex_format: attributes
                .iter()
                .map(|a| DiskVertexAttribute {
                    attribute_name: a.semantic_name.clone(),
                    attribute_semantic: match a.semantic {
                        gltf::mesh::Semantic::Positions => DiskVertexSemantic::Position,
                        gltf::mesh::Semantic::Normals => DiskVertexSemantic::Normal,
                        gltf::mesh::Semantic::Tangents => DiskVertexSemantic::Tangent,
                        gltf::mesh::Semantic::TexCoords(_) => DiskVertexSemantic::Interpolated,

                        _ => unimplemented!("unsupported attribute semantic"),
                    },
                    attribute_format: a.format.as_raw(),
                    attribute_location: a.location as _,
                    attribute_offset: a.offset,
                })
                .collect(),

            fragment_alpha_test,
            fragment_cull_flags,

            shader_image_mapping: images,
            shader_macro_definitions: Vec::new(),
        });

        id
    }
}
