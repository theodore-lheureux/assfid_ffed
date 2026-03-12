use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

pub trait MessageQueue: Send + Sync {
    fn publish(
        &self,
        stream: &str,
        payload: &[u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    fn subscribe(
        &self,
        stream: &str,
        consumer: &str,
        handler: Box<dyn Fn(Delivery) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

pub struct Delivery {
    pub data: Vec<u8>,
}
