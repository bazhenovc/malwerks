// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet as utv;

pub struct BoundingBox {
    pub min: utv::vec::Vec3,
    pub max: utv::vec::Vec3,
}

impl BoundingBox {
    pub fn new_empty() -> Self {
        Self {
            min: utv::vec::Vec3::new(std::f32::MAX, std::f32::MAX, std::f32::MAX),
            max: utv::vec::Vec3::new(-std::f32::MAX, -std::f32::MAX, -std::f32::MAX),
        }
    }

    pub fn insert_point(&mut self, pt: utv::vec::Vec3) {
        self.min = self.min.min_by_component(pt);
        self.max = self.max.max_by_component(pt);
    }

    pub fn get_transformed(&self, mat: &utv::mat::Mat4) -> Self {
        let min = utv::vec::Vec3::from(*mat * utv::vec::Vec4::new(self.min.x, self.min.y, self.min.z, 1.0));
        let max = utv::vec::Vec3::from(*mat * utv::vec::Vec4::new(self.max.x, self.max.y, self.max.z, 1.0));

        Self {
            min: min.min_by_component(max),
            max: max.max_by_component(min),
        }
    }
}

pub struct PrimitiveRemap {
    pub mesh_id: usize,
    pub primitives: Vec<(usize, usize, usize, BoundingBox)>, // mesh_index, material_id, material_instance_id
}
