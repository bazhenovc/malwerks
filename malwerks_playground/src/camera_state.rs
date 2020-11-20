// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use ultraviolet as utv;

use crate::input_map::{InputAction, InputActionType};

pub struct CameraState {
    camera: Camera,
    movement_speed: f32,
    forward_movement: f32,
    sideways_movement: f32,
    upwards_movement: f32,

    rotation_speed: f32,
    rotation_x: f32,
    rotation_y: f32,
}

impl CameraState {
    pub fn new(viewport: Viewport) -> Self {
        Self {
            camera: Camera::new(45.0, viewport),

            movement_speed: 10.0,
            forward_movement: 0.0,
            sideways_movement: 0.0,
            upwards_movement: 0.0,

            rotation_speed: 50.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
        }
    }

    pub fn handle_action_queue(&mut self, actions: &[InputAction]) {
        for action in actions {
            match action.action_type {
                InputActionType::CameraMove => self.forward_movement = action.action_value,
                InputActionType::CameraStrafe => self.sideways_movement = action.action_value,
                InputActionType::CameraLift => self.upwards_movement = action.action_value,
                InputActionType::CameraRotateX => self.rotation_x = action.action_value,
                InputActionType::CameraRotateY => self.rotation_y = action.action_value,
            }
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        let movement_delta = self.movement_speed * delta_time;
        //self.camera.move_forward(self.forward_movement * movement_delta);
        //self.camera.move_sideways(self.sideways_movement * movement_delta);
        //self.camera.move_up(self.upwards_movement * movement_delta);
        self.camera.move_by(
            utv::vec::Vec3::new(self.sideways_movement, self.upwards_movement, self.forward_movement) * movement_delta,
        );

        let rotation_delta = self.rotation_speed * delta_time;
        self.camera.rotate_by(utv::vec::Vec2::new(
            self.rotation_x * rotation_delta,
            self.rotation_y * rotation_delta,
        ));

        self.camera.update_matrices();

        self.rotation_x = 0.0;
        self.rotation_y = 0.0;
    }

    pub fn get_camera(&self) -> &Camera {
        &self.camera
    }

    pub fn get_camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }
}
