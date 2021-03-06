use crate::core::plugins::components::{DebugUiComponent, CameraComponent, TransformComponent, TransformUiComponent};
// use egui_winit::State;
use egui_vulkano::Painter;
use egui::Context;


use bevy_ecs::prelude::{
    Res,
    ResMut,
    Query,
    With,
};
// use puffin_egui;

use log;


pub struct EguiState{
    pub ctx: Context,
    pub painter: Painter,
}

pub fn DebugUiSystem(
    mut query: Query<&mut DebugUiComponent>,
    egui_state: Res<EguiState>,
    mut should_save: ResMut<bool>,
){
    log::debug!("Debug ui...");
    let ctx = egui_state.ctx.clone();
    for mut comp in query.iter_mut() {

        // draw ui
        egui::TopBottomPanel::top("Debug")
            .show(&ctx.clone(), |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New").clicked() {
                            log::info!("New project...");
                        }
                        if ui.button("Open").clicked() {
                            log::info!("Opening a file...");
                        }
                        if ui.button("Save").clicked() {
                            log::info!("Saving a file...");
                            *should_save = true;
                        }
                        if ui.button("Close").clicked() {
                            log::info!("Close scene...");
                        }
                    });

                    ui.menu_button("Debug Options", |ui| {
                        if ui.button("Toggle Profiling").clicked() {
                            log::info!("I still don't know why this breaks.");
                            comp.show_profiler = !comp.show_profiler;
                        }
                        if ui.button("Toggle wireframe").clicked() {
                            log::info!("Toggling wireframe");
                            comp.terrain_wireframe = !comp.terrain_wireframe;
                        }
                    });
                });
            }); // end of panel
    }
}

pub fn CameraUiSystem(
    mut query: Query<&mut CameraComponent>,
    egui_state: Res<EguiState>,
)
{
    log::debug!("Camera ui...");
    let ctx = egui_state.ctx.clone();
    for mut cam in query.iter_mut(){
        let mut fov = cam.fov;
        egui::Window::new("Camera Settings")
            .show(&ctx, |ui| {
                ui.label("FOV");
                ui.add(egui::Slider::new(&mut fov, 0.1..=3.0))
            });
        if cam.fov != fov {
            cam.fov = fov;
            cam.calculate_perspective();
        }
    }
}


pub fn TransformUiSystem(
    mut query: Query<&mut TransformComponent, With<TransformUiComponent>>,
    egui_state: Res<EguiState>,
){
    log::debug!("Transform ui....");
    let ctx = egui_state.ctx.clone();
    for _transform in query.iter_mut(){
        // let mut pos = transform.global_position();
        // let mut posx = pos[0];
        // let mut posy = pos[1];
        // let mut posz = pos[2];
        egui::Window::new("Transform")
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Position");
                    // ui.add(egui::DragValue::new(&mut posx).speed(0.1).clamp_range(-100.0..=100.0));
                    // ui.add(egui::DragValue::new(&mut posy).speed(0.1).clamp_range(-100.0..=100.0));
                    // ui.add(egui::DragValue::new(&mut posz).speed(0.1).clamp_range(-100.0..=100.0));
                });
            });
    }
}