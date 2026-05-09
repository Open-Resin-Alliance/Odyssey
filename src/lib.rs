use crate::{
    configuration::Configuration,
    display::PrintDisplay,
    hardware_control::{
        HardwareControl, klipper_uds::{KlipperUDS}
    },
    printer::{Operation, Printer},
    shutdown_handler::ShutdownHandler,
};
use git_version::git_version;
use std::sync::Arc;
use tokio::{net::UnixStream, runtime::Runtime, sync::mpsc, task};

pub mod api;
pub mod api_objects;
pub mod configuration;
pub mod display;
pub mod error;
pub mod hardware_control;
pub mod printer;
pub mod printfile;
pub mod shutdown_handler;
pub mod sl1;
pub mod updates;
pub mod uploads;
mod wrapped_framebuffer;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMPILE_TARGET: &str = env!("CARGO_COMPILE_TARGET");
const COMMIT_HASH: &str = git_version!(fallback = "unknown");

pub async fn run_odyssey(
    configuration: Arc<Configuration>,
    unix_stream: Option<UnixStream>,
    shutdown_handler: ShutdownHandler
) {
    let klipper_uds = KlipperUDS::new(&configuration.klipper_uds, unix_stream);

    let display: PrintDisplay = PrintDisplay::new(&configuration.display);

    let operation_channel = mpsc::channel::<Operation>(100);

    let operation_sender = operation_channel.0.clone();

    let klipper_uds_handle =
        task::spawn(klipper_uds.clone().run(shutdown_handler.cancellation_token.clone()));

    let printer = Printer::new(
        configuration.clone(),
        display,
        klipper_uds,
        operation_channel.1,
        shutdown_handler.cancellation_token.clone(),
    );
    let printer_state_receiver = printer.get_state_receiver();

    let statemachine_handle = task::spawn(printer.start_printer());

    let api_handle = task::spawn(api::start_api(
        configuration.clone(),
        operation_sender,
        printer_state_receiver,
        shutdown_handler.cancellation_token.clone(),
    ));

    shutdown_handler.until_shutdown().await;
    tracing::info!("Shutting down Odyssey");

    let _ = klipper_uds_handle.await;
    let _ = statemachine_handle.await;
    let _ = api_handle.await;
}
