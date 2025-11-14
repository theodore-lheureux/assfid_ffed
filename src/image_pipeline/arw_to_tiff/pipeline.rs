use log::{info, warn};
use std::io::Write;
use std::path::Path;

use crate::image_pipeline::{
    ConversionConfig, ConversionError, Result,
    arw_to_tiff::{RawImageReader, RawLoaderReader, StandardTiffWriter, TiffWriter, PipelineTimings, Timer},
};

pub struct ArwToTiffPipeline<R: RawImageReader, W: TiffWriter> {
    reader: R,
    writer: W,
    config: ConversionConfig,
}

impl ArwToTiffPipeline<RawLoaderReader, StandardTiffWriter> {
    pub fn new(config: ConversionConfig) -> Self {
        Self {
            reader: RawLoaderReader,
            writer: StandardTiffWriter,
            config,
        }
    }
}

impl<R: RawImageReader, W: TiffWriter> ArwToTiffPipeline<R, W> {
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

        if let Some(max) = self.config.max_dimension {
            if width > max || height > max {
                warn!(
                    "Image dimensions {}x{} exceed maximum {}",
                    width, height, max
                );
                return Err(ConversionError::InvalidDimensions(width, height));
            }
        }

        Ok(())
    }

    pub fn convert(&self, input_data: &[u8], output: &mut dyn Write) -> Result<()> {
        let mut timings = PipelineTimings::new();
        info!("Starting ARW to TIFF conversion");

        let timer = Timer::start("decode_raw");
        let raw_image = self.reader.read_raw(input_data)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("validate_dimensions");
        self.validate_dimensions(raw_image.width, raw_image.height)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("encode_tiff");
        self.writer.write_tiff(&raw_image, output, &self.config)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        info!(
            "Conversion complete: {}x{} in {:.3}ms",
            raw_image.width,
            raw_image.height,
            timings.total_duration().as_secs_f64() * 1000.0
        );
        Ok(())
    }

    pub fn convert_with_timings(
        &self,
        input_data: &[u8],
        output: &mut dyn Write,
    ) -> Result<PipelineTimings> {
        let mut timings = PipelineTimings::new();
        info!("Starting ARW to TIFF conversion");

        let timer = Timer::start("decode_raw");
        let raw_image = self.reader.read_raw(input_data)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("validate_dimensions");
        self.validate_dimensions(raw_image.width, raw_image.height)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("encode_tiff");
        self.writer.write_tiff(&raw_image, output, &self.config)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        info!(
            "Conversion complete: {}x{} in {:.3}ms",
            raw_image.width,
            raw_image.height,
            timings.total_duration().as_secs_f64() * 1000.0
        );
        Ok(timings)
    }

    pub fn convert_file<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<()> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        info!(
            "Converting file: {} -> {}",
            input_path.display(),
            output_path.display()
        );

        let timer = Timer::start("read_input_file");
        let input_data = std::fs::read(input_path).map_err(|e| {
            ConversionError::InputReadError(format!("{}: {}", input_path.display(), e))
        })?;
        let (name, duration) = timer.stop();
        info!("{}: {:.3}ms", name, duration.as_secs_f64() * 1000.0);

        let timer = Timer::start("create_output_file");
        let mut output_file = std::fs::File::create(output_path).map_err(|e| {
            ConversionError::OutputWriteError(format!("{}: {}", output_path.display(), e))
        })?;
        let (name, duration) = timer.stop();
        info!("{}: {:.3}ms", name, duration.as_secs_f64() * 1000.0);

        self.convert(&input_data, &mut output_file)?;

        Ok(())
    }

    pub fn convert_file_with_timings<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<PipelineTimings> {
        let mut timings = PipelineTimings::new();
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        info!(
            "Converting file: {} -> {}",
            input_path.display(),
            output_path.display()
        );

        let timer = Timer::start("read_input_file");
        let input_data = std::fs::read(input_path).map_err(|e| {
            ConversionError::InputReadError(format!("{}: {}", input_path.display(), e))
        })?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("create_output_file");
        let mut output_file = std::fs::File::create(output_path).map_err(|e| {
            ConversionError::OutputWriteError(format!("{}: {}", output_path.display(), e))
        })?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        let timer = Timer::start("conversion");
        let conversion_timings = self.convert_with_timings(&input_data, &mut output_file)?;
        let (name, duration) = timer.stop();
        timings.add_step(name, duration);

        for step in conversion_timings.steps() {
            timings.add_step(step.name.clone(), step.duration);
        }

        Ok(timings)
    }

    pub fn config(&self) -> &ConversionConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: ConversionConfig) {
        self.config = config;
    }
}
