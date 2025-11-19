//! Image processing pipeline module
//!
//! This module provides a structured approach to image format conversions,
//! with separate modules for RAW reading, TIFF writing, and conversion orchestration.

pub mod raw;
pub mod tiff;
pub mod conversions;
pub mod common;
pub mod debayer;

pub use common::{
    ConversionError,
    Result,
};

pub use raw::{
    RawImageData,
    RawImageReader,
    RawLoaderReader,
};

pub use tiff::{
    TiffCompression,
    ConversionConfig,
    ConversionConfigBuilder,
    TiffWriter,
    StandardTiffWriter,
};

pub use conversions::{
    RawToTiffPipeline,
};

pub use debayer::{
    RgbImageData,
    CudaDebayer,
    CpuDebayer,
};