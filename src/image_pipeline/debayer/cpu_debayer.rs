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
        let bayer_bytes: Vec<u8> = raw_image.data.iter()
            .flat_map(|&val| val.to_le_bytes())
            .collect();
        
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
        
        // Convert output buffer to u16 RGB data
        let rgb_data: Vec<u16> = if bytes_per_pixel == 1 {
            // 8-bit output: scale up to 16-bit
            let shift = raw_image.bits_per_sample.saturating_sub(8);
            output_buf.iter()
                .map(|&val| (val as u16) << shift)
                .collect()
        } else {
            // 16-bit output: convert byte pairs to u16
            output_buf.chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect()
        };
        
        Ok(RgbImageData {
            width,
            height,
            data: rgb_data,
            bits_per_sample: raw_image.bits_per_sample,
        })
    }
}
