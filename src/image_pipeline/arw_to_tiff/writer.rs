use std::io::Write;
use crate::image_pipeline::arw_to_tiff::error::Result;
use crate::image_pipeline::arw_to_tiff::types::{RawImageData, ConversionConfig};

pub trait TiffWriter {
    fn write_tiff(&self, image: &RawImageData, output: &mut dyn Write, config: &ConversionConfig) -> Result<()>;
}
