use crate::{
    api_objects::PrinterState,
    configuration::Configuration,
    display::PrintDisplay,
    gcode::Gcode,
    printer::{Operation, Printer},
    serial_handler::SerialHandler,
    shutdown_handler::ShutdownHandler,
};
use serialport::{ClearBuffer, SerialPort};
use std::sync::Arc;
use tokio::{
    runtime::{Builder, Runtime},
    sync::{broadcast, mpsc},
};

pub mod api;
pub mod api_objects;
pub mod configuration;
pub mod display;
pub mod error;
pub mod gcode;
pub mod printer;
pub mod printfile;
pub mod serial_handler;
pub mod shutdown_handler;
pub mod sl1;
pub mod updates;
mod wrapped_framebuffer;

pub fn start_odyssey(
    runtime: Runtime,
    configuration: Arc<Configuration>,
    serial_handler: Box<dyn SerialHandler + Send>,
) {
    let shutdown_handler = ShutdownHandler::new();

    let gcode = Gcode::new(
        &configuration.gcode,
        serial_handler.get_internal_comms().clone().invert(),
    );

    let display: PrintDisplay = PrintDisplay::new(&configuration.display);

    let operation_channel = mpsc::channel::<Operation>(100);
    let status_channel = broadcast::channel::<PrinterState>(100);

    let sender = operation_channel.0.clone();
    let receiver = status_channel.1.resubscribe();

    let serial_handle =
        runtime.spawn(serial_handler.run(shutdown_handler.cancellation_token.clone()));

    let statemachine_handle = runtime.spawn(Printer::start_printer(
        configuration.clone(),
        display,
        gcode,
        operation_channel.1,
        status_channel.0.clone(),
        shutdown_handler.cancellation_token.clone(),
    ));

    let api_handle = runtime.spawn(api::start_api(
        configuration.clone(),
        sender,
        receiver,
        shutdown_handler.cancellation_token.clone(),
    ));

    runtime.block_on(async {
        shutdown_handler.until_shutdown().await;

        let _ = serial_handle.await;
        let _ = statemachine_handle.await;
        let _ = api_handle.await;
    });
}
