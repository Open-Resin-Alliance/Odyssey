use std::str::FromStr;

use clap::Parser;

use serialport::{ClearBuffer, SerialPort};
use simple_logger::SimpleLogger;
use tokio::{
    runtime::{Builder, Runtime},
    sync::{broadcast, mpsc},
};

use odyssey::{
    api,
    api_objects::PrinterState,
    configuration::Configuration,
    display::PrintDisplay,
    gcode::Gcode,
    printer::{Operation, Printer},
    serial_handler,
    shutdown_handler::ShutdownHandler,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Odyssey config file
    #[arg(default_value_t=String::from("./default.yaml"), short, long)]
    config: String,
    #[arg(default_value_t=String::from("DEBUG"), short, long)]
    loglevel: String,
}

fn main() {
    let shutdown_handler = ShutdownHandler::new();

    let args = parse_cli();

    SimpleLogger::new()
        .with_level(log::LevelFilter::from_str(&args.loglevel).expect("Unable to parse loglevel"))
        .init()
        .unwrap();

    log::info!("Starting Odyssey");

    let configuration = parse_config(args.config);

    let mut serial = tokio_serial::new(
        configuration.printer.serial.clone(),
        configuration.printer.baudrate,
    )
    .open_native()
    .expect("Unable to open serial port");

    serial
        .set_exclusive(false)
        .expect("Unable to set serial port exclusivity(false)");
    serial
        .clear(ClearBuffer::All)
        .expect("Unable to clear serialport buffers");

    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

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

        let writer_serial = serial
            .try_clone_native()
            .expect("Unable to clone serial port handler");
        let listener_serial = serial
            .try_clone_native()
            .expect("Unable to clone serial port handler");

        let serial_read_handle = tokio::spawn(serial_handler::run_listener(
            listener_serial,
            serial_read_sender,
            shutdown_handler.cancellation_token.clone(),
        ));

        let serial_write_handle = tokio::spawn(serial_handler::run_writer(
            writer_serial,
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

        let _ = serial_read_handle.await;
        let _ = serial_write_handle.await;
        let _ = statemachine_handle.await;
        let _ = api_handle.await;
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

fn parse_cli() -> Args {
    Args::parse()
}

fn parse_config(config_file: String) -> Configuration {
    Configuration::from_file(config_file)
        .expect("Config could not be parsed. See example odyssey.yaml for expected fields:")
}
