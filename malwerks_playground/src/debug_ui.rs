// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;
use malwerks_vk::*;

use crate::camera_state::*;

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

pub fn show_pbr_forward_lit_window<'a>(
    ui: &imgui::Ui<'a>,
    assets_folder: &std::path::Path,

    bundle_loader: &mut BundleLoader,
    pbr_forward_lit: &mut PbrForwardLit,

    device: &Device,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) {
    use imgui::*;

    Window::new(im_str!("PbrForwardLit"))
        .always_auto_resize(true)
        .build(ui, || {
            macro_rules! bundle_checkbox {
                ($gltf_path: expr, $bundle_path: expr) => {{
                    static mut BUNDLE_FLAG: bool = false;
                    if ui.checkbox(im_str!($gltf_path), unsafe { &mut BUNDLE_FLAG }) {
                        if unsafe { BUNDLE_FLAG } {
                            pbr_forward_lit.add_render_bundle(
                                $gltf_path,
                                bundle_loader,
                                &assets_folder.join($gltf_path),
                                &assets_folder.join($bundle_path),
                                device,
                                factory,
                                queue,
                            );
                        } else {
                            pbr_forward_lit.remove_render_bundle($gltf_path, device, factory, queue);
                        }
                    }
                }};
            }

            bundle_checkbox!("lantern/Lantern.gltf", "Lantern.resource_bundle");
            bundle_checkbox!("sponza/Sponza.gltf", "Sponza.resource_bundle");
            bundle_checkbox!("house_test/housetest2.gltf", "housetest2.resource_bundle");

            let bundles = pbr_forward_lit.get_render_bundles();
            ui.text(ImString::from(format!("Bundles: {}", bundles.len())));
            for (bundle_name, bundle, shader_module_bundle, pipeline_bundle) in bundles {
                if CollapsingHeader::new(&ImString::from(format!("Bundle {}", bundle_name)))
                    .default_open(true)
                    .build(ui)
                {
                    let resource_bundle = bundle.borrow();
                    ui.text(ImString::from(format!("Buffers: {}", resource_bundle.buffers.len())));
                    ui.text(ImString::from(format!("Meshes: {}", resource_bundle.meshes.len())));
                    ui.text(ImString::from(format!("Images: {}", resource_bundle.images.len())));
                    ui.text(ImString::from(format!(
                        "Image views: {}",
                        resource_bundle.image_views.len()
                    )));
                    ui.text(ImString::from(format!("Samplers: {}", resource_bundle.samplers.len())));
                    ui.text(ImString::from(format!("Buckets: {}", resource_bundle.buffers.len())));
                    ui.text(ImString::from(format!(
                        "Descriptor layouts: {}",
                        resource_bundle.descriptor_layouts.len()
                    )));
                    ui.text(ImString::from(format!(
                        "Descriptor sets: {}",
                        resource_bundle.descriptor_sets.len()
                    )));
                    ui.text(ImString::from(format!(
                        "Materials: {}",
                        resource_bundle.materials.len()
                    )));

                    ui.text(ImString::from(format!(
                        "Shader stages: {}",
                        shader_module_bundle.shader_stages.len()
                    )));

                    ui.text(ImString::from(format!(
                        "Pipeline layouts: {}",
                        pipeline_bundle.pipeline_layouts.len()
                    )));
                    ui.text(ImString::from(format!(
                        "Pipelines: {}",
                        pipeline_bundle.pipelines.len()
                    )));
                }
            }
        });
}
