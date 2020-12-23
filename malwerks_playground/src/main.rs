// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod camera_state;
mod debug_ui;
mod imgui_winit;
mod input_map;

mod surface_pass;
mod surface_winit;

use malwerks_render::*;
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
    assets_folder: std::path::PathBuf,

    #[structopt(short = "v", long = "validation", help = "Enables Vulkan validation layers")]
    enable_validation: bool,

    #[structopt(
        short = "c",
        long = "compression_level",
        default_value = "9",
        help = "Controls compression level for all bundles"
    )]
    compression_level: u32,

    #[structopt(
        long = "force_import_bundles",
        help = "Forces the application to re-import all bundles even if their cached versions exist"
    )]
    force_import_bundles: bool,

    #[structopt(
        long = "force_compile_shaders",
        help = "Forces the application to compile all shaders even if their cached versions exist"
    )]
    force_compile_shaders: bool,

    #[structopt(long = "no_anti_aliasing", help = "Disables anti-aliasing filters completely")]
    no_anti_aliasing: bool,
}

struct Game {
    device: Device,
    factory: DeviceFactory,
    queue: DeviceQueue,

    surface: surface_winit::SurfaceWinit,
    surface_pass: surface_pass::SurfacePass,

    imgui: imgui::Context,
    imgui_platform: imgui_winit::WinitPlatform,
    imgui_renderer: ImguiRenderer,
    profiler_ui: puffin_imgui::ProfilerUi,

    bundle_loader: BundleLoader,
    pbr_forward_lit: PbrForwardLit,

    frame_time: std::time::Instant,
    input_map: input_map::InputMap,
    camera_state: camera_state::CameraState,

    command_line: CommandLineOptions,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.queue.wait_idle();
        self.device.wait_idle();

        self.imgui_renderer.destroy(&mut self.factory);

        self.pbr_forward_lit.destroy(&mut self.factory);
        self.bundle_loader.destroy(&mut self.factory);

        self.surface_pass.destroy(&mut self.factory);
        self.surface.destroy(&mut self.factory);

        self.queue.wait_idle();
        self.device.wait_idle();
    }
}

impl Game {
    fn new(window: &winit::window::Window, base_path: &std::path::Path, command_line: CommandLineOptions) -> Self {
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

        let surface = surface_winit::SurfaceWinit::new(&device);
        let surface_pass = surface_pass::SurfacePass::new(&surface, &device, &mut factory);
        let surface_size = window.inner_size();

        log::info!("surface size: {:?}", surface_size);

        let mut bundle_loader = BundleLoader::new(
            &BundleLoaderParameters {
                bundle_compression_level: command_line.compression_level,
                temporary_folder: &command_line.assets_folder.join("temporary_folder"),
                base_path,
                shader_bundle_path: &command_line.assets_folder.join("common_shaders.bundle"),
                pbr_resource_folder: &command_line.assets_folder.join("pbr_resources"),
                force_import_bundles: command_line.force_import_bundles,
                force_compile_shaders: command_line.force_compile_shaders,
            },
            &device,
            &mut factory,
            &mut queue,
        );

        let pbr_forward_lit = PbrForwardLit::new(
            &PbrForwardLitParameters {
                render_width: surface_size.width,
                render_height: surface_size.height,
                target_layer: Some(surface_pass.get_render_layer()),
                bundle_loader: &bundle_loader,
                enable_anti_aliasing: !command_line.no_anti_aliasing,
            },
            &device,
            &mut factory,
        );

        let mut imgui = imgui::Context::create();
        let mut imgui_platform = imgui_winit::WinitPlatform::init(&mut imgui);
        let imgui_renderer = bundle_loader.create_imgui_renderer(
            &mut imgui,
            surface_pass.get_render_layer(),
            &mut device,
            &mut factory,
            &mut queue,
        );

        let dpi = 1.0f32; //window.scale_factor() as f32;
        imgui_platform.attach_window(imgui.io_mut(), &window, imgui_winit::HiDpiMode::Locked(dpi as f64));
        imgui.io_mut().font_global_scale = dpi;
        imgui.io_mut().config_flags |= imgui::ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
        imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: include_bytes!("../../assets/fonts/Roboto-Regular.ttf"),
            size_pixels: 13.0 * dpi,
            config: None,
        }]);
        imgui.set_ini_filename(Some(base_path.join("target").join("imgui.ini")));

        puffin::set_scopes_on(true);
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
            surface,
            surface_pass,
            imgui,
            imgui_platform,
            imgui_renderer,
            profiler_ui,
            bundle_loader,
            pbr_forward_lit,
            frame_time: std::time::Instant::now(),
            input_map,
            camera_state: camera_state::CameraState::new(
                Some(
                    &command_line
                        .assets_folder
                        .join("temporary_folder")
                        .join("camera_state.bin"),
                ),
                Viewport {
                    x: 0,
                    y: 0,
                    width: surface_size.width,
                    height: surface_size.height,
                },
            ),
            command_line,
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
            puffin::profile_scope!("begin_frame");
            self.bundle_loader.begin_frame(&frame_context, &mut self.factory);
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

                // render world
                self.camera_state.update(time_delta);
                self.pbr_forward_lit.render(
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
                surface_layer.add_dependency(
                    &frame_context,
                    self.pbr_forward_lit.get_render_layer(),
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                );
                surface_layer
                    .add_wait_condition(image_ready_semaphore, vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT);
                surface_layer.begin_render_pass(&frame_context, screen_area);
                self.pbr_forward_lit
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
                    debug_ui::show_pbr_forward_lit_window(
                        &ui,
                        &self.command_line.assets_folder,
                        &mut self.bundle_loader,
                        &mut self.pbr_forward_lit,
                        &self.device,
                        &mut self.factory,
                        &mut self.queue,
                    );

                    let _profiler_window_open = self.profiler_ui.window(&ui);
                    //let mut demo_window_open = true;
                    //ui.show_demo_window(&mut demo_window_open);

                    self.imgui_renderer.draw(
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
            surface_layer.end_render_pass(&frame_context);

            // let command_buffer = surface_layer.get_command_buffer(&frame_context);
            // self.pbr_forward_lit.copy_images(command_buffer);

            surface_layer.submit_commands(&frame_context, &mut self.queue);
            self.surface.present(
                &mut self.queue,
                surface_layer.get_signal_semaphore(&frame_context),
                image_index,
            );
            self.device.end_frame(frame_context);
        }
    }
}

fn main() {
    let base_path = if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
        std::env::set_var("RUST_LOG", "info");
        std::path::PathBuf::from(manifest_path).join("..")
    } else {
        std::path::PathBuf::from(".")
    };

    pretty_env_logger::init();
    log::info!("base path set to {:?}", &base_path);

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

    let mut game = Game::new(&window, &base_path, command_line);

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
