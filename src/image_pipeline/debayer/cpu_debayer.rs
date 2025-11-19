use anyhow::Result;
use tracing::info;
use std::io::Cursor;
use bayer::{BayerDepth, CFA, Demosaic, RasterDepth, RasterMut};
use crate::image_pipeline::{RawImageData, debayer::RgbImageData};

pub struct CpuDebayer;

impl CpuDebayer {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn process(&self, raw_image: &RawImageData) -> Result<RgbImageData> {
        let width = raw_image.width;
        let height = raw_image.height;
        info!("Starting CPU debayering for image {}x{}", width, height);
        
        // Determine bit depth - bayer crate only supports 8 and 16 bit
        let (bayer_depth, raster_depth, bytes_per_pixel) = if raw_image.bits_per_sample <= 8 {
            (BayerDepth::Depth8, RasterDepth::Depth8, 1)
        } else {
            (BayerDepth::Depth16LE, RasterDepth::Depth16, 2)
        };
        
        // Convert u16 data to u8 bytes for bayer crate
        let bayer_bytes: Vec<u8> = if raw_image.bits_per_sample <= 8 {
            raw_image.data.iter().map(|&val| val as u8).collect()
        } else {
            raw_image.data.iter()
                .flat_map(|&val| val.to_le_bytes())
                .collect()
        };
        
        // Allocate output buffer for RGB data (matching input depth)
        let output_buf_size = width * height * 3 * bytes_per_pixel;
        let mut output_buf = vec![0u8; output_buf_size];
        
        // Create cursor for reading bytes
        let mut cursor = Cursor::new(&bayer_bytes[..]);
        
        info!("Running demosaic with depth={:?}, CFA=RGGB, algo=Linear", bayer_depth);
        info!("Input bytes: {}, Output buffer: {} ({}x{}x3x{})", 
              bayer_bytes.len(), output_buf_size, width, height, bytes_per_pixel);
        
        // Create output raster
        let mut output_raster = RasterMut::new(
            width,
            height,
            raster_depth,
            &mut output_buf
        );
        
        // Run demosaicing - assuming RGGB pattern
        bayer::run_demosaic(
            &mut cursor,
            bayer_depth,
            CFA::RGGB,
            Demosaic::Linear,
            &mut output_raster
        ).map_err(|e| anyhow::anyhow!("Demosaic failed: {:?}", e))?;
        
        // Convert output buffer to u16 RGB data with simple color correction (Black Level + WB)
        // This fixes the "too green" and "too dark" issues.
        
        // Full Color Pipeline: Black Level -> WB -> Color Matrix (Cam->XYZ->sRGB)
        
        // 1. Setup Color Matrix
        // Standard XYZ to sRGB D65 illuminant matrix
        const XYZ_TO_SRGB: [[f32; 3]; 3] = [
            [ 3.2404542, -1.5371385, -0.4985314],
            [-0.9692660,  1.8760108,  0.0415560],
            [ 0.0556434, -0.2040259,  1.0572252],
        ];

        // Compute combined matrix: Cam -> XYZ -> sRGB
        // cam_to_xyz is 3x4 (includes offset in col 3)
        let mut cam_to_srgb = [[0.0f32; 4]; 3];
        for r in 0..3 {
            for c in 0..4 {
                let mut sum = 0.0;
                for k in 0..3 {
                    sum += XYZ_TO_SRGB[r][k] * raw_image.cam_to_xyz[k][c];
                }
                cam_to_srgb[r][c] = sum;
            }
        }

        // Exposure compensation (matching NPP implementation)
        const EXPOSURE: f32 = 3.5;
        for r in 0..3 {
            for c in 0..4 {
                cam_to_srgb[r][c] *= EXPOSURE;
            }
        }

        // 2. Setup Levels & WB
        let black_level = raw_image.blacklevels[0] as f32;
        let white_level = raw_image.whitelevels[0] as f32;
        let range = (white_level - black_level).max(1.0);
        
        let wb_r = raw_image.wb_coeffs[0] / raw_image.wb_coeffs[1];
        let wb_g = 1.0;
        let wb_b = raw_image.wb_coeffs[2] / raw_image.wb_coeffs[1];

        // 3. Process Pixels
        let rgb_data: Vec<u16> = output_buf.chunks_exact(bytes_per_pixel * 3)
            .flat_map(|pixel_bytes| {
                // Extract RGB
                let (r_raw, g_raw, b_raw) = if bytes_per_pixel == 1 {
                    (pixel_bytes[0] as f32, pixel_bytes[1] as f32, pixel_bytes[2] as f32)
                } else {
                    (
                        u16::from_le_bytes([pixel_bytes[0], pixel_bytes[1]]) as f32,
                        u16::from_le_bytes([pixel_bytes[2], pixel_bytes[3]]) as f32,
                        u16::from_le_bytes([pixel_bytes[4], pixel_bytes[5]]) as f32
                    )
                };

                // Black Level & Normalize & WB
                let r_lin = ((r_raw - black_level).max(0.0) / range) * wb_r;
                let g_lin = ((g_raw - black_level).max(0.0) / range) * wb_g;
                let b_lin = ((b_raw - black_level).max(0.0) / range) * wb_b;

                // Color Matrix (Cam -> sRGB)
                let r_out = cam_to_srgb[0][0] * r_lin + cam_to_srgb[0][1] * g_lin + cam_to_srgb[0][2] * b_lin + cam_to_srgb[0][3];
                let g_out = cam_to_srgb[1][0] * r_lin + cam_to_srgb[1][1] * g_lin + cam_to_srgb[1][2] * b_lin + cam_to_srgb[1][3];
                let b_out = cam_to_srgb[2][0] * r_lin + cam_to_srgb[2][1] * g_lin + cam_to_srgb[2][2] * b_lin + cam_to_srgb[2][3];

                // Clamp and Scale to u16
                [
                    (r_out * 65535.0).clamp(0.0, 65535.0) as u16,
                    (g_out * 65535.0).clamp(0.0, 65535.0) as u16,
                    (b_out * 65535.0).clamp(0.0, 65535.0) as u16
                ]
            })
            .collect();
        
        Ok(RgbImageData {
            width,
            height,
            data: rgb_data,
            bits_per_sample: 16,
        })
    }
}
