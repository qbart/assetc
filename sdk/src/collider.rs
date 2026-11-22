//! `.acollider.bin` compound colliders: a model's convex hulls; see
//! docs/spec/acollider.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::read::Bytes;

const FILE: &str = "acollider.bin";

/// One convex hull, in the model's space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hull {
    /// Its corners; every one is on the hull.
    pub points: Vec<[f32; 3]>,
    /// Triangles, indices into `points`, counter-clockwise seen from outside.
    pub faces: Vec<[u32; 3]>,
    /// The center of its volume (its center of mass at even density).
    pub center: [f32; 3],
    pub volume: f32,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}

/// A model's collider: convex hulls that together follow it, largest first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Collider {
    pub hulls: Vec<Hull>,
    /// The box around every hull.
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    /// The hulls' volumes added up.
    pub volume: f32,
    /// The model's volume as voxelized when the collider was made.
    pub model_volume: f32,
    /// Asset ID of the model's `.amesh.bin`.
    pub mesh: u64,
}

impl Collider {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ACOLL", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let (hull_count, point_count, face_count) = (
            desc.u32(0)? as usize,
            desc.u32(4)? as usize,
            desc.u32(8)? as usize,
        );
        let points: Vec<[f32; 3]> = require("POINTS")?
            .records_n(12, point_count)?
            .map(|r| r.f32s(0))
            .collect::<Result<_>>()?;
        let faces: Vec<[u32; 3]> = require("FACES")?
            .records_n(12, face_count)?
            .map(|r| Ok([r.u32(0)?, r.u32(4)?, r.u32(8)?]))
            .collect::<Result<_>>()?;
        let hulls = require("HULLS")?
            .records_n(64, hull_count)?
            .map(|r| {
                let range = |first: usize, count: usize, len: usize| {
                    first
                        .checked_add(count)
                        .filter(|&end| end <= len)
                        .map(|end| first..end)
                        .ok_or_else(|| r.err("a hull's range runs past its chunk"))
                };
                let points =
                    points[range(r.u32(0)? as usize, r.u32(4)? as usize, points.len())?].to_vec();
                let faces =
                    faces[range(r.u32(8)? as usize, r.u32(12)? as usize, faces.len())?].to_vec();
                if faces.iter().flatten().any(|&i| i as usize >= points.len()) {
                    return Err(r.err("a face uses a point its hull does not have"));
                }
                Ok(Hull {
                    points,
                    faces,
                    center: r.f32s(16)?,
                    volume: r.f32(28)?,
                    aabb_min: r.f32s(32)?,
                    aabb_max: r.f32s(44)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            hulls,
            aabb_min: desc.f32s(16)?,
            aabb_max: desc.f32s(28)?,
            volume: desc.f32(40)?,
            model_volume: desc.f32(44)?,
            mesh: desc.u64(48)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;

    /// A collider of one tetrahedron.
    fn sample(face_index: u32) -> Vec<u8> {
        let mut desc = Vec::new();
        for v in [1u32, 4, 4, 0] {
            desc.extend(v.to_le_bytes());
        }
        for v in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0, 0.1667, 0.5] {
            desc.extend(v.to_le_bytes());
        }
        desc.extend(7u64.to_le_bytes());
        desc.extend(0u64.to_le_bytes());
        let mut hulls = Vec::new();
        for v in [0u32, 4, 0, 4] {
            hulls.extend(v.to_le_bytes());
        }
        for v in [0.25f32, 0.25, 0.25, 0.1667, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0] {
            hulls.extend(v.to_le_bytes());
        }
        hulls.extend(0u64.to_le_bytes());
        let points: Vec<u8> = [
            0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0,
        ]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
        let faces: Vec<u8> = [0u32, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, face_index]
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        file(
            "ACOLL",
            1,
            &[
                ("DESC", &desc),
                ("HULLS", &hulls),
                ("POINTS", &points),
                ("FACES", &faces),
            ],
        )
    }

    #[test]
    fn decodes_hulls() {
        let c = Collider::decode(&sample(3)).unwrap();
        assert_eq!(c.hulls.len(), 1);
        assert_eq!(c.mesh, 7);
        assert_eq!(c.aabb_max, [1.0; 3]);
        let h = &c.hulls[0];
        assert_eq!(h.points[3], [0.0, 0.0, 1.0]);
        assert_eq!(h.faces[3], [1, 2, 3]);
        assert_eq!(h.center, [0.25; 3]);
    }

    #[test]
    fn rejects_bad_indices() {
        let e = Collider::decode(&sample(4)).unwrap_err().to_string();
        assert!(
            e.contains("a face uses a point its hull does not have"),
            "{e}"
        );
        assert!(Collider::decode(b"AMESH").is_err());
    }
}
