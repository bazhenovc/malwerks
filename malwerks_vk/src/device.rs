// Copyright (c) 2020 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ash::version::*;
use ash::vk;

use crate::frame_context::*;
use crate::internal::*;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

pub enum SurfaceMode<T> {
    WindowSurface(T),
    Headless(T),
}

#[derive(Default)]
pub struct DeviceOptions {
    pub enable_validation: bool,
    pub enable_ray_tracing_nv: bool,
    pub _reserved: bool,
}

pub struct Device {
    entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: InternalQueue,
    surface_loader: Option<ash::extensions::khr::Surface>,
    surface_khr: vk::SurfaceKHR,
    _debug_report: Option<DebugReportCallback>,
    current_gpu_frame: usize,
}

impl Device {
    pub fn new<T>(surface_mode: SurfaceMode<T>, options: DeviceOptions) -> Self
    where
        T: Fn(&ash::Entry, &ash::Instance) -> vk::SurfaceKHR,
    {
        let entry = ash::Entry::new().unwrap();
        let instance = unsafe {
            let mut layer_name_data = Vec::with_capacity(1);
            let mut layer_names = Vec::with_capacity(1);
            if options.enable_validation {
                layer_name_data.push(CString::new("VK_LAYER_KHRONOS_validation").unwrap());
                layer_names.push(layer_name_data.last().unwrap().as_ptr());
            }

            let mut instance_extension_names = Vec::with_capacity(PLATFORM_EXTENSION_NAME_COUNT + 2);
            if let SurfaceMode::WindowSurface(_) = surface_mode {
                for ext in get_platform_extension_names().iter() {
                    instance_extension_names.push(*ext);
                }
            }
            if options.enable_validation {
                instance_extension_names.push(ash::extensions::ext::DebugReport::name().as_ptr());
            }
            if options.enable_ray_tracing_nv {
                instance_extension_names.push(vk::KhrGetPhysicalDeviceProperties2Fn::name().as_ptr());
            }

            let application_name = CString::new("malwerks_game").unwrap();
            let engine_name = CString::new("malwerks").unwrap();
            let application_info = vk::ApplicationInfo::builder()
                .application_name(&application_name)
                .application_version(0)
                .engine_name(&engine_name)
                .engine_version(0)
                .api_version(vk_make_version(1, 1, 0))
                .build();

            let mut instance_create_info = vk::InstanceCreateInfo::builder().application_info(&application_info);
            if !layer_names.is_empty() {
                instance_create_info = instance_create_info.enabled_layer_names(&layer_names);
            }
            if !instance_extension_names.is_empty() {
                log::info!("requested instance extensions: {:?}", &instance_extension_names);
                instance_create_info = instance_create_info.enabled_extension_names(&instance_extension_names);
            }

            entry.create_instance(&instance_create_info.build(), None).unwrap()
        };

        let (surface_loader, surface_khr) = match &surface_mode {
            SurfaceMode::WindowSurface(create_surface) => {
                let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);
                let surface_khr = create_surface(&entry, &instance);

                (Some(surface_loader), surface_khr)
            }

            SurfaceMode::Headless(create_surface) => {
                let surface_khr = create_surface(&entry, &instance);
                (None, surface_khr)
            }
        };

        let debug_report = if options.enable_validation {
            let loader = ash::extensions::ext::DebugReport::new(&entry, &instance);
            let callback = unsafe {
                loader
                    .create_debug_report_callback(
                        &vk::DebugReportCallbackCreateInfoEXT::builder()
                            .flags(
                                vk::DebugReportFlagsEXT::ERROR
                                    | vk::DebugReportFlagsEXT::WARNING
                                    | vk::DebugReportFlagsEXT::PERFORMANCE_WARNING,
                            )
                            .pfn_callback(Some(vulkan_debug_callback))
                            .build(),
                        None,
                    )
                    .unwrap()
            };

            Some(DebugReportCallback { loader, callback })
        } else {
            None
        };

        // Find suitable physical device
        let (physical_device, graphics_queue_index) = {
            let device_enumeration = unsafe { instance.enumerate_physical_devices().unwrap() };
            let mut devices: Vec<(
                &vk::PhysicalDevice,
                vk::PhysicalDeviceProperties,
                vk::PhysicalDeviceFeatures,
            )> = device_enumeration
                .iter()
                .map(|device| {
                    // Extract properties and features for later use
                    let (properties, _features) = unsafe {
                        (
                            instance.get_physical_device_properties(*device),
                            instance.get_physical_device_features(*device),
                        )
                    };

                    // Figure out whether this device supports needed extensions
                    let supports_needed_extensions = {
                        let mut supports_vk_khr_swapchain = false;

                        let extensions = unsafe { instance.enumerate_device_extension_properties(*device) };
                        for extension in extensions.unwrap() {
                            supports_vk_khr_swapchain |= unsafe {
                                libc::strcmp(extension.extension_name.as_ptr(), vk::KhrSwapchainFn::name().as_ptr())
                                    == 0
                            };
                        }

                        supports_vk_khr_swapchain
                    };

                    if supports_needed_extensions {
                        log::info!("Suitable physical device: {:?}", device);
                        Some((device, properties, _features))
                    } else {
                        None
                    }
                })
                .filter_map(|v| v)
                .collect();

            // Sort devices, put discrete GPUs first
            devices.sort_by(|device0, device1| {
                if device0.1.device_type == device1.1.device_type {
                    std::cmp::Ordering::Equal
                } else if device0.1.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                    std::cmp::Ordering::Less
                } else {
                    assert!(device1.1.device_type == vk::PhysicalDeviceType::DISCRETE_GPU);
                    std::cmp::Ordering::Greater
                }
            });

            // Find suitable graphics queue
            devices
                .iter()
                .map(|(physical_device, _device_properties, _device_features)| unsafe {
                    instance
                        .get_physical_device_queue_family_properties(**physical_device)
                        .iter()
                        .enumerate()
                        .filter_map(|(index, ref queue_properties)| {
                            let supports_graphics = queue_properties.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                            let supports_compute = queue_properties.queue_flags.contains(vk::QueueFlags::COMPUTE);

                            let supports_present = match &surface_loader {
                                Some(surface_loader) => surface_loader
                                    .get_physical_device_surface_support(**physical_device, index as u32, surface_khr)
                                    .unwrap_or(false),
                                None => true,
                            };

                            if supports_graphics && supports_compute && supports_present {
                                Some((**physical_device, index as u32))
                            } else {
                                None
                            }
                        })
                        .next()
                })
                .filter_map(|v| v)
                .next()
                .expect("Couldn't find suitable device.")
        };

        let device = {
            let device_features = vk::PhysicalDeviceFeatures { ..Default::default() };

            let queue_priorities = [1.0];
            let queue_create_info = [vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_index)
                .queue_priorities(&queue_priorities)
                .build()];

            let mut device_create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(&queue_create_info)
                .enabled_features(&device_features);

            let mut device_extension_names = Vec::with_capacity(3);
            if let SurfaceMode::WindowSurface(_) = surface_mode {
                device_extension_names.push(ash::extensions::khr::Swapchain::name().as_ptr());
            }
            if options.enable_ray_tracing_nv {
                device_extension_names.push(vk::KhrGetMemoryRequirements2Fn::name().as_ptr());
                device_extension_names.push(vk::NvRayTracingFn::name().as_ptr());
            }

            if !device_extension_names.is_empty() {
                log::info!("requested device extensions: {:?}", &device_extension_names);
                device_create_info = device_create_info.enabled_extension_names(&device_extension_names);
            }

            unsafe {
                instance
                    .create_device(physical_device, &device_create_info.build(), None)
                    .unwrap()
            }
        };

        // TODO: this is ugly and super unsafe, needs a rework at some point
        unsafe {
            let ray_tracing_nv = vk::NvRayTracingFn::load(|name| {
                std::mem::transmute(instance.get_device_proc_addr(device.handle(), name.as_ptr()))
            });
            ash_static_init(device.fp_v1_0().clone(), device.fp_v1_1().clone(), ray_tracing_nv);
        }
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };

        Device {
            entry,
            instance,
            physical_device,
            device,
            graphics_queue: InternalQueue {
                queue: graphics_queue,
                index: graphics_queue_index,
            },
            surface_loader,
            surface_khr,
            _debug_report: debug_report,
            current_gpu_frame: 0,
        }
    }
}

impl Device {
    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkResetFences.html"]
    pub fn reset_fences(&self, fences: &[vk::Fence]) {
        unsafe {
            self.device.reset_fences(fences).unwrap();
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkWaitForFences.html"]
    pub fn wait_for_fences(&self, fences: &[vk::Fence], wait_all: bool, timeout: u64) {
        unsafe { self.device.wait_for_fences(fences, wait_all, timeout).unwrap() }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetFenceStatus.html"]
    pub fn get_fence_status(&self, fence: vk::Fence) -> bool {
        unsafe {
            match self.device.get_fence_status(fence) {
                Ok(_) => true,
                _ => false,
            }
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkDeviceWaitIdle.html"]
    pub fn wait_idle(&self) {
        unsafe {
            self.device.device_wait_idle().expect("wait_idle() failed");
        }
    }

    #[doc = "https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/vkGetDeviceQueue.html"]
    pub fn get_device_queue(&self, queue_family_index: u32, queue_index: u32) -> crate::device_queue::DeviceQueue {
        crate::device_queue::DeviceQueue(unsafe { self.device.get_device_queue(queue_family_index, queue_index) })
    }
}

impl Device {
    pub fn begin_frame(&self) -> FrameContext {
        FrameContext::new(self.current_gpu_frame)
    }

    pub fn end_frame(&mut self, frame_context: FrameContext) {
        assert_eq!(frame_context.current_gpu_frame, self.current_gpu_frame);
        self.current_gpu_frame = (self.current_gpu_frame + 1) % NUM_BUFFERED_GPU_FRAMES;
    }
}

impl Device {
    pub fn get_entry(&self) -> &ash::Entry {
        &self.entry
    }

    pub fn get_instance(&self) -> &ash::Instance {
        &self.instance
    }

    pub fn get_physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn get_device(&self) -> &ash::Device {
        &self.device
    }

    pub fn get_graphics_queue(&self) -> crate::device_queue::DeviceQueue {
        crate::device_queue::DeviceQueue(self.graphics_queue.queue)
    }

    pub fn get_graphics_queue_index(&self) -> u32 {
        self.graphics_queue.index
    }

    pub fn get_surface_loader(&self) -> &Option<ash::extensions::khr::Surface> {
        &self.surface_loader
    }

    pub fn get_surface_khr(&self) -> vk::SurfaceKHR {
        self.surface_khr
    }
}

impl Device {
    pub fn create_factory(&self) -> crate::device_factory::DeviceFactory {
        crate::device_factory::DeviceFactory::new(self.device.clone(), self.instance.clone(), self.physical_device)
    }

    pub fn get_ray_tracing_properties(&self) -> vk::PhysicalDeviceRayTracingPropertiesNV {
        let mut ray_tracing_properties = vk::PhysicalDeviceRayTracingPropertiesNV::default();
        let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut ray_tracing_properties);
        unsafe {
            self.instance
                .get_physical_device_properties2(self.physical_device, &mut properties);
        }
        ray_tracing_properties
    }
}

struct InternalQueue {
    queue: vk::Queue,
    index: u32,
}

#[allow(dead_code)]
struct DebugReportCallback {
    loader: ash::extensions::ext::DebugReport,
    callback: vk::DebugReportCallbackEXT,
}

const PLATFORM_EXTENSION_NAME_COUNT: usize = 2;

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
fn get_platform_extension_names() -> [*const i8; PLATFORM_EXTENSION_NAME_COUNT] {
    [
        ash::extensions::khr::Surface::name().as_ptr(),
        ash::extensions::khr::XlibSurface::name().as_ptr(),
    ]
}

#[cfg(target_os = "macos")]
fn get_platform_extension_names() -> [*const i8; PLATFORM_EXTENSION_NAME_COUNT] {
    [
        ash::extensions::khr::Surface::name().as_ptr(),
        ash::extensions::khr::MacOSSurface::name().as_ptr(),
    ]
}

#[cfg(all(windows))]
fn get_platform_extension_names() -> [*const i8; PLATFORM_EXTENSION_NAME_COUNT] {
    [
        ash::extensions::khr::Surface::name().as_ptr(),
        ash::extensions::khr::Win32Surface::name().as_ptr(),
    ]
}

unsafe extern "system" fn vulkan_debug_callback(
    flags: vk::DebugReportFlagsEXT,
    _: vk::DebugReportObjectTypeEXT,
    _: u64,
    _: usize,
    _: i32,
    _: *const c_char,
    p_message: *const c_char,
    _: *mut c_void,
) -> u32 {
    if flags & vk::DebugReportFlagsEXT::INFORMATION == vk::DebugReportFlagsEXT::INFORMATION {
        log::info!("{:?}", CStr::from_ptr(p_message));
    } else {
        log::error!("{:?}", CStr::from_ptr(p_message));
        panic!("{:?}", CStr::from_ptr(p_message));
    }
    vk::FALSE
}

pub const fn vk_make_version(major: u32, minor: u32, patch: u32) -> u32 {
    (major << 22) | (minor << 12) | patch
}
