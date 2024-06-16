use config::{Config, ConfigError, File};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrinterConfig {
    pub serial: String,
    pub baudrate: u32,
    pub frame_buffer: String,
    pub fb_bit_depth: u8,
    pub fb_chunk_size: u8,
    pub max_z: f64,
    pub default_lift: f64,
    pub default_up_speed: f64,
    pub default_down_speed: f64,
    pub default_wait_before_exposure: f64,
    pub default_wait_after_exposure: f64,
    pub pause_lift: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct GcodeConfig {
    pub boot: String,
    pub shutdown: String,
    pub home_command: String,
    pub move_command: String,
    pub print_start: String,
    pub print_end: String,
    pub layer_start: String,
    pub cure_start: String,
    pub cure_end: String,
    pub move_sync: String,
    pub move_timeout: usize,
    pub status_check: String,
    pub status_desired: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct ApiConfig {
    pub upload_path: String,
    pub usb_glob: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct Configuration {
    pub printer: PrinterConfig,
    pub gcode: GcodeConfig,
    pub api: ApiConfig,
}

impl Configuration {
    pub fn load(config_file: String) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(config_file.as_str()).required(true))
            .build()?;

        s.try_deserialize()
    }
}
