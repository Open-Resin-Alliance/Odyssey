use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use odyssey::{
    error::OdysseyError,
    serial_handler::{InternalCommsHandler, SerialHandler},
};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

pub struct MockSerialHandler {
    internal_comms: InternalCommsHandler,
    pub response_map: HashMap<String, String>,
    pub default_response: String,
}

impl MockSerialHandler {
    pub fn new(default_response: String) -> MockSerialHandler {
        MockSerialHandler {
            internal_comms: InternalCommsHandler::new(),
            response_map: HashMap::new(),
            default_response,
        }
    }
    pub fn add_response(&mut self, message: String, response: String) {
        self.response_map.insert(message, response);
    }
}

#[async_trait]
impl SerialHandler for MockSerialHandler {
    fn get_internal_comms(&self) -> InternalCommsHandler {
        self.internal_comms.clone()
    }

    async fn run(
        mut self: Box<Self>,
        cancellation_token: CancellationToken,
    ) -> Result<(), OdysseyError> {
        let mut interval = interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            if let Some(message) = self.internal_comms.try_receive().await? {
                let response = self.response_map.get(message.trim()).map(|resp|resp.to_string()).unwrap_or(self.default_response.clone());
                
                tracing::debug!("Received message={}, emitting response={}", message,response);
                self.internal_comms
                    .send(response)
                    .await?
            
            }

            if cancellation_token.is_cancelled() {
                tracing::info!("Shutting down serial processing loop");
                return Ok(());
            }
        }
    }
}
