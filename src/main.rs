use ffed_protosat_rs::image_pipeline::{ConversionConfig, RawToTiffPipeline, TiffCompression};
use ffed_protosat_rs::logger;

use tracing::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init();

    info!("Starting ffed_protosat...");

    let config = ConversionConfig::builder()
        .compression(TiffCompression::None)
        .debayer(true)
        .build();
    let pipeline = RawToTiffPipeline::new(config)?;

    info!("RAW to TIFF pipeline initialized");
    info!("Compression: {:?}", pipeline.config().compression);
    info!(
        "Debayering: {}",
        if pipeline.config().debayer {
            "enabled"
        } else {
            "disabled"
        }
    );

    match pipeline.convert_file("input.arw", "output.tiff") {
        Ok(_) => info!("Conversion successful!"),
        Err(e) => error!("Conversion failed: {}", e),
    }

    Ok(())
}
