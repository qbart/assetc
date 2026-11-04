mod aanim;
mod adata;
mod agraph;
mod amesh;
mod amod;
mod aschema;
mod assets;
mod atlas;
mod atree;
mod build;
mod chart;
mod chunks;
mod color_picker;
mod compiled;
mod config;
mod enrich;
mod fileops;
mod font;
mod gizmo;
mod gradient;
mod hash;
mod hdr;
mod hull;
mod icons;
mod inspect;
mod ktx2;
mod licenses;
mod logo;
mod lut;
mod manifest;
mod material;
mod model;
mod noise;
mod pack;
mod palette;
mod path;
mod plan;
mod presets;
mod preview3d;
mod structs;
mod texture;
mod theme;
mod tree;
mod ui;
mod vertex_colors;
mod vision;

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

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
    Build {
        /// Build this target of assetc.toml instead of the default
        #[arg(long)]
        target: Option<String>,
    },
    /// Build, then bundle the output into one .pak file
    Pack {
        /// Pack this target of assetc.toml instead of the default
        #[arg(long)]
        target: Option<String>,
    },
    /// Build, then rebuild what changes until stopped
    Watch {
        /// Watch this target of assetc.toml instead of the default
        #[arg(long)]
        target: Option<String>,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Some(Command::Init) => init(),
        Some(Command::Build { target }) => build(target.as_deref()),
        Some(Command::Pack { target }) => pack(target.as_deref()),
        Some(Command::Watch { target }) => watch(target.as_deref()),
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

/// The project with `target` applied, and its input and output directories.
fn load_target(
    target: Option<&str>,
) -> Result<(config::Project, PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let mut project = config::load(&env::current_dir()?)?;
    let missing = || format!("no {} found (run assetc init)", config::FILE_NAME);
    if let Some(name) = target {
        if project.root.is_none() {
            return Err(missing().into());
        }
        project.config = project.config.target(name)?;
    }
    let (Some(input), Some(output)) = (project.input_dir(), project.output_dir()) else {
        return Err(missing().into());
    };
    Ok((project, input, output))
}

/// Prints an outcome as `assetc build` does; false when it failed.
fn report(outcome: &build::Outcome) -> bool {
    for built in &outcome.built {
        println!("{} -> {} ({})", built.source, built.output, built.detail);
    }
    for removed in &outcome.removed {
        println!("removed {removed}");
    }
    for failure in &outcome.failures {
        eprintln!("error: {failure}");
    }
    outcome.failures.is_empty()
}

/// Builds the default target, or `target`; see docs/features/cli.md#assetc-build.
fn build(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let (project, input, output) = load_target(target)?;
    let outcome = build::run(&input, &output, &project.config)?;
    if !report(&outcome) {
        return Err(outcome.summary().into());
    }
    println!("{} into {}", outcome.summary(), output.display());
    Ok(())
}

/// Builds, then packs the output; see docs/features/pack.md#cli.
fn pack(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let (project, _, output) = load_target(target)?;
    let settings = pack::Settings::new(&project.config.pack)?;
    build(target)?;
    let (file, contents) = pack::write(&output, &settings)?;
    println!("{}", pack::summary(&contents, &file.display().to_string()));
    for line in pack::stats(&contents) {
        println!("{line}");
    }
    Ok(())
}

/// Builds, then rebuilds incrementally on every change; see docs/features/cli.md#assetc-watch.
fn watch(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    const TICK: Duration = Duration::from_millis(500);
    let (mut project, mut input, mut output) = load_target(target)?;
    let config_file = project
        .root
        .as_ref()
        .map(|root| root.join(config::FILE_NAME))
        .unwrap_or_default();
    println!(
        "Watching {} into {} (Ctrl+C to stop)",
        input.display(),
        output.display()
    );
    let mut cache = build::Cache::default();
    let mut seen = listing(&input, &output, &config_file);
    loop {
        match build::run_cached(&input, &output, &project.config, &mut cache) {
            Ok(outcome) => {
                if report(&outcome) {
                    println!("{} into {}", outcome.summary(), output.display());
                } else {
                    eprintln!("{}", outcome.summary());
                }
            }
            Err(e) => eprintln!("error: {e}"),
        }
        // Wait for a change and for the listing to settle; a bad config waits for the next.
        loop {
            let mut now = listing(&input, &output, &config_file);
            while now == seen {
                std::thread::sleep(TICK);
                now = listing(&input, &output, &config_file);
            }
            loop {
                std::thread::sleep(TICK);
                let again = listing(&input, &output, &config_file);
                if again == now {
                    break;
                }
                now = again;
            }
            let config_changed = now.get(&config_file) != seen.get(&config_file);
            seen = now;
            if !config_changed {
                break;
            }
            match load_target(target) {
                Ok(loaded) => {
                    (project, input, output) = loaded;
                    seen = listing(&input, &output, &config_file);
                    cache = build::Cache::default();
                    break;
                }
                Err(e) => eprintln!("error: {e}"),
            }
        }
        println!();
    }
}

/// Size and modification time of every input file and of `config_file`, the output
/// directory left out.
fn listing(
    input: &Path,
    output: &Path,
    config_file: &Path,
) -> BTreeMap<PathBuf, (u64, SystemTime)> {
    let stat = |file: &Path| {
        let meta = std::fs::metadata(file).ok()?;
        Some((meta.len(), meta.modified().ok()?))
    };
    let mut files: BTreeMap<_, _> = build::files_under(input)
        .into_iter()
        .filter(|file| !file.starts_with(output))
        .filter_map(|file| Some((stat(&file)?, file)))
        .map(|(stat, file)| (file, stat))
        .collect();
    if let Some(s) = stat(config_file) {
        files.insert(config_file.to_path_buf(), s);
    }
    files
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
    // Opens maximized; see docs/features/gui.md.
    let mut window = Window {
        title: "assetc".into(),
        ..default()
    };
    window.set_maximized(true);
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window),
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
