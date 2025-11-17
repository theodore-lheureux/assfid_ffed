//! Types for debayering operations

/// RGB image data after debayering
#[derive(Debug, Clone)]
pub struct RgbImageData {
    /// Width of the image in pixels
    pub width: usize,
    /// Height of the image in pixels
    pub height: usize,
    /// RGB pixel data interleaved [R, G, B, R, G, B, ...]
    pub data: Vec<u16>,
    /// Actual bits per sample from the sensor (e.g., 12, 14, or 16)
    pub bits_per_sample: u32,
}
