use poem_openapi::Object;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use std::{error::Error, fmt::Debug, fs, io, sync::Arc};
use optional_struct::*;

#[optional_struct(UpdatePrinterConfig)]
#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrinterConfig {
    pub serial: String,
    pub baudrate: u32,
    pub max_z: f64,
    pub default_lift: f64,
    pub default_up_speed: f64,
    pub default_down_speed: f64,
    pub default_wait_before_exposure: f64,
    pub default_wait_after_exposure: f64,
    pub pause_lift: f64,
}

#[optional_struct(UpdateDisplayConfig)]
#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct DisplayConfig {
    pub frame_buffer: String,
    pub bit_depth: Vec<u8>,
    pub screen_width: u32,
    pub screen_height: u32,
}

#[optional_struct(UpdateGcodeConfig)]
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

#[optional_struct(UpdateApiConfig)]
#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct ApiConfig {
    pub upload_path: String,
    pub usb_glob: String,
    pub port: u16,
}

#[optional_struct(UpdateConfiguration)]
#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct Configuration {
    #[optional_wrap]
    #[optional_rename(UpdatePrinterConfig)]
    pub printer: PrinterConfig,

    #[optional_wrap]
    #[optional_rename(UpdateGcodeConfig)]
    pub gcode: GcodeConfig,

    #[optional_wrap]
    #[optional_rename(UpdateApiConfig)]
    pub api: ApiConfig,

    #[optional_wrap]
    #[optional_rename(UpdateDisplayConfig)]
    pub display: DisplayConfig,

    
    #[serde(skip_serializing)]
    pub config_file: Option<String>
}

impl Configuration {
    pub fn from_file(config_file: String) -> Result<Self, Box<dyn Error>> {
        let mut config: Configuration = serde_yaml::from_reader(io::BufReader::new(fs::File::open(&config_file)?))?;
        config.config_file = Some(config_file);

        Ok(config)
    }

    pub fn overwrite_file(config: &Configuration) -> Result<(),Box<dyn Error + Send + Sync>> {
        
        if let Some(config_file) = &config.config_file.clone() {
            return Configuration::write_to_file(config_file, config);
        }
        else {
            log::error!("Config destination unknown, unable to save changes");
            return Err(io::Error::new(io::ErrorKind::NotFound, "config_file not set on Configuration struct").into());
        }
    }
    pub fn write_to_file(config_file: &String, config: &Configuration) -> Result<(),Box<dyn Error + Send + Sync>> {
            
        let content = serde_yaml::to_string(&config).unwrap();

        let timestamp = std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH)?.as_secs();

        log::info!("Writing config to {}", config_file);

        if fs::exists(config_file)? {
            let old_config = format!("{}.{}.old",config_file,timestamp);
            log::info!("Moving existing config file to {}", old_config);
            fs::rename(config_file, old_config).map_err(|err| io::Error::new(err.kind(), format!("Unable to backup existing config file {:?}", err)))?;
        }
        
        fs::write(config_file, content)?;

        Ok(())
    }

}

pub type LockedConfig = Arc<RwLock<Configuration>>;
