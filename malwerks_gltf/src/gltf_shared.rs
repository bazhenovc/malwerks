// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet as utv;

pub struct BoundingCone {
    pub cone_apex: utv::vec::Vec3,
    pub cone_axis: utv::vec::Vec3,
    pub cone_cutoff: f32,
}

impl BoundingCone {
    pub fn get_transformed(&self, matrix: &utv::mat::Mat4) -> BoundingCone {
        let cone_apex = utv::vec::Vec3::from(
            *matrix * utv::vec::Vec4::new(self.cone_apex.x, self.cone_apex.y, self.cone_apex.z, 1.0),
        );
        let cone_axis = utv::vec::Vec3::from(
            *matrix * utv::vec::Vec4::new(self.cone_axis.x, self.cone_axis.y, self.cone_axis.z, 0.0),
        );
        let cone_cutoff = self.cone_cutoff;
        Self {
            cone_apex,
            cone_axis,
            cone_cutoff,
        }
    }
}

pub struct PrimitiveRemap {
    pub mesh_id: usize,
    pub primitives: Vec<(usize, usize, usize, Vec<BoundingCone>)>, // mesh_index, material_id, material_instance_id
}
