use anyhow::{Context, Result};
use futures_lite::StreamExt;
use lapin::{
    options::*,
    types::FieldTable,
    BasicProperties,
    Channel,
    Connection,
    ConnectionProperties,
    ExchangeKind,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{error, info};

use super::traits::{Delivery, MessageQueue};

pub struct RabbitMqQueue {
    url: String,
    connection: Arc<OnceCell<Connection>>,
}

impl RabbitMqQueue {
    pub fn new(url: String) -> Self {
        Self {
            url,
            connection: Arc::new(OnceCell::new()),
        }
    }

    async fn connection(&self) -> Result<&Connection> {
        self.connection
            .get_or_try_init(|| async {
                info!(url = %self.url, "Connecting to RabbitMQ");
                Connection::connect(&self.url, ConnectionProperties::default())
                    .await
                    .context("Failed to connect to RabbitMQ")
            })
            .await
    }

    async fn channel(&self) -> Result<Channel> {
        let conn = self.connection().await?;
        conn.create_channel()
            .await
            .context("Failed to create channel")
    }

    async fn declare_stream(&self, channel: &Channel, stream: &str) -> Result<()> {
        channel
            .exchange_declare(
                stream,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;
        Ok(())
    }

    async fn declare_subscription_queue(
        &self,
        channel: &Channel,
        stream: &str,
        consumer: &str,
    ) -> Result<String> {
        let queue_name = format!("{}.{}", stream, consumer);
        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        channel
            .queue_bind(
                &queue_name,
                stream,
                "",
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(queue_name)
    }
}

impl MessageQueue for RabbitMqQueue {
    fn publish(
        &self,
        stream: &str,
        payload: &[u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let stream = stream.to_string();
        let payload = payload.to_vec();
        Box::pin(async move {
            let channel = self.channel().await?;
            self.declare_stream(&channel, &stream).await?;

            channel
                .basic_publish(
                    &stream,
                    "",
                    BasicPublishOptions::default(),
                    &payload,
                    BasicProperties::default().with_delivery_mode(2),
                )
                .await?
                .await?;

            info!(stream, bytes = payload.len(), "Published message");
            Ok(())
        })
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
        Box::pin(async move {
            let channel = self.channel().await?;
            self.declare_stream(&channel, &stream).await?;
            let queue_name = self
                .declare_subscription_queue(&channel, &stream, &consumer)
                .await?;

            channel
                .basic_qos(1, BasicQosOptions::default())
                .await?;

            let mut consumer = channel
                .basic_consume(
                    &queue_name,
                    &queue_name,
                    BasicConsumeOptions {
                        no_ack: false,
                        ..Default::default()
                    },
                    FieldTable::default(),
                )
                .await?;

            info!(stream, queue_name, "Subscribed, waiting for messages");

            while let Some(delivery_result) = consumer.next().await {
                match delivery_result {
                    Ok(delivery) => {
                        let data = delivery.data.clone();
                        let tag = delivery.delivery_tag;
                        info!(stream, bytes = data.len(), tag, "Received message");

                        match handler(Delivery { data }).await {
                            Ok(()) => {
                                delivery.ack(BasicAckOptions::default()).await?;
                                info!(stream, tag, "Acked");
                            }
                            Err(e) => {
                                error!(stream, tag, error = %e, "Handler failed, nacking");
                                delivery
                                    .nack(BasicNackOptions {
                                        requeue: true,
                                        ..Default::default()
                                    })
                                    .await?;
                            }
                        }
                    }
                    Err(e) => {
                        error!(stream, error = %e, "Consumer error");
                        return Err(e.into());
                    }
                }
            }

            Ok(())
        })
    }
}

