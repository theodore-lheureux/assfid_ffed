use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Failed to read input file: {0}")]
    InputReadError(String),
    
    #[error("Failed to write output file: {0}")]
    OutputWriteError(String),
    
    #[error("Failed to decode ARW image: {0}")]
    DecodeError(String),
    
    #[error("Failed to encode TIFF image: {0}")]
    EncodeError(String),
    
    #[error("Invalid image dimensions: width={0}, height={1}")]
    InvalidDimensions(usize, usize),
    
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    
    #[error("CUDA error: {0}")]
    CudaError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConversionError>;
