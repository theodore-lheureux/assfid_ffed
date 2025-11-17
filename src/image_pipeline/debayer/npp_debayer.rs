use cudarc::driver::safe::*;
use cudarc::nvrtc::Ptx;
use std::sync::Arc;

use super::types::RgbImageData;
use crate::image_pipeline::raw::types::RawImageData;

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
mod npp {
    include!(concat!(env!("OUT_DIR"), "/npp_bindings.rs"));
}

/// NPP + Custom CUDA Debayer Pipeline
pub struct NppDebayer {
    stream: Arc<CudaStream>,
    color_kernel: CudaFunction,
}

impl NppDebayer {
    /// Initialize CUDA context and load color pipeline kernel
    pub fn new() -> anyhow::Result<Self> {
        // Load color pipeline kernel
        let ptx = include_str!(concat!(env!("OUT_DIR"), "/color_pipeline.ptx"));
        let kernel_name = "apply_color_pipeline";

        let ctx = CudaContext::new(0)?;
        let stream = ctx.default_stream();
        let module = ctx.load_module(Ptx::from_src(ptx))?;
        let color_kernel = module.load_function(kernel_name)?;

        Ok(Self { stream, color_kernel })
    }

    /// Process RAW image using NPP debayer + custom color pipeline
    pub fn process(&self, raw_image: &RawImageData) -> anyhow::Result<RgbImageData> {
        let width = raw_image.width;
        let height = raw_image.height;
        
        // Copy RAW Bayer data to GPU
        let d_bayer = self.stream.clone_htod(&raw_image.data)?;

        // Allocate output for NPP debayer (RGB u16)
        let num_pixels = width * height;
        let mut d_rgb_debayered = self.stream.alloc_zeros::<u16>(num_pixels * 3)?;

        // ---- Stage 1: NPP Debayering ----
        let src_size = npp::NppiSize { 
            width: width as i32, 
            height: height as i32 
        };
        
        let src_roi = npp::NppiRect {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        };
        
        let src_step = (width * std::mem::size_of::<u16>()) as i32;
        let dst_step = (width * 3 * std::mem::size_of::<u16>()) as i32;
        
        unsafe {
            let (src_ptr, _src_guard) = d_bayer.device_ptr(&self.stream);
            let (dst_ptr, _dst_guard) = d_rgb_debayered.device_ptr_mut(&self.stream);
            
            let status = npp::nppiCFAToRGB_16u_C1C3R(
                src_ptr as *const npp::Npp16u,
                src_step,
                src_size,
                src_roi,
                dst_ptr as *mut npp::Npp16u,
                dst_step,
                npp::NppiBayerGridPosition_NPPI_BAYER_RGGB,
                npp::NppiInterpolationMode_NPPI_INTER_UNDEFINED,
            );
            
            if status != 0 {
                anyhow::bail!("NPP debayer failed with status {}", status);
            }
        }

        // ---- Stage 2: Custom Color Pipeline ----
        
        // Allocate output for color pipeline (RGB f32)
        let mut d_rgb_final = self.stream.alloc_zeros::<f32>(num_pixels * 3)?;

        // Prepare white balance multipliers (normalize by green)
        let wb_r = raw_image.wb_coeffs[0] / raw_image.wb_coeffs[1];
        let wb_g = 1.0f32;
        let wb_b = raw_image.wb_coeffs[2] / raw_image.wb_coeffs[1];
        
        // Black and white levels
        let black_level = raw_image.blacklevels[0] as i32;
        let white_level = raw_image.whitelevels[0] as i32;

        // Flatten camera-to-XYZ matrix
        let cam_to_xyz_flat: Vec<f32> = raw_image.cam_to_xyz
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect();
        let mut d_cam_to_xyz = self.stream.clone_htod(&cam_to_xyz_flat)?;

        // Launch color pipeline kernel
        let width_i32 = width as i32;
        let height_i32 = height as i32;
        
        let mut launch_args = self.stream.launch_builder(&self.color_kernel);
        launch_args.arg(&mut d_rgb_debayered);
        launch_args.arg(&mut d_rgb_final);
        launch_args.arg(&width_i32);
        launch_args.arg(&height_i32);
        launch_args.arg(&wb_r);
        launch_args.arg(&wb_g);
        launch_args.arg(&wb_b);
        launch_args.arg(&black_level);
        launch_args.arg(&white_level);
        launch_args.arg(&mut d_cam_to_xyz);

        let threads = (32, 32, 1);
        let blocks = (
            ((width + 32 - 1) / 32),
            ((height + 32 - 1) / 32),
            1,
        );
        let cfg = LaunchConfig {
            grid_dim: (blocks.0 as u32, blocks.1 as u32, blocks.2 as u32),
            block_dim: threads,
            shared_mem_bytes: 0,
        };

        unsafe { launch_args.launch(cfg)? };

        // Copy back from GPU
        let rgb_data_f32 = self.stream.clone_dtoh(&d_rgb_final)?;

        // Convert f32 RGB to u16 for TIFF output (scaling 0..1 → 0..65535)
        let rgb_data_u16: Vec<u16> = rgb_data_f32
            .iter()
            .map(|&v| {
                let v = v.clamp(0.0, 1.0);
                (v * 65535.0) as u16
            })
            .collect();

        Ok(RgbImageData {
            width,
            height,
            data: rgb_data_u16,
            bits_per_sample: 16,
        })
    }
}
