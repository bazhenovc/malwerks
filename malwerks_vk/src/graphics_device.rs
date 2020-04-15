#[cfg(target_os = "macos")]
use cocoa::appkit::{NSView, NSWindow};
#[cfg(target_os = "macos")]
use cocoa::base::id as cocoa_id;
#[cfg(target_os = "macos")]
use metal::CoreAnimationLayer;
#[cfg(target_os = "macos")]
use objc::runtime::YES;
#[cfg(target_os = "macos")]
use std::mem;

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
use ash::extensions::khr::XlibSurface;
use ash::extensions::{
    ext::DebugReport,
    khr::{Surface, Swapchain},
};

#[cfg(target_os = "windows")]
use ash::extensions::khr::Win32Surface;
#[cfg(target_os = "macos")]
use ash::extensions::mvk::MacOSSurface;

use ash::version::*;
use ash::vk;

use std::default::Default;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

use crate::frame_context::*;
use crate::internal::*;

pub enum GraphicsDeviceType {
    Regular,
    Headless,
}

#[allow(dead_code)]
pub struct GraphicsDevice {
    entry: ash::Entry,
    debug_report: DebugReportCallback,
    instance: ash::Instance,
    internal_surface: Option<InternalSurface>,
    internal_swapchain: Option<InternalSwapchain>,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: InternalQueue,
    current_gpu_frame: usize,
}

impl GraphicsDevice {
    pub fn new(window: &winit::window::Window, device_type: GraphicsDeviceType) -> Self {
        let entry = ash::Entry::new().unwrap();
        let instance = unsafe {
            let layer_names = [CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
            let layer_names_ptr: Vec<*const i8> = layer_names.iter().map(|name| name.as_ptr()).collect();

            entry
                .create_instance(
                    &vk::InstanceCreateInfo::builder()
                        .application_info(
                            &vk::ApplicationInfo::builder()
                                .application_name(&CString::new("malwerks_game").unwrap())
                                .application_version(0)
                                .engine_name(&CString::new("malwerks").unwrap())
                                .engine_version(0)
                                .api_version(vk_make_version(1, 1, 0))
                                .build(),
                        )
                        .enabled_layer_names(&layer_names_ptr)
                        .enabled_extension_names(&get_platform_extension_names())
                        .build(),
                    None,
                )
                .unwrap()
        };

        let (debug_report_loader, debug_report_callback) = {
            let debug_report_loader = DebugReport::new(&entry, &instance);

            let debug_report_callback = unsafe {
                debug_report_loader
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

            (debug_report_loader, debug_report_callback)
        };

        // Create surface
        let (surface, surface_loader) = match device_type {
            GraphicsDeviceType::Regular => {
                let surface = unsafe { create_surface(&entry, &instance, &window).unwrap() };
                let surface_loader = Surface::new(&entry, &instance);
                (Some(surface), Some(surface_loader))
            }

            GraphicsDeviceType::Headless => (None, None),
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
                        for _extension in extensions.unwrap() {
                            //let extension_name_c = unsafe {
                            //    let ptr = extension.extension_name.as_ptr() as *mut c_char;
                            //    std::mem::forget(ptr);
                            //    CString::from_raw(ptr)
                            //};

                            //let extension_name = extension_name_c.to_str().unwrap();
                            //log::info!("{:?}", extension_name);

                            //supports_vk_khr_swapchain |= extension_name == "VK_KHR_swapchain";
                            supports_vk_khr_swapchain = true;
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
                            let supports_present = match device_type {
                                GraphicsDeviceType::Regular => surface_loader
                                    .as_ref()
                                    .unwrap()
                                    .get_physical_device_surface_support(
                                        **physical_device,
                                        index as u32,
                                        surface.unwrap(),
                                    )
                                    .unwrap_or(false),

                                GraphicsDeviceType::Headless => true,
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
            let device_extensions = [Swapchain::name().as_ptr()];

            let device_features = vk::PhysicalDeviceFeatures { ..Default::default() };

            let queue_priorities = [1.0];
            let queue_create_info = [vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_index)
                .queue_priorities(&queue_priorities)
                .build()];

            let device_create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(&queue_create_info)
                .enabled_extension_names(&device_extensions)
                .enabled_features(&device_features);

            unsafe {
                instance
                    .create_device(physical_device, &device_create_info, None)
                    .unwrap()
            }
        };

        // TODO: this is ugly and super unsafe, needs a rework at some point
        unsafe {
            ash_static_init(device.fp_v1_0().clone(), device.fp_v1_1().clone());
        }
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };

        let (internal_surface, internal_swapchain) = match device_type {
            GraphicsDeviceType::Regular => {
                let (surface, surface_loader) = (surface.unwrap(), surface_loader.unwrap());

                // Pick suitable surface format
                let surface_format = {
                    let surface_formats = unsafe {
                        surface_loader
                            .get_physical_device_surface_formats(physical_device, surface)
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
                        .get_physical_device_surface_capabilities(physical_device, surface)
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
                            .get_physical_device_surface_present_modes(physical_device, surface)
                            .unwrap()
                    };

                    present_modes
                        .iter()
                        .cloned()
                        .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
                        .unwrap_or(vk::PresentModeKHR::FIFO)
                };

                let swapchain_loader = Swapchain::new(&instance, &device);

                let swapchain = unsafe {
                    swapchain_loader
                        .create_swapchain(
                            &vk::SwapchainCreateInfoKHR::builder()
                                //.flags(vk::SwapchainCreateFlagsKHR::NONE)
                                .surface(surface)
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
                    loader: surface_loader,
                    surface,
                    format: surface_format,
                    extent: surface_extent,
                };
                let internal_swapchain = InternalSwapchain {
                    loader: swapchain_loader,
                    swapchain,
                    present_mode,
                };

                (Some(internal_surface), Some(internal_swapchain))
            }

            GraphicsDeviceType::Headless => (None, None),
        };

        Self {
            entry,
            debug_report: DebugReportCallback {
                loader: debug_report_loader,
                callback: debug_report_callback,
            },
            instance,
            internal_surface,
            internal_swapchain,
            physical_device,
            device,
            graphics_queue: InternalQueue {
                queue: graphics_queue,
                index: graphics_queue_index,
            },
            current_gpu_frame: 0,
        }
    }
}

impl GraphicsDevice {
    pub fn acquire_frame(&self) -> FrameContext {
        FrameContext::new(self.current_gpu_frame)
    }

    pub fn acquire_next_image(&mut self, timeout: u64, image_ready_semaphore: vk::Semaphore) -> u32 {
        let internal_swapchain = self.internal_swapchain.as_ref().unwrap();
        let swapchain = &internal_swapchain.swapchain;
        let (image_index, _) = unsafe {
            internal_swapchain
                .loader
                .acquire_next_image(*swapchain, timeout, image_ready_semaphore, vk::Fence::null())
                .expect("acquire_next_image() failed")
        };
        image_index
    }

    pub fn present(&mut self, frame_ready_semaphore: vk::Semaphore, image_index: u32) {
        let internal_swapchain = self.internal_swapchain.as_ref().unwrap();
        let swapchain = &internal_swapchain.swapchain;
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&[frame_ready_semaphore])
            .swapchains(&[*swapchain])
            .image_indices(&[image_index])
            //.results(results: &'a mut [Result])
            .build();

        unsafe {
            internal_swapchain
                .loader
                .queue_present(self.graphics_queue.queue, &present_info)
                .expect("queue_present() failed");
        }

        self.current_gpu_frame = (self.current_gpu_frame + 1) % NUM_BUFFERED_GPU_FRAMES;
    }
}

impl GraphicsDevice {
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

impl GraphicsDevice {
    pub fn get_surface_format(&self) -> vk::Format {
        self.internal_surface.as_ref().unwrap().format.format
    }

    pub fn get_surface_extent(&self) -> vk::Extent2D {
        self.internal_surface.as_ref().unwrap().extent
    }

    pub fn get_swapchain_loader(&self) -> &ash::extensions::khr::Swapchain {
        &self.internal_swapchain.as_ref().unwrap().loader
    }

    pub fn get_swapchain(&self) -> vk::SwapchainKHR {
        self.internal_swapchain.as_ref().unwrap().swapchain
    }

    pub fn get_physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn get_device(&self) -> ash::Device {
        self.device.clone()
    }

    pub fn get_instance(&self) -> ash::Instance {
        self.instance.clone()
    }

    pub fn get_graphics_queue(&self) -> crate::device_queue::DeviceQueue {
        crate::device_queue::DeviceQueue(self.graphics_queue.queue)
    }

    pub fn get_graphics_queue_index(&self) -> u32 {
        self.graphics_queue.index
    }

    pub fn create_graphics_factory(&self) -> crate::graphics_factory::GraphicsFactory {
        crate::graphics_factory::GraphicsFactory::new(self.device.clone(), self.instance.clone(), self.physical_device)
    }
}

#[allow(dead_code)]
struct InternalSurface {
    loader: ash::extensions::khr::Surface,
    surface: vk::SurfaceKHR,
    format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
}

#[allow(dead_code)]
struct InternalSwapchain {
    loader: ash::extensions::khr::Swapchain,
    swapchain: vk::SwapchainKHR,
    present_mode: vk::PresentModeKHR,
}

struct InternalQueue {
    queue: vk::Queue,
    index: u32,
}

#[allow(dead_code)]
struct DebugReportCallback {
    loader: DebugReport,
    callback: vk::DebugReportCallbackEXT,
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
fn get_platform_extension_names() -> Vec<*const i8> {
    vec![
        Surface::name().as_ptr(),
        XlibSurface::name().as_ptr(),
        DebugReport::name().as_ptr(),
    ]
}

#[cfg(target_os = "macos")]
fn get_platform_extension_names() -> Vec<*const i8> {
    vec![
        Surface::name().as_ptr(),
        MacOSSurface::name().as_ptr(),
        DebugReport::name().as_ptr(),
    ]
}

#[cfg(all(windows))]
fn get_platform_extension_names() -> Vec<*const i8> {
    vec![
        Surface::name().as_ptr(),
        Win32Surface::name().as_ptr(),
        DebugReport::name().as_ptr(),
    ]
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
pub fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::window::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use winit::platform::unix::WindowExtUnix;
    let x11_display = window.get_xlib_display().unwrap();
    let x11_window = window.get_xlib_window().unwrap();
    let x11_create_info = vk::XlibSurfaceCreateInfoKHR::builder()
        .window(x11_window)
        .dpy(x11_display as *mut vk::Display);

    let xlib_surface_loader = XlibSurface::new(entry, instance);
    xlib_surface_loader.create_xlib_surface(&x11_create_info, None)
}

#[cfg(target_os = "macos")]
pub fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::window::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use std::ptr;
    use winit::platform::macos::WindowExtMacOS;

    let wnd: cocoa_id = mem::transmute(window.get_nswindow());

    let layer = CoreAnimationLayer::new();

    layer.set_edge_antialiasing_mask(0);
    layer.set_presents_with_transaction(false);
    layer.remove_all_animations();

    let view = wnd.contentView();

    layer.set_contents_scale(view.backingScaleFactor());
    view.setLayer(mem::transmute(layer.as_ref()));
    view.setWantsLayer(YES);

    let create_info = vk::MacOSSurfaceCreateInfoMVK {
        s_type: vk::StructureType::MACOS_SURFACE_CREATE_INFO_M,
        p_next: ptr::null(),
        flags: Default::default(),
        p_view: window.get_nsview() as *const c_void,
    };

    let macos_surface_loader = MacOSSurface::new(entry, instance);
    macos_surface_loader.create_mac_os_surface_mvk(&create_info, None)
}

#[cfg(target_os = "windows")]
unsafe fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::window::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use std::ptr;
    use winapi::shared::windef::HWND;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winit::platform::windows::WindowExtWindows;

    let hwnd = window.hwnd() as HWND;
    let hinstance = GetModuleHandleW(ptr::null()) as *const c_void;
    let win32_create_info = vk::Win32SurfaceCreateInfoKHR {
        s_type: vk::StructureType::WIN32_SURFACE_CREATE_INFO_KHR,
        p_next: ptr::null(),
        flags: Default::default(),
        hinstance: hinstance as _,
        hwnd: hwnd as *const c_void,
    };
    let win32_surface_loader = Win32Surface::new(entry, instance);
    win32_surface_loader.create_win32_surface(&win32_create_info, None)
}

unsafe extern "system" fn vulkan_debug_callback(
    _: vk::DebugReportFlagsEXT,
    _: vk::DebugReportObjectTypeEXT,
    _: u64,
    _: usize,
    _: i32,
    _: *const c_char,
    p_message: *const c_char,
    _: *mut c_void,
) -> u32 {
    log::warn!("{:?}", CStr::from_ptr(p_message));
    vk::FALSE
}

pub const fn vk_make_version(major: u32, minor: u32, patch: u32) -> u32 {
    (major << 22) | (minor << 12) | patch
}
