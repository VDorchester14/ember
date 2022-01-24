// internal imports
use crate::core::{
    plugins::{
        components::{
            renderable_component::RenderableComponent,
            transform_component::TransformComponent,
            camera_component::CameraComponent,
        },
    },
    rendering::{
        geometries::{
            geometry::{
                Vertex
            }
        },
        shaders::triangle::{
            vs,
            fs,
        },
    },
    scene::{
        scene::{Scene, Initialized}
    },
};

// ecs
use specs::{System, ReadStorage, ReadExpect, WriteStorage, Join};
use specs::prelude::*;

// Vulkano imports
use vulkano::{
    instance::{
        Instance,
        InstanceExtensions,
    },
    device::{
        physical::{
            PhysicalDevice,
            PhysicalDeviceType,
            QueueFamily,
        },
        Device,
        DeviceExtensions,
        Features,
        Queue,
        QueuesIter
    },
    swapchain::{
        AcquireError,
        Swapchain,
        SwapchainCreationError,
        SwapchainAcquireFuture,
    },
    swapchain,
    image::{
        view::{
            ImageView,
        },
        ImageUsage,
        SwapchainImage,
        ImageAccess,
    },
    render_pass::{
        Framebuffer,
        RenderPass,
        Subpass,
    },
    pipeline::{
        graphics::{
            vertex_input::BuffersDefinition,
            input_assembly::InputAssemblyState,
            viewport::{Viewport, ViewportState}
        },
        GraphicsPipeline,
    },
    sync::{
        FlushError,
        GpuFuture,
    },
    sync,
    command_buffer::{
        AutoCommandBufferBuilder,
        CommandBufferUsage,
        SubpassContents,
    },
    buffer::{
        BufferUsage,
        CpuBufferPool,
        TypedBufferAccess,
    },
    Version,
};

// vulkano_win imports
use vulkano_win::{
    VkSurfaceBuild,
};

// winit imports
use winit::{
    event_loop::{
        EventLoop
    },
    window::{
        Window,
        WindowBuilder
    },
};

// std imports
use std::sync::Arc;

// math
use cgmath::Matrix4;

// logging
use log;


pub struct RenderManager{
    // ECS Systems
    scene_prep_system: RenderableInitializerSystem,
    primitive_command_buffer_builder_system: PrimitiveCommandBufferBuilderSystem,

    // Vulkan
    required_extensions: Option<InstanceExtensions>,
    device_extensions: Option<DeviceExtensions>,
    minimal_features: Option<Features>,
    optimal_features: Option<Features>,
    instance: Option<Arc<Instance>>,
    pub surface: Option<Arc<vulkano::swapchain::Surface<winit::window::Window>>>,
    pub device: Option<Arc<Device>>,
    pub queue: Option<Arc<Queue>>,
    pub swapchain: Option<Arc<Swapchain<winit::window::Window>>>,
    pub render_pass: Option<Arc<RenderPass>>,
    pub pipeline: Option<Arc<GraphicsPipeline>>,
    pub framebuffers: Option<Vec<Arc<Framebuffer>>>,
    pub recreate_swapchain: bool,
    pub previous_frame_end: Option<Box<dyn GpuFuture>>,
    pub viewport: Option<Viewport>,
}

impl RenderManager{
    pub fn startup(&mut self) -> (EventLoop<()>, Arc<vulkano::swapchain::Surface<winit::window::Window>>){
        log::info!("Starting RenderManager...");

        // get extensions
        let (required_extensions, device_extensions) = RenderManager::get_required_extensions();

        // create an instance of vulkan with the required extensions
        let instance = Instance::new(None, Version::V1_1, &required_extensions, None).unwrap();

        // create event_loop and surface
        let (event_loop, surface) = RenderManager::create_event_loop_and_surface(instance.clone());

        // get our physical device and queue family
        let (physical_device, queue_family) = RenderManager::get_physical_device_and_queue_family(
            &instance,
            device_extensions.clone(),
            surface.clone()
        );

        // logging the physical device
        log::info!(
            "Using device: {} (type: {:?})",
            physical_device.properties().device_name,
            physical_device.properties().device_type,
        );

        // now create the logical device and queues
        let (device, mut queues) = RenderManager::get_logical_device_and_queues(
            physical_device,
            &device_extensions,
            queue_family
        );

        // get queue
        let queue = queues.next().unwrap();

        // create swapchain, images
        let (swapchain, images) = RenderManager::create_swapchain_and_images(
            physical_device,
            surface.clone(),
            device.clone(),
            queue.clone()
        );

        // compile our shaders
        let vs = vs::load(device.clone()).unwrap();
        let fs = fs::load(device.clone()).unwrap();

        // create our render pass
        let render_pass = vulkano::single_pass_renderpass!(
                device.clone(),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: swapchain.format(),
                        samples: 1,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {}
                }
            )
            .unwrap();
        
        // create our pipeline. like an opengl program but more specific
        let pipeline = GraphicsPipeline::start()
                // We need to indicate the layout of the vertices.
                .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
                // A Vulkan shader can in theory contain multiple entry points, so we have to specify
                // which one.
                .vertex_shader(vs.entry_point("main").unwrap(), ())
                // The content of the vertex buffer describes a list of triangles.
                .input_assembly_state(InputAssemblyState::new())
                // Use a resizable viewport set to draw over the entire window
                .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
                // See `vertex_shader`.
                .fragment_shader(fs.entry_point("main").unwrap(), ())
                // We have to indicate which subpass of which render pass this pipeline is going to be used
                // in. The pipeline will only be usable from this particular subpass.
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                // Now that our builder is filled, we call `build()` to obtain an actual pipeline.
                .build(device.clone())
                .unwrap();

        let mut viewport = Viewport {
            origin: [0.0, 0.0],
            dimensions: [0.0, 0.0],
            depth_range: 0.0..1.0,
        };

        let framebuffers = RenderManager::window_size_dependent_setup(&images, render_pass.clone(), &mut viewport);
        let recreate_swapchain = false;
        let previous_frame_end = Some(sync::now(device.clone()).boxed());

        // clone the surface so we can return this clone
        let return_surface = surface.clone();

        // fill options with initialized values
        self.required_extensions = Some(required_extensions);
        self.device_extensions = Some(device_extensions);
        self.instance = Some(instance);
        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.swapchain = Some(swapchain);
        self.render_pass = Some(render_pass);
        self.pipeline = Some(pipeline);
        self.framebuffers = Some(framebuffers);
        self.previous_frame_end = previous_frame_end;
        self.recreate_swapchain = false;
        self.viewport = Some(viewport);

        (event_loop, return_surface)
    }

    // shut down render manager
    pub fn shutdown(&mut self){
        log::info!("Shutting down render manager...");
    }

    // update render manager
    pub fn update(&mut self){
    }

    // create a new render manager with uninitialized values
    pub fn create_new() -> Self {
        log::info!("Creating RenderManager...");

        // initialize our render system with all of the required vulkan components
        let render_sys = RenderManager{
            // ECS Systemes
            scene_prep_system: RenderableInitializerSystem{},
            primitive_command_buffer_builder_system: PrimitiveCommandBufferBuilderSystem{},

            // Vulkan
            required_extensions: None,
            device_extensions: None,
            minimal_features: None,
            optimal_features: None,
            instance: None,
            surface: None,
            device: None,
            queue: None,
            swapchain: None,
            render_pass: None,
            pipeline: None,
            framebuffers: None,
            recreate_swapchain: false,
            previous_frame_end: None,
            viewport: None,
        };
        render_sys
    }

    // run the render manager
    pub fn run(&mut self) {
        // self.window.run();
    }

    pub fn draw(&mut self, scene: &mut Scene<Initialized>){
        // prep scene by inserting device and other operations
        self.insert_render_data_into_scene(scene); // inserts vulkan resources into scene
        self.scene_prep_system.run(scene.get_world().unwrap().system_data()); // initializes renderables

        self.previous_frame_end.as_mut().unwrap().cleanup_finished();

        // acquire an image from the swapchain
        let (image_num, suboptimal, acquire_future) = self.acquire_swapchain_image();

        if suboptimal {
            self.recreate_swapchain()
        }

        // this is the default color of the framebuffer
        let clear_values = vec![[0.2, 0.2, 0.2, 1.0].into()];

        // create a command buffer builder
        let mut builder = AutoCommandBufferBuilder::primary(
            self.device(),
            self.queue().family(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        let world = scene.get_world().unwrap();
        let system_data: ReadStorage<RenderableComponent> = world.system_data();
        let renderables = world.read_storage::<RenderableComponent>();
        let transforms = world.read_storage::<TransformComponent>();
        // let vertex_buffers: Vec<_> = (&renderables).join();
        // let mut vertex_buffers = vec!();
        // let mut index_buffers = vec!();

        builder
            .begin_render_pass(
                self.framebuffers()[image_num].clone(),
                SubpassContents::Inline,
                clear_values,
            )
            .unwrap()
            .set_viewport(0, [self.viewport.clone().unwrap()])
            .bind_pipeline_graphics(self.pipeline());

        let dimensions: [u32; 2] = self.surface().window().inner_size().into();
        let aspect = dimensions[0] as f64 / dimensions[1] as f64;

        let (view, perspective) = {
            let mut cameras = world.write_storage::<CameraComponent>();
            let mut view: Matrix4<f64> = Matrix4::from_scale(1.0);
            let mut perspective: Matrix4<f64> = Matrix4::from_scale(1.0);

            for camera in (&mut cameras).join() {
                camera.aspect = aspect;
                camera.calculate_view();
                view = camera.get_view();
                perspective = camera.get_perspective();
            }
            (view, perspective)
        };

        let uniform_buffer: CpuBufferPool::<Matrix4<f64>> = CpuBufferPool::new(self.device(), BufferUsage::all());

        for (renderable, transform) in (&renderables, &transforms).join() {
            // create matrix
            let translation_matrix = Matrix4::from_translation(transform.global_position);
            let rotation_matrix = transform.rotation;
            let model_to_world = rotation_matrix * translation_matrix;

            let uniform_buffer_subbuffer = {
                uniform_buffer.next(perspective * view * model_to_world).unwrap()
            };
            let pipeline = self.pipeline();
            let g_arc = &renderable.geometry();
            let geometry = g_arc.lock().unwrap();
            // let a: u32 = geometry;
            &builder
                .bind_vertex_buffers(0, geometry.vertex_buffer().clone())
                .bind_index_buffer(geometry.index_buffer().clone())
                .draw_indexed(
                    (*geometry.index_buffer()).len() as u32,
                    1,
                    0,
                    0,
                    0
                )
                .unwrap();
        }
        log::info!("made it all the way out");
        builder.end_render_pass().unwrap();

        // actually build command buffer now
        let command_buffer = builder.build().unwrap();

        // now get future state and try to draw
        // let x: u32 = _previous_frame_end.take().unwrap();
        // TODO : Fix crash here
        let future = self.previous_frame_end
            .take()
            .unwrap()
            .join(acquire_future)
            .then_execute(self.queue(), command_buffer)
            .unwrap()
            .then_swapchain_present(self.queue(), self.swapchain(), image_num)
            .then_signal_fence_and_flush();

        match future {
            Ok(future) => {
                self.previous_frame_end = Some(future.boxed());
            }
            Err(FlushError::OutOfDate) => {
                self.recreate_swapchain = true;
                self.previous_frame_end = Some(sync::now(self.device().clone()).boxed());
            }
            Err(e) => {
                log::error!("Failed to flush future: {:?}", e);
                self.previous_frame_end = Some(sync::now(self.device().clone()).boxed());
            }
        }

    }

    // ================= //
    // Helper Functions  //
    // ================= //

    /// This method is called once during initialization, then again whenever the window is resized
    fn window_size_dependent_setup(
        images: &[Arc<SwapchainImage<Window>>],
        render_pass: Arc<RenderPass>,
        viewport: &mut Viewport,
    ) -> Vec<Arc<Framebuffer>> {
        let dimensions = images[0].dimensions().width_height();
        viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

        images
            .iter()
            .map(|image| {
                let view = ImageView::new(image.clone()).unwrap();
                Framebuffer::start(render_pass.clone())
                    .add(view)
                    .unwrap()
                    .build()
                    .unwrap()
            })
            .collect::<Vec<_>>()
    }

    // insert required render data into scene so systems can run
    pub fn insert_render_data_into_scene(&mut self, scene: &mut Scene<Initialized>) {
        scene.insert_resource(self.device.clone().unwrap().clone());
        scene.insert_resource(self.pipeline.clone().unwrap().clone());
    }

    // returns the required winit extensions and the required extensions of my choosing
    pub fn get_required_extensions() -> (InstanceExtensions, DeviceExtensions) {
        // what extensions do we need to have in vulkan to draw a window
        let required_extensions = vulkano_win::required_extensions();

        // choose the logical device extensions we're going to use
        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::none()
        };

        (required_extensions, device_extensions)
    }

    // creates a surface and ties it to the event loop
    pub fn create_event_loop_and_surface(instance: Arc<Instance>) -> (EventLoop<()>, Arc<vulkano::swapchain::Surface<winit::window::Window>>) {
        let event_loop = EventLoop::new();
        let surface = WindowBuilder::new()
            .with_title("Ember")
            .build_vk_surface(&event_loop, instance)
            .unwrap();
        (event_loop, surface)
    }

    // gets physical GPU and queues
    pub fn get_physical_device_and_queue_family(
        instance: &Arc<Instance>,
        device_extensions: DeviceExtensions,
        surface: Arc<vulkano::swapchain::Surface<winit::window::Window>>
    ) -> (PhysicalDevice, QueueFamily) {
        // get our physical device and queue family
        let (physical_device, queue_family) = PhysicalDevice::enumerate(&instance)
            .filter(|&p| { // filter to devices that contain desired features
                p.supported_extensions().is_superset_of(&device_extensions)
            })
            .filter_map(|p| { // filter queue families to ones that support graphics
                p.queue_families() // TODO : pick beter queue families since this is one single queue
                    .find(|&q| {
                        q.supports_graphics() && surface.is_supported(q).unwrap_or(false)
                    })
                    .map(|q| (p, q))
            })
            .min_by_key(|(p, _)| { // pick the best device based on a score we assign
                match p.properties().device_type {
                    PhysicalDeviceType::DiscreteGpu => 0,
                    PhysicalDeviceType::IntegratedGpu => 1,
                    PhysicalDeviceType::VirtualGpu => 2,
                    PhysicalDeviceType::Cpu => 3,
                    PhysicalDeviceType::Other => 4,
                }
            })
            .unwrap();

            (physical_device, queue_family)
    }

    // create logical device and queues. Currently a very thin pass-through
    // but it's here in case i ever want to extend this
    pub fn get_logical_device_and_queues(
        physical_device: PhysicalDevice,
        device_extensions: &DeviceExtensions,
        queue_family: QueueFamily,
    ) -> (Arc<Device>, QueuesIter){
        // now create logical device and queues
        let (device, mut queues) = Device::new(
            physical_device,
            &Features::none(),
            &physical_device
                .required_extensions()
                .union(&device_extensions),
            [(queue_family, 0.5)].iter().cloned(),
        ).unwrap();

        (device, queues)
    }

    // Create swapchain and images
    pub fn create_swapchain_and_images(
        physical_device: PhysicalDevice,
        surface: Arc<vulkano::swapchain::Surface<winit::window::Window>>,
        device: Arc<Device>,
        queue: Arc<Queue>
    ) -> (Arc<Swapchain<Window>>, Vec<Arc<SwapchainImage<Window>>>) {
        let caps = surface.capabilities(physical_device).unwrap();
        let composite_alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;
        let dimensions: [u32; 2] = surface.window().inner_size().into();

        Swapchain::start(device.clone(), surface.clone())
            .num_images(caps.min_image_count)
            .format(format)
            .dimensions(dimensions)
            .usage(ImageUsage::color_attachment())
            .sharing_mode(&queue)
            .composite_alpha(composite_alpha)
            .build()
            .unwrap()
    }

    // if the swapchain needs to be recreated
    pub fn recreate_swapchain(&mut self){
        log::debug!("Recreating swapchain...");
        let dimensions: [u32; 2] = self.surface.clone().unwrap().clone().window().inner_size().into();
        let (new_swapchain, new_images) =
        match self.swapchain
            .clone()
            .unwrap()
            .clone()
            .recreate()
            .dimensions(dimensions)
            .build() {
                Ok(r) => r,
                // This error tends to happen when the user is manually resizing the window.
                // Simply restarting the loop is the easiest way to fix this issue.
                Err(SwapchainCreationError::UnsupportedDimensions) => return,
                Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
        };
        self.recreate_swapchain = false;
        self.framebuffers = Some(
            RenderManager::window_size_dependent_setup(
                &new_images,
                self.render_pass
                    .clone()
                    .unwrap()
                    .clone(),
                &mut self.viewport
                    .clone()
                    .unwrap()
            )
        );
        self.swapchain = Some(new_swapchain);
    } // end of if on swapchain recreation

    // acquires the next swapchain image
    pub fn acquire_swapchain_image(&mut self) -> (usize, bool, SwapchainAcquireFuture<Window>) {
        match swapchain::acquire_next_image(self.swapchain(), None) {
            Ok(r) => r,
            Err(AcquireError::OutOfDate) => {
                self.recreate_swapchain();
                self.acquire_swapchain_image()
            }
            Err(e) => panic!("Failed to acquire next image: {:?}", e),
        }
    }

    // getters
    pub fn framebuffers(&self) -> Vec<Arc<Framebuffer>> {
        self.framebuffers.clone().unwrap().clone()
    }

    pub fn pipeline(&self) -> Arc<GraphicsPipeline> {
        self.pipeline.clone().unwrap().clone()
    }

    pub fn device(&self) -> Arc<Device> {
        self.device.clone().unwrap().clone()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.queue.clone().unwrap().clone()
    }

    pub fn surface(&self) -> Arc<vulkano::swapchain::Surface<winit::window::Window>> {
        self.surface.clone().unwrap().clone()
    }

    pub fn render_pass(&self) -> Arc<RenderPass> {
        self.render_pass.clone().unwrap().clone()
    }

    pub fn swapchain(&self) -> Arc<Swapchain<winit::window::Window>> {
        self.swapchain.clone().unwrap().clone()
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport.clone().unwrap()
    }
}


pub struct RenderableInitializerSystem;


impl<'a> System<'a> for RenderableInitializerSystem{
    type SystemData = (
        ReadExpect<'a, Arc<Device>>,
        WriteStorage<'a, RenderableComponent>,
    );

    fn run(&mut self, data: Self::SystemData) {

        let (device, mut renderable) = data;
        let device = &*device;
        for renderable in (&mut renderable).join() {
            if renderable.initialized() == false{
                renderable.initialize(device.clone());
            }
        }
    }

}

pub struct PrimitiveCommandBufferBuilderSystem;
//
// impl<'a> System<'a> for PrimitiveCommandBufferBuilderSystem{
//     type SystemData = (
//         ReadExpect<'a, Arc<GraphicsPipeline<BuffersDefinition>>>,
//         ReadExpect<'a, Arc<DynamicState>>,
//         ReadExpect<'a, Arc<Device>>,
//         ReadStorage<'a, RenderableComponent>,
//         // ReadExpect<'a, AutoCommandBufferBuilder<SecondaryAutoCommandBuffer>>,
//     );
//
//     fn run(&mut self, data: Self::SystemData){
//         let(pipeline, dynamic_state, device, renderable) = data;
//         // let(pipeline, dynamic_state, renderable, command_buffer) = data;
//
//         // create a command buffer builder
//         let mut builder = AutoCommandBufferBuilder::secondary_graphics(
//             self.device(),
//             self.queue().family(),
//             CommandBufferUsage::OneTimeSubmit,
//         )
//         .unwrap();
//
//     }
// }
