#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use crate::image_pipeline::arw_to_tiff::error::{Result, ConversionError};
    use crate::image_pipeline::arw_to_tiff::types::{RawImageData, ConversionConfig, TiffCompression};
    use crate::image_pipeline::arw_to_tiff::reader::RawImageReader;
    use crate::image_pipeline::arw_to_tiff::writer::TiffWriter;
    use crate::image_pipeline::arw_to_tiff::pipeline::ArwToTiffPipeline;
    use std::io::Write;
    
    struct MockReader {
        should_fail: bool,
        mock_data: Option<RawImageData>,
    }
    
    impl RawImageReader for MockReader {
        fn read_raw(&self, _data: &[u8]) -> Result<RawImageData> {
            if self.should_fail {
                return Err(ConversionError::DecodeError("Mock decode error".to_string()));
            }
            Ok(self.mock_data.clone().unwrap_or(RawImageData {
                width: 100,
                height: 100,
                data: vec![0u16; 100 * 100],
                bits_per_sample: 16,
            }))
        }
    }
    
    struct MockWriter {
        should_fail: bool,
        written_data: std::sync::Arc<std::sync::Mutex<Vec<RawImageData>>>,
    }
    
    impl TiffWriter for MockWriter {
        fn write_tiff(&self, image: &RawImageData, _output: &mut dyn Write, _config: &ConversionConfig) -> Result<()> {
            if self.should_fail {
                return Err(ConversionError::EncodeError("Mock encode error".to_string()));
            }
            self.written_data.lock().unwrap().push(image.clone());
            Ok(())
        }
    }
    
    #[test]
    fn test_config_builder() {
        let config = ConversionConfig::builder()
            .compression(TiffCompression::Deflate)
            .predictor(None)
            .validate_dimensions(false)
            .max_dimension(Some(10000))
            .build();
        
        assert!(matches!(config.compression, TiffCompression::Deflate));
        assert_eq!(config.predictor, None);
        assert!(!config.validate_dimensions);
        assert_eq!(config.max_dimension, Some(10000));
    }
    
    #[test]
    fn test_successful_conversion() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader { should_fail: false, mock_data: None };
        let writer = MockWriter { should_fail: false, written_data: written.clone() };
        
        let pipeline = ArwToTiffPipeline::with_custom(
            reader,
            writer,
            ConversionConfig::default(),
        );
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_ok());
        assert_eq!(written.lock().unwrap().len(), 1);
    }
    
    #[test]
    fn test_reader_failure() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader { should_fail: true, mock_data: None };
        let writer = MockWriter { should_fail: false, written_data: written.clone() };
        
        let pipeline = ArwToTiffPipeline::with_custom(
            reader,
            writer,
            ConversionConfig::default(),
        );
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConversionError::DecodeError(_)));
    }
    
    #[test]
    fn test_writer_failure() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader { should_fail: false, mock_data: None };
        let writer = MockWriter { should_fail: true, written_data: written };
        
        let pipeline = ArwToTiffPipeline::with_custom(
            reader,
            writer,
            ConversionConfig::default(),
        );
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConversionError::EncodeError(_)));
    }
    
    #[test]
    fn test_dimension_validation_success() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader {
            should_fail: false,
            mock_data: Some(RawImageData {
                width: 1000,
                height: 1000,
                data: vec![0u16; 1000 * 1000],
                bits_per_sample: 16,
            }),
        };
        let writer = MockWriter { should_fail: false, written_data: written };
        
        let config = ConversionConfig::builder()
            .validate_dimensions(true)
            .max_dimension(Some(2000))
            .build();
        
        let pipeline = ArwToTiffPipeline::with_custom(reader, writer, config);
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_dimension_validation_failure() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader {
            should_fail: false,
            mock_data: Some(RawImageData {
                width: 10000,
                height: 10000,
                data: vec![0u16; 100],
                bits_per_sample: 16,
            }),
        };
        let writer = MockWriter { should_fail: false, written_data: written };
        
        let config = ConversionConfig::builder()
            .validate_dimensions(true)
            .max_dimension(Some(5000))
            .build();
        
        let pipeline = ArwToTiffPipeline::with_custom(reader, writer, config);
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConversionError::InvalidDimensions(_, _)));
    }
    
    #[test]
    fn test_dimension_validation_disabled() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = MockReader {
            should_fail: false,
            mock_data: Some(RawImageData {
                width: 10000,
                height: 10000,
                data: vec![0u16; 100],
                bits_per_sample: 16,
            }),
        };
        let writer = MockWriter { should_fail: false, written_data: written };
        
        let config = ConversionConfig::builder()
            .validate_dimensions(false)
            .build();
        
        let pipeline = ArwToTiffPipeline::with_custom(reader, writer, config);
        
        let mut output = Cursor::new(Vec::new());
        let result = pipeline.convert(b"fake arw data", &mut output);
        
        assert!(result.is_ok());
    }
}
