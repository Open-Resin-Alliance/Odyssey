use std::{
    fs::{DirBuilder, File},
    sync::Arc,
    time::Duration,
};

use crate::common::{mock_serial_handler::MockSerialHandler, test_resource_path};
use odyssey::configuration::{Configuration, FileDirectory, PixelFormat};

use tokio::{
    runtime::{Builder, Runtime},
    sync::broadcast::{self, Receiver, Sender},
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::Level;

mod common;

#[derive(Default)]
struct NoHardwareSettings {
    temp_uploads: bool,
    zero_times: bool,
    screen_width: Option<u32>,
    screen_height: Option<u32>,
    pixel_format: Option<PixelFormat>,
}

#[test]
#[ignore]
fn no_hardware_tmp() {
    _no_hardware_mode(NoHardwareSettings {
        temp_uploads: true,
        zero_times: true,
        ..Default::default()
    });
}

#[test]
#[ignore]
fn emulated_fb() {
    _no_hardware_mode(NoHardwareSettings {
        temp_uploads: true,
        zero_times: false,
        screen_width: Some(192),
        screen_height: Some(108),
        pixel_format: Some(PixelFormat {
            bit_depth: vec![8],
            left_pad_bits: 0,
            right_pad_bits: 0,
        }),
    });
}

#[test]
#[ignore]
fn no_hardware_mode() {
    _no_hardware_mode(NoHardwareSettings {
        temp_uploads: false,
        zero_times: true,
        ..Default::default()
    });
}

/**
 * Run Odyssey without any hardware. This is a manual testing utility, not an automated test.
 */
fn _no_hardware_mode(settings: NoHardwareSettings) {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    let temp_dir = tempfile::Builder::new()
        .prefix("odysseyTest")
        .tempdir()
        .expect("Unable to create temp directory for test");

    DirBuilder::new()
        .create(temp_dir.path().join("uploads"))
        .expect("Unable to generate uploads directory");
    DirBuilder::new()
        .create(temp_dir.path().join("config"))
        .expect("Unable to generate config directory");

    let temp_config = temp_dir.path().join("config/mockConfig.yaml");
    let temp_fb = temp_dir.path().join("config/mockFb");
    File::create(&temp_fb).expect("Unable to generate mock FrameBuffer file");

    tracing::info!("Write frames to {}", temp_fb.display());

    let mut configuration = Configuration::from_file(test_resource_path("default.yaml".to_owned()))
        .expect("Config could not be parsed");

    configuration.display.frame_buffer = temp_fb.as_os_str().to_str().unwrap().to_owned();
    configuration.config_file = Some(temp_config.as_os_str().to_str().unwrap().to_owned());

    if let Some(pixel_format) = settings.pixel_format {
        configuration.display.pixel_format = pixel_format;
    }
    if let Some(screen_width) = settings.screen_width {
        configuration.display.screen_width = screen_width;
    }
    if let Some(screen_height) = settings.screen_height {
        configuration.display.screen_height = screen_height;
    }
    if settings.zero_times {
        configuration.printer.default_wait_before_exposure = 0.0;
        configuration.printer.default_wait_after_exposure = 0.0;
    }
    if settings.temp_uploads {
        configuration.api.file_dirs = vec![
            FileDirectory {
                label: "Uploads".to_string(),
                description: None,
                path: temp_dir
                    .path()
                    .join("uploads")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            },
            FileDirectory {
                label: "Config".to_string(),
                description: Some("Houses the test Odyssey Config and Comms files".to_string()),
                path: temp_dir
                    .path()
                    .join("config")
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            },
        ];
    }

    Configuration::overwrite_file(&configuration).expect("Unable to save temporary config file");

    let config = Arc::new(configuration);

    let mut serial_handler = MockSerialHandler::new(config.gcode.move_sync.clone());
    serial_handler.add_response(
        config.gcode.status_check.trim().to_string(),
        config.gcode.status_desired.trim().to_string(),
    );

    odyssey::start_odyssey(build_runtime(), config, Box::new(serial_handler));
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
            log::info!("Shutting down simulated serial feedback loop");
            break;
        }
        interval.tick().await;
        match receiver.try_recv() {
            Ok(command) => {
                tracing::info!("{}", command);

                let response: String =
                if command.as_str().trim() == status_check.trim() {
                    status_desired.clone()
                } else {
                     move_sync.clone()
                };

                tracing::info!("command='{}', response='{}'", command.trim(), response);

                sender
                    .send(response)
                    .expect("Unable to send gcode response message");
            }
            Err(err) => {
                if err == broadcast::error::TryRecvError::Empty {
                    continue;
                }
            }
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
