use async_trait::async_trait;
use serialport::TTYPort;
use std::error::Error;
use std::io::{self, BufRead, BufReader, Write};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::time::{interval, timeout, Duration};
use tokio_util::sync::CancellationToken;

use crate::error::OdysseyError;

#[async_trait]
pub trait SerialHandler {
    async fn send(&mut self, message: String) -> Result<(), OdysseyError>;
    async fn receive(&mut self) -> Result<String, OdysseyError>;
    async fn await_response(
        &mut self,
        response: &String,
        timeout_duration: Duration,
    ) -> Result<(), OdysseyError>;
    async fn flush_input(&mut self) -> Result<(), OdysseyError>;
    async fn run(&mut self, cancellation_token: CancellationToken) -> Result<(), OdysseyError>;
}

pub struct TTYPortHandler {
    serial_port: TTYPort,
    output_sender: Sender<String>,
    output_receiver: Receiver<String>,
    input_sender: Sender<String>,
    input_receiver: Receiver<String>,
}

impl TTYPortHandler {
    pub fn new(serial_port: TTYPort) -> TTYPortHandler {
        let (output_sender, output_receiver) = broadcast::channel(200);
        let (input_sender, input_receiver) = broadcast::channel(200);

        TTYPortHandler {
            serial_port,
            output_sender,
            output_receiver,
            input_sender,
            input_receiver,
        }
    }

    async fn check_response(&mut self, expected: &String) -> Result<bool, OdysseyError> {
        self.receive()
            .await
            .map(|msg| msg.contains(expected))
            .map_err(|err| err.into())
    }

    async fn await_response(&mut self, expected: &String) -> Result<(), OdysseyError> {
        let mut interv = interval(Duration::from_millis(100));
        while !self.check_response(expected).await? {
            interv.tick();
        }
        Ok(())
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
    async fn send(&mut self, message: String) -> Result<(), OdysseyError> {
        self.output_sender.send(message)?;
        Ok(())
    }
    async fn receive(&mut self) -> Result<String, OdysseyError> {
        self.input_receiver.recv().await.map_err(|err| err.into())
    }
    async fn await_response(
        &mut self,
        response: &String,
        timeout_duration: Duration,
    ) -> Result<(), OdysseyError> {
        match timeout(timeout_duration, self.await_response(response)).await {
            Ok(res) => res.map(|_| ()),
            Err(elapsed) => {
                tracing::warn!("Timed out waiting for response over serialport");
                Err(OdysseyError::hardware_error(Box::new(elapsed), 0))
            }
        }
    }
    async fn flush_input(&mut self) -> Result<(), OdysseyError> {
        while !self.input_receiver.is_empty() {
            let _ = self.input_receiver.recv().await?;
        }
        Ok(())
    }

    async fn run(&mut self, cancellation_token: CancellationToken) -> Result<(), OdysseyError> {
        let mut buf_reader = BufReader::new(
            self.serial_port
                .try_clone_native()
                .map_err(|err| OdysseyError::hardware_error(Box::new(err), 0))?,
        );

        let mut interval = interval(Duration::from_millis(100));

        loop {
            interval.tick();

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
                        self.input_sender.send(read_string)?;
                    }
                }
            };

            match self.output_receiver.try_recv() {
                Err(e) => match e {
                    TryRecvError::Closed => {
                        tracing::error!("Unable to receive data to write to serial, inter-thread channel closed");
                        Err(e)?
                    }
                    _ => continue,
                },
                Ok(message) => {
                    tracing::debug!("Writing to serial message={}", message);
                    self._send_serial(&message).await?;
                }
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
