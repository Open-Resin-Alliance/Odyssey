use serialport::TTYPort;
use std::io::{self, BufRead, BufReader, Write};
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::time::{interval, Duration};

pub async fn run_listener(serial_port: TTYPort, sender: Sender<String>) {
    let mut buf_reader = BufReader::new(
        serial_port
            .try_clone_native()
            .expect("Unable to clone serial port"),
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
                other_error => panic!("Error reading from serial port: {:?}", other_error),
            },
            Ok(n) => {
                if n > 0 {
                    log::debug!("Read {} bytes from serial: {}", n, read_string.trim_end());
                    sender
                        .send(read_string)
                        .expect("Unable to send message to channel");
                }
            }
        };
    }
}

pub async fn run_writer(mut serial_port: TTYPort, mut receiver: Receiver<String>) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        interval.tick().await;

        match receiver.recv().await {
            Ok(message) => {
                while let Err(e) = send_serial(&mut serial_port, message.clone()).await {
                    match e.kind() {
                        io::ErrorKind::Interrupted => {
                            continue;
                        }
                        _ => break,
                    }
                }
            }
            Err(_) => todo!(),
        }
    }
}

async fn send_serial(serial_port: &mut TTYPort, message: String) -> io::Result<usize> {
    let n = serial_port.write(message.as_bytes())?;

    serial_port
        .flush()
        .expect("Unable to flush serial connection");

    log::trace!("Wrote {} bytes", n);
    Ok(n)
}
