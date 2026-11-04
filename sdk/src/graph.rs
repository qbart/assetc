//! Placeholder, being written.

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Graph;

impl Graph {
    pub fn decode(_bytes: &[u8]) -> Result<Self> {
        Err(Error::NotFound("decoder".into()))
    }
}
