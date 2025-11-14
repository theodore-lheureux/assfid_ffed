//! RAW image reader implementation using the rawloader library.
//!
//! This module provides support for reading various RAW image formats (ARW, CR2, NEF, DNG, etc.)
//! using the rawloader library. It handles decoding RAW sensor data and extracting metadata
//! to properly represent the image data.

use std::io::Cursor;

use tracing::debug;
use rawloader::RawImageData as RawloaderImageData;
use crate::image_pipeline::common::error::{Result, ConversionError};
use crate::image_pipeline::raw::types::RawImageData;
use crate::image_pipeline::raw::reader::RawImageReader;

/// RAW image reader that uses the rawloader library for decoding.
///
/// This reader supports any RAW format that rawloader can decode, including but not limited to:
/// - Sony ARW
/// - Fujifilm RAF
pub struct RawLoaderReader;

/// Default bit depth when no white level information is available from the RAW file.
const DEFAULT_BITS_PER_SAMPLE: u32 = 16;

/// The bit width of the u16 data type, used for calculating actual bits per sample.
const U16_BITS: u32 = 16;

impl RawImageReader for RawLoaderReader {
    /// Reads and decodes RAW image data from a byte array.
    ///
    /// This method:
    /// 1. Decodes the RAW file using rawloader
    /// 2. Converts the data to u16 format (handles both integer and float RAW data)
    /// 3. Calculates the actual bits per sample from the sensor's white level metadata
    ///
    /// # Arguments
    ///
    /// * `data` - Raw bytes of the RAW image file
    ///
    /// # Returns
    ///
    /// * `Ok(RawImageData)` - Successfully decoded image with metadata
    /// * `Err(ConversionError)` - Failed to decode the RAW file
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ffed_protosat_rs::image_pipeline::arw_to_tiff::rawloader_reader::RawLoaderReader;
    /// use ffed_protosat_rs::image_pipeline::arw_to_tiff::reader::RawImageReader;
    ///
    /// let reader = RawLoaderReader;
    /// let raw_bytes = std::fs::read("image.arw").unwrap();
    /// let image_data = reader.read_raw(&raw_bytes).unwrap();
    /// ```
    fn read_raw(&self, data: &[u8]) -> Result<RawImageData> {
        debug!("Decoding RAW image, {} bytes", data.len());
        
        let decoded = rawloader::decode(&mut Cursor::new(data))
            .map_err(|e| ConversionError::DecodeError(e.to_string()))?;
        
        let width = decoded.width;
        let height = decoded.height;
        
        debug!("Decoded image: {}x{}", width, height);
        
        // Convert RAW data to u16 format
        // Integer data is cast directly, float data (normalized 0.0-1.0) is scaled to u16 range
        let data: Vec<u16> = match decoded.data {
            RawloaderImageData::Integer(values) => {
                values.iter().map(|&v| v as u16).collect()
            }
            // If the data is in float format, we scale it to u16 range
            RawloaderImageData::Float(values) => {
                values.iter().map(|&v| (v * u16::MAX as f32) as u16).collect()
            }
        };
        
        // Calculate the actual bits per sample from the sensor's white level metadata.
        // The white level represents the maximum pixel value the sensor can produce,
        // which tells us the actual bit depth of the sensor (e.g., 12-bit, 14-bit, 16-bit).
        // This makes the reader format-agnostic and works with any RAW format.
        let max_white_level = decoded.whitelevels.iter().max().copied().unwrap_or( u16::MAX);
        let bits_per_sample = if max_white_level == 0 {
            // If white level is 0 (invalid), default to 16-bit
            DEFAULT_BITS_PER_SAMPLE
        } else {
            // Calculate minimum bits needed to represent the max value
            // e.g., max_white_level = 4095 (0xFFF) -> 12 bits
            //       max_white_level = 16383 (0x3FFF) -> 14 bits
            (U16_BITS - max_white_level.leading_zeros()) as u32
        };
        
        debug!("Calculated bits_per_sample: {} (max white level: {})", bits_per_sample, max_white_level);
        
        Ok(RawImageData {
            width,
            height,
            data,
            bits_per_sample,
        })
    }
}
