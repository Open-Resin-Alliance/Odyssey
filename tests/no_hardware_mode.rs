use std::time::Duration;

use odyssey::{
    api,
    api_objects::PrinterState,
    configuration::Configuration,
    display::PrintDisplay,
    gcode::Gcode,
    printer::{Operation, Printer},
    shutdown_handler::ShutdownHandler,
};
use simple_logger::SimpleLogger;
use tokio::{
    runtime::{Builder, Runtime},
    sync::{
        broadcast::{self, Receiver, Sender},
        mpsc,
    },
    time::interval,
};
use tokio_util::sync::CancellationToken;

mod common;

/**
 * Run Odyssey without any hardware. This is a manual testing utility, not an automated test.
 */
#[test]
#[ignore]
fn no_hardware_mode() {
    let shutdown_handler = ShutdownHandler::new();

    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let tmp_file = tempfile::Builder::new()
        .prefix("odysseyTest")
        .tempfile()
        .expect("Unable to make temporary file");

    log::info!("Write frames to {}", tmp_file.path().display());

    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

    let configuration =
        hardwareless_config(tmp_file.path().as_os_str().to_str().unwrap().to_owned());

    let gcode = Gcode::new(
        configuration.clone(),
        serial_read_receiver,
        serial_write_sender,
    );

    let display: PrintDisplay = PrintDisplay::new(configuration.display.clone());

    let operation_channel = mpsc::channel::<Operation>(100);
    let status_channel = broadcast::channel::<PrinterState>(100);

    let runtime = build_runtime();

    runtime.block_on(async {
        let sender = operation_channel.0.clone();
        let receiver = status_channel.1.resubscribe();

        let serial_handle = tokio::spawn(serial_feedback_loop(
            configuration.clone(),
            serial_read_sender,
            serial_write_receiver,
            shutdown_handler.cancellation_token.clone(),
        ));

        let statemachine_handle = tokio::spawn(Printer::start_printer(
            configuration.printer.clone(),
            display,
            gcode,
            operation_channel.1,
            status_channel.0.clone(),
            shutdown_handler.cancellation_token.clone(),
        ));

        let api_handle = tokio::spawn(api::start_api(
            configuration,
            sender,
            receiver,
            shutdown_handler.cancellation_token.clone(),
        ));

        shutdown_handler.until_shutdown().await;

        let _ = serial_handle.await;
        let _ = statemachine_handle.await;
        let _ = api_handle.await;

        tmp_file.close().expect("Unable to remove tempdir");
        log::info!("Shutting down");
    });

    runtime.shutdown_background();
}

pub async fn serial_feedback_loop(
    configuration: Configuration,
    sender: Sender<String>,
    mut receiver: Receiver<String>,
    cancellation_token: CancellationToken,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        if cancellation_token.is_cancelled() {
            break;
        }
        interval.tick().await;
        match receiver.try_recv() {
            Ok(command) => {
                log::info!("{}", command);

                let response: String;
                if command.as_str().trim() == configuration.gcode.status_check.as_str().trim() {
                    response = configuration.gcode.status_desired.clone();
                } else {
                    response = configuration.gcode.move_sync.clone();
                };

                log::info!("command='{}', response='{}'", command.trim(), response);

                sender
                    .send(response)
                    .expect("Unable to send gcode response message");
            }
            Err(err) => match err {
                broadcast::error::TryRecvError::Empty => continue,
                _ => panic!(),
            },
        };
    }
}

fn build_runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("odyssey-worker")
        .thread_stack_size(3 * 1024 * 1024)
        .enable_time()
        .enable_io()
        .build()
        .expect("Unable to start Tokio runtime")
}

fn hardwareless_config(framebuffer_filename: String) -> Configuration {
    let mut default_config = common::default_test_configuration();

    default_config.display.frame_buffer = framebuffer_filename;
    default_config.printer.serial = String::from("n/a");

    default_config
}
