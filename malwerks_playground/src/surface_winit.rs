// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use malwerks_vk::*;

use ash::version::*;

pub struct SurfaceWinit {
    internal_surface: InternalSurface,
    internal_swapchain: InternalSwapchain,
}

impl SurfaceWinit {
    pub fn new(device: &Device) -> Self {
        let surface_loader = device.get_surface_loader().as_ref().unwrap();
        let surface_khr = device.get_surface_khr();

        // Pick suitable surface format
        let surface_format = {
            let surface_formats = unsafe {
                surface_loader
                    .get_physical_device_surface_formats(device.get_physical_device(), surface_khr)
                    .unwrap()
            };

            let fallback_format = surface_formats
                .iter()
                .cloned()
                .map(|format| match format.format {
                    vk::Format::UNDEFINED => vk::SurfaceFormatKHR {
                        format: vk::Format::B8G8R8A8_UNORM,
                        color_space: format.color_space,
                    },

                    _ => format,
                })
                .next()
                .expect("Unable to find fallback surface format");

            // Try to find SRGB format first
            surface_formats
                .iter()
                .cloned()
                .map(|format| match format.color_space {
                    vk::ColorSpaceKHR::SRGB_NONLINEAR => format,

                    _ => fallback_format,
                })
                .next()
                .unwrap_or(fallback_format)
        };

        log::info!("{:?}", surface_format);

        // Validate surface caps
        let surface_caps = unsafe {
            surface_loader
                .get_physical_device_surface_capabilities(device.get_physical_device(), surface_khr)
                .unwrap()
        };

        //let image_count = surface_caps.min_image_count + 1;
        let image_count = NUM_BUFFERED_GPU_FRAMES as u32;
        assert!(image_count >= surface_caps.min_image_count && image_count < surface_caps.max_image_count);

        let pre_transform = if surface_caps
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_caps.current_transform
        };

        let surface_extent = surface_caps.current_extent;
        let present_mode = {
            let present_modes = unsafe {
                surface_loader
                    .get_physical_device_surface_present_modes(device.get_physical_device(), surface_khr)
                    .unwrap()
            };

            present_modes
                .iter()
                .cloned()
                .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
                .unwrap_or(vk::PresentModeKHR::FIFO)
        };

        let swapchain_loader = ash::extensions::khr::Swapchain::new(device.get_instance(), device.get_device());

        let swapchain = unsafe {
            swapchain_loader
                .create_swapchain(
                    &vk::SwapchainCreateInfoKHR::builder()
                        //.flags(vk::SwapchainCreateFlagsKHR::NONE)
                        .surface(surface_khr)
                        .min_image_count(image_count)
                        .image_format(surface_format.format)
                        .image_color_space(surface_format.color_space)
                        .image_extent(surface_extent)
                        .image_array_layers(1)
                        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                        //.queue_family_indices(...)
                        .pre_transform(pre_transform)
                        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                        .present_mode(present_mode)
                        .clipped(true)
                        .build(),
                    None,
                )
                .unwrap()
        };

        let internal_surface = InternalSurface {
            surface_khr,
            format: surface_format,
            extent: surface_extent,
        };

        let internal_swapchain = InternalSwapchain {
            loader: swapchain_loader,
            swapchain,
            present_mode,
        };

        Self {
            internal_surface,
            internal_swapchain,
        }
    }

    pub fn destroy(&mut self, _factory: &mut DeviceFactory) {
        unsafe {
            self.internal_swapchain
                .loader
                .destroy_swapchain(self.internal_swapchain.swapchain, None);
        }
    }

    pub fn acquire_next_image(&mut self, timeout: u64, image_ready_semaphore: vk::Semaphore) -> u32 {
        let swapchain = &self.internal_swapchain.swapchain;
        let (image_index, _) = unsafe {
            self.internal_swapchain
                .loader
                .acquire_next_image(*swapchain, timeout, image_ready_semaphore, vk::Fence::null())
                .expect("acquire_next_image() failed")
        };
        image_index
    }

    pub fn present(&mut self, queue: &mut DeviceQueue, frame_ready_semaphore: vk::Semaphore, image_index: u32) {
        let swapchain = &self.internal_swapchain.swapchain;
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&[frame_ready_semaphore])
            .swapchains(&[*swapchain])
            .image_indices(&[image_index])
            //.results(results: &'a mut [Result])
            .build();

        unsafe {
            self.internal_swapchain
                .loader
                .queue_present(queue.clone().into(), &present_info)
                .expect("queue_present() failed");
        }
    }

    pub fn get_surface_format(&self) -> vk::Format {
        self.internal_surface.format.format
    }

    pub fn get_surface_extent(&self) -> vk::Extent2D {
        self.internal_surface.extent
    }

    pub fn get_swapchain_loader(&self) -> &ash::extensions::khr::Swapchain {
        &self.internal_swapchain.loader
    }

    pub fn get_swapchain(&self) -> vk::SwapchainKHR {
        self.internal_swapchain.swapchain
    }
}

#[allow(dead_code)]
struct InternalSurface {
    surface_khr: vk::SurfaceKHR,
    format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
}

#[allow(dead_code)]
struct InternalSwapchain {
    loader: ash::extensions::khr::Swapchain,
    swapchain: vk::SwapchainKHR,
    present_mode: vk::PresentModeKHR,
}

pub fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::window::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    unsafe { ash_window::create_surface(entry, instance, window, None) }
}
