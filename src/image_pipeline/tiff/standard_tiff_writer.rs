use std::io::Write;
use tracing::debug;
use crate::image_pipeline::common::error::{Result, ConversionError};
use crate::image_pipeline::raw::types::RawImageData;
use crate::image_pipeline::tiff::types::{ConversionConfig, TiffCompression};
use crate::image_pipeline::tiff::writer::TiffWriter;

pub struct StandardTiffWriter;

impl TiffWriter for StandardTiffWriter {
    fn write_tiff(&self, image: &RawImageData, output: &mut dyn Write, config: &ConversionConfig) -> Result<()> {
        debug!("Encoding TIFF image: {}x{}", image.width, image.height);
        
        let mut buffer = Vec::new();
        
        let compression = match config.compression {
            TiffCompression::None => tiff::encoder::Compression::Uncompressed,
            TiffCompression::Lzw => tiff::encoder::Compression::Lzw,
            TiffCompression::DeflateFast => tiff::encoder::Compression::Deflate(tiff::encoder::compression::DeflateLevel::Fast),
            TiffCompression::DeflateBalanced => tiff::encoder::Compression::Deflate(tiff::encoder::compression::DeflateLevel::Balanced),
            TiffCompression::DeflateBest => tiff::encoder::Compression::Deflate(tiff::encoder::compression::DeflateLevel::Best),
        };
        
        let mut encoder = tiff::encoder::TiffEncoder::new(std::io::Cursor::new(&mut buffer))
            .map_err(|e| ConversionError::EncodeError(e.to_string()))?
            .with_compression(compression);
        
        if let Some(predictor_val) = config.predictor {
            let predictor = match predictor_val {
                2 => tiff::tags::Predictor::Horizontal,
                _ => tiff::tags::Predictor::None,
            };
            encoder = encoder.with_predictor(predictor);
        }
        
        encoder.write_image::<tiff::encoder::colortype::Gray16>(
            image.width as u32,
            image.height as u32,
            &image.data,
        ).map_err(|e| ConversionError::EncodeError(e.to_string()))?;
        
        output.write_all(&buffer)?;
        
        debug!("TIFF encoding complete");
        Ok(())
    }
}
