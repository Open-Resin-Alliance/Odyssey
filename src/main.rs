use std::{str::FromStr, sync::Arc};

use clap::Parser;

use tokio::{net::UnixStream, runtime::{Builder, Runtime}};

use odyssey::{configuration::Configuration, shutdown_handler::ShutdownHandler};
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

    let runtime = build_runtime();

    runtime.block_on(odyssey::run_odyssey(configuration, None, ShutdownHandler::new()))
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
