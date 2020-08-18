// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod camera_state;
mod debug_ui;
mod imgui_graphics;
mod imgui_winit;
mod input_map;

mod surface_pass;
mod surface_winit;

// use malwerks_light_baker::*;
use malwerks_render::*;

const RENDER_WIDTH: u32 = 1920;
const RENDER_HEIGHT: u32 = 1080;

// const ENVIRONMENT_PROBE_WIDTH: u32 = 1024;
// const ENVIRONMENT_PROBE_HEIGHT: u32 = 1024;

//const RENDER_WIDTH: u32 = 2880;
//const RENDER_HEIGHT: u32 = 1620;

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "malwerks_playground", about = "Playground application")]
struct CommandLineOptions {
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input_file: std::path::PathBuf,

    #[structopt(short = "v", long = "validation")]
    enable_validation: bool,
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

    render_world: RenderWorld,
    post_process: PostProcess,
    frame_time: std::time::Instant,

    input_map: input_map::InputMap,
    camera_state: camera_state::CameraState,
    // acceleration_structure: AccelerationStructure,
    // environment_probes: EnvironmentProbes,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.queue.wait_idle();
        self.device.wait_idle();

        self.temporary_command_buffer.destroy(&mut self.factory);
        self.imgui_graphics.destroy(&mut self.factory);
        self.render_world.destroy(&mut self.factory);
        self.post_process.destroy(&mut self.factory);

        // self.acceleration_structure.destroy(&mut self.factory);
        // self.environment_probes.destroy(&mut self.factory);

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

        let mut imgui = imgui::Context::create();
        let mut imgui_platform = imgui_winit::WinitPlatform::init(&mut imgui);
        let imgui_graphics = imgui_graphics::ImguiGraphics::new(
            &mut imgui,
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
        let gpu_profiler = GpuProfiler::new();
        let profiler_ui = puffin_imgui::ProfilerUi::default();

        let world_path = resource_path.join(&command_line.input_file);
        log::info!("loading world: {:?}", world_path);
        let render_world = RenderWorld::from_file(
            &world_path,
            (RENDER_WIDTH, RENDER_HEIGHT),
            &mut temporary_command_buffer.command_buffer,
            &device,
            &mut factory,
            &mut queue,
        );
        let post_process = PostProcess::new(
            &include_spirv!("/shaders/post_process.vert.spv"),
            &include_spirv!("/shaders/post_process.frag.spv"),
            render_world.get_render_pass(),
            &surface_pass,
            &mut factory,
        );

        // let ray_tracing_properties = device.get_ray_tracing_properties_nv();
        // log::info!("{:?}", &ray_tracing_properties);

        // let mut acceleration_structure = AccelerationStructure::new(&render_world, &mut factory);
        // acceleration_structure.build(&mut temporary_command_buffer.command_buffer, &mut factory, &mut queue);

        // let mut environment_probes = EnvironmentProbes::new(
        //     ENVIRONMENT_PROBE_WIDTH,
        //     ENVIRONMENT_PROBE_HEIGHT,
        //     &static_scenery,
        //     &ray_tracing_properties,
        //     &acceleration_structure,
        //     &mut factory,
        // );
        // environment_probes.build(&mut temporary_command_buffer.command_buffer, &mut factory, &mut queue);

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
            render_world,
            post_process,
            frame_time: std::time::Instant::now(),
            input_map,
            camera_state: camera_state::CameraState::new(Viewport {
                x: 0,
                y: 0,
                width: RENDER_WIDTH,
                height: RENDER_HEIGHT,
            }),
            // acceleration_structure,
            // environment_probes,
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
        // self.environment_probes.bake_environment_probes(
        //     ENVIRONMENT_PROBE_WIDTH,
        //     ENVIRONMENT_PROBE_HEIGHT,
        //     &mut self.temporary_command_buffer.command_buffer,
        //     &mut self.device,
        //     &mut self.factory,
        //     &mut self.queue,
        // );
        (*puffin::GlobalProfiler::lock()).new_frame();

        let frame_context = self.device.begin_frame();
        {
            puffin::profile_scope!("gather_gpu_profile");
            let render_world_timestamps = self
                .render_world
                .try_get_oldest_timestamps(&frame_context, &mut self.factory);
            for scope in render_world_timestamps.iter() {
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

        let image_index = {
            puffin::profile_scope!("acquire_frame");
            let image_ready_semaphore = self.surface_pass.get_image_ready_semaphore(&frame_context);
            let frame_fence = self.surface_pass.get_signal_fence(&frame_context);

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
                // setup render passes
                self.surface_pass.add_dependency(
                    &frame_context,
                    self.render_world.get_render_pass(),
                    vk::PipelineStageFlags::ALL_GRAPHICS,
                );

                // render world
                self.camera_state.update(time_delta);
                self.render_world.render(
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
                self.surface_pass
                    .begin(&frame_context, &mut self.device, &mut self.factory, screen_area);
                self.post_process
                    .render(screen_area, &frame_context, &mut self.surface_pass);
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
                        &mut self.render_world,
                        1000.0 / average_delta,
                        average_delta,
                    );

                    let _profiler_window_open = self.profiler_ui.window(&ui);

                    //let mut demo_window_open = true;
                    //ui.show_demo_window(&mut demo_window_open);

                    self.imgui_graphics.draw(
                        &frame_context,
                        &mut self.factory,
                        self.surface_pass.get_command_buffer(&frame_context),
                        ui.render(),
                    );
                }
            }
        }

        {
            puffin::profile_scope!("present");
            self.surface_pass.end(&frame_context);
            self.surface_pass.submit_commands(&frame_context, &mut self.queue);
            self.surface.present(
                &mut self.queue,
                self.surface_pass.get_signal_semaphore(&frame_context),
                image_index,
            );
            self.device.end_frame(frame_context);
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
        .with_inner_size(winit::dpi::PhysicalSize::new(RENDER_WIDTH, RENDER_HEIGHT))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

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
                //let mut render_events = world.write_resource::<Vec<systems::render::RenderEvent>>();
                //render_events.push(systems::render::RenderEvent::ResizeRequest(
                //    s.width, s.height,
                //));
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
