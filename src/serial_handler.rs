use async_trait::async_trait;
use serialport::TTYPort;
use std::io::{self, BufRead, BufReader, Write};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::time::{interval, timeout, Duration};
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
        self.receive()
            .await
            .map(|msg| msg.contains(expected))
            .map_err(|err| err)
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
}

pub struct TTYPortHandler {
    serial_port: TTYPort,
    internal_comms: InternalCommsHandler,
}

impl TTYPortHandler {
    pub fn new(serial_port: TTYPort) -> TTYPortHandler {
        TTYPortHandler {
            serial_port,
            internal_comms: InternalCommsHandler::new(),
        }
    }

    async fn _send_serial(&mut self, message: &String) -> Result<usize, OdysseyError> {
        loop {
            match self.serial_port.write(message.as_bytes()) {
                Ok(n) => {
                    tracing::trace!("Wrote {} bytes to serial connection", n);

                    self.serial_port.flush()?;
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
impl SerialHandler for TTYPortHandler {
    fn get_internal_comms(&self) -> InternalCommsHandler {
        self.internal_comms.clone()
    }

    async fn run(
        mut self: Box<Self>,
        cancellation_token: CancellationToken,
    ) -> Result<(), OdysseyError> {
        let mut buf_reader = BufReader::new(
            self.serial_port
                .try_clone_native()
                .map_err(|err| OdysseyError::hardware_error(Box::new(err), 0))?,
        );

        let mut interval = interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            let mut read_string = String::new();
            match buf_reader.read_line(&mut read_string) {
                Err(e) => match e.kind() {
                    io::ErrorKind::TimedOut => {
                        continue;
                    }
                    // Broken Pipe here
                    _ => Err(e)?,
                },
                Ok(n) => {
                    if n > 0 {
                        tracing::debug!("Read {} bytes from serial: {}", n, read_string.trim_end());
                        self.internal_comms.send(read_string).await?;
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

pub async fn run_listener(
    serial_port: TTYPort,
    sender: Sender<String>,
    cancellation_token: CancellationToken,
) {
    let mut buf_reader = BufReader::new(
        serial_port
            .try_clone_native()
            .expect("Unable to clone serial port"),
    );
    let mut interval = interval(Duration::from_millis(100));

    loop {
        if cancellation_token.is_cancelled() {
            log::info!("Shutting down serial read loop");
            break;
        }
        interval.tick().await;
        let mut read_string = String::new();
        match buf_reader.read_line(&mut read_string) {
            Err(e) => match e.kind() {
                io::ErrorKind::TimedOut => {
                    continue;
                }
                // Broken Pipe here
                other_error => panic!("Error reading from serial port: {:?}", other_error),
            },
            Ok(n) => {
                if n > 0 {
                    tracing::debug!("Read {} bytes from serial: {}", n, read_string.trim_end());
                    sender
                        .send(read_string)
                        .expect("Unable to send message to channel");
                }
            }
        };
    }
}

pub async fn run_writer(
    mut serial_port: TTYPort,
    mut receiver: Receiver<String>,
    cancellation_token: CancellationToken,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        if cancellation_token.is_cancelled() {
            log::info!("Shutting down exiting serial write loop");
            break;
        }
        interval.tick().await;

        if let Ok(message) = receiver.recv().await {
            while let Err(e) = send_serial(&mut serial_port, message.clone()).await {
                match e.kind() {
                    io::ErrorKind::Interrupted => {
                        continue;
                    }
                    _ => break,
                }
            }
        }
    }
}

async fn send_serial(serial_port: &mut TTYPort, message: String) -> io::Result<usize> {
    let n = serial_port.write(message.as_bytes())?;

    serial_port
        .flush()
        .expect("Unable to flush serial connection");

    tracing::trace!("Wrote {} bytes", n);
    Ok(n)
}
