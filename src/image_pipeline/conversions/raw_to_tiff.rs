use tracing::{info, instrument};
use std::io::Write;
use std::path::Path;

use crate::image_pipeline::{
    common::error::{ConversionError, Result},
    raw::{RawImageReader, RawLoaderReader},
    tiff::{TiffWriter, StandardTiffWriter, ConversionConfig},
};

pub struct RawToTiffPipeline<R: RawImageReader, W: TiffWriter> {
    reader: R,
    writer: W,
    config: ConversionConfig,
}

impl RawToTiffPipeline<RawLoaderReader, StandardTiffWriter> {
    pub fn new(config: ConversionConfig) -> Self {
        Self {
            reader: RawLoaderReader,
            writer: StandardTiffWriter,
            config,
        }
    }
}

impl<R: RawImageReader, W: TiffWriter> RawToTiffPipeline<R, W> {
    pub fn with_custom(reader: R, writer: W, config: ConversionConfig) -> Self {
        Self {
            reader,
            writer,
            config,
        }
    }

    fn validate_dimensions(&self, width: usize, height: usize) -> Result<()> {
        if !self.config.validate_dimensions {
            return Ok(());
        }

        if width == 0 || height == 0 {
            return Err(ConversionError::InvalidDimensions(width, height));
        }

        Ok(())
    }

    #[instrument(skip(self, input_data, output), fields(input_size = input_data.len()))]
    pub fn convert(&self, input_data: &[u8], output: &mut dyn Write) -> Result<()> {
        info!("Starting RAW to TIFF conversion");

        let raw_image = {
            let _span = tracing::info_span!("decode_raw").entered();
            self.reader.read_raw(input_data)?
        };

        {
            let _span = tracing::info_span!("validate_dimensions", 
                width = raw_image.width, 
                height = raw_image.height
            ).entered();
            self.validate_dimensions(raw_image.width, raw_image.height)?;
        }

        {
            let _span = tracing::info_span!("encode_tiff").entered();
            self.writer.write_tiff(&raw_image, output, &self.config)?;
        }

        info!(
            width = raw_image.width,
            height = raw_image.height,
            "Conversion complete"
        );
        Ok(())
    }

    #[instrument(skip(self, input_path, output_path))]
    pub fn convert_file<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<()> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        info!(
            input = %input_path.display(),
            output = %output_path.display(),
            "Converting file"
        );

        let input_data = {
            let _span = tracing::info_span!("read_input_file").entered();
            std::fs::read(input_path).map_err(|e| {
                ConversionError::InputReadError(format!("{}: {}", input_path.display(), e))
            })?
        };

        let mut output_file = {
            let _span = tracing::info_span!("create_output_file").entered();
            std::fs::File::create(output_path).map_err(|e| {
                ConversionError::OutputWriteError(format!("{}: {}", output_path.display(), e))
            })?
        };

        self.convert(&input_data, &mut output_file)?;

        Ok(())
    }

    pub fn config(&self) -> &ConversionConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: ConversionConfig) {
        self.config = config;
    }
}
