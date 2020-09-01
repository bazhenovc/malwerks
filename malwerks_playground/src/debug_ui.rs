// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;

use crate::camera_state::*;

pub fn show_debug_window<'a>(
    ui: &imgui::Ui<'a>,
    _window: &winit::window::Window,
    gilrs: &gilrs::Gilrs,
    camera_state: &mut CameraState,
    render_world: &mut RenderWorld,
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

            // cluster culling
            if CollapsingHeader::new(im_str!("Mesh cluster culling"))
                .default_open(true)
                .build(ui)
            {
                static mut APEX_CULLING_ENABLED: bool = true;
                if ui.checkbox(im_str!("Apex culling enabled"), unsafe { &mut APEX_CULLING_ENABLED }) {
                    render_world.debug_set_apex_culling_enabled(unsafe { APEX_CULLING_ENABLED });
                }

                static mut APEX_CULLING_PAUSED: bool = false;
                if ui.checkbox(im_str!("Apex culling paused"), unsafe { &mut APEX_CULLING_PAUSED }) {
                    render_world.debug_set_apex_culling_paused(unsafe { APEX_CULLING_PAUSED });
                }

                static mut OCCLUSION_CULLING_ENABLED: bool = true;
                if ui.checkbox(im_str!("Occlusion culling enabled"), unsafe {
                    &mut OCCLUSION_CULLING_ENABLED
                }) {
                    render_world.debug_set_occlusion_culling_enabled(unsafe { OCCLUSION_CULLING_ENABLED });
                }

                static mut OCCLUSION_CULLING_PAUSED: bool = false;
                if ui.checkbox(im_str!("Occlusion culling paused"), unsafe {
                    &mut OCCLUSION_CULLING_PAUSED
                }) {
                    render_world.debug_set_occlusion_culling_paused(unsafe { OCCLUSION_CULLING_PAUSED });
                }
            }
        });
}
