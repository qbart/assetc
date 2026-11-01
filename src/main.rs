mod assets;
mod build;
mod chart;
mod color_picker;
mod config;
mod fileops;
mod font;
mod gradient;
mod hash;
mod icons;
mod inspect;
mod ktx2;
mod logo;
mod lut;
mod manifest;
mod material;
mod model;
mod noise;
mod plan;
mod preview3d;
mod texture;
mod theme;
mod tree;
mod ui;

use std::env;
use std::process::ExitCode;

use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};
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
    /// Compile input into output
    Build,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Some(Command::Init) => init(),
        Some(Command::Build) => build(),
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

fn build() -> Result<(), Box<dyn std::error::Error>> {
    let project = config::load(&env::current_dir()?)?;
    let (Some(input), Some(output)) = (project.input_dir(), project.output_dir()) else {
        return Err(format!("no {} found (run assetc init)", config::FILE_NAME).into());
    };
    let outcome = build::run(&input, &output, &project.config.textures)?;
    for built in &outcome.built {
        println!("{} -> {} ({})", built.source, built.output, built.detail);
    }
    for removed in &outcome.removed {
        println!("removed {removed}");
    }
    for failure in &outcome.failures {
        eprintln!("error: {failure}");
    }
    if !outcome.failures.is_empty() {
        return Err(outcome.summary().into());
    }
    println!("{} into {}", outcome.summary(), output.display());
    Ok(())
}

fn open() -> Result<(), Box<dyn std::error::Error>> {
    let project = config::load(&env::current_dir()?)?;
    let mut app = App::new();
    // Models load from the input directory; sources must exist before `AssetPlugin`.
    if let Some(input) = project.input_dir() {
        app.register_asset_source(
            preview3d::INPUT_SOURCE,
            AssetSourceBuilder::platform_default(&input.to_string_lossy(), None),
        );
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "assetc".into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    // The overlay camera is chosen explicitly: the 3D preview camera must not get egui.
    .insert_resource(EguiGlobalSettings {
        auto_create_primary_context: false,
        ..default()
    })
    .add_plugins((ui::UiPlugin, preview3d::Preview3dPlugin))
    .insert_resource(project)
    .add_systems(Startup, setup_camera)
    .run();
    Ok(())
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, PrimaryEguiContext));
}
