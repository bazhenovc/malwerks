// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_resources::*;

pub fn import_material_instances(
    materials: gltf::iter::Materials,
) -> (Vec<DiskMaterialLayout>, Vec<DiskMaterialInstance>) {
    let mut out_material_layouts = Vec::<DiskMaterialLayout>::with_capacity(materials.len());
    let mut out_material_instances = Vec::with_capacity(materials.len());

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

        let material_layout = match out_material_layouts
            .iter()
            .position(|item| item.image_count == images.len())
        {
            Some(id) => id,
            None => {
                let new_id = out_material_layouts.len();
                out_material_layouts.push(DiskMaterialLayout {
                    image_count: images.len(),
                });
                new_id
            }
        };

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct PackedMaterialData {
            base_color_factor: [f32; 4],
            metallic_roughness_discard_unused: [f32; 4],
            emissive_rgb_unused: [f32; 4],
            unused: [f32; 4],
        };
        unsafe impl bytemuck::Zeroable for PackedMaterialData {}
        unsafe impl bytemuck::Pod for PackedMaterialData {}
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
        let material_instance_data = bytemuck::bytes_of(&packed_data).to_vec();
        assert_eq!(material_instance_data.len(), 64);

        out_material_instances.push(DiskMaterialInstance {
            material_layout,
            material_instance_data,
            images,
        });
    }

    (out_material_layouts, out_material_instances)
}
