// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_dds::*;
use malwerks_vk::*;

use crate::forward_pass::*;

trait CaptureRenderTargets {
    fn capture_render_targets(
        &self,
        frame_context: &FrameContext,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Vec<(&'static str, ScratchImage)>;
}

impl CaptureRenderTargets for ForwardPass {
    fn capture_render_targets(
        &self,
        frame_context: &FrameContext,
        command_buffer: &mut CommandBuffer,
        factory: &mut DeviceFactory,
        queue: &mut DeviceQueue,
    ) -> Vec<(&'static str, ScratchImage)> {
        use crate::render_pass::*;

        let signal_semaphore = self.get_signal_semaphore(frame_context);
        let stage_mask = vk::PipelineStageFlags::ALL_GRAPHICS;

        let color_image = capture_render_target(
            signal_semaphore,
            stage_mask,
            self.get_color_resource(),
            self.get_extent(),
            vk::ImageAspectFlags::COLOR,
            DXGI_FORMAT_R11G11B10_FLOAT,
            1,
            1,
            command_buffer,
            factory,
            queue,
        );
        vec![("color", color_image)]

        // let depth_image = capture_render_target(
        //     self.get_depth_resource(),
        //     self.get_extent(),
        //     vk::ImageAspectFlags::DEPTH,
        //     DXGI_FORMAT_D32_FLOAT,
        //     1,
        //     1,
        //     command_buffer,
        //     factory,
        //     queue,
        // );
        // vec![("depth", depth_image), ("color", color_image)]
    }
}

fn capture_render_target(
    wait_semaphore: vk::Semaphore,
    wait_dst_stage_mask: vk::PipelineStageFlags,
    image: &HeapAllocatedResource<vk::Image>,
    image_extent: vk::Extent3D,
    image_aspect: vk::ImageAspectFlags,
    image_dxgi_format: u32,
    num_mip_levels: usize,
    num_array_layers: usize,
    command_buffer: &mut CommandBuffer,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) -> ScratchImage {
    command_buffer.reset();
    command_buffer.begin(
        &vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build(),
    );

    let temp_buffer = factory.allocate_buffer(
        &vk::BufferCreateInfo::builder()
            .size(image.1.get_size() as _)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .build(),
        &vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuOnly,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        },
    );

    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::HOST,
        vk::PipelineStageFlags::TRANSFER,
        None,
        &[],
        &[],
        &[vk::ImageMemoryBarrier::builder()
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .image(image.0)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(image_aspect)
                    .base_mip_level(0)
                    .level_count(num_mip_levels as _)
                    .base_array_layer(0)
                    .layer_count(num_array_layers as _)
                    .build(),
            )
            .build()],
    );
    command_buffer.copy_image_to_buffer(
        image.0,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        temp_buffer.0,
        &[vk::BufferImageCopy::builder()
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(image_aspect)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(image_extent)
            .buffer_offset(0)
            .build()],
    );
    command_buffer.pipeline_barrier(
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        None,
        &[],
        &[],
        &[vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(!0)
            .dst_queue_family_index(!0)
            .image(image.0)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(image_aspect)
                    .base_mip_level(0)
                    .level_count(num_mip_levels as _)
                    .base_array_layer(0)
                    .layer_count(num_array_layers as _)
                    .build(),
            )
            .build()],
    );

    command_buffer.end();
    queue.submit(
        &[vk::SubmitInfo::builder()
            .wait_semaphores(&[wait_semaphore])
            .wait_dst_stage_mask(&[wait_dst_stage_mask])
            .command_buffers(&[command_buffer.clone().into()])
            .build()],
        vk::Fence::null(),
    );
    queue.wait_idle();

    let mut scratch_image = ScratchImage::new(
        image_extent.width,
        image_extent.height,
        image_extent.depth,
        num_mip_levels as _,
        num_array_layers as _,
        image_dxgi_format,
        false,
    );

    let temp_memory = factory.map_allocation_memory(&temp_buffer);
    unsafe {
        assert_eq!(scratch_image.as_slice().len(), temp_buffer.1.get_size());

        let dst_slice = scratch_image.as_slice_mut();
        std::ptr::copy_nonoverlapping(temp_memory, dst_slice.as_mut_ptr(), dst_slice.len());
    }
    factory.unmap_allocation_memory(&temp_buffer);

    factory.deallocate_buffer(&temp_buffer);

    scratch_image
}

use crate::camera::*;
use crate::render_world::*;

fn render_test_frame(
    world_test_path: &std::path::Path,
    test_name: &str,
    command_buffer: &mut CommandBuffer,
    camera: &mut Camera,
    render_world: &mut RenderWorld,
    device: &mut Device,
    factory: &mut DeviceFactory,
    queue: &mut DeviceQueue,
) {
    let frame_context = device.begin_frame();
    camera.update_matrices();
    render_world.render(camera, &frame_context, device, factory, queue);
    let images = render_world
        .get_render_pass()
        .capture_render_targets(&frame_context, command_buffer, factory, queue);

    device.end_frame(frame_context);

    queue.wait_idle();
    device.wait_idle();

    for (image_name, scratch_image) in images {
        log::info!("testing {}/{}", test_name, image_name);
        let image_name = String::from(test_name) + "_" + image_name;

        let dds_path = world_test_path.join(&image_name).with_extension("dds");
        scratch_image.save_to_file(&dds_path);

        let texconv_args = vec![
            "-nologo",
            "-dx10",
            "-y",
            "-m",
            "1",
            "-f",
            "R32G32B32A32_FLOAT",
            "-o",
            world_test_path.to_str().unwrap(),
            dds_path.to_str().unwrap(),
        ];
        // log::info!("texconv.exe {:?}", &texconv_args);
        let texconv = std::process::Command::new("texconv.exe")
            .args(&texconv_args)
            .current_dir(std::env::current_dir().expect("failed to get current process dir"))
            .output()
            .expect("failed to spawn texconv.exe process");
        if !texconv.status.success() {
            panic!("texconv finished with status {:?}", texconv.status);
        }

        let reference_path = world_test_path
            .join("reference")
            .join(&image_name)
            .with_extension("dds");
        let reference_image = ScratchImage::from_file(&reference_path);
        let test_image = ScratchImage::from_file(&dds_path);

        let mut absolute_difference = 0.0;
        let mut difference_image = test_image.clone();

        #[repr(C)]
        #[derive(Clone, Copy)]
        struct RGBA32F(f32, f32, f32, f32);

        assert_eq!(reference_image.image_size(), test_image.image_size());

        let image_size = reference_image.image_size();
        let reference_data = reference_image.as_typed_slice::<RGBA32F>();
        let test_data = test_image.as_typed_slice::<RGBA32F>();
        let difference_data = difference_image.as_typed_slice_mut::<RGBA32F>();

        for z in 0..image_size.2 {
            for y in 0..image_size.1 {
                for x in 0..image_size.0 {
                    let offset = (z * image_size.1 * image_size.0 + y * image_size.0 + x) as usize;
                    let reference_pixel = reference_data[offset];
                    let test_pixel = test_data[offset];
                    let difference_pixel = RGBA32F(
                        (test_pixel.0 - reference_pixel.0).abs(),
                        (test_pixel.1 - reference_pixel.1).abs(),
                        (test_pixel.2 - reference_pixel.2).abs(),
                        (test_pixel.3 - reference_pixel.3).abs(),
                    );
                    absolute_difference += difference_pixel.0;
                    absolute_difference += difference_pixel.1;
                    absolute_difference += difference_pixel.2;
                    absolute_difference += difference_pixel.3;
                    difference_data[offset] = difference_pixel;
                }
            }
        }

        let difference_name = image_name + "_difference";
        let difference_path = world_test_path.join(difference_name).with_extension("dds");
        difference_image.save_to_file(&difference_path);

        log::info!("absolute difference: {}", absolute_difference);
        assert_eq!(absolute_difference, 0.0);
    }
}

#[test]
fn test_render_passes() {
    use malwerks_vk::*;

    pretty_env_logger::init();

    let resource_path = if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
        std::env::set_var("RUST_LOG", "info");
        std::path::PathBuf::from(manifest_path).join("..").join("assets")
    } else {
        std::path::PathBuf::from("assets/")
    };
    log::info!("resource path set to {:?}", &resource_path);

    let mut device = Device::new(
        SurfaceMode::Headless(|_: &ash::Entry, _: &ash::Instance| vk::SurfaceKHR::null()),
        DeviceOptions {
            enable_validation: true,
            enable_render_target_export: true,
            ..Default::default()
        },
    );
    let mut queue = device.get_graphics_queue();
    let mut factory = device.create_factory();

    {
        struct TemporaryCommandBuffer {
            command_pool: vk::CommandPool,
            command_buffer: CommandBuffer,
        }

        impl TemporaryCommandBuffer {
            fn destroy(&mut self, factory: &mut DeviceFactory) {
                factory.destroy_command_pool(self.command_pool);
            }
        }

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

        const RENDER_WIDTH: u32 = 1024;
        const RENDER_HEIGHT: u32 = 1024;

        {
            let world_folder = resource_path.join("lantern");
            let world_test_path = world_folder.join("tests");
            let world_path = world_folder.join("Lantern.world");

            let mut render_world = RenderWorld::from_file(
                &world_path,
                (RENDER_WIDTH, RENDER_HEIGHT),
                &mut temporary_command_buffer.command_buffer,
                &device,
                &mut factory,
                &mut queue,
            );

            let mut camera = Camera::new(
                45.0,
                Viewport {
                    x: 0,
                    y: 0,
                    width: RENDER_WIDTH,
                    height: RENDER_HEIGHT,
                },
            );
            camera.position = ultraviolet::vec::Vec3::new(0.0, -12.0, -35.0);
            camera.orientation = ultraviolet::rotor::Rotor3::identity();

            use ultraviolet::rotor::Rotor3;
            use ultraviolet::vec::Vec3;

            let test_cameras = [
                ("00", Vec3::new(0.0, -12.0, -35.0), Rotor3::identity()),
                ("01", Vec3::new(0.0, -2.5, -7.5), Rotor3::identity()),
                ("02", Vec3::new(0.0, -20.5, -7.5), Rotor3::identity()),
                (
                    "10",
                    Vec3::new(35.0, -14.0, 0.0),
                    Rotor3::from_rotation_xz(-90.0 * (std::f32::consts::PI / 180.0)),
                ),
                (
                    "11",
                    Vec3::new(7.5, -2.5, 0.0),
                    Rotor3::from_rotation_xz(-90.0 * (std::f32::consts::PI / 180.0)),
                ),
                (
                    "12",
                    Vec3::new(7.5, -20.5, 0.0),
                    Rotor3::from_rotation_xz(-90.0 * (std::f32::consts::PI / 180.0)),
                ),
                (
                    "20",
                    Vec3::new(-35.0, -14.0, 0.0),
                    Rotor3::from_rotation_xz(90.0 * (std::f32::consts::PI / 180.0)),
                ),
                (
                    "21",
                    Vec3::new(-7.5, -2.5, 0.0),
                    Rotor3::from_rotation_xz(90.0 * (std::f32::consts::PI / 180.0)),
                ),
                (
                    "22",
                    Vec3::new(-7.5, -20.5, 0.0),
                    Rotor3::from_rotation_xz(90.0 * (std::f32::consts::PI / 180.0)),
                ),
            ];

            for (name, position, orientation) in test_cameras.iter() {
                camera.position = *position;
                camera.orientation = *orientation;
                render_test_frame(
                    &world_test_path,
                    name,
                    &mut temporary_command_buffer.command_buffer,
                    &mut camera,
                    &mut render_world,
                    &mut device,
                    &mut factory,
                    &mut queue,
                );
            }

            temporary_command_buffer.destroy(&mut factory);
            render_world.destroy(&mut factory);
        }
    }

    queue.wait_idle();
    device.wait_idle();
}
