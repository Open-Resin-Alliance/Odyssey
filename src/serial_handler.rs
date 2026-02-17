use async_trait::async_trait;
use std::io::{self, BufRead, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::time::{interval, timeout, Duration};
use tokio_serial::{ClearBuffer, SerialPort, SerialStream};
use tokio_util::bytes::BytesMut;
use tokio_util::sync::CancellationToken;

use crate::error::OdysseyError;

#[derive(Debug)]
pub struct InternalCommsHandler {
    outgoing_sender: Sender<String>,
    outgoing_receiver: Receiver<String>,
    incoming_sender: Sender<String>,
    incoming_receiver: Receiver<String>,
}

impl Clone for InternalCommsHandler {
    fn clone(&self) -> Self {
        Self {
            outgoing_sender: self.outgoing_sender.clone(),
            outgoing_receiver: self.outgoing_receiver.resubscribe(),
            incoming_sender: self.incoming_sender.clone(),
            incoming_receiver: self.incoming_receiver.resubscribe(),
        }
    }
}

impl Default for InternalCommsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalCommsHandler {
    pub fn new() -> Self {
        let (outgoing_sender, outgoing_receiver) = broadcast::channel(200);
        let (incoming_sender, incoming_receiver) = broadcast::channel(200);
        Self {
            outgoing_sender,
            outgoing_receiver,
            incoming_sender,
            incoming_receiver,
        }
    }
    pub fn invert(&self) -> Self {
        Self {
            outgoing_sender: self.incoming_sender.clone(),
            outgoing_receiver: self.incoming_receiver.resubscribe(),
            incoming_sender: self.outgoing_sender.clone(),
            incoming_receiver: self.outgoing_receiver.resubscribe(),
        }
    }

    async fn flush_input(&mut self) -> Result<(), OdysseyError> {
        while !self.incoming_receiver.is_empty() {
            let _ = self.incoming_receiver.recv().await?;
        }
        Ok(())
    }

    async fn _await_response(&mut self, expected: &String) -> Result<(), OdysseyError> {
        let mut interv = interval(Duration::from_millis(100));
        while !self.check_response(expected).await? {
            interv.tick().await;
        }
        Ok(())
    }

    pub async fn send(&self, message: String) -> Result<(), OdysseyError> {
        self.outgoing_sender.send(message)?;
        Ok(())
    }
    pub async fn receive(&mut self) -> Result<String, OdysseyError> {
        self.incoming_receiver
            .recv()
            .await
            .map_err(|err| err.into())
    }

    pub async fn try_receive(&mut self) -> Result<Option<String>, OdysseyError> {
        match self.incoming_receiver.try_recv() {
            Ok(message) => Ok(Some(message)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(e) => {
                if let TryRecvError::Lagged(n) = e {
                    tracing::error!(
                        "Internal Communication channel fell too far behind! {} messages skipped!",
                        n
                    );
                }
                Err(e)?
            }
        }
    }

    pub async fn check_response(&mut self, expected: &String) -> Result<bool, OdysseyError> {
        self.receive().await.map(|msg| msg.contains(expected))
    }
    pub async fn await_response(
        &mut self,
        response: &String,
        timeout_duration: Duration,
    ) -> Result<(), OdysseyError> {
        match timeout(timeout_duration, self._await_response(response)).await {
            Ok(res) => res.map(|_| ()),
            Err(elapsed) => {
                tracing::warn!("Timed out waiting for response over serialport");
                Err(OdysseyError::hardware_error(Box::new(elapsed), 0))
            }
        }
    }

    pub async fn send_and_check(
        &mut self,
        message: String,
        expected: &String,
    ) -> Result<bool, OdysseyError> {
        self.flush_input().await?;
        self.send(message).await?;
        self.check_response(expected).await
    }

    pub async fn send_and_await(
        &mut self,
        message: String,
        expected: &String,
        timeout_duration: Duration,
    ) -> Result<(), OdysseyError> {
        self.flush_input().await?;
        self.send(message).await?;
        self.await_response(expected, timeout_duration).await
    }
}

#[async_trait]
pub trait SerialHandler {
    async fn run(
        mut self: Box<Self>,
        cancellation_token: CancellationToken,
    ) -> Result<(), OdysseyError>;
    fn get_internal_comms(&self) -> InternalCommsHandler;
    async fn is_ready(&mut self) -> Result<(), OdysseyError>;
}

pub struct SerialPortHandler {
    path: String,
    baudrate: u32,
    serial_stream: Option<SerialStream>,
    internal_comms: InternalCommsHandler,
}

impl SerialPortHandler {
    pub fn new(path: &String, baudrate: u32) -> Result<SerialPortHandler, OdysseyError> {
        let mut serial_stream =
            tokio_serial::SerialStream::open(&tokio_serial::new(path, baudrate))?;

        serial_stream.clear(ClearBuffer::All)?;
        serial_stream.set_exclusive(false)?;

        Ok(SerialPortHandler {
            path: path.to_string(),
            baudrate,
            serial_stream: None,
            internal_comms: InternalCommsHandler::new(),
        })
    }

    async fn get_serial_stream(&mut self) -> Result<&mut SerialStream, OdysseyError> {
        if self.serial_stream.is_none() {
            let mut serial_stream = tokio_serial::SerialStream::open(&tokio_serial::new(
                self.path.clone(),
                self.baudrate,
            ))?;

            serial_stream.clear(ClearBuffer::All)?;
            serial_stream.set_exclusive(false)?;
            self.serial_stream = Some(serial_stream);
        }
        return self
            .serial_stream
            .as_mut()
            .ok_or(OdysseyError::internal_state_error(
                "Unable to open SerialPort".into(),
                500,
            ));
    }

    async fn _send_serial(&mut self, message: &String) -> Result<usize, OdysseyError> {
        let serial_stream: &mut SerialStream = self.get_serial_stream().await?;
        loop {
            match serial_stream.try_write(message.as_bytes()) {
                Ok(n) => {
                    tracing::trace!("Wrote {} bytes to serial connection", n);

                    serial_stream.flush()?;
                    return Ok(n);
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl SerialHandler for SerialPortHandler {
    fn get_internal_comms(&self) -> InternalCommsHandler {
        self.internal_comms.clone()
    }

    async fn is_ready(&mut self) -> Result<(), OdysseyError> {
        let serial_stream: &mut SerialStream = self.get_serial_stream().await?;
        serial_stream.readable().await?;
        serial_stream.writable().await?;
        Ok(())
    }

    async fn run(
        mut self: Box<Self>,
        cancellation_token: CancellationToken,
    ) -> Result<(), OdysseyError> {
        let mut interval = interval(Duration::from_millis(100));

        let mut read_buf: [u8; 1024] = [0; 1024];
        loop {
            interval.tick().await;

            match self.get_serial_stream().await?.try_read(&mut read_buf) {
                Err(e) => match e.kind() {
                    io::ErrorKind::TimedOut => {
                        continue;
                    }
                    // Broken Pipe here
                    _ => Err(e)?,
                },
                Ok(n) => {
                    if n > 0 {
                        let read_string = str::from_utf8(&read_buf[0..n]).unwrap_or_default();
                        tracing::debug!("Read {} bytes from serial: {}", n, read_string.trim_end());
                        self.internal_comms.send(read_string.to_string()).await?;
                    }
                }
            };

            if let Some(message) = self.internal_comms.try_receive().await? {
                tracing::debug!("Writing to serial message={}", message);
                self._send_serial(&message).await?;
            }

            if cancellation_token.is_cancelled() {
                tracing::info!("Shutting down serial processing loop");
                return Ok(());
            }
        }
    }
}
