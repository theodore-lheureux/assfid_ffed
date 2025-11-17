use cudarc::driver::safe::*;
use cudarc::nvrtc::Ptx;
use std::sync::Arc;

use super::types::RgbImageData;
use crate::image_pipeline::raw::types::RawImageData;

/// CUDA Debayer + White Balance + Camera→XYZ
pub struct CudaDebayer {
    stream: Arc<CudaStream>,
    kernel: CudaFunction,
}

impl CudaDebayer {
    /// Initialize CUDA context and load kernel
    pub fn new() -> anyhow::Result<Self> {
        // Include compiled PTX from build.rs
        let ptx = include_str!(concat!(env!("OUT_DIR"), "/debayer_rggb_bilinear.ptx"));
        let kernel_name = "debayer16_to_xyz";

        let ctx = CudaContext::new(0)?;
        let stream = ctx.default_stream();
        let module = ctx.load_module(Ptx::from_src(ptx))?;
        let kernel = module.load_function(kernel_name)?;

        Ok(Self { stream, kernel })
    }

    /// Process RAW image into linear XYZ
    pub fn process(&self, raw_image: &RawImageData) -> anyhow::Result<RgbImageData> {
        // Copy RAW Bayer data to GPU
        let mut d_bayer = self.stream.clone_htod(&raw_image.data)?;

        // Allocate output on GPU (3 floats per pixel)
        let num_pixels = raw_image.width * raw_image.height;
        let mut d_xyz = self.stream.alloc_zeros::<f32>(num_pixels * 3)?;

        // Prepare white balance multipliers (normalize by green)
        let wb_r = raw_image.wb_coeffs[0] / raw_image.wb_coeffs[1];
        let wb_g = 1.0f32;
        let wb_b = raw_image.wb_coeffs[2] / raw_image.wb_coeffs[1];
        
        // Black and white levels (use first channel, assuming they're the same for RGGB)
        let black_level = raw_image.blacklevels[0] as i32;
        let white_level = raw_image.whitelevels[0] as i32;

        // Flatten camera-to-XYZ matrix (3x4) to 1D array for GPU
        let cam_to_xyz_flat: Vec<f32> = raw_image.cam_to_xyz
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect();
        let mut d_cam_to_xyz = self.stream.clone_htod(&cam_to_xyz_flat)?;

        // Kernel arguments
        let width = raw_image.width as i32;
        let height = raw_image.height as i32;
        let mut launch_args = self.stream.launch_builder(&self.kernel);
        launch_args.arg(&mut d_bayer);
        launch_args.arg(&mut d_xyz);
        launch_args.arg(&width);
        launch_args.arg(&height);
        launch_args.arg(&wb_r);
        launch_args.arg(&wb_g);
        launch_args.arg(&wb_b);
        launch_args.arg(&black_level);
        launch_args.arg(&white_level);
        launch_args.arg(&mut d_cam_to_xyz);

        // Configure threads and blocks
        let threads = (32, 32, 1);
        let blocks = (
            ((raw_image.width + 32 - 1) / 32),
            ((raw_image.height + 32 - 1) / 32),
            1,
        );
        let cfg = LaunchConfig {
            grid_dim: (blocks.0 as u32, blocks.1 as u32, blocks.2 as u32),
            block_dim: threads,
            shared_mem_bytes: 0,
        };

        // Launch kernel
        unsafe { launch_args.launch(cfg)? };

        // Copy back from GPU
        let xyz_data_f32 = self.stream.clone_dtoh(&d_xyz)?;

        // Convert f32 RGB to u16 for TIFF output (scaling 0..1 → 0..65535)
        let rgb_data_u16: Vec<u16> = xyz_data_f32
            .iter()
            .map(|&v| {
                let v = v.clamp(0.0, 1.0);
                (v * 65535.0) as u16
            })
            .collect();

        Ok(RgbImageData {
            width: raw_image.width,
            height: raw_image.height,
            data: rgb_data_u16,
            bits_per_sample: 16,
        })
    }
}
