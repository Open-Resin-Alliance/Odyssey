use std::{fs, sync::Arc, time::Duration};

use crate::common::test_resource_path;
use odyssey::{
    api,
    api_objects::PrinterState,
    configuration::Configuration,
    display::PrintDisplay,
    gcode::Gcode,
    printer::{Operation, Printer},
    shutdown_handler::ShutdownHandler,
};
use tokio::{
    runtime::{Builder, Runtime},
    sync::{
        broadcast::{self, Receiver, Sender},
        mpsc,
    },
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::Level;

mod common;

/**
 * Run Odyssey without any hardware. This is a manual testing utility, not an automated test.
 */
#[test]
#[ignore]
fn no_hardware_mode() {
    let shutdown_handler = ShutdownHandler::new();

    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    let temp_dir = tempfile::TempDir::new().expect("Unable to create temp directory for test");

    let temp_config = temp_dir.path().join("mockConfig.yaml");
    let temp_fb = temp_dir.path().join("mockFb");
    fs::File::create(&temp_fb).expect("Unable to generate mock FrameBuffer file");

    tracing::info!("Write frames to {}", temp_fb.display());

    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

    let mut configuration = Configuration::from_file(test_resource_path("default.yaml".to_owned()))
        .expect("Config could not be parsed");

    configuration.display.frame_buffer = temp_fb.as_os_str().to_str().unwrap().to_owned();
    configuration.config_file = Some(temp_config.as_os_str().to_str().unwrap().to_owned());
    configuration.api.upload_path = temp_dir.path().as_os_str().to_str().unwrap().to_owned();

    Configuration::overwrite_file(&configuration).expect("Unable to save temporary config file");

    let config = Arc::new(configuration);

    let gcode = Gcode::new(&config.gcode, serial_read_receiver, serial_write_sender);

    let display: PrintDisplay = PrintDisplay::new(&config.display);

    let operation_channel = mpsc::channel::<Operation>(100);
    let status_channel = broadcast::channel::<PrinterState>(100);

    let runtime = build_runtime();

    let sender = operation_channel.0.clone();
    let receiver = status_channel.1.resubscribe();

    let serial_handle = runtime.spawn(serial_feedback_loop(
        serial_read_sender,
        serial_write_receiver,
        shutdown_handler.cancellation_token.clone(),
        config.gcode.status_check.clone(),
        config.gcode.status_desired.clone(),
        config.gcode.move_sync.clone(),
    ));

    let statemachine_handle = runtime.spawn(Printer::start_printer(
        config.clone(),
        display,
        gcode,
        operation_channel.1,
        status_channel.0.clone(),
        shutdown_handler.cancellation_token.clone(),
    ));

    let api_handle = runtime.spawn(api::start_api(
        config.clone(),
        sender,
        receiver,
        shutdown_handler.cancellation_token.clone(),
    ));

    runtime.block_on(async {
        shutdown_handler.until_shutdown().await;

        let _ = serial_handle.await;
        let _ = statemachine_handle.await;
        let _ = api_handle.await;

        temp_dir.close().expect("Unable to remove tempdir");
        tracing::info!("Shutting down");
    });

    runtime.shutdown_background();
}

pub async fn serial_feedback_loop(
    sender: Sender<String>,
    mut receiver: Receiver<String>,
    cancellation_token: CancellationToken,
    status_check: String,
    status_desired: String,
    move_sync: String,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        if cancellation_token.is_cancelled() {
            break;
        }
        interval.tick().await;
        match receiver.try_recv() {
            Ok(command) => {
                tracing::info!("{}", command);

                let response: String;
                if command.as_str().trim() == status_check.trim() {
                    response = status_desired.clone();
                } else {
                    response = move_sync.clone();
                };

                tracing::info!("command='{}', response='{}'", command.trim(), response);

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
