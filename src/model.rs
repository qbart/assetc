//! glTF model inspection, see docs/features/models.md.

use std::path::Path;

use bevy::gltf::gltf::{self, Gltf, mesh::Mode};

/// Whether `file` is a model assetc reads (`*.gltf`, `*.glb`, any case).
pub fn is_model(file: &Path) -> bool {
    file.extension().is_some_and(|ext| {
        ["gltf", "glb"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

fn parse(data: &[u8]) -> Result<Gltf, String> {
    Gltf::from_slice(data).map_err(|e| format!("not a valid glTF file: {e}"))
}

/// Inspector rows for the glTF in `data`; see docs/features/models.md#inspector.
pub fn rows(data: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let gltf = parse(data)?;
    let asset = &gltf.as_json().asset;
    let mut rows = Vec::new();
    if let Some(generator) = &asset.generator {
        rows.push(("Generator", generator.clone()));
    }
    rows.push(("Version", asset.version.clone()));
    rows.push(("Scenes", gltf.scenes().len().to_string()));
    rows.push(("Nodes", gltf.nodes().len().to_string()));

    let (mut primitives, mut vertices, mut triangles) = (0, 0, 0);
    for primitive in gltf.meshes().flat_map(|m| m.primitives()) {
        primitives += 1;
        let count = primitive
            .get(&gltf::Semantic::Positions)
            .map_or(0, |a| a.count());
        vertices += count;
        let points = primitive.indices().map_or(count, |a| a.count());
        triangles += match primitive.mode() {
            Mode::Triangles => points / 3,
            Mode::TriangleStrip | Mode::TriangleFan => points.saturating_sub(2),
            _ => 0,
        };
    }
    rows.push((
        "Meshes",
        format!("{} ({primitives} primitives)", gltf.meshes().len()),
    ));
    rows.push(("Vertices", vertices.to_string()));
    rows.push(("Triangles", triangles.to_string()));
    rows.push(("Materials", gltf.materials().len().to_string()));
    rows.push(("Textures", gltf.textures().len().to_string()));
    rows.push(("Skins", gltf.skins().len().to_string()));
    rows.push(("Animations", gltf.animations().len().to_string()));
    Ok(rows)
}

/// Each animation's name (`Animation <n>` without one) and duration in seconds, in file order;
/// see docs/features/models.md#animations-tab.
pub fn animations(data: &[u8]) -> Result<Vec<(String, f32)>, String> {
    let gltf = parse(data)?;
    Ok(gltf
        .animations()
        .map(|animation| {
            let name = animation.name().map_or_else(
                || format!("Animation {}", animation.index() + 1),
                str::to_owned,
            );
            let duration = animation
                .channels()
                .filter_map(|c| c.sampler().input().max()?.as_array()?.first()?.as_f64())
                .fold(0.0, f64::max);
            (name, duration as f32)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOX: &[u8] = include_bytes!("../samples/Fox.glb");

    fn row<'a>(rows: &'a [(&str, String)], label: &str) -> &'a str {
        &rows.iter().find(|(l, _)| *l == label).unwrap().1
    }

    #[test]
    fn models_by_extension() {
        assert!(is_model(Path::new("a/Fox.GLB")));
        assert!(is_model(Path::new("scene.gltf")));
        assert!(!is_model(Path::new("mesh.obj")));
    }

    #[test]
    fn rows_count_the_document() {
        let rows = rows(FOX).unwrap();
        assert_eq!(row(&rows, "Version"), "2.0");
        assert_eq!(row(&rows, "Meshes"), "1 (1 primitives)");
        assert_eq!(row(&rows, "Skins"), "1");
        assert_eq!(row(&rows, "Animations"), "3");
        assert!(row(&rows, "Vertices").parse::<u32>().unwrap() > 0);
        assert!(row(&rows, "Triangles").parse::<u32>().unwrap() > 0);
    }

    #[test]
    fn json_without_buffers_is_enough() {
        let json = r#"{
            "asset": {"version": "2.0", "generator": "hand"},
            "buffers": [{"byteLength": 36, "uri": "missing.bin"}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                           "min": [0, 0, 0], "max": [1, 1, 0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"mesh": 0}, {"mesh": 0}],
            "scenes": [{"nodes": [0, 1]}]
        }"#;
        let rows = rows(json.as_bytes()).unwrap();
        assert_eq!(row(&rows, "Generator"), "hand");
        assert_eq!(row(&rows, "Nodes"), "2");
        // Per mesh, not per instance.
        assert_eq!(row(&rows, "Vertices"), "3");
        assert_eq!(row(&rows, "Triangles"), "1");
    }

    #[test]
    fn animations_have_names_and_durations() {
        let animations = animations(FOX).unwrap();
        let names: Vec<_> = animations.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Survey", "Walk", "Run"]);
        assert!(animations.iter().all(|(_, d)| *d > 0.0));
    }

    #[test]
    fn invalid_files_fail() {
        assert!(
            rows(b"nope")
                .unwrap_err()
                .starts_with("not a valid glTF file")
        );
    }
}
