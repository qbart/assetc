//! Placeholder, being written.

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Mesh;

impl Mesh {
    pub fn decode(_bytes: &[u8]) -> Result<Self> {
        Err(Error::NotFound("decoder".into()))
    }
}
