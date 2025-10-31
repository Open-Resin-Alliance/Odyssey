use std::sync::Arc;

use crate::{
    configuration::Configuration, display::PrintDisplay, gcode::Gcode,
    serial_handler::SerialHandler,
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

fn start_odyssey(configuration: Arc<Configuration>, serial_handler: impl SerialHandler) {
    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

    let gcode = Gcode::new(
        &configuration.gcode,
        serial_read_receiver,
        serial_write_sender,
    );

    let display: PrintDisplay = PrintDisplay::new(&configuration.display);

    let operation_channel = mpsc::channel::<Operation>(100);
    let status_channel = broadcast::channel::<PrinterState>(100);

    let runtime = build_runtime();

    let sender = operation_channel.0.clone();
    let receiver = status_channel.1.resubscribe();

    let writer_serial = serial
        .try_clone_native()
        .expect("Unable to clone serial port handler");
    let listener_serial = serial
        .try_clone_native()
        .expect("Unable to clone serial port handler");

    let serial_read_handle = runtime.spawn(serial_handler::run_listener(
        listener_serial,
        serial_read_sender,
        shutdown_handler.cancellation_token.clone(),
    ));

    let serial_write_handle = runtime.spawn(serial_handler::run_writer(
        writer_serial,
        serial_write_receiver,
        shutdown_handler.cancellation_token.clone(),
    ));

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
        args.apidocs,
    ));

    runtime.block_on(async {
        shutdown_handler.until_shutdown().await;

        let _ = serial_read_handle.await;
        let _ = serial_write_handle.await;
        let _ = statemachine_handle.await;
        let _ = api_handle.await;
    });
}
