use crate::image_pipeline::common::error::Result;
use crate::image_pipeline::raw::types::RawImageData;

pub trait RawImageReader {
    fn read_raw(&self, data: &[u8]) -> Result<RawImageData>;
}
