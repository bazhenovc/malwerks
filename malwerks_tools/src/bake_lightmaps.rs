// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_render::*;
use malwerks_light_baker::*;

use ash::vk;

mod acceleration_structure;
mod environment_probes;
mod shader_binding_table;

use acceleration_structure::*;
use environment_probes::*;

const RENDER_WIDTH: u32 = 1024;
const RENDER_HEIGHT: u32 = 1024;

struct TemporaryCommandBuffer {
    command_pool: vk::CommandPool,
    command_buffer: CommandBuffer,
}

impl TemporaryCommandBuffer {
    fn destroy(&mut self, factory: &mut DeviceFactory) {
        factory.destroy_command_pool(self.command_pool);
    }
}

struct LightBaker {
    device: Device,
    factory: DeviceFactory,
    queue: DeviceQueue,

    temporary_command_buffer: TemporaryCommandBuffer,

    render_world: RenderWorld,
    acceleration_structure: AccelerationStructure,
    environment_probes: EnvironmentProbes,
}

impl Drop for LightBaker {
    fn drop(&mut self) {
        self.queue.wait_idle();
        self.device.wait_idle();

        self.environment_probes.destroy(&mut self.factory);
        self.acceleration_structure.destroy(&mut self.factory);
        self.render_world.destroy(&mut self.factory);
        self.temporary_command_buffer.destroy(&mut self.factory);
    }
}

impl LightBaker {
    fn new(world_path: &std::path::Path) -> Self {
        let device = Device::new(
            SurfaceMode::Headless(|_: &ash::Entry, _: &ash::Instance| vk::SurfaceKHR::null()),
            DeviceOptions {
                enable_validation: true,
                enable_render_target_export: true,
                enable_ray_tracing_nv: true,
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

        let ray_tracing_properties = device.get_ray_tracing_properties_nv();
        log::info!("{:?}", &ray_tracing_properties);

        log::info!("loading world: {:?}", world_path);
        let static_scenery = {
            use std::io::Read;
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .open(world_path)
                .expect("failed to open world file");

            let mut encoded = Vec::new();
            file.read_to_end(&mut encoded).expect("failed to read world file");
            bincode::deserialize(&encoded).expect("failed to deserialize world file")
        };
        let render_world = RenderWorld::from_disk(
            &static_scenery,
            (RENDER_WIDTH, RENDER_HEIGHT),
            &mut temporary_command_buffer.command_buffer,
            &device,
            &mut factory,
            &mut queue,
        );

        let mut acceleration_structure = AccelerationStructure::new(&render_world, &mut factory);
        acceleration_structure.build(&mut temporary_command_buffer.command_buffer, &mut factory, &mut queue);

        let mut environment_probes = EnvironmentProbes::new(
            RENDER_WIDTH,
            RENDER_HEIGHT,
            &static_scenery,
            &ray_tracing_properties,
            &acceleration_structure,
            &mut factory,
        );
        environment_probes.build(&mut temporary_command_buffer.command_buffer, &mut factory, &mut queue);

        Self {
            device,
            queue,
            factory,

            temporary_command_buffer,

            render_world,
            acceleration_structure,
            environment_probes,
        }
    }

    fn bake_lightmaps(&mut self) {
        self.environment_probes.bake_environment_probes(
            RENDER_WIDTH,
            RENDER_HEIGHT,
            &mut self.temporary_command_buffer.command_buffer,
            &mut self.device,
            &mut self.factory,
            &mut self.queue,
        );
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

    let args: Vec<String> = std::env::args().collect();
    log::info!("resource path set to {:?}", &resource_path);
    log::info!("command line: {:?}", args);

    if args.len() < 2 {
        log::error!("usage: malwerks_light_baker <world file name>");
        return;
    }

    let mut light_baker = LightBaker::new(&resource_path.join(&args[1]));
    light_baker.bake_lightmaps();
}
