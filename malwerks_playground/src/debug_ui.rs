// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::camera_state::*;

use crate::demo_pbr_forward_lit::*;

pub fn show_debug_window<'a>(
    ui: &imgui::Ui<'a>,
    _window: &winit::window::Window,
    gilrs: &gilrs::Gilrs,
    camera_state: &mut CameraState,
    average_frame_time: f32,
    average_fps: f32,
) {
    use imgui::*;

    puffin::profile_function!();
    Window::new(im_str!("Debugging tools"))
        .always_auto_resize(true)
        .build(ui, || {
            // profiler
            if CollapsingHeader::new(im_str!("Performance"))
                .default_open(true)
                .build(ui)
            {
                ui.text(im_str!(
                    "Application frame time: {}ms, FPS: {}",
                    average_frame_time,
                    average_fps
                ));

                if ui.button(im_str!("Toggle profiler"), [0.0, 0.0]) {
                    puffin::set_scopes_on(!puffin::are_scopes_on());
                }
            }

            // camera
            if CollapsingHeader::new(im_str!("Camera")).default_open(true).build(ui) {
                let camera = camera_state.get_camera_mut();
                ui.text(ImString::from(format!("{:?}", camera.position)));
                ui.text(ImString::from(format!("{:?}", camera.orientation)));
                ui.text(ImString::from(format!("{:?}", camera.get_viewport())));
                if ui.button(im_str!("Reset all"), [0.0, 0.0]) {
                    camera.position = Default::default();
                    camera.orientation = Default::default();
                }
                ui.same_line(0.0);
                if ui.button(im_str!("Reset position"), [0.0, 0.0]) {
                    camera.position = Default::default();
                }
                ui.same_line(0.0);
                if ui.button(im_str!("Reset orientation"), [0.0, 0.0]) {
                    camera.orientation = Default::default();
                }
            }

            // input
            if CollapsingHeader::new(im_str!("Input")).default_open(true).build(ui) {
                ui.text_wrapped(im_str!(
                    "WASD for camera movement, right mouse click + drag to rotate, Space/LeftControl to move up/down"
                ));
                if gilrs.gamepads().count() > 0 {
                    ui.text_wrapped(im_str!(
                        "Right stick for camera movement, left stick to rotate, RB/LB to move up/down"
                    ));

                    for (_id, gamepad) in gilrs.gamepads() {
                        ui.text(ImString::from(format!("{} {:?}", gamepad.name(), gamepad.power_info())));
                    }
                }
            }
        });
}

pub fn show_gltf_import_window<'a>(ui: &imgui::Ui<'a>, gltf_import_parameters: &mut GltfImportParameters) {
    use imgui::*;

    puffin::profile_function!();

    Window::new(im_str!("GLTF tools"))
        .always_auto_resize(true)
        .build(ui, || {
            ui.checkbox(im_str!("Force import"), &mut gltf_import_parameters.gltf_force_import);
            if ui.button(im_str!("Lantern.gltf"), [0.0, 0.0]) {
                let mut source_path = gltf_import_parameters.gltf_file.clone();
                source_path.pop();

                gltf_import_parameters.gltf_file = source_path.join("..").join("lantern").join("Lantern.gltf");
                gltf_import_parameters.gltf_queue_import = true;
            }
            if ui.button(im_str!("Sponza.gltf"), [0.0, 0.0]) {
                let mut source_path = gltf_import_parameters.gltf_file.clone();
                source_path.pop();

                gltf_import_parameters.gltf_file = source_path.join("..").join("sponza").join("Sponza.gltf");
                gltf_import_parameters.gltf_queue_import = true;
            }
            if ui.button(im_str!("housetest2.gltf"), [0.0, 0.0]) {
                let mut source_path = gltf_import_parameters.gltf_file.clone();
                source_path.pop();

                gltf_import_parameters.gltf_file = source_path.join("..").join("house_test").join("housetest2.gltf");
                gltf_import_parameters.gltf_queue_import = true;
            }
        });
}
