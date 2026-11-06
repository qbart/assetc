//! `.acmat.bin` custom materials: a shader's values (GPU block bytes, texture IDs, params);
//! see docs/spec/acmat.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::data::Value;
use crate::read::Bytes;
use crate::shader::{Shader, State};
use crate::structs::{Attached, Scope, Structs};

const FILE: &str = "acmat.bin";

/// A material of a custom [`Shader`]; the block's layout and the textures' kinds are the
/// shader's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CustomMaterial {
    /// Asset ID of the shader's `.ashader.bin`.
    pub shader: u64,
    /// The shader's state with the material's overrides applied.
    pub state: State,
    /// The GPU block: [`Shader::block_size`] bytes, std140, to upload as the `Params` uniform
    /// buffer as is; empty without a block.
    pub params: Vec<u8>,
    /// Texture asset IDs in the shader's texture order; `0` is empty (bind the texture's
    /// [`default`](crate::shader::ShaderTexture::default)).
    pub textures: Vec<u64>,
    /// [`Scope::Params`] the params (item `0`, when the shader has params),
    /// [`Scope::Asset`] the material's attached structs.
    pub structs: Structs,
}

impl CustomMaterial {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ACMAT", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let block_size = desc.u32(16)? as usize;
        let texture_count = desc.u32(20)? as usize;
        let params = require("PARAMS")?;
        if params.bytes.len() != block_size {
            return Err(params.err(format!(
                "PARAMS is {} bytes, not the block's {block_size}",
                params.bytes.len()
            )));
        }
        let ids = require("TEXTURES")?;
        if Some(ids.bytes.len()) != texture_count.checked_mul(8) {
            return Err(ids.err(format!(
                "TEXTURES is {} bytes, not {texture_count} IDs",
                ids.bytes.len()
            )));
        }
        let textures = ids.records(8)?.map(|r| r.u64(0)).collect::<Result<_>>()?;
        file.require("STRUCTS", FILE)?;
        Ok(Self {
            shader: desc.u64(0)?,
            state: State::read(&desc, 8, 12)?,
            params: params.bytes.to_vec(),
            textures,
            structs: Structs::from_chunks(&file, FILE)?,
        })
    }

    /// The params struct; `None` when the shader has no params.
    pub fn params_struct(&self) -> Option<&Attached> {
        self.structs.of(Scope::Params, 0).next()
    }

    /// The params' field `name`, resolved.
    pub fn param(&self, name: &str) -> Option<&Value> {
        self.params_struct()?.get(name)
    }

    /// The asset ID of `shader`'s texture `name` (`0` when empty); `None` when the shader
    /// has no such texture.
    pub fn texture(&self, shader: &Shader, name: &str) -> Option<u64> {
        shader
            .texture_index(name)
            .and_then(|i| self.textures.get(i).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::shader::{Blend, Cull};

    /// A material of shader `0x5ade`: a `block`-byte `PARAMS` (the desc says 16), `ids` bytes
    /// of `TEXTURES` (the desc says 2 IDs, `[7, 0]`), and, with `structs`, the params `Step`
    /// (scope 3, item 0) beside the asset's `Meta`.
    fn sample(block: usize, ids: usize, structs: bool) -> Vec<u8> {
        let mut desc = 0x5adeu64.to_le_bytes().to_vec();
        desc.extend((1u32 | 1 << 3 | 1 << 5 | 1 << 6).to_le_bytes());
        desc.extend(0.75f32.to_le_bytes());
        desc.extend(16u32.to_le_bytes());
        desc.extend(2u32.to_le_bytes());
        desc.extend(0u64.to_le_bytes());
        let params: Vec<u8> = (0..block as u8).collect();
        let textures: Vec<u8> = [7u64, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .take(ids)
            .collect();
        let mut chunks = crate::structs::tests::chunks(3);
        // The params are item 0 of scope 3.
        chunks[0].1[40 + 4..40 + 8].copy_from_slice(&0u32.to_le_bytes());
        let mut all: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc),
            ("PARAMS", &params),
            ("TEXTURES", &textures),
        ];
        if structs {
            all.extend(chunks.iter().map(|(n, p)| (*n, &p[..])));
        }
        file("ACMAT", 1, &all)
    }

    #[test]
    fn decodes_values_and_params() {
        let m = CustomMaterial::decode(&sample(16, 16, true)).unwrap();
        assert_eq!(m.shader, 0x5ade);
        assert_eq!((m.state.blend, m.state.cull), (Blend::Mask, Cull::Front));
        assert!(m.state.depth_write);
        assert_eq!(m.state.alpha_cutoff, 0.75);
        assert_eq!(m.params, (0..16).collect::<Vec<u8>>());
        assert_eq!(m.textures, [7, 0]);
        assert_eq!(m.params_struct().unwrap().type_name, "Step");
        assert_eq!(m.param("speed"), Some(&Value::Float(2.5)));
        assert!(m.structs.get(Scope::Asset, 0, "Meta").is_some());

        let shader = Shader::decode(&crate::shader::tests::sample(&Default::default())).unwrap();
        assert_eq!(m.texture(&shader, "color"), Some(7));
        assert_eq!(m.texture(&shader, "mask"), Some(0));
        assert_eq!(m.texture(&shader, "nope"), None);
    }

    #[test]
    fn empty_struct_chunks_mean_no_params() {
        let mut desc = vec![0u8; 32];
        desc[8..12].copy_from_slice(&(1u32 << 5 | 1 << 6).to_le_bytes());
        let chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc),
            ("PARAMS", &[]),
            ("TEXTURES", &[]),
            ("STRUCTS", &[]),
            ("SVALUES", &[]),
            ("SREFS", &[]),
            ("SSTRINGS", &[]),
        ];
        let m = CustomMaterial::decode(&file("ACMAT", 1, &chunks)).unwrap();
        assert!(m.params.is_empty() && m.textures.is_empty() && m.structs.is_empty());
        assert!(m.params_struct().is_none() && m.param("speed").is_none());
    }

    #[test]
    fn rejects_bad_sizes() {
        assert!(CustomMaterial::decode(&sample(15, 16, true)).is_err());
        assert!(CustomMaterial::decode(&sample(16, 8, true)).is_err());
        assert!(CustomMaterial::decode(&sample(16, 16, false)).is_err());
    }
}
