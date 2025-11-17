//! TIFF conversion configuration types

/// TIFF compression methods
#[derive(Debug, Clone, Copy)]
pub enum TiffCompression {
    /// No compression (fastest, largest file)
    None,
    /// LZW compression (slow, good compression)
    Lzw,
    /// Deflate compression - fast level (good speed/size balance)
    DeflateFast,
    /// Deflate compression - best compression (slower)
    DeflateBest,
    /// Deflate compression - balanced (default)
    DeflateBalanced,
}

/// Configuration for RAW to TIFF conversion
#[derive(Debug, Clone)]
pub struct ConversionConfig {
    /// Compression method to use
    pub compression: TiffCompression,
    /// Predictor value for compression (typically 2 for horizontal differencing)
    /// Note: Predictor adds processing time, set to None for maximum speed
    pub predictor: Option<u16>,
    /// Whether to validate image dimensions before conversion
    pub validate_dimensions: bool,
    /// Whether to debayer the image to RGB (true) or output grayscale Bayer (false)
    pub debayer: bool,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            compression: TiffCompression::None,
            predictor: None,
            validate_dimensions: true,
            debayer: false,
        }
    }
}

impl ConversionConfig {
    pub fn builder() -> ConversionConfigBuilder {
        ConversionConfigBuilder::default()
    }
}

/// Builder for ConversionConfig
#[derive(Default)]
pub struct ConversionConfigBuilder {
    compression: Option<TiffCompression>,
    predictor: Option<Option<u16>>,
    validate_dimensions: Option<bool>,
    debayer: Option<bool>,
}

impl ConversionConfigBuilder {
    pub fn compression(mut self, compression: TiffCompression) -> Self {
        self.compression = Some(compression);
        self
    }
    
    pub fn predictor(mut self, predictor: Option<u16>) -> Self {
        self.predictor = Some(predictor);
        self
    }
    
    pub fn validate_dimensions(mut self, validate: bool) -> Self {
        self.validate_dimensions = Some(validate);
        self
    }
    
    pub fn debayer(mut self, enable: bool) -> Self {
        self.debayer = Some(enable);
        self
    }
    
    pub fn build(self) -> ConversionConfig {
        let default = ConversionConfig::default();
        ConversionConfig {
            compression: self.compression.unwrap_or(default.compression),
            predictor: self.predictor.unwrap_or(default.predictor),
            validate_dimensions: self.validate_dimensions.unwrap_or(default.validate_dimensions),
            debayer: self.debayer.unwrap_or(default.debayer),
        }
    }
}

