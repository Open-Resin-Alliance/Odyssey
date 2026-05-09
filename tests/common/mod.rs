use odyssey::configuration::{
    ApiConfig, Configuration, DisplayConfig, FileDirectory, GcodeConfig, KlipperUDSConfig, PrinterConfig
};

pub mod mock_uds_handler;

#[allow(unused_variables)]
pub static TEST_RESOURCE_DIR: &str = "tests/resources";
pub static RESOURCE_DIR: &str = "resources";
pub static UPLOAD_DIR: &str = "uploads";
pub static CARGO_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[allow(dead_code)]
pub fn default_test_configuration() -> Configuration {
    Configuration {
        config_file: Some("".to_owned()),
        printer: PrinterConfig {
            max_z: 300.0,
            default_lift: 10.0,
            default_up_speed: 3.4,
            default_down_speed: 3.4,
            default_wait_before_exposure: 2.2,
            default_wait_after_exposure: 1.5,
            pause_lift: 100.0,
        },
        klipper_uds: KlipperUDSConfig {
            connection_path: String::from("/dev/null"),
            boot: String::from("G90"),
            shutdown: String::from("M84\nUVLED_OFF"),
            home_command: String::from("HOME_AXIS"),
            move_command: String::from("MOVE_PLATE Z={z} F={speed}"),
            print_start: String::from("START_GCODE TOTAL_LAYERS={total_layers}"),
            print_end: String::from("END_GCODE"),
            layer_start: String::from("LAYER_START_GCODE LAYER={layer}"),
            cure_start: String::from("START_CURE"),
            cure_end: String::from("END_CURE"),
            curing_device: None,
            manual_move_command: None,
        },
        api: ApiConfig {
            file_dirs: vec![FileDirectory {
                label: "Uploads".to_string(),
                description: None,
                path: "uploads".to_string(),
            }],
            port: 12357,
            enable_docs: Some(true),
        },
        display: DisplayConfig {
            frame_buffer: "/dev/null".to_owned(),
            bit_depth: vec![5, 6, 5],
            screen_width: 1920,
            screen_height: 1080,
        },
    }
}

#[allow(dead_code)]
pub fn resource_path(resource_file: String) -> String {
    format!("{CARGO_DIR}/{RESOURCE_DIR}/{resource_file}")
}

#[allow(dead_code)]
pub fn test_resource_path(resource_file: String) -> String {
    format!("{CARGO_DIR}/{TEST_RESOURCE_DIR}/{resource_file}")
}

#[allow(dead_code)]
pub fn upload_path() -> String {
    format!("{CARGO_DIR}/{UPLOAD_DIR}")
}
