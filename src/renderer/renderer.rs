use crate::renderer::render3d::vertex::Vertex;
use crate::renderer::render3d::{triangle, vertex};
use crate::renderer::shaders;
use anyhow::{Context, Result};
use log::{debug, info};
use std::sync::Arc;
use std::time::Duration;
use vulkano::buffer::{CpuAccessibleBuffer, TypedBufferAccess};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, SubpassContents};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType, QueueFamily};
use vulkano::device::{Device, DeviceExtensions, Features, Queue};
use vulkano::image::view::ImageView;
use vulkano::image::{ImageAccess, ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, InstanceExtensions};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::render_pass::{Framebuffer, RenderPass, Subpass};
use vulkano::swapchain::{
    AcquireError, ColorSpace, FullscreenExclusive, PresentMode, Surface, SurfaceTransform,
    Swapchain, SwapchainCreationError,
};
use vulkano::sync::{FlushError, GpuFuture};
use vulkano::Version;
use vulkano_win::create_vk_surface_from_handle;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

pub struct Renderer {
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    surface: Arc<Surface<Window>>,
    event_loop: EventLoop<()>,
    swap_chain: Arc<Swapchain<Window>>,
    images: Vec<Arc<SwapchainImage<Window>>>,
    frame_buffers: Vec<Arc<Framebuffer>>,
    render_pass: Arc<RenderPass>,
    viewport: Viewport,
    pipelines: Pipelines,
    example_object: Arc<CpuAccessibleBuffer<[Vertex]>>,
}

pub struct Pipelines {
    draw_object: Arc<GraphicsPipeline>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        // init vulkan
        let required_extensions = vulkano_win::required_extensions();
        let instance = Instance::new(None, Version::V1_1, &required_extensions, None)?;
        println!("require {:?}", &required_extensions);
        println!("enabled {:?}", instance.enabled_extensions());

        // open window
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(500, 500))
            .build(&event_loop)?;
        let surface = create_vk_surface_from_handle(window, Arc::clone(&instance))?;

        // chose and open device
        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::none()
        };
        let (physical, queue_family) = PhysicalDevice::enumerate(&instance)
            // only supported devices
            .filter(|dev: &PhysicalDevice| {
                dev.supported_extensions()
                    .is_superset_of(&device_extensions)
            })
            // choose supported queue family for each device
            .filter_map(|dev: PhysicalDevice| {
                dev.queue_families()
                    .find(|family| {
                        family.supports_graphics() && surface.is_supported(*family).unwrap_or(false)
                    })
                    .map(|it: QueueFamily| (dev, it))
            })
            // prioritize based on device type
            .min_by_key(|(dev, _)| match dev.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
            })
            .context("Could not find a supported physical device.")?;
        info!(
            "Using {:?} device {}",
            physical.properties().device_type,
            physical.properties().device_name
        );
        info!("Using queue family {}", queue_family.id());
        let (device, mut queues) = Device::new(
            physical,
            &Features::none(),
            &device_extensions.union(physical.required_extensions()),
            [(queue_family, 0.5)],
        )?;
        let queue = queues.next().unwrap(); // unwrap: we requested exactly one queue

        // swap chain, for drawing on the window using the device
        let caps = surface.capabilities(physical)?;
        let (swap_chain, images) = Swapchain::start(Arc::clone(&device), Arc::clone(&surface))
            .usage(ImageUsage::color_attachment())
            .dimensions(surface.window().inner_size().into())
            .num_images(2.clamp(
                caps.min_image_count,
                caps.max_image_count.unwrap_or(u32::MAX),
            ))
            .build()?;

        // render pass
        let render_pass = vulkano::single_pass_renderpass!(
            Arc::clone(&device),
            attachments: {
                color: {                // `color` is a custom name we give to the first and only attachment.
                    load: Clear,
                    store: Store,
                    format: swap_chain.format(),
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        )
        .unwrap();

        // create frame buffers for drawing on the swap chain images
        let mut viewport = Viewport {
            origin: [0.0, 0.0],
            dimensions: [0.0, 0.0],
            depth_range: 0.0..1.0,
        };
        let frame_buffers = window_size_dependent_setup(&images, &render_pass, &mut viewport)?;

        let pipelines = init_pipelines(&device, &render_pass)?;

        let example_object = triangle(&device)?;

        let renderer = Renderer {
            instance,
            device,
            queue,
            event_loop,
            surface,
            swap_chain,
            images,
            frame_buffers,
            render_pass,
            viewport,
            pipelines,
            example_object,
        };
        Ok(renderer)
    }

    pub fn run_event_loop(mut self) {
        let mut recreate_swapchain = false;
        let mut previous_frame_end = Some(vulkano::sync::now(Arc::clone(&self.device)).boxed());

        self.event_loop
            .run(move |event, _, control_flow| match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(..) => recreate_swapchain = true,
                    _ => {}
                },
                Event::RedrawEventsCleared => {
                    previous_frame_end.as_mut().unwrap().cleanup_finished();

                    // update after window resized
                    if recreate_swapchain {
                        let (new_swap_chain, new_images) = match self
                            .swap_chain
                            .recreate()
                            .dimensions(self.surface.window().inner_size().into())
                            .build()
                        {
                            Ok(r) => r,
                            Err(SwapchainCreationError::UnsupportedDimensions) => return,
                            Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                        };

                        self.swap_chain = new_swap_chain;
                        self.frame_buffers = window_size_dependent_setup(
                            &new_images,
                            &self.render_pass,
                            &mut self.viewport,
                        )
                        .unwrap();
                        recreate_swapchain = false;
                    }

                    // render
                    let (image_num, suboptimal, acquire_future) =
                        match vulkano::swapchain::acquire_next_image(
                            Arc::clone(&self.swap_chain),
                            None,
                        ) {
                            Ok(r) => r,
                            Err(AcquireError::OutOfDate) => {
                                recreate_swapchain = true;
                                return;
                            }
                            Err(e) => panic!("Failed to acquire next image: {:?}", e),
                        };
                    if suboptimal {
                        recreate_swapchain = true;
                    }
                    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
                        Arc::clone(&self.device),
                        self.queue.family(),
                        CommandBufferUsage::OneTimeSubmit,
                    )
                    .unwrap();
                    command_buffer_builder
                        .begin_render_pass(
                            Arc::clone(&self.frame_buffers[image_num]),
                            SubpassContents::Inline,
                            vec![[0.0, 0.0, 1.0, 1.0].into()],
                        )
                        .unwrap()
                        .set_viewport(0, [self.viewport.clone()])
                        .bind_pipeline_graphics(Arc::clone(&self.pipelines.draw_object))
                        .bind_vertex_buffers(0, Arc::clone(&self.example_object))
                        .draw(self.example_object.len() as u32, 1, 0, 0)
                        .unwrap()
                        .end_render_pass()
                        .unwrap();
                    let command_buffer = command_buffer_builder.build().unwrap();
                    let future = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .then_execute(Arc::clone(&self.queue), command_buffer)
                        .unwrap()
                        .then_swapchain_present(
                            Arc::clone(&self.queue),
                            Arc::clone(&self.swap_chain),
                            image_num,
                        )
                        .then_signal_fence_and_flush();
                    match future {
                        Ok(future) => {
                            previous_frame_end = Some(future.boxed());
                        }
                        Err(FlushError::OutOfDate) => {
                            recreate_swapchain = true;
                            previous_frame_end =
                                Some(vulkano::sync::now(Arc::clone(&self.device)).boxed());
                        }
                        Err(e) => {
                            println!("Failed to flush future: {:?}", e);
                            previous_frame_end =
                                Some(vulkano::sync::now(Arc::clone(&self.device)).boxed());
                        }
                    }
                }
                _ => {}
            });
    }
}

fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: &Arc<RenderPass>,
    viewport: &mut Viewport,
) -> Result<Vec<Arc<Framebuffer>>> {
    let dimensions = images[0].dimensions().width_height();
    viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

    let frame_buffers = images
        .iter()
        .map(|image| {
            let view = ImageView::new(Arc::clone(image))?;
            let frame_buffer = Framebuffer::start(Arc::clone(render_pass))
                .add(view)?
                .build()?;
            Ok(frame_buffer)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(frame_buffers)
}

fn init_pipelines(device: &Arc<Device>, render_pass: &Arc<RenderPass>) -> Result<Pipelines> {
    let pipelines = Pipelines {
        draw_object: {
            let vs = shaders::vert::load(Arc::clone(device))?;
            let fs = shaders::frag::load(Arc::clone(device))?;
            GraphicsPipeline::start()
                .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
                .vertex_shader(vs.entry_point("main").unwrap(), ())
                .input_assembly_state(InputAssemblyState::new())
                .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
                .fragment_shader(fs.entry_point("main").unwrap(), ())
                .render_pass(Subpass::from(Arc::clone(render_pass), 0).unwrap())
                .build(Arc::clone(device))?
        },
    };
    Ok(pipelines)
}
