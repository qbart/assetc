//! Placeholder, being written.

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Data;

impl Data {
    pub fn decode(_bytes: &[u8]) -> Result<Self> {
        Err(Error::NotFound("decoder".into()))
    }
}
