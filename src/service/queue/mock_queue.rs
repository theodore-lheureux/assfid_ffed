use anyhow::Result;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tracing::info;

use super::traits::{Delivery, MessageQueue};

pub struct MockQueue {
    streams: Arc<Mutex<HashMap<String, HashMap<String, Vec<Vec<u8>>>>>>,
    notify: Arc<Notify>,
}

impl MockQueue {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl MessageQueue for MockQueue {
    fn publish(
        &self,
        stream: &str,
        payload: &[u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let mut streams = self.streams.lock().unwrap();
        if let Some(consumers) = streams.get_mut(stream) {
            for queue in consumers.values_mut() {
                queue.push(payload.to_vec());
            }
        }
        self.notify.notify_waiters();
        info!(stream, bytes = payload.len(), "Mock: published message");
        Box::pin(async { Ok(()) })
    }

    fn subscribe(
        &self,
        stream: &str,
        consumer: &str,
        handler: Box<
            dyn Fn(Delivery) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync,
        >,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let stream = stream.to_string();
        let consumer = consumer.to_string();
        let streams = Arc::clone(&self.streams);
        let notify = Arc::clone(&self.notify);
        {
            let mut all_streams = self.streams.lock().unwrap();
            all_streams
                .entry(stream.clone())
                .or_default()
                .entry(consumer.clone())
                .or_default();
        }
        Box::pin(async move {
            info!(stream, consumer, "Mock: subscribed");
            loop {
                let item = streams
                    .lock()
                    .unwrap()
                    .get_mut(&stream)
                    .and_then(|consumers| consumers.get_mut(&consumer))
                    .and_then(|queue| if queue.is_empty() { None } else { Some(queue.remove(0)) });

                if let Some(data) = item {
                    handler(Delivery { data }).await?;
                } else {
                    notify.notified().await;
                }
            }
        })
    }
}

