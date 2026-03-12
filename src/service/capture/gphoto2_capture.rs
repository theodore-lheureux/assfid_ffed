use anyhow::{Context, Result};
use std::process::Command;
use tracing::info;

use super::traits::ImageCapture;

pub struct Gphoto2Capture {
    output_dir: std::path::PathBuf,
}

impl Gphoto2Capture {
    pub fn new(output_dir: std::path::PathBuf) -> Self {
        Self { output_dir }
    }
}

impl ImageCapture for Gphoto2Capture {
    fn capture(&self) -> Result<Vec<u8>> {
        std::fs::create_dir_all(&self.output_dir)?;

        let filename = format!(
            "capture_{}.arw",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        let output_path = self.output_dir.join(&filename);

        info!(path = %output_path.display(), "Capturing image via gphoto2");

        let output = Command::new("gphoto2")
            .args(["--capture-image-and-download", "--filename"])
            .arg(&output_path)
            .output()
            .context("Failed to run gphoto2 — is it installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gphoto2 capture failed: {}", stderr);
        }

        info!("Captured: {}", filename);
        std::fs::read(&output_path).context("Failed to read captured image")
    }
}
