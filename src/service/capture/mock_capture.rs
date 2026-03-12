use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

use super::traits::ImageCapture;

pub struct MockCapture {
    file_path: PathBuf,
}

impl MockCapture {
    pub fn new(file_path: PathBuf) -> Self {
        Self { file_path }
    }
}

impl ImageCapture for MockCapture {
    fn capture(&self) -> Result<Vec<u8>> {
        info!(path = %self.file_path.display(), "Mock capture: reading file");
        std::fs::read(&self.file_path)
            .context(format!("Failed to read mock image: {}", self.file_path.display()))
    }
}
