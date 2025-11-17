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
    /// White balance coefficients [R, G, B, E] from camera
    /// These are used to correct color casts in the raw sensor data
    pub wb_coeffs: [f32; 4],
    /// Black levels [R, G, B, E] - sensor baseline that should be subtracted
    pub blacklevels: [u16; 4],
    /// White levels [R, G, B, E] - maximum sensor values
    pub whitelevels: [u16; 4],
    /// Camera to XYZ color conversion matrix (normalized, 3x4, row-major)
    /// This is the inverse of xyz_to_cam, already computed and normalized
    pub cam_to_xyz: [[f32; 4]; 3],
    /// XYZ to Camera color conversion matrix (raw, 4x3, row-major)
    /// Used for debayering and color correction
    pub xyz_to_cam: [[f32; 3]; 4],
}
