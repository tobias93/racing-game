use crate::renderer::debug::vulkan_debug_utils_callback;
use ash::extensions::{ext, khr};
use ash::prelude::VkResult;
use ash::{vk, Device, Entry, Instance};
use log::{debug, error, info, log, trace};
use std::ffi::{CStr, CString};
use std::ptr::drop_in_place;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

pub struct RenderLoopSettings {
    window_title: String,
    window_size: (u32, u32),
}

impl Default for RenderLoopSettings {
    fn default() -> Self {
        RenderLoopSettings {
            window_title: "".to_string(),
            window_size: (500, 500),
        }
    }
}

pub struct DrawContext {}

pub trait App {
    fn draw(&mut self, context: &mut DrawContext);
}

/// Main loop, initializes vulkan, opens a window and starts drawing.
///
/// Must run on main thread. Never returns.
/// CAUTION: Since the main loop hijacks the main thread and never returns, variables living on the
/// stack will not be dropped when the application exits. Anything that needs to be cleaned up
/// should be owned by the [app] object.
pub fn main_loop(settings: RenderLoopSettings, mut app: impl App + 'static) -> ! {
    unsafe {
        // window
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title(&settings.window_title)
            .with_inner_size(LogicalSize::new(
                settings.window_size.0,
                settings.window_size.1,
            ))
            .build(&event_loop)
            .expect("Could not create window");

        // Vulkan
        let entry = Entry::load().expect("Failed to load the vulkan library.");
        let mut debug_utils = None;
        let instance = create_instance(&entry, &window, &mut debug_utils);

        // Instance extensions
        let ext_surface = khr::Surface::new(&entry, &instance);

        // surface
        let mut surface = ash_window::create_surface(&entry, &instance, &window, None)
            .expect("Could not create surface.");

        // Device
        let (physical_device, device, queue) = create_device(&instance, &ext_surface, &surface);

        // device extensions
        let ext_swapchain = khr::Swapchain::new(&instance, &device);

        // Swapchain
        let (swapchain, swapchain_image_views) = create_swapchain(
            physical_device,
            &device,
            surface,
            &ext_swapchain,
            &ext_surface,
            &window,
        );

        // todo continue tutorial here https://hoj-senna.github.io/ashen-aetna/text/009_Pipelines_Renderpasses.html
        // https://github.com/ash-rs/ash/blob/master/examples/src/lib.rs

        // run event loop
        event_loop.run(move |event, _, control_flow| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                _ => {}
            },
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                app.draw(&mut DrawContext {});
                *control_flow = ControlFlow::Exit; // todo remove
            }
            Event::LoopDestroyed => {
                shutdown(
                    &entry,
                    &instance,
                    &mut debug_utils,
                    &device,
                    surface,
                    swapchain,
                    &swapchain_image_views,
                    &ext_swapchain,
                    &ext_surface,
                );
            }
            _ => {}
        });
    }
}

/// Creates the vulkan instance. Panicks on failure.
unsafe fn create_instance(
    entry: &Entry,
    window: &Window,
    debug_utils_state: &mut Option<(ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
) -> Instance {
    let mut create_options = vk::InstanceCreateInfo {
        p_application_info: &vk::ApplicationInfo {
            api_version: vk::make_api_version(0, 1, 0, 0),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut p_enabled_extension_names = vec![];
    let mut p_enabled_layer_names = vec![];

    // add validation, if requested
    let validation_layer_name = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
    if cfg!(feature = "validation") {
        p_enabled_layer_names.push(validation_layer_name.as_ptr());
        p_enabled_extension_names.push(ext::DebugUtils::name().as_ptr());
    }

    // add support for drawing on the window
    let windowing_extensions = ash_window::enumerate_required_extensions(window)
        .expect("enumerate_required_extensions failed");
    p_enabled_extension_names.extend(windowing_extensions);

    // extensions and layers
    let p_enabled_extension_names = p_enabled_extension_names; // drops the mut
    let p_enabled_layer_names = p_enabled_layer_names;
    create_options.enabled_extension_count = p_enabled_extension_names.len() as u32;
    create_options.pp_enabled_extension_names = p_enabled_extension_names.as_ptr();
    create_options.enabled_layer_count = p_enabled_layer_names.len() as u32;
    create_options.pp_enabled_layer_names = p_enabled_layer_names.as_ptr();

    let instance = entry
        .create_instance(&create_options, None)
        .expect("Failed to create the vulkan instance.");

    // configure validation layer
    if cfg!(feature = "validation") {
        let debug_utils = ext::DebugUtils::new(entry, &instance);
        let messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT {
            message_severity: vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            message_type: vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
            pfn_user_callback: Some(vulkan_debug_utils_callback),
            ..Default::default()
        };
        let messenger = debug_utils
            .create_debug_utils_messenger(&messenger_create_info, None)
            .expect("Failed to install debug messenger for validation layer");
        *debug_utils_state = Some((debug_utils, messenger));
    }
    instance
}

unsafe fn shutdown(
    entry: &Entry,
    instance: &Instance,
    debug_utils_state: &mut Option<(ext::DebugUtils, vk::DebugUtilsMessengerEXT)>,
    device: &Device,
    surface: vk::SurfaceKHR,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: &Vec<vk::ImageView>,
    ext_swapchain: &khr::Swapchain,
    ext_surface: &khr::Surface,
) {
    info!("Vulkan Shutdown");
    for image_view in swapchain_image_views {
        device.destroy_image_view(*image_view, None);
    }
    ext_swapchain.destroy_swapchain(swapchain, None);
    ext_surface.destroy_surface(surface, None);
    device.destroy_device(None);
    if let Some((debug_utils, messenger)) = debug_utils_state.take() {
        debug_utils.destroy_debug_utils_messenger(messenger, None)
    }
    instance.destroy_instance(None);
}

unsafe fn create_device(
    instance: &Instance,
    ext_surface: &khr::Surface,
    surface: &vk::SurfaceKHR,
) -> (vk::PhysicalDevice, Device, vk::Queue) {
    let physical_devices = instance
        .enumerate_physical_devices()
        .expect("Failed to list physical devices");

    let required_extensions_names = [khr::Swapchain::name()];

    // only supported devices
    let mut ok_physical_devices = physical_devices
        .iter()
        .map(|&physical_device| {
            let properties = instance.get_physical_device_properties(physical_device);
            let queue_families =
                instance.get_physical_device_queue_family_properties(physical_device);
            (physical_device, properties, queue_families)
        })
        .filter_map(|(physical_device, properties, queue_families)| {
            let device_name = CStr::from_ptr(properties.device_name.as_ptr()).to_string_lossy();

            // check device extensions
            let extensions = instance
                .enumerate_device_extension_properties(physical_device)
                .expect("Failed getting the supported device extensions");
            for required_extension in required_extensions_names {
                let supported = extensions
                    .iter()
                    .any(|it| required_extension == CStr::from_ptr(it.extension_name.as_ptr()));
                if !supported {
                    debug!(
                        "Device '{}': Missing required extension '{}'",
                        device_name,
                        required_extension.to_string_lossy()
                    );
                    return None;
                }
            }

            // look for a supported graphics queue family in this physical device
            let queue_family_index =
                queue_families
                    .iter()
                    .enumerate()
                    .position(|(index, queue_family)| {
                        let has_graphics =
                            queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                        let supports_surface = ext_surface
                            .get_physical_device_surface_support(
                                physical_device,
                                index as u32,
                                *surface,
                            )
                            .expect("Failed to check for surface support");
                        has_graphics && supports_surface
                    });

            if let Some(queue_family_index) = queue_family_index {
                debug!("Device '{}': Compatible", device_name);
                Some((physical_device, properties, queue_family_index as u32))
            } else {
                debug!(
                    "Device '{}': Has no suitable graphics queue family",
                    device_name,
                );
                None
            }
        })
        .collect::<Vec<_>>();

    // select the best available device type
    ok_physical_devices.sort_by_key(|(_, properties, _)| match properties.device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 0,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        vk::PhysicalDeviceType::CPU => 3,
        _ => 4,
    });
    let (physical_device, properties, graphics_queue_family_index) = ok_physical_devices
        .first()
        .expect("There is no compatible physical device (GPU)");
    info!(
        "Using physical device: {}",
        CStr::from_ptr(properties.device_name.as_ptr()).to_string_lossy()
    );

    let device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&[vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(*graphics_queue_family_index)
            .queue_priorities(&[1.0])
            .build()])
        .enabled_extension_names(&required_extensions_names.map(|it| it.as_ptr()))
        .build();
    let device = instance
        .create_device(*physical_device, &device_create_info, None)
        .expect("Could not create device.");
    let queue = device.get_device_queue(*graphics_queue_family_index, 0);

    (*physical_device, device, queue)
}

unsafe fn create_swapchain(
    physical_device: vk::PhysicalDevice,
    device: &Device,
    surface: vk::SurfaceKHR,
    ext_swapchain: &khr::Swapchain,
    ext_surface: &khr::Surface,
    window: &Window,
) -> (vk::SwapchainKHR, Vec<vk::ImageView>) {
    let surface_cap = ext_surface
        .get_physical_device_surface_capabilities(physical_device, surface)
        .expect("Could not get surface capabilities");

    let surface_formats = ext_surface
        .get_physical_device_surface_formats(physical_device, surface)
        .expect("Could not get surface formats");
    let surface_format = surface_formats.first().unwrap();

    let surface_present_modes = ext_surface
        .get_physical_device_surface_present_modes(physical_device, surface)
        .expect("Could not get surface presentation modes");

    let presentation_mode = if surface_present_modes.contains(&vk::PresentModeKHR::FIFO) {
        vk::PresentModeKHR::FIFO
    } else if surface_present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else if surface_present_modes.contains(&vk::PresentModeKHR::FIFO_RELAXED) {
        vk::PresentModeKHR::FIFO_RELAXED
    } else if surface_present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
        vk::PresentModeKHR::IMMEDIATE
    } else {
        *surface_present_modes
            .first()
            .expect("No supported presentation modes")
    };

    // image count: one more than surface_cap.min_image_count, unless surface_cap.max_image_count does not allow that
    let mut image_count = (surface_cap.min_image_count + 1);
    if surface_cap.max_image_count != 0 {
        image_count = image_count.min(surface_cap.max_image_count);
    }

    let (extent_x, extent_y) = if surface_cap.current_extent.width == u32::MAX
        && surface_cap.current_extent.height == u32::MAX
    {
        (
            window.inner_size().width.clamp(
                surface_cap.min_image_extent.width,
                surface_cap.max_image_extent.width,
            ),
            window.inner_size().height.clamp(
                surface_cap.min_image_extent.height,
                surface_cap.max_image_extent.height,
            ),
        )
    } else {
        (
            surface_cap.current_extent.width,
            surface_cap.current_extent.height,
        )
    };

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(image_count)
        .image_color_space(surface_format.color_space)
        .image_format(surface_format.format)
        .image_extent(vk::Extent2D {
            width: extent_x,
            height: extent_y,
        })
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(surface_cap.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(presentation_mode)
        .clipped(true)
        .image_array_layers(1)
        .build();
    let swapchain = ext_swapchain
        .create_swapchain(&swapchain_create_info, None)
        .expect("Failed to create swapchain.");

    let swapchain_images = ext_swapchain
        .get_swapchain_images(swapchain)
        .expect("Could not get swapchain images");

    let swapchain_image_views = swapchain_images
        .iter()
        .map(|image| {
            let image_view_create_info = vk::ImageViewCreateInfo::builder()
                .image(*image)
                .format(surface_format.format)
                .view_type(vk::ImageViewType::TYPE_2D)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            device.create_image_view(&image_view_create_info, None)
        })
        .collect::<Result<Vec<_>, vk::Result>>()
        .expect("Could not create Image View for swapchain image.");

    (swapchain, swapchain_image_views)
}
