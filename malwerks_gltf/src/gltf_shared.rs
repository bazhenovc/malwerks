// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub struct PrimitiveRemap {
    pub mesh_id: usize,
    pub primitives: Vec<(usize, usize, usize)>, // mesh_index, material_id, material_instance_id
}
