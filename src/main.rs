use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "assetc".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup_camera)
        .add_systems(EguiPrimaryContextPass, ui)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn ui(mut contexts: EguiContexts) -> Result {
    egui::Window::new("assetc").show(contexts.ctx_mut()?, |ui| {
        ui.label("Asset Compiler");
    });
    Ok(())
}
