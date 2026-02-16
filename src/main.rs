use std::{str::FromStr, sync::Arc};

use clap::Parser;

use serialport::{ClearBuffer, SerialPort};
use tokio::runtime::{Builder, Runtime};

use odyssey::{configuration::Configuration, serial_handler::SerialPortHandler};
use tracing::level_filters::LevelFilter;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Odyssey config file
    #[arg(default_value_t=String::from("./default.yaml"), short, long)]
    config: String,
    #[arg(default_value_t=String::from("DEBUG"), short, long)]
    loglevel: String,
    #[arg(default_value_t = false, short, long)]
    apidocs: bool,
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

    let serial_handler = Box::new(
        SerialPortHandler::new(
            &configuration.printer.serial,
            configuration.printer.baudrate,
        )
        .expect("Unable to open serialport"),
    );

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
