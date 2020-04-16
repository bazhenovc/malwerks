// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

mod camera_state;
mod debug_ui;
mod imgui_graphics;
mod imgui_winit;
mod input_map;

use malwerks_render::*;

const RENDER_WIDTH: u32 = 1920;
const RENDER_HEIGHT: u32 = 1080;

//const RENDER_WIDTH: u32 = 2880;
//const RENDER_HEIGHT: u32 = 1620;

struct TemporaryCommandBuffer {
    command_pool: vk::CommandPool,
    command_buffer: CommandBuffer,
}

impl TemporaryCommandBuffer {
    fn destroy(&mut self, factory: &mut GraphicsFactory) {
        factory.destroy_command_pool(self.command_pool);
    }
}

struct Game {
    graphics_device: GraphicsDevice,
    graphics_factory: GraphicsFactory,
    graphics_queue: DeviceQueue,

    // TODO: remove and implement transfer queue
    temporary_command_buffer: TemporaryCommandBuffer,
    backbuffer_pass: BackbufferPass,

    imgui: imgui::Context,
    imgui_platform: imgui_winit::WinitPlatform,
    imgui_graphics: imgui_graphics::ImguiGraphics,

    render_world: RenderWorld,
    post_process: PostProcess,
    frame_time: std::time::Instant,

    input_map: input_map::InputMap,
    camera_state: camera_state::CameraState,
}

impl Drop for Game {
    fn drop(&mut self) {
        self.graphics_queue.wait_idle();
        self.graphics_device.wait_idle();

        self.temporary_command_buffer.destroy(&mut self.graphics_factory);
        self.backbuffer_pass.destroy(&mut self.graphics_factory);
        self.imgui_graphics.destroy(&mut self.graphics_factory);
        self.render_world.destroy(&mut self.graphics_factory);
        self.post_process.destroy(&mut self.graphics_factory);
        self.graphics_device.wait_idle();
    }
}

impl Game {
    fn new(window: &winit::window::Window, world_path: &std::path::Path) -> Self {
        let graphics_device = GraphicsDevice::new(
            Some(window),
            GraphicsDeviceOptions {
                enable_validation: true,
                ..Default::default()
            },
        );
        let mut graphics_queue = graphics_device.get_graphics_queue();
        let mut graphics_factory = graphics_device.create_graphics_factory();

        let transient_command_pool = graphics_factory.create_command_pool(
            &vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(graphics_device.get_graphics_queue_index())
                .build(),
        );
        let mut temporary_command_buffer = TemporaryCommandBuffer {
            command_pool: transient_command_pool,
            command_buffer: graphics_factory.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(1)
                    .command_pool(transient_command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .build(),
            )[0],
        };

        let backbuffer_pass = BackbufferPass::new(&graphics_device, &mut graphics_factory);

        let mut imgui = imgui::Context::create();
        let mut imgui_platform = imgui_winit::WinitPlatform::init(&mut imgui);
        let imgui_graphics = imgui_graphics::ImguiGraphics::new(
            &mut imgui,
            &backbuffer_pass,
            &graphics_device,
            &mut graphics_factory,
            &mut temporary_command_buffer.command_buffer,
            &mut graphics_queue,
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

        let render_world = RenderWorld::from_file(
            world_path,
            (RENDER_WIDTH, RENDER_HEIGHT),
            &graphics_device,
            &mut graphics_factory,
            &mut temporary_command_buffer.command_buffer,
            &mut graphics_queue,
        );
        let post_process = PostProcess::new(
            &include_spirv!("/shaders/post_process.vert.spv"),
            &include_spirv!("/shaders/post_process.frag.spv"),
            render_world.get_render_pass(),
            &backbuffer_pass,
            &mut graphics_factory,
        );

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
            graphics_device,
            graphics_factory,
            graphics_queue,
            temporary_command_buffer,
            backbuffer_pass,
            imgui,
            imgui_platform,
            imgui_graphics,
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

    fn draw_and_present(&mut self, window: &winit::window::Window, gilrs: &gilrs::Gilrs) {
        let time_now = std::time::Instant::now();
        let time_delta = time_now - self.frame_time;
        self.frame_time = time_now;

        let frame_context = self.graphics_device.acquire_frame();
        let image_index = {
            microprofile::scope!("acquire_frame", "total", 0);
            let image_ready_semaphore = self.backbuffer_pass.get_image_ready_semaphore(&frame_context);
            let frame_fence = self.backbuffer_pass.get_signal_fence(&frame_context);

            // acquire next image
            self.graphics_device
                .wait_for_fences(&[frame_fence], true, u64::max_value());
            self.graphics_device
                .acquire_next_image(u64::max_value(), image_ready_semaphore)
        };
        microprofile::scope!("draw_and_present", "total", 0);

        // render world
        self.camera_state.update(time_delta.as_secs_f32());
        self.render_world.render(
            self.camera_state.get_camera(),
            &mut self.backbuffer_pass,
            &frame_context,
            &mut self.graphics_device,
            &mut self.graphics_factory,
            &mut self.graphics_queue,
        );

        // process backbuffer pass and post processing
        let screen_area = {
            let surface_extent = self.graphics_device.get_surface_extent();
            vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: surface_extent,
            }
        };
        self.backbuffer_pass.begin(
            &frame_context,
            &mut self.graphics_device,
            &mut self.graphics_factory,
            screen_area,
        );
        self.post_process
            .render(screen_area, &frame_context, &mut self.backbuffer_pass);

        // process imgui
        let io = self.imgui.io_mut();
        io.delta_time = time_delta.as_secs_f32();

        self.imgui_platform.prepare_frame(io, window).unwrap();

        let ui = self.imgui.frame();
        self.imgui_platform.prepare_render(&ui, window);
        {
            debug_ui::show_debug_window(&ui, &window, &gilrs, &mut self.camera_state, &mut self.render_world);

            //let mut demo_window_open = true;
            //ui.show_demo_window(&mut demo_window_open);

            self.imgui_graphics.draw(
                &frame_context,
                &mut self.graphics_factory,
                self.backbuffer_pass.get_command_buffer(&frame_context),
                ui.render(),
            );
        }

        // present
        self.backbuffer_pass.end(&frame_context);
        self.backbuffer_pass
            .submit_commands(&frame_context, &mut self.graphics_queue);
        self.graphics_device
            .present(self.backbuffer_pass.get_signal_semaphore(&frame_context), image_index);

        // flip profiler
        microprofile::flip!();
    }
}

fn main() {
    let resource_path = if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
        std::env::set_var("RUST_LOG", "info");
        std::path::PathBuf::from(manifest_path).join("..").join("assets")
    } else {
        std::path::PathBuf::from("assets/")
    };

    pretty_env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    log::info!("resource path set to {:?}", &resource_path);
    log::info!("command line: {:?}", args);

    if args.len() < 2 {
        log::error!("usage: malwerks_playground <world file name>");
        return;
    }

    microprofile::init!();
    microprofile::set_enable_all_groups!(true);

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

    let mut game = Game::new(&window, &resource_path.join(&args[1]));

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
                game.draw_and_present(&window, &gilrs);
            }

            Event::LoopDestroyed => {
                //microprofile::dump_file_immediately!("profile.html", "");
                microprofile::shutdown!();
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
