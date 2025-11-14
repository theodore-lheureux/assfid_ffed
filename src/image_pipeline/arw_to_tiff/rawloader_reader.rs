use log::debug;
use crate::image_pipeline::arw_to_tiff::error::{Result, ConversionError};
use crate::image_pipeline::arw_to_tiff::types::RawImageData;
use crate::image_pipeline::arw_to_tiff::reader::RawImageReader;

pub struct RawLoaderReader;

impl RawImageReader for RawLoaderReader {
    fn read_raw(&self, data: &[u8]) -> Result<RawImageData> {
        debug!("Decoding ARW image, {} bytes", data.len());
        
        let decoded = rawloader::decode(&mut std::io::Cursor::new(data))
            .map_err(|e| ConversionError::DecodeError(e.to_string()))?;
        
        let width = decoded.width;
        let height = decoded.height;
        
        debug!("Decoded image: {}x{}", width, height);
        
        let data: Vec<u16> = match decoded.data {
            rawloader::RawImageData::Integer(values) => {
                values.iter().map(|&v| v as u16).collect()
            }
            rawloader::RawImageData::Float(values) => {
                values.iter().map(|&v| (v * 65535.0) as u16).collect()
            }
        };
        
        Ok(RawImageData {
            width,
            height,
            data,
            bits_per_sample: 16,
        })
    }
}
