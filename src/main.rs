use anyhow::Result;
use chrono::NaiveTime;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use assfid_ffed::image_pipeline::{ConversionConfig, RawToTiffPipeline, TiffCompression};
use assfid_ffed::logger;
use assfid_ffed::service::{
    capture::{Gphoto2Capture, ImageCapture, MockCapture},
    config::{AppConfig, CaptureSource, QueueBackend},
    indices::{IndexCalculator, StubIndexCalculator},
    queue::{Delivery, MessageQueue, MockQueue, RabbitMqQueue},
    scheduler::{CaptureScheduler, TriggerMode},
};

const TIFF_QUEUE: &str = "tiff_queue";
const INDICES_QUEUE: &str = "indices_queue";
const MODEL_DATA_QUEUE: &str = "model_data_queue";
const INDEX_CONSUMER: &str = "index-calculator";
const MODEL_DATA_CONSUMER: &str = "model-data";

fn main() -> Result<()> {
    logger::init();

    let config_path = std::env::args().nth(1).map(PathBuf::from);
    let config = match config_path {
        Some(ref path) => {
            info!(path = %path.display(), "Loading config");
            AppConfig::load(path)?
        }
        None => {
            info!("No config file specified, using defaults");
            AppConfig::default_config()
        }
    };

    info!(?config, "Starting ffed_protosat");

    let compression = match config.pipeline.compression.as_str() {
        "lzw" => TiffCompression::Lzw,
        "deflate" => TiffCompression::DeflateBalanced,
        _ => TiffCompression::None,
    };

    let pipeline_config = ConversionConfig::builder()
        .compression(compression)
        .debayer(config.pipeline.debayer)
        .build();

    let pipeline = RawToTiffPipeline::new(pipeline_config)?;

    let capture: Box<dyn ImageCapture> = match config.capture.source {
        CaptureSource::Mock => {
            let mock_file = config
                .capture
                .mock_file
                .unwrap_or_else(|| PathBuf::from("input.arw"));
            info!(path = %mock_file.display(), "Using mock capture");
            Box::new(MockCapture::new(mock_file))
        }
        CaptureSource::Gphoto2 => {
            info!("Using gphoto2 capture");
            Box::new(Gphoto2Capture::new(config.capture.output_dir.clone()))
        }
    };

    let trigger = match config.capture.start_time {
        Some(ref time_str) => {
            let time = NaiveTime::parse_from_str(time_str, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(time_str, "%H:%M"))?;
            TriggerMode::Scheduled(time)
        }
        None => TriggerMode::Immediate,
    };

    let runtime = tokio::runtime::Runtime::new()?;

    runtime.block_on(async {
        let queue: Arc<dyn MessageQueue> = match config.queue.backend {
            QueueBackend::Mock => {
                info!("Using mock queue");
                Arc::new(MockQueue::new())
            }
            QueueBackend::Rabbitmq => {
                info!(url = %config.queue.rabbitmq_url, "Using RabbitMQ");
                Arc::new(RabbitMqQueue::new(config.queue.rabbitmq_url.clone()))
            }
        };

        let producer_queue = Arc::clone(&queue);
        let producer = tokio::spawn(async move {
            let scheduler = CaptureScheduler::new(capture, config.capture.interval_secs, trigger);
            info!(
                "Capture loop starting (interval={}s)",
                config.capture.interval_secs
            );
            scheduler
                .run(|raw_data| {
                    let pipeline = &pipeline;
                    let queue = &*producer_queue;
                    async move {
                        let mut tiff_buf = Vec::new();
                        pipeline.convert(&raw_data, &mut tiff_buf)?;
                        info!(tiff_bytes = tiff_buf.len(), "Converted to TIFF");

                        queue.publish(TIFF_QUEUE, &tiff_buf).await?;
                        info!("Published TIFF to {}", TIFF_QUEUE);

                        Ok(())
                    }
                })
                .await
        });

        let index_queue = Arc::clone(&queue);
        let index_calculator: Arc<dyn IndexCalculator> = Arc::new(StubIndexCalculator);
        let index_consumer = tokio::spawn(async move {
            let publish_queue = Arc::clone(&index_queue);
            index_queue
                .subscribe(
                    TIFF_QUEUE,
                    INDEX_CONSUMER,
                    Box::new(move |delivery: Delivery| {
                        let calc = Arc::clone(&index_calculator);
                        let q = Arc::clone(&publish_queue);
                        let delivery_data = delivery.data.clone();
                        Box::pin(async move {
                            let indices = calc.calculate(&delivery_data)?;
                            let payload = calc.serialize(&indices)?;
                            q.publish(INDICES_QUEUE, &payload).await?;
                            info!("Published indices to {}", INDICES_QUEUE);

                            Ok(())
                        })
                    }),
                )
                .await
        });

        let data_queue = Arc::clone(&queue);
        let data_consumer = tokio::spawn(async move {
            let publish_queue = Arc::clone(&data_queue);
            data_queue
                .subscribe(
                    TIFF_QUEUE,
                    MODEL_DATA_CONSUMER,
                    Box::new(move |delivery: Delivery| {
                        let q = Arc::clone(&publish_queue);
                        Box::pin(async move {
                            q.publish(MODEL_DATA_QUEUE, &delivery.data).await?;
                            info!("Published data to {}", MODEL_DATA_QUEUE);
                            Ok(())
                        })
                    }),
                )
                .await
        });

        tokio::select! {
            r = producer => r??,
            r = index_consumer => r??,
            r = data_consumer => r??,
        }

        Ok(())
    })
}

