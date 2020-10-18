// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet as utv;

#[derive(Debug)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct Camera {
    pub position: utv::vec::Vec3,
    pub orientation: utv::rotor::Rotor3,

    viewport: Viewport,
    view_projection: utv::mat::Mat4,
    field_of_view: f32,
    aspect_ratio: f32,
}

impl Camera {
    pub fn new(field_of_view: f32, viewport: Viewport) -> Self {
        let width = (viewport.width - (viewport.x as u32)) as f32;
        let height = (viewport.height - (viewport.y as u32)) as f32;
        let aspect_ratio = width / height;

        Self {
            position: utv::vec::Vec3::new(0.0, 0.0, 0.0),
            orientation: utv::rotor::Rotor3::identity(),

            viewport,
            view_projection: utv::mat::Mat4::identity(),
            field_of_view,
            aspect_ratio,
        }
    }

    //pub fn set_target(&mut self, target: utv::vec::Vec3) {
    //    let target_direction = (target - self.position).normalized();
    //    self.orientation = utv::rotor::Rotor3::from_rotation_between()
    //}

    pub fn get_viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn get_view_projection(&self) -> &utv::mat::Mat4 {
        &self.view_projection
    }

    pub fn move_by(&mut self, amount: utv::vec::Vec3) {
        self.position += self.orientation.reversed() * amount;
    }

    pub fn rotate_by(&mut self, angles: utv::vec::Vec2) {
        self.orientation = utv::rotor::Rotor3::from_rotation_xz(to_radians(angles.x)) * self.orientation;
        self.orientation = utv::rotor::Rotor3::from_rotation_yz(to_radians(angles.y)) * self.orientation;
        self.orientation = self.orientation.normalized();
    }

    pub fn update_matrices(&mut self) {
        self.view_projection =
            utv::projection::perspective_reversed_infinite_z_vk(to_radians(self.field_of_view), self.aspect_ratio, 0.1)
                * self.orientation.into_matrix().into_homogeneous()
                * utv::mat::Mat4::from_translation(self.position)
    }
}

fn to_radians(f: f32) -> f32 {
    f * (std::f32::consts::PI / 180.0)
}
