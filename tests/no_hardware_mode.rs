use std::{
    fs::{DirBuilder, File},
    sync::Arc,
    time::Duration,
};

use crate::common::{mock_uds_handler::{self, MockHardwareUDS}, test_resource_path};
use odyssey::{configuration::{Configuration, FileDirectory}, shutdown_handler::{self, ShutdownHandler}};
use tokio::{
    net::UnixStream, runtime::{Builder, Runtime}, sync::broadcast::{self, Receiver, Sender}, task, time::{interval, timeout}
};
use tokio_util::sync::CancellationToken;
use tracing::Level;

mod common;

#[test]
#[ignore]
fn no_hardware_tmp() {
    _no_hardware_mode(true);
}

#[test]
#[ignore]
fn no_hardware_mode() {
    _no_hardware_mode(false);
}

/**
 * Run Odyssey without any hardware. This is a manual testing utility, not an automated test.
 */
fn _no_hardware_mode(temp_uploads: bool) {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .init();

    let temp_dir = tempfile::TempDir::new().expect("Unable to create temp directory for test");

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

    if temp_uploads {
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

    let runtime = build_runtime();
    let shutdown_handler = ShutdownHandler::new();
    
    runtime.block_on(async move {
        let (odyssey_side, mock_side) = UnixStream::pair().unwrap();

        let mut mock_hardware = MockHardwareUDS { unix_stream: mock_side, mock_state: Default::default()};

        let mock_hardware_handle = task::spawn(async move {
            mock_hardware.run().await
        });
        //        let mock_hardware_handle = task::spawn(async move { mock_hardware_cancellation.run_until_cancelled(mock_hardware.run()).await });
        let odyssey_handle = task::spawn(odyssey::run_odyssey(config, Some(odyssey_side), shutdown_handler.clone()));

        shutdown_handler.until_shutdown().await;

        let _ = timeout(Duration::from_secs(1), odyssey_handle).await;
        let _ = timeout(Duration::from_secs(1), mock_hardware_handle).await;

    });

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
