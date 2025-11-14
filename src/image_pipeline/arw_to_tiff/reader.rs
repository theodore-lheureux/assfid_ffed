use crate::image_pipeline::arw_to_tiff::error::Result;
use crate::image_pipeline::arw_to_tiff::types::RawImageData;

pub trait RawImageReader {
    fn read_raw(&self, data: &[u8]) -> Result<RawImageData>;
}
