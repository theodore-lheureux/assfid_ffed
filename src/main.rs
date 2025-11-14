

use ffed_protosat_rs::logger;
use ffed_protosat_rs::image_pipeline::{ArwToTiffPipeline, ConversionConfig, TiffCompression};

use log::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init();

    info!("Starting ffed_protosat...");

    // Example: Create a conversion pipeline with custom configuration
    let config = ConversionConfig::builder()
        .compression(TiffCompression::Lzw)
        .predictor(Some(2))
        .validate_dimensions(true)
        .max_dimension(Some(50000))
        .build();
    
    let pipeline = ArwToTiffPipeline::new(config);
    
    info!("ARW to TIFF pipeline initialized");
    info!("Compression: {:?}", pipeline.config().compression);
    
    // Example usage (uncomment when you have actual files):
    match pipeline.convert_file("input.arw", "output.tiff") {
        Ok(_) => info!("Conversion successful!"),
        Err(e) => error!("Conversion failed: {}", e),
    }

    Ok(())
}
