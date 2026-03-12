use anyhow::Result;
use chrono::{Local, NaiveTime};
use tokio::time::{sleep, Duration};
use tracing::info;

use super::capture::ImageCapture;

pub enum TriggerMode {
    Immediate,
    Scheduled(NaiveTime),
}

pub struct CaptureScheduler<C: ImageCapture> {
    capture: C,
    interval: Duration,
    trigger: TriggerMode,
}

impl<C: ImageCapture> CaptureScheduler<C> {
    pub fn new(capture: C, interval_secs: u64, trigger: TriggerMode) -> Self {
        Self {
            capture,
            interval: Duration::from_secs(interval_secs),
            trigger,
        }
    }

    pub async fn run<F, Fut>(&self, on_capture: F) -> Result<()>
    where
        F: Fn(Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        match &self.trigger {
            TriggerMode::Immediate => {
                info!("Starting capture immediately");
            }
            TriggerMode::Scheduled(start_time) => {
                let now = Local::now().time();
                if now < *start_time {
                    let wait = *start_time - now;
                    let secs = wait.num_seconds() as u64;
                    info!(%start_time, wait_secs = secs, "Waiting for scheduled start");
                    sleep(Duration::from_secs(secs)).await;
                }
                info!(%start_time, "Scheduled start time reached");
            }
        }

        loop {
            match self.capture.capture() {
                Ok(data) => {
                    info!(bytes = data.len(), "Image captured");
                    if let Err(e) = on_capture(data).await {
                        tracing::error!(error = %e, "Failed to process captured image");
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Capture failed");
                }
            }

            info!(interval_secs = self.interval.as_secs(), "Sleeping until next capture");
            sleep(self.interval).await;
        }
    }
}
