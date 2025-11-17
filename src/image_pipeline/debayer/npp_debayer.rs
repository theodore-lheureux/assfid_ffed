use cudarc::driver::safe::*;
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

/// NPP-Only Debayer Pipeline
///
/// This implementation uses NVIDIA Performance Primitives (NPP) for the entire
/// image processing pipeline, replacing the previous custom CUDA color correction kernel.
///
/// Pipeline stages:
/// 1. **Debayering**: `nppiCFAToRGB_16u_C1C3R` - Converts Bayer pattern to RGB
/// 2. **Type conversion**: `nppiConvert_16u32f_C3R` - Converts u16 to f32 for processing
/// 3. **Black level subtraction**: `nppiSubC_32f_C3IR` - Removes sensor black level
/// 4. **Normalization + White balance**: `nppiMulC_32f_C3IR` - Scales to 0..1 and applies WB
/// 5. **Color matrix transform**: `nppiColorTwist_32f_C3R` - Applies camera→XYZ→sRGB transform
///
/// Benefits over custom kernel:
/// - Leverages highly optimized NPP library functions
/// - Easier to maintain (no custom CUDA code for color correction)
/// - Portable across NVIDIA GPU architectures
/// - Well-tested and validated by NVIDIA
///
/// Note: The ColorTwist matrix parameter must remain in **host memory** (not device memory)
/// as NPP reads it directly during the kernel launch.
pub struct NppDebayer {
    stream: Arc<CudaStream>,
}

impl NppDebayer {
    /// Initialize CUDA context
    pub fn new() -> anyhow::Result<Self> {
        let ctx = CudaContext::new(0)?;
        let stream = ctx.default_stream();

        Ok(Self { stream })
    }

    /// Process RAW image using NPP debayer + NPP color pipeline
    pub fn process(&self, raw_image: &RawImageData) -> anyhow::Result<RgbImageData> {
        let width = raw_image.width;
        let height = raw_image.height;
        
        // Copy RAW Bayer data to GPU
        let d_bayer = self.stream.clone_htod(&raw_image.data)?;

        // Allocate output for NPP debayer (RGB u16)
        let num_pixels = width * height;
        let mut d_rgb_u16 = self.stream.alloc_zeros::<u16>(num_pixels * 3)?;

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
            let (dst_ptr, _dst_guard) = d_rgb_u16.device_ptr_mut(&self.stream);
            
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

        // ---- Stage 2: NPP Color Pipeline ----
        
        // Allocate f32 workspace for color corrections
        let mut d_rgb_f32 = self.stream.alloc_zeros::<f32>(num_pixels * 3)?;

        // Step 2.1: Convert u16 → f32
        let roi_size = npp::NppiSize {
            width: width as i32,
            height: height as i32,
        };
        
        unsafe {
            let (src_ptr, _src_guard) = d_rgb_u16.device_ptr(&self.stream);
            let (dst_ptr, _dst_guard) = d_rgb_f32.device_ptr_mut(&self.stream);
            
            let status = npp::nppiConvert_16u32f_C3R(
                src_ptr as *const npp::Npp16u,
                dst_step,
                dst_ptr as *mut npp::Npp32f,
                (width * 3 * std::mem::size_of::<f32>()) as i32,
                roi_size,
            );
            
            if status != 0 {
                anyhow::bail!("NPP Convert 16u→32f failed with status {}", status);
            }
        }

        // Step 2.2: Subtract black level from each channel
        let black_level = raw_image.blacklevels[0] as f32;
        let black_levels = [black_level, black_level, black_level];
        
        unsafe {
            let (ptr, _guard) = d_rgb_f32.device_ptr_mut(&self.stream);
            
            let status = npp::nppiSubC_32f_C3IR(
                black_levels.as_ptr(),
                ptr as *mut npp::Npp32f,
                (width * 3 * std::mem::size_of::<f32>()) as i32,
                roi_size,
            );
            
            if status != 0 {
                anyhow::bail!("NPP SubC (black level) failed with status {}", status);
            }
        }

        // Step 2.3: Normalize by (white - black) and apply white balance
        let white_level = raw_image.whitelevels[0] as f32;
        let range = white_level - black_level;
        
        // Combine normalization with white balance: (1/range) * wb_coeff
        let wb_r = (raw_image.wb_coeffs[0] / raw_image.wb_coeffs[1]) / range;
        let wb_g = 1.0f32 / range;
        let wb_b = (raw_image.wb_coeffs[2] / raw_image.wb_coeffs[1]) / range;
        let wb_multipliers = [wb_r, wb_g, wb_b];
        
        unsafe {
            let (ptr, _guard) = d_rgb_f32.device_ptr_mut(&self.stream);
            
            let status = npp::nppiMulC_32f_C3IR(
                wb_multipliers.as_ptr(),
                ptr as *mut npp::Npp32f,
                (width * 3 * std::mem::size_of::<f32>()) as i32,
                roi_size,
            );
            
            if status != 0 {
                anyhow::bail!("NPP MulC (normalize + white balance) failed with status {}", status);
            }
        }

        // Step 2.4: Apply camera-to-XYZ → XYZ-to-sRGB color matrix transformation
        // Combine both matrices: sRGB_from_XYZ * cam_to_XYZ
        // Standard XYZ to sRGB D65 illuminant matrix:
        const XYZ_TO_SRGB: [[f32; 3]; 3] = [
            [ 3.2404542, -1.5371385, -0.4985314],
            [-0.9692660,  1.8760108,  0.0415560],
            [ 0.0556434, -0.2040259,  1.0572252],
        ];
        
        // Multiply: combined = XYZ_TO_SRGB * cam_to_xyz (3×3 result from 3×3 × 3×4)
        // We also compute the offset (4th column) from cam_to_xyz
        let mut combined = [[0.0f32; 4]; 3];
        for i in 0..3 {
            for j in 0..3 {
                combined[i][j] = XYZ_TO_SRGB[i][0] * raw_image.cam_to_xyz[0][j]
                               + XYZ_TO_SRGB[i][1] * raw_image.cam_to_xyz[1][j]
                               + XYZ_TO_SRGB[i][2] * raw_image.cam_to_xyz[2][j];
            }
            // Offset column (4th): apply XYZ_TO_SRGB to cam_to_xyz offset
            combined[i][3] = XYZ_TO_SRGB[i][0] * raw_image.cam_to_xyz[0][3]
                           + XYZ_TO_SRGB[i][1] * raw_image.cam_to_xyz[1][3]
                           + XYZ_TO_SRGB[i][2] * raw_image.cam_to_xyz[2][3];
        }
        
        // Apply exposure scaling to entire matrix (including offset)
        const EXPOSURE: f32 = 3.5;
        for i in 0..3 {
            for j in 0..4 {
                combined[i][j] *= EXPOSURE;
            }
        }
        
        // NPP ColorTwist uses a 3×4 matrix in row-major order:
        // [m00 m01 m02 m03]  where the 4th column is constant offset per channel
        // [m10 m11 m12 m13]
        // [m20 m21 m22 m23]
        // NOTE: The aTwist parameter expects HOST memory, not device memory!
        let twist_matrix: [[f32; 4]; 3] = combined;
        
        // Allocate output for color twist
        let mut d_rgb_twisted = self.stream.alloc_zeros::<f32>(num_pixels * 3)?;
        
        unsafe {
            let (src_ptr, _src_guard) = d_rgb_f32.device_ptr(&self.stream);
            let (dst_ptr, _dst_guard) = d_rgb_twisted.device_ptr_mut(&self.stream);
            
            let step = (width * 3 * std::mem::size_of::<f32>()) as i32;
            
            // Pass the host memory pointer directly
            let status = npp::nppiColorTwist_32f_C3R(
                src_ptr as *const npp::Npp32f,
                step,
                dst_ptr as *mut npp::Npp32f,
                step,
                roi_size,
                twist_matrix.as_ptr(),
            );
            
            if status != 0 {
                anyhow::bail!("NPP ColorTwist (color matrix) failed with status {}", status);
            }
        }

        // Copy back from GPU and convert to u16 (0..1 → 0..65535)
        let rgb_data_f32 = self.stream.clone_dtoh(&d_rgb_twisted)?;

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
