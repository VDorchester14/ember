use bevy_ecs::prelude::{
    Query,
    Res,
    ResMut,
    With,
};

use cgmath::Matrix4;

use crate::core::plugins::components::TerrainComponent;
use crate::core::plugins::components::TransformComponent;
use crate::core::systems::RequiresGraphicsPipeline;
use crate::core::rendering::shaders;
use crate::core::rendering::geometries::Vertex;
use crate::core::managers::render_manager::TriangleSecondaryBuffers;
use crate::core::rendering::SceneState;
use crate::core::systems::render_systems::CameraState;
use crate::core::managers::input_manager::KeyInputQueue;
use crate::core::systems::ui_systems::EguiState;
use crate::core::plugins::components::TerrainUiComponent;

use vulkano::buffer::CpuBufferPool;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::TypedBufferAccess;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor_set::WriteDescriptorSet;
use vulkano::render_pass::RenderPass;
use vulkano::render_pass::Subpass;
use vulkano::pipeline::PartialStateMode;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::graphics::rasterization::{RasterizationState, CullMode, FrontFace};
use vulkano::pipeline::StateMode;
use vulkano::pipeline::Pipeline;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::ViewportState;
use vulkano::pipeline::graphics::depth_stencil::DepthStencilState;
use vulkano::pipeline::PipelineBindPoint;

use winit::event::VirtualKeyCode;
use winit::event::ModifiersState;

use std::sync::{Arc};

pub fn TerrainInitSystem(
    mut query: Query<&mut TerrainComponent>,
    device: Res<Arc<Device>>,
){
    log::info!("Terrain init system...");
    for mut terrain in query.iter_mut() {
        {
            terrain.geometry.lock().unwrap().generate_terrain();
        }
        terrain.initialize(device.clone());
    }
}

pub struct TerrainDrawSystemPipeline;
impl RequiresGraphicsPipeline for TerrainDrawSystemPipeline{
    fn create_graphics_pipeline(device: Arc<Device>, render_pass: Arc<RenderPass>) -> Arc<GraphicsPipeline>{

            // compile our shaders
            let vs = shaders::triangle::vs::load(device.clone()).expect("Failed to create vertex shader for triangle draw system.");
            let fs = shaders::triangle::fs::load(device.clone()).expect("Failed to create fragment shader for triangle draw system.");

            let rs = RasterizationState{
                cull_mode: StateMode::Fixed(CullMode::Back),
                front_face: StateMode::Fixed(FrontFace::CounterClockwise),
                ..Default::default()
            };

            let input_assembly_state = InputAssemblyState::new().topology(PrimitiveTopology::TriangleList);

            // create our pipeline. like an opengl program but more specific
            let pipeline = GraphicsPipeline::start()
                // We need to indicate the layout of the vertices.
                .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
                // A Vulkan shader can in theory contain multiple entry points, so we have to specify
                // which one.
                .vertex_shader(vs.entry_point("main").unwrap(), ())
                // The content of the vertex buffer describes a list of triangles.
                .input_assembly_state(input_assembly_state)
                // Use a resizable viewport set to draw over the entire window
                .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
                // See `vertex_shader`.
                .fragment_shader(fs.entry_point("main").unwrap(), ())
                .depth_stencil_state(DepthStencilState::simple_depth_test())
                .rasterization_state(rs)
                // We have to indicate which subpass of which render pass this pipeline is going to be used
                // in. The pipeline will only be usable from this particular subpass.
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                // Now that our builder is filled, we call `build()` to obtain an actual pipeline.
                .build(device.clone())
                .expect("Can't build pipeline for renderable draw system.");
            pipeline
    }
}


pub fn TerrainDrawSystem(
    query: Query<(&TransformComponent, &TerrainComponent)>,
    camera_state: Res<CameraState>,
    queue: Res<Arc<Queue>>,
    scene_state: Res<Arc<SceneState>>,
    mut buffer_vec: ResMut<TriangleSecondaryBuffers>,
){
    log::debug!("Running Terrain Draw System...");

    let viewport = scene_state.viewport();
    let pipeline: Arc<GraphicsPipeline> = scene_state.get_pipeline_for_system::<TerrainDrawSystemPipeline>().expect("Could not get pipeline from scene_state.");

    let layout = pipeline.layout().set_layouts().get(0).unwrap();
    for (transform, terrain) in query.iter() {
        log::debug!("Creating secondary command buffer builder...");
        // create buffer buildres
        // create a command buffer builder
        let mut builder = AutoCommandBufferBuilder::secondary_graphics(
            queue.device().clone(),
            queue.family(),
            CommandBufferUsage::OneTimeSubmit,
            pipeline.subpass().clone(),
        )
        .unwrap();
        
        log::debug!("Binding pipeline graphics for secondary command buffer....");
        // this is the default color of the framebuffer
        builder
            .set_viewport(0, [viewport.clone()])
            .bind_pipeline_graphics(pipeline.clone());

        let uniform_buffer: CpuBufferPool::<shaders::triangle::vs::ty::Data> = CpuBufferPool::new(
            queue.device().clone(),
            BufferUsage::all()
        );

        let g_arc = &terrain.geometry.clone();
        let geometry = g_arc.lock().unwrap();
        let uniform_buffer_subbuffer = {
            // create matrix
            let translation_matrix: Matrix4<f32> = Matrix4::from_translation(transform.global_position());
            let rotation_matrix: Matrix4<f32> = transform.rotation();
            let scale_matrix: Matrix4<f32> = Matrix4::from_scale(transform.scale());
            let model_to_world: Matrix4<f32> = rotation_matrix * translation_matrix * scale_matrix;

            
            let uniform_buffer_data = shaders::triangle::vs::ty::Data{
                mwv: (camera_state[1] * camera_state[0] * model_to_world).into()
            };
            uniform_buffer.next(uniform_buffer_data).unwrap()
        };

        let set = PersistentDescriptorSet::new(
            layout.clone(),
            [WriteDescriptorSet::buffer(0, uniform_buffer_subbuffer)]
        ).unwrap();

        log::debug!("Building secondary commands...");
        let _ = &builder
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                pipeline.layout().clone(),
                0,
                set.clone(),
            )
            .bind_vertex_buffers(0, geometry.vertex_buffer.clone().unwrap().clone())
            .bind_index_buffer(geometry.index_buffer.clone().unwrap().clone())
            .draw_indexed(
                (*geometry.index_buffer.clone().unwrap()).len() as u32,
                1,
                0,
                0,
                0
            )
            .unwrap();
        let command_buffer = builder.build().unwrap();
        buffer_vec.buffers.push(Box::new(command_buffer));
    }
}


pub fn TerrainAssemblyStateModifierSystem(
    scene_state: Res<Arc<SceneState>>,
    read_input: Res<KeyInputQueue>,
    read_modifiers: Res<ModifiersState>,
    device: Res<Arc<Device>>,
){
    log::debug!("Terrain wireframe system...");
    let input = read_input.clone();
    let modifiers = read_modifiers.clone();
    if modifiers.shift() && modifiers.alt() && input.contains(&VirtualKeyCode::Z){
        let topology = match scene_state
            .get_pipeline_for_system::<TerrainDrawSystemPipeline>()
            .expect("Couldn't get pipeline for renderable draw in wireframe system.")
            .input_assembly_state()
            .topology
        {
            PartialStateMode::Fixed(PrimitiveTopology::TriangleList) => PrimitiveTopology::LineStrip,
            PartialStateMode::Fixed(PrimitiveTopology::LineStrip) => PrimitiveTopology::TriangleList,
            _ => unreachable!(),
        };
        let subpass = Subpass::from(scene_state.render_passes[0].clone(), 0).unwrap();
        let pipeline = TerrainAssemblyStateSystemPipeline.create_pipeline(device.clone(), subpass, topology);
        scene_state.set_pipeline_for_system::<TerrainDrawSystemPipeline>(pipeline);
    }
}

pub struct TerrainAssemblyStateSystemPipeline;
impl TerrainAssemblyStateSystemPipeline {
    pub fn create_pipeline(&self, device: Arc<Device>, subpass: Subpass, topology: PrimitiveTopology) -> Arc<GraphicsPipeline> {
        // compile our shaders
        let vs = shaders::triangle::vs::load(device.clone()).expect("Failed to create vertex shader for triangle draw system.");
        let fs = shaders::triangle::fs::load(device.clone()).expect("Failed to create fragment shader for triangle draw system.");

        let rs = RasterizationState{
            cull_mode: StateMode::Fixed(CullMode::Back),
            front_face: StateMode::Fixed(FrontFace::CounterClockwise),
            ..Default::default()
        };

        let input_assembly_state = InputAssemblyState::new().topology(topology);

        // create our pipeline. like an opengl program but more specific
        let pipeline = GraphicsPipeline::start()
            // We need to indicate the layout of the vertices.
            .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
            // A Vulkan shader can in theory contain multiple entry points, so we have to specify
            // which one.
            .vertex_shader(vs.entry_point("main").unwrap(), ())
            // The content of the vertex buffer describes a list of triangles.
            .input_assembly_state(input_assembly_state)
            // Use a resizable viewport set to draw over the entire window
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            // See `vertex_shader`.
            .fragment_shader(fs.entry_point("main").unwrap(), ())
            .depth_stencil_state(DepthStencilState::simple_depth_test())
            .rasterization_state(rs)
            // We have to indicate which subpass of which render pass this pipeline is going to be used
            // in. The pipeline will only be usable from this particular subpass.
            .render_pass(subpass)
            // Now that our builder is filled, we call `build()` to obtain an actual pipeline.
            .build(device.clone())
            .expect("Can't build pipeline for renderable draw system.");
        pipeline
    }
}


pub fn TerrainUiSystem(
    mut query: Query<&mut TerrainComponent, With<TerrainUiComponent>>,
    egui_state: Res<EguiState>,
){
    log::debug!("Terrain ui system...");

    let ctx = egui_state.ctx.clone();
    for terrain in query.iter_mut(){
        let mut size = terrain.get_size();
        let mut amplitude = {
            terrain.geometry.lock().expect("Cannot get terrain in terrain ui system.").amplitude.clone()
        };

        egui::Window::new("Terrain Settings")
            .show(&ctx, |ui| {
                ui.horizontal(|ui|{
                    ui.label("Size");
                    ui.add(egui::Slider::new(&mut size, 2..=100).step_by(1.0));
                });
                ui.horizontal(|ui|{
                    ui.label("Amplidutde");
                    ui.add(egui::Slider::new(&mut amplitude, 0.1..=50.0).step_by(0.1));
                });
            });
        if size < 1 {
            size = 1;
        }
        terrain.set_size(size as usize);
        terrain.geometry.lock().expect("Cannot get terrain in terrain ui system.").amplitude = amplitude;
    }
}