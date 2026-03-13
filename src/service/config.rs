use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub capture: CaptureConfig,
    pub queue: QueueConfig,
    pub pipeline: PipelineConfig,
}

#[derive(Debug, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_capture_source")]
    pub source: CaptureSource,
    #[serde(default = "default_capture_interval")]
    pub interval_secs: u64,
    pub start_time: Option<String>,
    pub mock_file: Option<PathBuf>,
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaptureSource {
    Gphoto2,
    #[default]
    Mock,
}

#[derive(Debug, Deserialize)]
pub struct QueueConfig {
    #[serde(default = "default_queue_backend")]
    pub backend: QueueBackend,
    #[serde(default = "default_rabbitmq_url")]
    pub rabbitmq_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum QueueBackend {
    #[default]
    Mock,
    Rabbitmq,
}

#[derive(Debug, Deserialize)]
pub struct PipelineConfig {
    #[serde(default)]
    pub debayer: bool,
    #[serde(default = "default_compression")]
    pub compression: String,
}

fn default_capture_source() -> CaptureSource {
    CaptureSource::Mock
}

fn default_capture_interval() -> u64 {
    45
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("/tmp/ffed_captures")
}

fn default_queue_backend() -> QueueBackend {
    QueueBackend::Mock
}

fn default_rabbitmq_url() -> String {
    "amqp://ffed:password@localhost:5672/%2f".to_string()
}

fn default_compression() -> String {
    "none".to_string()
}

impl AppConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            capture: CaptureConfig {
                source: CaptureSource::Mock,
                interval_secs: 45,
                start_time: None,
                mock_file: Some(PathBuf::from("input.arw")),
                output_dir: default_output_dir(),
            },
            queue: QueueConfig {
                backend: QueueBackend::Mock,
                rabbitmq_url: default_rabbitmq_url(),
            },
            pipeline: PipelineConfig {
                debayer: true,
                compression: "none".to_string(),
            },
        }
    }
}
