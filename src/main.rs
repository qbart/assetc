mod config;
mod theme;
mod ui;

use std::env;
use std::process::ExitCode;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create assetc.toml in the current directory
    Init,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Some(Command::Init) => init(),
        None => open(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let path = config::init(&env::current_dir()?)?;
    println!("Created {}", path.display());
    Ok(())
}

fn open() -> Result<(), Box<dyn std::error::Error>> {
    let project = config::load(&env::current_dir()?)?;
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "assetc".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(ui::UiPlugin)
        .insert_resource(project)
        .add_systems(Startup, setup_camera)
        .run();
    Ok(())
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
