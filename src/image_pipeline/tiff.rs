//! TIFF writing module
//!
//! This module provides TIFF file writing capabilities with various compression options.

mod writer;
mod standard_tiff_writer;
pub mod types;

pub use writer::TiffWriter;
pub use standard_tiff_writer::StandardTiffWriter;
pub use types::{TiffCompression, ConversionConfig, ConversionConfigBuilder};
