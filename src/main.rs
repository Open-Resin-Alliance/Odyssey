use std::{str::FromStr, sync::Arc};

use clap::Parser;

use serialport::{ClearBuffer, SerialPort};
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
    serial_handler::{self, SerialHandler, TTYPortHandler},
    shutdown_handler::ShutdownHandler,
};
use tracing::level_filters::LevelFilter;

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
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from_str(&args.loglevel).expect("Unable to parse loglevel"))
        .init();

    tracing::info!("Starting Odyssey");

    let configuration = Arc::new(
        Configuration::from_file(args.config)
            .expect("Config could not be parsed. See example odyssey.yaml for expected fields:"),
    );

    let mut serial = tokio_serial::new(
        &configuration.printer.serial,
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

    let serial_handler = Box::new(TTYPortHandler::new(serial));

    odyssey::start_odyssey(build_runtime(), configuration, serial_handler);
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
