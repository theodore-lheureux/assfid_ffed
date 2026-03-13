use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
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

    fn capture_dir(&self) -> Result<PathBuf> {
        if is_writable_dir(&self.output_dir)? {
            return Ok(self.output_dir.clone());
        }

        let fallback_dir = std::env::temp_dir().join("ffed_captures");
        fs::create_dir_all(&fallback_dir)?;
        if !is_writable_dir(&fallback_dir)? {
            anyhow::bail!(
                "Neither configured output dir {} nor fallback dir {} is writable",
                self.output_dir.display(),
                fallback_dir.display()
            );
        }

        info!(
            configured = %self.output_dir.display(),
            fallback = %fallback_dir.display(),
            "Configured capture directory is not writable, using fallback"
        );

        Ok(fallback_dir)
    }
}

impl ImageCapture for Gphoto2Capture {
    fn capture(&self) -> Result<Vec<u8>> {
        let capture_dir = self.capture_dir()?;

        let filename = format!(
              "capture_{}_{}.arw",
            chrono::Utc::now().format("%Y%m%d_%H%M%S"),
            std::process::id()
        );
        let output_path = capture_dir.join(&filename);

        info!(path = %output_path.display(), "Capturing image via gphoto2");

        let output = Command::new("gphoto2")
            .args(["--capture-image-and-download", "--force-overwrite", "--filename"])
            .arg(&output_path)
            .output()
            .context("Failed to run gphoto2 — is it installed?")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        if !output.status.success() {
            anyhow::bail!("gphoto2 capture failed: {}", combined.trim());
        }

        if combined.contains("Could not detect any camera")
            || combined.contains("*** Error")
            || combined.contains("Unknown model")
        {
            anyhow::bail!("gphoto2 reported camera error: {}", combined.trim());
        }

        info!("Captured: {}", filename);
        if output_path.exists() {
            return fs::read(&output_path)
                .with_context(|| format!("Failed to read captured image at {}", output_path.display()));
        }

        anyhow::bail!(
            "gphoto2 finished but expected image file was not found at {}. stdout/stderr: {}",
            output_path.display(),
            combined.trim()
        )
    }
}

fn is_writable_dir(dir: &Path) -> Result<bool> {
    fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".ffed-write-test-{}", std::process::id()));
    match fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
