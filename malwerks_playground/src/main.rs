// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod camera;
mod camera_state;
mod debug_ui;
mod imgui_graphics;
mod imgui_winit;
mod input_map;

mod surface_pass;
mod surface_winit;

mod forward_pass;
mod post_process;
mod shared_frame_data;
mod shared_resource_bundle;
mod sky_box;

mod demo_pbr_forward_lit;

use malwerks_core::*;
use malwerks_vk::*;

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "malwerks_playground", about = "Playground application")]
struct CommandLineOptions {
    #[structopt(
        short = "i",
        long = "input",
        default_value = "./assets/",
        help = "Folder where playground assets are located",
        parse(from_os_str)
    )]
    input_folder: std::path::PathBuf,

    #[structopt(short = "v", long = "validation", help = "Enables Vulkan validation layers")]
    enable_validation: bool,

    #[structopt(
        short = "f",
        long = "force_import",
        help = "Forces the application to re-import all .gltf files. This operation can take a few minutes."
    )]
    force_import: bool,

    #[structopt(
        short = "c",
        long = "compression_level",
        default_value = "9",
        help = "Controls compression level for all bundles"
    )]
    compression_level: u32,
}

struct TemporaryCommandBuffer {
    command_pool: vk::CommandPool,
    command_buffer: CommandBuffer,
}

impl TemporaryCommandBuffer {
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_command_pool(self.command_pool);
    }
}

struct Game {
    device: Device,
    factory: DeviceFactory,
    queue: DeviceQueue,

    surface: surface_winit::SurfaceWinit,
    surface_pass: surface_pass::SurfacePass,

    // TODO: remove and implement transfer queue
    temporary_command_buffer: TemporaryCommandBuffer,

    imgui: imgui::Context,
    imgui_platform: imgui_winit::WinitPlatform,
    imgui_graphics: imgui_graphics::ImguiGraphics,

    gpu_profiler: GpuProfiler,
    profiler_ui: puffin_imgui::ProfilerUi,

    render_shared_resources: shared_resource_bundle::RenderSharedResources,
    gltf_import_parameters: demo_pbr_forward_lit::GltfImportParameters,
    demo_pbr_forward_lit: demo_pbr_forward_lit::DemoPbrForwardLit,
    frame_time: std::time::Instant,

    input_map: input_map::InputMap,
    camera_state: camera_state::CameraState,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.queue.wait_idle();
        self.device.wait_idle();

        self.temporary_command_buffer.destroy(&mut self.factory);
        self.imgui_graphics.destroy(&mut self.factory);

        self.render_shared_resources.destroy(&mut self.factory);
        self.demo_pbr_forward_lit.destroy(&mut self.factory);

        self.surface_pass.destroy(&mut self.factory);
        self.surface.destroy(&mut self.factory);
        self.device.wait_idle();
    }
}

impl Game {
    fn new(window: &winit::window::Window, resource_path: &std::path::Path, command_line: CommandLineOptions) -> Self {
        let mut device = Device::new(
            SurfaceMode::WindowSurface(|entry: &ash::Entry, instance: &ash::Instance| {
                surface_winit::create_surface(entry, instance, window).expect("failed to create KHR surface")
            }),
            DeviceOptions {
                enable_validation: command_line.enable_validation,
                // enable_ray_tracing_nv: true,
                ..Default::default()
            },
        );
        let mut queue = device.get_graphics_queue();
        let mut factory = device.create_factory();

        let temporary_command_pool = factory.create_command_pool(
            &vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(device.get_graphics_queue_index())
                .build(),
        );
        let mut temporary_command_buffer = TemporaryCommandBuffer {
            command_pool: temporary_command_pool,
            command_buffer: factory.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(temporary_command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build(),
            )[0],
        };

        let surface = surface_winit::SurfaceWinit::new(&device);
        let surface_pass = surface_pass::SurfacePass::new(&surface, &device, &mut factory);
        let surface_size = window.inner_size();

        log::info!("surface size: {:?}", surface_size);

        let shaders_folder = resource_path.join("malwerks_shaders");
        let shared_temp_path = resource_path.join("target").join("temp");
        let shared_bundle_path = resource_path.join("assets").join("shared_resources.bundle");
        let shared_resource_bundle = if command_line.force_import || !shared_bundle_path.exists() {
            log::info!("generating shared_resources.bundle");
            let shared_resources_path = command_line.input_folder.join("shared_resources");
            let temp_path = shared_temp_path.join("shared_resources.bundle");
            let bundle = shared_resource_bundle::import_shared_resources(&shared_resources_path, &temp_path);

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(shared_bundle_path)
                .expect("failed to open shared bundle file for writing");
            bundle
                .serialize_into(file, command_line.compression_level)
                .expect("failed to serialize shared resource bundle");
            bundle
        } else {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .open(shared_bundle_path)
                .expect("failed to open shared bundle file for reading");
            shared_resource_bundle::DiskSharedResources::deserialize_from(file)
                .expect("failed to deserialize shared resources")
        };
        let render_shared_resources = shared_resource_bundle::RenderSharedResources::new(
            &shared_resource_bundle,
            &mut temporary_command_buffer.command_buffer,
            &mut factory,
            &mut queue,
        );

        let gltf_import_parameters = demo_pbr_forward_lit::GltfImportParameters {
            gltf_file: resource_path.join("assets").join("lantern").join("Lantern.gltf"),
            gltf_bundle_folder: resource_path.join("assets").into(),
            gltf_temp_folder: shared_temp_path,
            gltf_force_import: command_line.force_import,
            gltf_shaders_folder: shaders_folder,
            gltf_bundle_compression_level: command_line.compression_level,
            gltf_queue_import: false,
        };

        let demo_pbr_forward_lit = demo_pbr_forward_lit::DemoPbrForwardLit::new(
            &gltf_import_parameters,
            &shared_resource_bundle,
            &render_shared_resources,
            (surface_size.width, surface_size.height),
            surface_pass.get_render_layer(),
            &mut temporary_command_buffer.command_buffer,
            &device,
            &mut factory,
            &mut queue,
        );

        let mut imgui = imgui::Context::create();
        let mut imgui_platform = imgui_winit::WinitPlatform::init(&mut imgui);
        let imgui_graphics = imgui_graphics::ImguiGraphics::new(
            &mut imgui,
            &shared_resource_bundle,
            &surface_pass,
            &mut temporary_command_buffer.command_buffer,
            &mut device,
            &mut factory,
            &mut queue,
        );

        {
            let dpi_factor = 1.0; //window.scale_factor() as f32;

            imgui_platform.attach_window(imgui.io_mut(), &window, imgui_winit::HiDpiMode::Locked(1.0));
            imgui.io_mut().font_global_scale = dpi_factor;
            imgui.io_mut().config_flags |= imgui::ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
            imgui.fonts().add_font(&[imgui::FontSource::TtfData {
                data: include_bytes!("../../assets/fonts/Roboto-Regular.ttf"),
                size_pixels: 13.0 * dpi_factor,
                config: None,
            }]);
        }
        imgui.set_ini_filename(None);

        puffin::set_scopes_on(true);
        let gpu_profiler = GpuProfiler::default();
        let profiler_ui = puffin_imgui::ProfilerUi::default();

        let input_map = {
            use input_map::*;

            use gilrs::{Axis, Button};
            use winit::event::{MouseButton, VirtualKeyCode};

            let mut input_map = InputMap::new();
            input_map.bind_keyboard(VirtualKeyCode::W, InputActionType::CameraMove, 1.0, 0.0);
            input_map.bind_keyboard(VirtualKeyCode::S, InputActionType::CameraMove, -1.0, 0.0);
            input_map.bind_keyboard(VirtualKeyCode::A, InputActionType::CameraStrafe, 1.0, 0.0);
            input_map.bind_keyboard(VirtualKeyCode::D, InputActionType::CameraStrafe, -1.0, 0.0);
            input_map.bind_keyboard(VirtualKeyCode::Space, InputActionType::CameraLift, -1.0, 0.0);
            input_map.bind_keyboard(VirtualKeyCode::LControl, InputActionType::CameraLift, 1.0, 0.0);
            input_map.bind_mouse_drag(
                MouseButton::Right,
                InputActionType::CameraRotateX,
                InputActionType::CameraRotateY,
            );

            input_map.bind_gamepad_axis(Axis::LeftStickY, InputActionType::CameraMove, 1.0);
            input_map.bind_gamepad_axis(Axis::LeftStickX, InputActionType::CameraStrafe, -1.0);
            input_map.bind_gamepad_axis(Axis::RightStickX, InputActionType::CameraRotateX, -1.0);
            input_map.bind_gamepad_axis(Axis::RightStickY, InputActionType::CameraRotateY, -1.0);
            input_map.bind_gamepad_button(Button::LeftTrigger, InputActionType::CameraLift, 1.0, 0.0);
            input_map.bind_gamepad_button(Button::RightTrigger, InputActionType::CameraLift, -1.0, 0.0);

            input_map
        };

        Self {
            device,
            factory,
            queue,
            temporary_command_buffer,
            surface,
            surface_pass,
            imgui,
            imgui_platform,
            imgui_graphics,
            gpu_profiler,
            profiler_ui,
            render_shared_resources,
            gltf_import_parameters,
            demo_pbr_forward_lit,
            frame_time: std::time::Instant::now(),
            input_map,
            camera_state: camera_state::CameraState::new(camera::Viewport {
                x: 0,
                y: 0,
                width: surface_size.width,
                height: surface_size.height,
            }),
        }
    }

    fn handle_event<T>(&mut self, window: &winit::window::Window, event: &winit::event::Event<T>) {
        let io = self.imgui.io_mut();
        self.imgui_platform.handle_event(io, window, event);
        self.input_map
            .handle_event(io.want_capture_keyboard, io.want_capture_mouse, window, event);
    }

    fn handle_gamepad_event(&mut self, event: &gilrs::Event) {
        self.input_map.handle_gamepad_event(event);
    }

    fn process_events(&mut self) {
        self.input_map.process_events();
        self.camera_state.handle_action_queue(self.input_map.get_action_queue());
    }

    fn render_and_present(&mut self, window: &winit::window::Window, gilrs: &gilrs::Gilrs) {
        (*puffin::GlobalProfiler::lock()).new_frame();

        let frame_context = self.device.begin_frame();
        {
            puffin::profile_scope!("gather_gpu_profile");
            let demo_timestamps = self
                .demo_pbr_forward_lit
                .try_get_oldest_timestamps(&frame_context, &mut self.factory);
            for scope in demo_timestamps.iter() {
                let scope_offset = self.gpu_profiler.begin_scope(scope.0, scope.1[0]);
                self.gpu_profiler.end_scope(scope_offset, scope.1[1]);
            }

            if let Some(surface_pass_scope) = self
                .surface_pass
                .try_get_oldest_timestamp(&frame_context, &mut self.factory)
            {
                let scope_offset = self.gpu_profiler.begin_scope("Final", surface_pass_scope[0]);
                self.gpu_profiler.end_scope(scope_offset, surface_pass_scope[1]);
            }
            self.gpu_profiler.report_frame();
        }

        let image_ready_semaphore = self.surface_pass.get_image_ready_semaphore(&frame_context);
        let surface_layer = self.surface_pass.get_render_layer_mut();

        let image_index = {
            puffin::profile_scope!("acquire_frame");
            let frame_fence = surface_layer.get_signal_fence(&frame_context);

            // acquire next image
            self.device.wait_for_fences(&[frame_fence], true, u64::max_value());
            self.surface.acquire_next_image(u64::max_value(), image_ready_semaphore)
        };

        {
            puffin::profile_scope!("render");

            let time_now = std::time::Instant::now();
            let time_delta = (time_now - self.frame_time).as_secs_f32();
            self.frame_time = time_now;

            {
                puffin::profile_scope!("render_world");

                // setup render layers
                surface_layer.add_dependency(
                    &frame_context,
                    self.demo_pbr_forward_lit.get_final_layer(),
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                );

                // render world
                self.camera_state.update(time_delta);
                self.demo_pbr_forward_lit.render(
                    &self.render_shared_resources,
                    self.camera_state.get_camera(),
                    &frame_context,
                    &mut self.device,
                    &mut self.factory,
                    &mut self.queue,
                );

                // process backbuffer pass and post processing
                let screen_area = {
                    let surface_extent = self.surface.get_surface_extent();
                    vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: surface_extent,
                    }
                };
                surface_layer.acquire_frame(&frame_context, &mut self.device, &mut self.factory);
                surface_layer
                    .add_wait_condition(image_ready_semaphore, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT);
                surface_layer.begin_command_buffer(&frame_context, screen_area);
                self.demo_pbr_forward_lit
                    .post_process(self.camera_state.get_camera(), &frame_context, surface_layer);
            }

            // process imgui
            {
                puffin::profile_scope!("render_imgui");
                let io = self.imgui.io_mut();
                io.delta_time = time_delta;
                let average_delta = io.framerate;

                self.imgui_platform.prepare_frame(io, window).unwrap();

                let ui = self.imgui.frame();
                self.imgui_platform.prepare_render(&ui, window);
                {
                    debug_ui::show_debug_window(
                        &ui,
                        &window,
                        &gilrs,
                        &mut self.camera_state,
                        1000.0 / average_delta,
                        average_delta,
                    );
                    debug_ui::show_gltf_import_window(&ui, &mut self.gltf_import_parameters);

                    let _profiler_window_open = self.profiler_ui.window(&ui);

                    //let mut demo_window_open = true;
                    //ui.show_demo_window(&mut demo_window_open);

                    self.imgui_graphics.draw(
                        &frame_context,
                        &mut self.factory,
                        surface_layer.get_command_buffer(&frame_context),
                        ui.render(),
                    );
                }
            }
        }

        {
            puffin::profile_scope!("present");
            surface_layer.end_command_buffer(&frame_context);
            surface_layer.submit_commands(&frame_context, &mut self.queue);
            self.surface.present(
                &mut self.queue,
                surface_layer.get_signal_semaphore(&frame_context),
                image_index,
            );
            self.device.end_frame(frame_context);
        }

        {
            if self.gltf_import_parameters.gltf_queue_import {
                self.demo_pbr_forward_lit.import_bundles(
                    &self.gltf_import_parameters,
                    &self.render_shared_resources,
                    &mut self.temporary_command_buffer.command_buffer,
                    &self.device,
                    &mut self.factory,
                    &mut self.queue,
                );
                self.gltf_import_parameters.gltf_queue_import = false;
            }
        }
    }
}

fn main() {
    let resource_path = if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
        std::env::set_var("RUST_LOG", "info");
        std::path::PathBuf::from(manifest_path).join("..")
    } else {
        std::path::PathBuf::from(".")
    };

    pretty_env_logger::init();

    log::info!("resource path set to {:?}", &resource_path);

    let command_line = {
        use structopt::StructOpt;
        CommandLineOptions::from_args()
    };
    log::info!("command line: {:?}", &command_line);

    let event_loop = winit::event_loop::EventLoop::new();
    let window = winit::window::WindowBuilder::new()
        .with_title("Málwerks")
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

    let monitor = window.current_monitor().expect("current_monitor() failed");
    let monitor_size = monitor.size();

    let window_size = winit::dpi::PhysicalSize::new((monitor_size.width / 3) * 2, (monitor_size.height / 3) * 2);
    window.set_inner_size(window_size);
    log::info!("monitor size: {:?}, window size: {:?}", monitor_size, window_size);

    let mut gilrs = gilrs::Gilrs::new().expect("failed to initialize gamepad input");
    for (_id, gamepad) in gilrs.gamepads() {
        log::info!("gamepad detected: {} {:?}", gamepad.name(), gamepad.power_info());
    }

    let mut game = Game::new(&window, &resource_path, command_line);

    // run events loop
    event_loop.run(move |event, _, control_flow| {
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::ControlFlow;

        *control_flow = ControlFlow::Poll;

        game.handle_event(&window, &event);
        match event {
            Event::MainEventsCleared => {
                while let Some(gamepad_event) = gilrs.next_event() {
                    game.handle_gamepad_event(&gamepad_event);
                }

                game.process_events();
                window.request_redraw();
            }

            Event::RedrawRequested(_) => {
                game.render_and_present(&window, &gilrs);
            }

            Event::LoopDestroyed => {
                // Nothing right now
            }

            // user input
            Event::WindowEvent {
                event: WindowEvent::Resized(_size),
                ..
            } => {
                // Nothing right now
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
