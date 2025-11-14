#[derive(Debug, Clone)]
pub struct RawImageData {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u16>,
    pub bits_per_sample: u32,
}

#[derive(Debug, Clone)]
pub struct ConversionConfig {
    pub compression: TiffCompression,
    pub predictor: Option<u16>,
    pub validate_dimensions: bool,
    pub max_dimension: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum TiffCompression {
    None,
    Lzw,
    Deflate,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            compression: TiffCompression::Lzw,
            predictor: Some(2),
            validate_dimensions: true,
            max_dimension: Some(50000),
        }
    }
}

impl ConversionConfig {
    pub fn builder() -> ConversionConfigBuilder {
        ConversionConfigBuilder::default()
    }
}

#[derive(Default)]
pub struct ConversionConfigBuilder {
    compression: Option<TiffCompression>,
    predictor: Option<Option<u16>>,
    validate_dimensions: Option<bool>,
    max_dimension: Option<Option<usize>>,
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
    
    pub fn max_dimension(mut self, max: Option<usize>) -> Self {
        self.max_dimension = Some(max);
        self
    }
    
    pub fn build(self) -> ConversionConfig {
        let default = ConversionConfig::default();
        ConversionConfig {
            compression: self.compression.unwrap_or(default.compression),
            predictor: self.predictor.unwrap_or(default.predictor),
            validate_dimensions: self.validate_dimensions.unwrap_or(default.validate_dimensions),
            max_dimension: self.max_dimension.unwrap_or(default.max_dimension),
        }
    }
}
