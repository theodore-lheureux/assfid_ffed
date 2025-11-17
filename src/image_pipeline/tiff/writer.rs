use std::io::Write;
use crate::image_pipeline::common::error::Result;
use crate::image_pipeline::raw::types::RawImageData;
use crate::image_pipeline::debayer::types::RgbImageData;
use crate::image_pipeline::tiff::types::ConversionConfig;

pub trait TiffWriter {
    fn write_tiff(&self, image: &RawImageData, output: &mut dyn Write, config: &ConversionConfig) -> Result<()>;
    fn write_rgb_tiff(&self, image: &RgbImageData, output: &mut dyn Write, config: &ConversionConfig) -> Result<()>;
}
