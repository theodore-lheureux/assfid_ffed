//! Debayering module for converting Bayer pattern RAW images to RGB


#[cfg(jetson_cuda)]
pub mod cuda_debayer;
#[cfg(jetson_cuda)]
pub mod npp_debayer;
pub mod cpu_debayer;
pub mod types;

// Fallback CPU implementations when NOT on Jetson
#[cfg(not(jetson_cuda))]
pub struct CudaDebayer;

#[cfg(not(jetson_cuda))]
impl CudaDebayer {
    pub fn new() -> anyhow::Result<Self> { Ok(Self) }
    #[allow(unused)]
    pub fn process(&self, raw_image: &RawImageData) -> anyhow::Result<RgbImageData> {
        panic!("CUDA debayer is not available on this platform.");
    }
}

#[cfg(not(jetson_cuda))]
pub struct NppDebayer;

#[cfg(not(jetson_cuda))]
impl NppDebayer {
    pub fn new() -> anyhow::Result<Self> { Ok(Self) }
    #[allow(unused)]
    pub fn process(&self, raw_image: &RawImageData) -> anyhow::Result<RgbImageData> {
        panic!("NPP debayer is not available on this platform.");
    }
}

#[cfg(jetson_cuda)]
pub use cuda_debayer::CudaDebayer;
#[cfg(jetson_cuda)]
pub use npp_debayer::NppDebayer;
pub use cpu_debayer::CpuDebayer;
pub use types::RgbImageData;

#[cfg(not(jetson_cuda))]
use crate::image_pipeline::RawImageData;
