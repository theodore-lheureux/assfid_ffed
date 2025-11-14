//! RAW image data types

/// Represents decoded RAW image data
#[derive(Debug, Clone)]
pub struct RawImageData {
    /// Width of the image in pixels
    pub width: usize,
    /// Height of the image in pixels
    pub height: usize,
    /// Raw pixel data (single channel Bayer pattern)
    pub data: Vec<u16>,
    /// Actual bits per sample from the sensor (e.g., 12, 14, or 16)
    pub bits_per_sample: u32,
}
